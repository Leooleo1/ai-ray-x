//! Who uses this panel, for its profile corner: the account's full name and
//! the GitHub login the `gh` CLI is signed in with, both read on this machine
//! (no network, no token).

use std::path::Path;
use std::sync::OnceLock;

use serde_json::{json, Value};

static PROFILE: OnceLock<Value> = OnceLock::new();

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

pub fn get(home: &Path) -> Value {
    PROFILE
        .get_or_init(|| {
            let hosts = std::fs::read_to_string(gh_hosts(home)).unwrap_or_default();
            json!({ "name": full_name(), "github": github_login(&hosts) })
        })
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_github_login_comes_from_its_own_block_only() {
        let hosts = "github.com:\n    git_protocol: https\n    users:\n        alguem:\n    user: alguem\nexemplo.com:\n    user: outro\n";
        assert_eq!(github_login(hosts).as_deref(), Some("alguem"));
        assert_eq!(github_login("exemplo.com:\n    user: outro\n"), None);
        assert_eq!(github_login("github.com:\n    user: a b\n"), None);
    }
}
