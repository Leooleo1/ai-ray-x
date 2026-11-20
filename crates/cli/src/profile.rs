//! Who uses this panel, for its profile corner: the account's full name and
//! the GitHub login the `gh` CLI is signed in with, both read on this machine
//! (no network, no token), and the name and photo the person chose on the
//! first run, kept next to the index. The GitHub photo is fetched only when
//! the person asks for it.

use std::path::Path;
use std::sync::OnceLock;

use anyhow::{bail, Context, Result};
use base64::Engine;
use serde_json::{json, Value};

static PROFILE: OnceLock<Value> = OnceLock::new();

/// The name the person chose, and the photo's type.
const SAVED: &str = "perfil.json";
/// The photo's bytes.
const PHOTO: &str = "perfil-foto";
const MAX_NAME: usize = 60;
/// The panel shrinks a photo to 256 px before it sends it; anything far bigger is not one.
const MAX_PHOTO_BYTES: usize = 400 * 1024;
/// Image types a photo may be, with the bytes each file starts with.
const TYPES: [(&str, &[u8]); 3] = [("image/png", b"\x89PNG"), ("image/jpeg", b"\xff\xd8\xff"), ("image/webp", b"RIFF")];
const GITHUB_WAIT_SECS: &str = "10";

/// The account's full name (macOS `id -F`), else the user name.
fn full_name() -> Option<String> {
    let named = if cfg!(target_os = "macos") {
        crate::procs::command("id").arg("-F").output().ok().filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    } else {
        None
    };
    named.filter(|n| !n.is_empty()).or_else(|| std::env::var("USER").or_else(|_| std::env::var("USERNAME")).ok()).filter(|n| !n.is_empty())
}

/// gh's hosts file: `$GH_CONFIG_DIR`, else `~/.config/gh` (`%APPDATA%\GitHub CLI` on Windows).
fn gh_hosts(home: &Path) -> std::path::PathBuf {
    let folder = std::env::var_os("GH_CONFIG_DIR").map(std::path::PathBuf::from).unwrap_or_else(|| {
        if cfg!(windows) {
            dirs::config_dir().unwrap_or_else(|| home.to_path_buf()).join("GitHub CLI")
        } else {
            home.join(".config/gh")
        }
    });
    folder.join("hosts.yml")
}

/// The login under `github.com` in gh's hosts file (the `user:` line; the token is never read out).
fn github_login(hosts: &str) -> Option<String> {
    let mut in_github = false;
    for line in hosts.lines() {
        if !line.starts_with(' ') {
            in_github = line.trim_end() == "github.com:";
        } else if in_github && let Some(user) = line.strip_prefix("    user:") {
            let user = user.trim();
            return (!user.is_empty() && user.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')).then(|| user.to_string());
        }
    }
    None
}

/// What this machine says about the person (read once).
fn found(home: &Path) -> &'static Value {
    PROFILE.get_or_init(|| {
        let hosts = std::fs::read_to_string(gh_hosts(home)).unwrap_or_default();
        json!({ "name": full_name(), "github": github_login(&hosts) })
    })
}

fn saved(state_dir: &Path) -> Value {
    std::fs::read_to_string(state_dir.join(SAVED)).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_else(|| json!({}))
}

