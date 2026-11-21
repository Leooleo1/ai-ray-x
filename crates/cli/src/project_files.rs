use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{Context, Result, bail, ensure};
use ring::digest::{SHA256, digest};
use serde_json::{Value, json};

const MAX_TEXT_BYTES: usize = 256 * 1024;
const MAX_RESULTS: usize = 100;
const MAX_VISITED: usize = 4_000;
static SAVING: Mutex<()> = Mutex::new(());

pub(crate) fn visible(name: &str) -> bool {
    !name.is_empty() && !matches!(name.to_ascii_lowercase().as_str(),
        "." | ".." | ".git" | ".ssh" | ".gnupg" | ".aws" | ".azure" | ".kube" | ".docker" | ".codex" | ".claude" | ".raiox" | "node_modules")
        && !name.contains([':', '\\', '/']) && !name.ends_with([' ', '.'])
        && !name.chars().any(char::is_control) && !raiox_core::secrets::is_secret_file(name)
}

fn resolve(root: &Path, relative: &str) -> Result<PathBuf> {
    let mut path = root.to_path_buf();
    if !relative.is_empty() {
        for part in relative.split('/') {
            ensure!(visible(part), "path unavailable");
            path.push(part);
        }
    }
    ensure!(path.canonicalize().ok().as_ref() == Some(&path), "path unavailable");
    Ok(path)
}

fn relative(root: &Path, path: &Path) -> Option<String> {
    let names = path.strip_prefix(root).ok()?.iter().map(|part| part.to_str().filter(|name| visible(name))).collect::<Option<Vec<_>>>()?;
    Some(names.join("/"))
}

fn read(path: &Path) -> Result<String> {
    let meta = fs::symlink_metadata(path)?;
    ensure!(meta.is_file() && meta.len() <= MAX_TEXT_BYTES as u64, "file is not editable text or exceeds 256 KiB");
    let mut value = String::new();
    File::open(path)?.take(MAX_TEXT_BYTES as u64 + 1).read_to_string(&mut value).context("file is not UTF-8 text")?;
    ensure!(value.len() <= MAX_TEXT_BYTES && !value.contains('\0'), "file is not editable text or exceeds 256 KiB");
    Ok(value)
}

fn version(text: &str) -> String {
    digest(&SHA256, text.as_bytes()).as_ref().iter().map(|byte| format!("{byte:02x}")).collect()
}

fn entries(root: &Path, path: &Path) -> Result<Value> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(path)?.filter_map(Result::ok) {
        let kind = entry.file_type()?;
        if !(kind.is_file() || kind.is_dir()) {
            continue;
        }
        if let Some(path) = relative(root, &entry.path()) {
            entries.push((!kind.is_dir(), entry.file_name().to_string_lossy().into_owned(), path));
        }
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.to_lowercase().cmp(&b.1.to_lowercase())).then_with(|| a.1.cmp(&b.1)));
    Ok(json!({ "entries": entries.into_iter().map(|(file, name, path)| json!({ "name": name, "path": path, "directory": !file })).collect::<Vec<_>>() }))
}

fn search(root: &Path, query: &str) -> Value {
    let needle = query.trim().to_lowercase();
    let mut matches = Vec::new();
    let mut limited = false;
    if !needle.is_empty() {
        let walker = ignore::WalkBuilder::new(root)
            .hidden(false)
            .follow_links(false)
            .filter_entry(|entry| entry.depth() == 0 || entry.file_name().to_str().is_some_and(visible))
            .build();
        for (visited, entry) in walker.enumerate() {
            if visited >= MAX_VISITED || matches.len() >= MAX_RESULTS {
                limited = true;
                break;
            }
            let Ok(entry) = entry else { continue };
            if !entry.file_type().is_some_and(|kind| kind.is_file()) {
                continue;
            }
            let Some(path) = relative(root, entry.path()) else { continue };
            let Ok(file) = resolve(root, &path) else { continue };
            if path.to_lowercase().contains(&needle) {
                matches.push(json!({ "path": path }));
            }
            if let Ok(text) = read(&file) {
                for (line, excerpt) in text.lines().enumerate().filter(|(_, text)| text.to_lowercase().contains(&needle)) {
                    if matches.len() >= MAX_RESULTS {
                        limited = true;
                        break;
                    }
                    matches.push(json!({ "path": path, "line": line + 1, "excerpt": excerpt.trim().chars().take(180).collect::<String>() }));
                }
            }
        }
    }
    json!({ "matches": matches, "limited": limited })
}

