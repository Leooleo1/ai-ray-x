//! Files delivered by an agent are copied into panel storage before the answer is saved.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use base64::Engine;
use ring::rand::{SecureRandom, SystemRandom};
use serde::Serialize;
use serde_json::{Value, json};

const OUTPUT: &str = "raiox-output";
const MAX_BYTES: usize = crate::uploads::MAX_BYTES;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct Artifact {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mime: Option<&'static str>,
}

fn allowed_part(part: &str) -> bool {
    !part.is_empty() && !part.starts_with('.') && !part.ends_with(['.', ' '])
        && !part.contains(':') && !part.chars().any(char::is_control)
        && !raiox_core::secrets::is_secret_file(part)
}

fn mime(ext: &str, bytes: &[u8]) -> Option<&'static str> {
    if let Some(format) = crate::uploads::detect(bytes) {
        return (ext == format.ext() || (ext == "jpeg" && format.ext() == "jpg")).then(|| format.mime());
    }
    match ext {
        "pdf" if bytes.starts_with(b"%PDF-") => Some("application/pdf"),
        "txt" | "md" | "csv" if !bytes.contains(&0) && std::str::from_utf8(bytes).is_ok() => Some("text/plain"),
        _ => None,
    }
}

fn read(path: &Path) -> Result<Vec<u8>> {
    let meta = fs::symlink_metadata(path)?;
    ensure!(meta.is_file() && meta.len() <= MAX_BYTES as u64, "invalid attachment file");
    let mut bytes = Vec::new();
    File::open(path)?.take(MAX_BYTES as u64 + 1).read_to_end(&mut bytes)?;
    ensure!(!bytes.is_empty() && bytes.len() <= MAX_BYTES, "invalid attachment size");
    Ok(bytes)
}

fn source(project: &Path, requested: &str) -> Result<PathBuf> {
    let relative = requested.strip_prefix("raiox-output/").context("outside the output directory")?;
    ensure!(relative.split('/').all(allowed_part), "invalid attachment path");
    let root = project.canonicalize()?.join(OUTPUT);
    ensure!(root.canonicalize()? == root, "invalid output directory");
    let path = root.join(relative);
    let resolved = path.canonicalize()?;
    ensure!(resolved == path, "invalid attachment path");
    Ok(path)
}

fn folder(state: &Path, bot: &str) -> PathBuf {
    state.join("artifacts").join(bot)
}

fn keep(state: &Path, bot: &str, project: &Path, requested: &str) -> Result<(String, &'static str)> {
    let path = source(project, requested)?;
    let ext = path.extension().and_then(|part| part.to_str()).context("unsupported attachment")?.to_ascii_lowercase();
    let bytes = read(&path)?;
    let kind = mime(&ext, &bytes).context("unsupported attachment")?;
    let mut random = [0_u8; 16];
    SystemRandom::new().fill(&mut random).map_err(|_| anyhow::anyhow!("could not assign attachment ID"))?;
    let key: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
    let id = format!("{key}.{ext}");
    let dir = folder(&state.canonicalize()?, bot);
    fs::create_dir_all(&dir)?;
    ensure!(dir.canonicalize()? == dir, "invalid attachment directory");
    let destination = dir.join(&id);
    let mut file = OpenOptions::new().write(true).create_new(true).open(&destination)?;
    if let Err(error) = file.write_all(&bytes) {
        drop(file);
        let _ = fs::remove_file(&destination);
        return Err(error.into());
    }
    Ok((id, kind))
}

pub(crate) fn collect(state: &Path, bot: &str, project: &Path, requests: &[String]) -> Vec<Artifact> {
    requests.iter().map(|request| {
        let normalized = request.replace('\\', "/");
        let name: String = normalized.rsplit('/').next().unwrap_or("").chars().filter(|c| !c.is_control()).take(160).collect();
        let (id, mime) = match keep(state, bot, project, &normalized) {
            Ok((id, kind)) => (Some(id), Some(kind)),
            Err(_) => (None, None),
        };
        Artifact { name, id, mime }
    }).collect()
}

pub(crate) fn load(state: &Path, bot: &str, id: &str) -> Result<Value> {
    let Some((key, ext)) = id.split_once('.') else { bail!("attachment unavailable") };
    ensure!(key.len() == 32 && key.bytes().all(|b| b.is_ascii_hexdigit()), "attachment unavailable");
    ensure!(matches!(ext, "png" | "jpg" | "jpeg" | "webp" | "gif" | "pdf" | "txt" | "md" | "csv"), "attachment unavailable");
    let path = state.canonicalize()?.join("artifacts").join(bot).join(id);
    ensure!(path.canonicalize().ok().as_ref() == Some(&path), "attachment unavailable");
    let bytes = read(&path).context("attachment unavailable")?;
    let kind = mime(ext, &bytes).context("attachment unavailable")?;
    Ok(json!({ "mime": kind, "bytes": base64::engine::general_purpose::STANDARD.encode(bytes) }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attachments_survive_removing_their_source_and_stay_with_their_bot() {
        let root = std::env::temp_dir().join(format!("raiox-artifacts-{}", std::process::id()));
        let project = root.join("project");
        let output = project.join(OUTPUT);
        let state = root.join("state");
        fs::create_dir_all(&state).unwrap();
        fs::create_dir_all(&output).unwrap();
        let png = b"\x89PNG\r\n\x1a\n";
        fs::write(output.join("example image.png"), png).unwrap();
        fs::write(output.join("example.csv"), b"label,value\nexample,1").unwrap();
        fs::write(output.join("invalid.png"), b"not an image").unwrap();
        fs::write(output.join(".env"), b"synthetic fixture").unwrap();
        fs::write(output.join("large.txt"), vec![b'x'; MAX_BYTES + 1]).unwrap();
        fs::write(project.join("outside.csv"), b"example,value").unwrap();
        let requests = [
            r"raiox-output\example image.png", "raiox-output/example.csv", "raiox-output/invalid.png",
            "raiox-output/missing.png", "raiox-output/.env", "raiox-output/../outside.csv",
            "outside.csv", "raiox-output/large.txt", "raiox-output/example.csv:stream",
        ].map(str::to_string);
        let files = collect(&state, "bot1", &project, &requests);
        assert_eq!(files[0].name, "example image.png");
        assert_eq!(files[0].mime, Some("image/png"));
        assert_eq!(files[1].mime, Some("text/plain"));
        assert!(files[2..].iter().all(|file| file.id.is_none()));
        let id = files[0].id.as_deref().unwrap();
        fs::remove_file(output.join("example image.png")).unwrap();
        let loaded = load(&state, "bot1", id).unwrap();
        assert_eq!(loaded["bytes"], base64::engine::general_purpose::STANDARD.encode(png));
        assert!(load(&state, "bot2", id).is_err());
        assert!(load(&state, "bot1", "../outside.csv").is_err());
        fs::remove_dir_all(&root).unwrap();
    }
}
