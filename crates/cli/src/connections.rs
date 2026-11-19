//! The AI coding CLIs raiox knows: whether each is installed and signed in,
//! and what raiox can do with it (read its logs for the x-ray, run it as a
//! bot). Only how someone is signed in is read, never who (no e-mail).

use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

use raiox_core::Store;
use serde::Serialize;
use serde_json::Value;

use crate::settings::Settings;

/// A CLI that does not answer `--version` in this long is treated as missing.
const PROBE_TIMEOUT: Duration = Duration::from_secs(4);
const POLL: Duration = Duration::from_millis(40);

pub struct Known {
    pub id: &'static str,
    pub name: &'static str,
    binary: &'static str,
    /// The install line for macOS and Linux, and for Windows (PowerShell).
    install: &'static str,
    install_windows: &'static str,
    login: &'static str,
    /// raiox can run it as a project bot.
    pub bots: bool,
    /// raiox reads its logs for the x-ray.
    xray: bool,
}

pub const KNOWN: [Known; 6] = [
    Known { id: "claude", name: "Claude Code", binary: "claude", install: "curl -fsSL https://claude.ai/install.sh | bash", install_windows: "irm https://claude.ai/install.ps1 | iex", login: "claude auth login", bots: true, xray: true },
    Known { id: "codex", name: "Codex", binary: "codex", install: "npm install -g @openai/codex", install_windows: "npm install -g @openai/codex", login: "codex login", bots: true, xray: true },
    Known { id: "cursor", name: "Cursor", binary: "cursor-agent", install: "curl https://cursor.com/install -fsS | bash", install_windows: "irm 'https://cursor.com/install?win32=true' | iex", login: "agent login", bots: true, xray: false },
    Known { id: "opencode", name: "opencode", binary: "opencode", install: "curl -fsSL https://opencode.ai/install | bash", install_windows: "npm install -g opencode-ai", login: "opencode auth login", bots: false, xray: false },
    // Google replaced Gemini CLI with Antigravity CLI (`agy`); its first run signs in with Google.
    Known { id: "antigravity", name: "Antigravity CLI", binary: "agy", install: "curl -fsSL https://antigravity.google/cli/install.sh | bash", install_windows: "irm https://antigravity.google/cli/install.ps1 | iex", login: "agy", bots: false, xray: false },
    Known { id: "copilot", name: "GitHub Copilot CLI", binary: "copilot", install: "npm install -g @github/copilot", install_windows: "npm install -g @github/copilot", login: "copilot login", bots: false, xray: false },
];

pub fn known(id: &str) -> Option<&'static Known> {
    KNOWN.iter().find(|k| k.id == id)
}

/// Ids raiox can run as bots.
pub fn bot_agents() -> impl Iterator<Item = &'static str> {
    KNOWN.iter().filter(|k| k.bots).map(|k| k.id)
}

#[derive(Debug, Serialize)]
pub struct Status {
    pub id: &'static str,
    pub name: &'static str,
    pub installed: bool,
    pub version: Option<String>,
    /// None when raiox cannot tell (the CLI has no status command it knows).
    pub logged_in: Option<bool>,
    pub method: Option<String>,
    pub plan: Option<String>,
    pub sessions: u64,
    pub requests: u64,
    pub install: &'static str,
    pub login: &'static str,
    pub bots: bool,
    pub xray: bool,
    /// The person chose to use it (bots and setup show only these).
    pub enabled: bool,
}

/// Runs a short command and gives up after the probe timeout.
pub fn probe(program: &str, args: &[&str]) -> Option<std::process::Output> {
    let mut child = crate::procs::command(program).args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().ok()?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return child.wait_with_output().ok(),
            Ok(None) if started.elapsed() < PROBE_TIMEOUT => std::thread::sleep(POLL),
            _ => {
                let _ = child.kill();
                return None;
            }
        }
    }
}

fn text(out: &std::process::Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
}

fn version(binary: &str) -> Option<String> {
    let out = probe(binary, &["--version"]).filter(|o| o.status.success())?;
    let line = text(&out).lines().map(str::trim).find(|l| !l.is_empty())?.to_string();
    // "codex-cli 0.170.0", "2.1.327 (Claude Code)", "opencode v2.0.12": keep the version itself.
    line.split_whitespace().find(|w| w.trim_start_matches('v').starts_with(|c: char| c.is_ascii_digit())).map(|w| w.trim_start_matches('v').to_string())
}

/// Whether opencode's database holds a sign-in (rows counted, never read; read-only).
fn opencode_signed_in(db: &Path) -> bool {
    let Ok(conn) = rusqlite::Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX) else { return false };
    ["credential", "account", "control_account"]
        .iter()
        .any(|table| conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get::<_, i64>(0)).unwrap_or(0) > 0)
}

