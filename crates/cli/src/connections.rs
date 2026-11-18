//! The AI coding CLIs raiox knows: whether each is installed and signed in,
//! and what raiox can do with it (read its logs for the x-ray, run it as a
//! bot). Only how someone is signed in is read, never who (no e-mail).

use std::path::Path;
use std::process::{Command, Stdio};
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
    install: &'static str,
    login: &'static str,
    /// raiox can run it as a project bot.
    pub bots: bool,
    /// raiox reads its logs for the x-ray.
    xray: bool,
}

pub const KNOWN: [Known; 6] = [
    Known { id: "claude", name: "Claude Code", binary: "claude", install: "npm install -g @anthropic-ai/claude-code", login: "claude auth login", bots: true, xray: true },
    Known { id: "codex", name: "Codex", binary: "codex", install: "npm install -g @openai/codex", login: "codex login", bots: true, xray: true },
    Known { id: "cursor", name: "Cursor", binary: "cursor-agent", install: "curl https://cursor.com/install -fsS | bash", login: "cursor-agent login", bots: true, xray: false },
    Known { id: "opencode", name: "opencode", binary: "opencode", install: "curl -fsSL https://opencode.ai/install | bash", login: "opencode auth login", bots: false, xray: false },
    Known { id: "gemini", name: "Gemini CLI", binary: "gemini", install: "npm install -g @google/gemini-cli", login: "gemini", bots: false, xray: false },
    Known { id: "copilot", name: "GitHub Copilot CLI", binary: "copilot", install: "npm install -g @github/copilot", login: "copilot", bots: false, xray: false },
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
    let mut child = Command::new(program).args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().ok()?;
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
            let providers: Value = std::fs::read_to_string(home.join(".local/share/opencode/auth.json"))
                .ok()
                .and_then(|t| serde_json::from_str(&t).ok())
                .unwrap_or(Value::Null);
            let ok = providers.as_object().is_some_and(|o| !o.is_empty());
            (Some(ok), None, None)
        }
        "gemini" => (Some(home.join(".gemini/oauth_creds.json").exists() || std::env::var_os("GEMINI_API_KEY").is_some()), None, None),
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

/// Every known CLI, probed now.
pub fn detect(store: &Store, home: &Path, settings: &Settings) -> Vec<Status> {
    KNOWN
        .iter()
        .map(|k| {
            let version = version(k.binary);
            let installed = version.is_some();
            let (logged_in, method, plan) = if installed { sign_in(k.id, home) } else { (Some(false), None, None) };
            let (sessions, requests) = if k.xray { usage(store, k.id) } else { (0, 0) };
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
                install: k.install,
                login: k.login,
                bots: k.bots,
                xray: k.xray,
                enabled: settings.is_enabled(k.id),
            }
        })
        .collect()
}
