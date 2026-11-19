//! Who uses this panel, for its profile corner: the account's full name and
//! the GitHub login the `gh` CLI is signed in with, both read on this machine
//! (no network, no token).

use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;

use serde_json::{json, Value};

static PROFILE: OnceLock<Value> = OnceLock::new();

/// The account's full name (macOS `id -F`), else the user name.
fn full_name() -> Option<String> {
    let named = Command::new("id").arg("-F").output().ok().filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
    named.filter(|n| !n.is_empty()).or_else(|| std::env::var("USER").ok()).filter(|n| !n.is_empty())
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
            let hosts = std::fs::read_to_string(home.join(".config/gh/hosts.yml")).unwrap_or_default();
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