/// The person: the name they chose (else this machine's), their GitHub login and their photo, if any.
pub fn get(home: &Path, state_dir: &Path) -> Value {
    let machine = found(home);
    let kept = saved(state_dir);
    let photo = kept["type"].as_str().and_then(|kind| {
        let bytes = std::fs::read(state_dir.join(PHOTO)).ok()?;
        Some(format!("data:{kind};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
    });
    let name = kept["name"].as_str().filter(|n| !n.is_empty()).map(str::to_string).or_else(|| machine["name"].as_str().map(str::to_string));
    json!({ "name": name, "github": machine["github"], "photo": photo })
}

/// The type of a photo's bytes, when they are one of the image types raiox keeps.
fn image_type(bytes: &[u8]) -> Option<&'static str> {
    TYPES.iter().find(|(_, start)| bytes.starts_with(start)).map(|(kind, _)| *kind)
}

fn keep_photo(state_dir: &Path, kept: &mut Value, bytes: &[u8], kind: &str) -> Result<()> {
    if bytes.len() > MAX_PHOTO_BYTES {
        bail!("the photo is too large; pick one under {} KB", MAX_PHOTO_BYTES / 1024);
    }
    if image_type(bytes) != Some(kind) {
        bail!("that file is not a PNG, JPEG or WebP image");
    }
    std::fs::write(state_dir.join(PHOTO), bytes)?;
    kept["type"] = json!(kind);
    Ok(())
}

/// Keeps the name and photo the person chose (a field left out stays as it was; a null photo removes it).
pub fn save(state_dir: &Path, input: &Value) -> Result<()> {
    let mut kept = saved(state_dir);
    if let Some(name) = input.get("name").and_then(Value::as_str) {
        let name = name.trim();
        if name.chars().count() > MAX_NAME || name.chars().any(char::is_control) {
            bail!("a name has up to {MAX_NAME} characters, on one line");
        }
        kept["name"] = json!(name);
    }
    match input.get("photo") {
        Some(Value::Null) => {
            let _ = std::fs::remove_file(state_dir.join(PHOTO));
            kept["type"] = Value::Null;
        }
        Some(Value::String(url)) => {
            let (head, data) = url.split_once(";base64,").context("send the photo as a data URL")?;
            let kind = head.strip_prefix("data:").filter(|k| TYPES.iter().any(|(t, _)| t == k)).context("that file is not a PNG, JPEG or WebP image")?;
            let bytes = base64::engine::general_purpose::STANDARD.decode(data).context("the photo did not arrive whole")?;
            keep_photo(state_dir, &mut kept, &bytes, kind)?;
        }
        _ => {}
    }
    std::fs::write(state_dir.join(SAVED), serde_json::to_string_pretty(&kept)?)?;
    Ok(())
}

/// The photo of the GitHub account `gh` is signed in with, fetched now because the person asked.
pub fn photo_from_github(home: &Path, state_dir: &Path) -> Result<()> {
    let login = found(home)["github"].as_str().context("sign in to GitHub with gh first")?.to_string();
    let url = format!("https://github.com/{login}.png?size=256");
    let out = crate::procs::command("curl").args(["-fsSL", "--max-time", GITHUB_WAIT_SECS, &url]).output().context("could not run curl")?;
    if !out.status.success() {
        bail!("GitHub did not hand over the photo");
    }
    let kind = image_type(&out.stdout).context("GitHub's answer was not an image")?;
    let mut kept = saved(state_dir);
    keep_photo(state_dir, &mut kept, &out.stdout, kind)?;
    std::fs::write(state_dir.join(SAVED), serde_json::to_string_pretty(&kept)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(test: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("raiox-profile-{test}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A 1×1 PNG.
    const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGNgYGD4DwABBAEAwS2OUAAAAABJRU5ErkJggg==";

    #[test]
    fn a_name_and_a_photo_the_person_chose_are_kept() {
        let dir = state("kept");
        let home = Path::new("/nonexistent");
        save(&dir, &json!({ "name": "  Alex Rivera  ", "photo": format!("data:image/png;base64,{PNG}") })).unwrap();
        let profile = get(home, &dir);
        assert_eq!(profile["name"], "Alex Rivera");
        assert_eq!(profile["photo"].as_str().unwrap(), format!("data:image/png;base64,{PNG}"));
        save(&dir, &json!({ "photo": null })).unwrap();
        assert!(get(home, &dir)["photo"].is_null(), "the photo can be taken away");
        assert_eq!(get(home, &dir)["name"], "Alex Rivera", "and the name stays");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn only_a_real_image_of_a_sensible_size_becomes_the_photo() {
        let dir = state("refused");
        assert!(save(&dir, &json!({ "photo": "data:image/png;base64,aGVsbG8=" })).is_err(), "bytes that are not a PNG");
        assert!(save(&dir, &json!({ "photo": format!("data:text/html;base64,{PNG}") })).is_err(), "not an image type");
        let big = base64::engine::general_purpose::STANDARD.encode(vec![0u8; MAX_PHOTO_BYTES + 1]);
        assert!(save(&dir, &json!({ "photo": format!("data:image/png;base64,{big}") })).is_err(), "too large");
        assert!(save(&dir, &json!({ "name": "x".repeat(MAX_NAME + 1) })).is_err(), "name too long");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_github_login_comes_from_its_own_block_only() {
        let hosts = "github.com:\n    git_protocol: https\n    users:\n        alguem:\n    user: alguem\nexemplo.com:\n    user: outro\n";
        assert_eq!(github_login(hosts).as_deref(), Some("alguem"));
        assert_eq!(github_login("exemplo.com:\n    user: outro\n"), None);
        assert_eq!(github_login("github.com:\n    user: a b\n"), None);
    }
}