fn save(root: &Path, relative: &str, previous: &str, contents: &str) -> Result<Value> {
    ensure!(contents.len() <= MAX_TEXT_BYTES && !contents.contains('\0'), "file exceeds the editor limit");
    let _guard = SAVING.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let path = resolve(root, relative)?;
    if version(&read(&path)?) != previous {
        return Ok(json!({ "conflict": true }));
    }
    let permissions = fs::metadata(&path)?.permissions();
    ensure!(!permissions.readonly(), "file is read-only");
    let mut nonce = [0_u8; 12];
    ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut nonce).map_err(|_| anyhow::anyhow!("could not prepare file save"))?;
    let name: String = nonce.iter().map(|byte| format!("{byte:02x}")).collect();
    let temp = path.with_file_name(format!(".raiox-save-{name}"));
    let mut file = OpenOptions::new().write(true).create_new(true).open(&temp)?;
    let prepared = file.write_all(contents.as_bytes()).and_then(|()| file.set_permissions(permissions));
    drop(file);
    let result = prepared.map_err(anyhow::Error::from).and_then(|()| {
        let current = resolve(root, relative)?;
        if version(&read(&current)?) != previous {
            return Ok(false);
        }
        fs::rename(&temp, current)?;
        Ok(true)
    });
    if !matches!(result, Ok(true)) {
        let _ = fs::remove_file(&temp);
    }
    if !result? {
        return Ok(json!({ "conflict": true }));
    }
    Ok(json!({ "version": version(contents) }))
}

pub(crate) fn request(root: &Path, input: &Value) -> Result<Value> {
    let field = |name: &str| input.get(name).and_then(Value::as_str).unwrap_or("");
    let root = root.canonicalize().context("project folder unavailable")?;
    let path = field("path");
    match field("action") {
        "list" => entries(&root, &resolve(&root, path)?),
        "read" => {
            let contents = read(&resolve(&root, path)?)?;
            Ok(json!({ "version": version(&contents), "contents": contents }))
        }
        "search" => {
            let query = field("query");
            ensure!(query.len() <= 256, "search query is too long");
            Ok(search(&root, query))
        }
        "save" => save(&root, path, field("version"), input["contents"].as_str().context("missing file contents")?),
        _ => bail!("unknown project file action"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_files_search_and_save_without_overwriting_external_edits() {
        let root = std::env::temp_dir().join(format!("raiox-editor-{}", std::process::id()));
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/app.ts"), "first\r\nSearch example\r\n").unwrap();
        fs::write(root.join(".env"), "fixture").unwrap();
        let list = request(&root, &json!({"action":"list"})).unwrap();
        assert_eq!(list["entries"], json!([{"name":"src", "path":"src", "directory":true}]));
        assert!(request(&root, &json!({"action":"read", "path":".env"})).is_err());
        assert!(request(&root, &json!({"action":"read", "path":"src/../.env"})).is_err());
        let matches = request(&root, &json!({"action":"search", "query":"search example"})).unwrap();
        assert_eq!(matches["matches"][0]["line"], 2);
        assert_eq!(matches["matches"][0]["path"], "src/app.ts");
        let path = root.join("src/app.ts");
        let read = request(&root, &json!({"action":"read", "path":"src/app.ts"})).unwrap();
        let change = json!({"action":"save", "path":"src/app.ts", "version":read["version"], "contents":"changed\r\n"});
        let saved = request(&root, &change).unwrap();
        assert!(saved["version"].is_string());
        assert_eq!(fs::read_to_string(&path).unwrap(), "changed\r\n");
        assert_eq!(request(&root, &change).unwrap()["conflict"], true);
        assert_eq!(fs::read_to_string(path).unwrap(), "changed\r\n");
        fs::remove_dir_all(root).unwrap();
    }
}