/// (signed in, how, plan) for the CLIs raiox can ask.
fn sign_in(id: &str, home: &Path) -> (Option<bool>, Option<String>, Option<String>) {
    match id {
        "claude" => {
            let status: Value = probe("claude", &["auth", "status"]).and_then(|o| serde_json::from_slice(&o.stdout).ok()).unwrap_or(Value::Null);
            let field = |key: &str| status.get(key).and_then(Value::as_str).map(str::to_string);
            (Some(status.get("loggedIn").and_then(Value::as_bool).unwrap_or(false)), field("authMethod"), field("subscriptionType"))
        }
        "codex" => {
            let out = probe("codex", &["login", "status"]);
            let said = out.as_ref().map(text).unwrap_or_default();
            let ok = out.as_ref().is_some_and(|o| o.status.success()) && said.contains("Logged in");
            (Some(ok), ok.then(|| if said.contains("ChatGPT") { "ChatGPT" } else { "API key" }.to_string()), None)
        }
        "cursor" => {
            let ok = probe("cursor-agent", &["status"]).is_some_and(|o| text(&o).contains("Logged in"));
            (Some(ok), ok.then(|| "Cursor".to_string()), None)
        }
        "opencode" => {
            let data = home.join(".local/share/opencode");
            let providers: Value = std::fs::read_to_string(data.join("auth.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(Value::Null);
            // opencode 2 keeps its sign-ins in its database; 1.x in auth.json.
            let ok = providers.as_object().is_some_and(|o| !o.is_empty()) || opencode_signed_in(&data.join("opencode.db"));
            (Some(ok), None, None)
        }
        // No status command: listing its models answers only when signed in.
        "antigravity" => {
            let ok = probe("agy", &["models"]).is_some_and(|o| o.status.success() && text(&o).lines().any(|l| l.contains('\t')));
            (Some(ok), ok.then(|| "Google".to_string()), None)
        }
        _ => (None, None, None),
    }
}

fn usage(store: &Store, source: &str) -> (u64, u64) {
    store
        .conn()
        .query_row("SELECT COUNT(DISTINCT session), COUNT(*) FROM calls WHERE source = ?1 AND is_sub = 0", [source], |r| {
            Ok((r.get::<_, i64>(0)?.max(0) as u64, r.get::<_, i64>(1)?.max(0) as u64))
        })
        .unwrap_or((0, 0))
}

/// What running a CLI tells: its version, and whether (and how) it is signed in.
#[derive(Default)]
pub struct Probe {
    version: Option<String>,
    sign_in: (Option<bool>, Option<String>, Option<String>),
}

/// Every known CLI's version and sign-in, checked all at once (some take most of a second each).
pub fn probe_all(home: &Path) -> Vec<Probe> {
    std::thread::scope(|scope| {
        let checks: Vec<_> = KNOWN
            .iter()
            .map(|k| {
                scope.spawn(move || {
                    let version = version(k.binary);
                    let sign_in = if version.is_some() { sign_in(k.id, home) } else { (Some(false), None, None) };
                    Probe { version, sign_in }
                })
            })
            .collect();
        checks.into_iter().map(|check| check.join().unwrap_or_default()).collect()
    })
}

/// Sessions and requests of each known CLI in the index (fast; needs the store).
pub fn usage_all(store: &Store) -> Vec<(u64, u64)> {
    KNOWN.iter().map(|k| if k.xray { usage(store, k.id) } else { (0, 0) }).collect()
}

/// Every known CLI, probed now.
pub fn detect(store: &Store, home: &Path, settings: &Settings) -> Vec<Status> {
    statuses(probe_all(home), &usage_all(store), settings)
}

/// The probes, the usage and the person's choices, as the settings page shows each CLI.
pub fn statuses(probes: Vec<Probe>, usage: &[(u64, u64)], settings: &Settings) -> Vec<Status> {
    KNOWN
        .iter()
        .zip(probes)
        .zip(usage)
        .map(|((k, probe), &(sessions, requests))| {
            let Probe { version, sign_in: (logged_in, method, plan) } = probe;
            let installed = version.is_some();
            Status {
                id: k.id,
                name: k.name,
                installed,
                version,
                logged_in,
                method,
                plan,
                sessions,
                requests,
                install: if cfg!(windows) { k.install_windows } else { k.install },
                login: k.login,
                bots: k.bots,
                xray: k.xray,
                enabled: settings.is_enabled(k.id),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_opencode_sign_in_is_found_in_its_database() {
        let dir = std::env::temp_dir().join(format!("raiox-opencode-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("opencode.db");
        assert!(!opencode_signed_in(&db), "no database, no sign-in");
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch("CREATE TABLE credential (id TEXT); CREATE TABLE account (id TEXT);").unwrap();
        assert!(!opencode_signed_in(&db), "empty tables");
        conn.execute("INSERT INTO credential VALUES ('x')", []).unwrap();
        assert!(opencode_signed_in(&db));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
