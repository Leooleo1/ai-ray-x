//! The AI coding CLIs raiox knows: whether each is installed and signed in,
//! and what raiox can do with it (read its logs for the x-ray, run it as a
//! bot). Only how someone is signed in is read, never who (no e-mail).

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use raiox_core::Store;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::settings::Settings;

/// A CLI that does not answer `--version` in this long is treated as missing.
const PROBE_TIMEOUT: Duration = Duration::from_secs(4);
/// Maximum time allowed for the "Test now" minimal prompt health check.
const TEST_TIMEOUT: Duration = Duration::from_secs(30);
const POLL: Duration = Duration::from_millis(40);
/// Disk cache of the last CLI probe, storing only version and sign-in status (never account identity).
const PROBE_CACHE_FILE: &str = "cli_probes.json";
/// Minimal prompt used by "Test now" to confirm the CLI answers end-to-end.
const TEST_PROMPT: &str = "Reply only: ok";
/// Characters kept from a failed test command's error output.
const TEST_ERROR_TAIL: usize = 240;
/// Tool-less opencode permission override for the "Test now" health check.
const OPENCODE_TEST_CONFIG: &str = r#"{"permission":{"bash":"deny","edit":"deny","read":"deny","webfetch":"deny","external_directory":"deny"}}"#;

pub struct Known {
    pub id: &'static str,
    pub name: &'static str,
    pub binary: &'static str,
    /// The install line for macOS and Linux, and for Windows (PowerShell).
    pub install: &'static str,
    pub install_windows: &'static str,
    pub login: &'static str,
    /// raiox can run it as a project bot.
    pub bots: bool,
    /// raiox reads its logs for the x-ray.
    pub xray: bool,
}

pub const KNOWN: [Known; 6] = [
    Known { id: "claude", name: "Claude Code", binary: "claude", install: "curl -fsSL https://claude.ai/install.sh | bash", install_windows: "irm https://claude.ai/install.ps1 | iex", login: "claude auth login", bots: true, xray: true },
    Known { id: "codex", name: "Codex", binary: "codex", install: "npm install -g @openai/codex", install_windows: "npm install -g @openai/codex", login: "codex login", bots: true, xray: true },
    Known { id: "cursor", name: "Cursor", binary: "cursor-agent", install: "curl https://cursor.com/install -fsS | bash", install_windows: "irm 'https://cursor.com/install?win32=true' | iex", login: "agent login", bots: true, xray: false },
    Known { id: "opencode", name: "opencode", binary: "opencode", install: "curl -fsSL https://opencode.ai/install | bash", install_windows: "npm install -g opencode-ai", login: "opencode auth login", bots: true, xray: false },
    // Google replaced Gemini CLI with Antigravity CLI (`agy`); its first run signs in with Google.
    Known { id: "antigravity", name: "Antigravity CLI", binary: "agy", install: "curl -fsSL https://antigravity.google/cli/install.sh | bash", install_windows: "irm https://antigravity.google/cli/install.ps1 | iex", login: "agy", bots: true, xray: false },
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
            // Without an account it still runs its free models: usable, sign-in unknown.
            if ok { (Some(true), None, None) } else { (None, None, None) }
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
/// Deliberately omits e-mail, username, organization or tokens.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Probe {
    pub version: Option<String>,
    pub logged_in: Option<bool>,
    pub method: Option<String>,
    pub plan: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ProbeCache {
    checked_ms: i64,
    probes: BTreeMap<String, Probe>,
}

static REFRESHING: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();

fn refreshing() -> std::sync::MutexGuard<'static, HashSet<PathBuf>> {
    REFRESHING.get_or_init(Default::default).lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Every known CLI's version and sign-in, checked all at once (some take most of a second each).
pub fn probe_all(home: &Path) -> Vec<Probe> {
    std::thread::scope(|scope| {
        let checks: Vec<_> = KNOWN
            .iter()
            .map(|k| {
                scope.spawn(move || {
                    let version = version(k.binary);
                    let (logged_in, method, plan) = if version.is_some() { sign_in(k.id, home) } else { (Some(false), None, None) };
                    Probe { version, logged_in, method, plan }
                })
            })
            .collect();
        checks.into_iter().map(|check| check.join().unwrap_or_default()).collect()
    })
}

/// Reads the last saved CLI probe from disk, aligned with `KNOWN`.
pub fn load_cached(state_dir: &Path) -> Option<Vec<Probe>> {
    let raw = fs::read_to_string(state_dir.join(PROBE_CACHE_FILE)).ok()?;
    let cache: ProbeCache = serde_json::from_str(&raw).ok()?;
    Some(KNOWN.iter().map(|k| cache.probes.get(k.id).cloned().unwrap_or_default()).collect())
}

/// Saves the latest CLI probe to disk atomically, storing only version, sign-in state, method and plan.
pub fn save_cached(state_dir: &Path, probes: &[Probe]) {
    let entries: BTreeMap<String, Probe> = KNOWN.iter().zip(probes).map(|(k, p)| (k.id.to_string(), p.clone())).collect();
    let cache = ProbeCache { checked_ms: crate::room::now_ms(), probes: entries };
    let Ok(body) = serde_json::to_string_pretty(&cache) else { return };
    if fs::create_dir_all(state_dir).is_err() {
        return;
    }
    let file = state_dir.join(PROBE_CACHE_FILE);
    let temp = file.with_extension("tmp");
    if fs::write(&temp, body + "\n").is_ok() {
        let _ = fs::rename(temp, file);
    }
}

/// Probes all known CLIs and updates the disk cache.
pub fn probe_and_cache(state_dir: &Path, home: &Path) -> Vec<Probe> {
    let probes = probe_all(home);
    save_cached(state_dir, &probes);
    probes
}

/// Triggers a deduplicated background probe that refreshes the disk cache without blocking the caller.
pub fn refresh_in_background(state_dir: PathBuf, home: PathBuf) {
    {
        let mut active = refreshing();
        if !active.insert(state_dir.clone()) {
            return;
        }
    }
    std::thread::spawn(move || {
        struct Guard(PathBuf);
        impl Drop for Guard {
            fn drop(&mut self) {
                refreshing().remove(&self.0);
            }
        }
        let _guard = Guard(state_dir.clone());
        probe_and_cache(&state_dir, &home);
    });
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
            let Probe { version, logged_in, method, plan } = probe;
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

/// Outcome of the "Test now" minimal prompt health check for one AI CLI.
#[derive(Debug, Serialize)]
pub struct TestResult {
    pub ok: bool,
    pub duration_ms: u64,
    pub reply: Option<String>,
    pub error: Option<String>,
}

fn cheapest_antigravity_model() -> Option<String> {
    let models = crate::bot_usage::antigravity_models();
    models
        .iter()
        .find(|m| m.contains("flash") && m.ends_with("-low"))
        .or_else(|| models.iter().find(|m| m.contains("flash")))
        .cloned()
}

/// Builds the tool-less, cheapest-model command and stdin payload for "Test now".
fn test_command(id: &str, home: &Path, dir: &Path) -> Result<(Command, String)> {
    if !bot_agents().any(|a| a == id) {
        bail!("raiox cannot test {id}");
    }
    match id {
        "claude" => {
            let mut cmd = crate::procs::command("claude");
            cmd.current_dir(dir).args([
                "-p",
                "--tools",
                "",
                "--strict-mcp-config",
                "--no-session-persistence",
                "--model",
                "haiku",
                "--output-format",
                "text",
            ]);
            Ok((cmd, TEST_PROMPT.to_string()))
        }
        "codex" => {
            let mut cmd = crate::procs::command("codex");
            cmd.current_dir(dir).args([
                "exec",
                "--json",
                "--skip-git-repo-check",
                "-c",
                "sandbox_mode=\"read-only\"",
                "-c",
                "model_reasoning_effort=\"low\"",
                "-",
            ]);
            Ok((cmd, TEST_PROMPT.to_string()))
        }
        "cursor" => {
            let mut cmd = crate::procs::command("cursor-agent");
            cmd.current_dir(dir)
                .args(["-p", "--output-format", "stream-json", "--mode", "ask", "--trust", "--workspace"])
                .arg(dir);
            Ok((cmd, TEST_PROMPT.to_string()))
        }
        "opencode" => {
            let mut cmd = crate::procs::command("opencode");
            cmd.current_dir(dir)
                .env("PWD", dir)
                .env("OPENCODE_CONFIG_CONTENT", OPENCODE_TEST_CONFIG)
                .args(["run", "--standalone", "--format", "json", "--agent", "plan"]);
            if sign_in("opencode", home).0 != Some(true) {
                cmd.args(["-m", crate::bot_clis::OPENCODE_FREE[0]]);
            }
            Ok((cmd, TEST_PROMPT.to_string()))
        }
        "antigravity" => {
            let mut cmd = crate::procs::command("agy");
            cmd.current_dir(dir)
                .env("PWD", dir)
                .args(["--input-format", "stream-json", "--output-format", "stream-json", "--sandbox", "--mode", "plan"]);
            if let Some(model) = cheapest_antigravity_model() {
                cmd.args(["--model", &model]);
            }
            Ok((cmd, crate::bot_clis::antigravity_input(TEST_PROMPT, false)))
        }
        _ => bail!("raiox cannot test {id}"),
    }
}

fn short_error(text: &str) -> String {
    let trimmed = text.trim();
    let start = trimmed.char_indices().rev().nth(TEST_ERROR_TAIL).map_or(0, |(i, _)| i);
    trimmed[start..].lines().last().unwrap_or(trimmed).trim().to_string()
}

fn parse_test_reply(id: &str, out: &str) -> Result<String> {
    match id {
        "claude" => {
            let trimmed = out.trim();
            if trimmed.is_empty() {
                bail!("Claude Code gave no answer");
            }
            Ok(trimmed.to_string())
        }
        "codex" => crate::bot_run::parse_codex_text(out),
        "cursor" => crate::bot_run::parse_cursor_text(out),
        "opencode" => crate::bot_clis::parse_opencode(out).map(|r| r.text),
        "antigravity" => crate::bot_clis::parse_antigravity(out).map(|r| r.text),
        _ => bail!("unknown CLI {id}"),
    }
}

/// Sends a minimal prompt ("Reply only: ok") without tools to confirm the CLI works end-to-end.
pub fn test_cli(id: &str, home: &Path) -> Result<TestResult> {
    let dir = std::env::temp_dir();
    let (mut cmd, prompt) = test_command(id, home, &dir)?;
    let started = Instant::now();
    let mut child = match cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn() {
        Ok(child) => child,
        Err(error) => {
            return Ok(TestResult {
                ok: false,
                duration_ms: started.elapsed().as_millis() as u64,
                reply: None,
                error: Some(format!("could not start {id}: {error}")),
            });
        }
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(prompt.as_bytes());
    }
    let out = loop {
        match child.try_wait().context("could not wait on CLI")? {
            Some(_) => break child.wait_with_output().context("could not read CLI output")?,
            None if started.elapsed() < TEST_TIMEOUT => std::thread::sleep(POLL),
            None => {
                let _ = child.kill();
                let _ = child.wait();
                return Ok(TestResult {
                    ok: false,
                    duration_ms: started.elapsed().as_millis() as u64,
                    reply: None,
                    error: Some(format!("timed out after {} s", TEST_TIMEOUT.as_secs())),
                });
            }
        }
    };
    let duration_ms = started.elapsed().as_millis() as u64;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    if !out.status.success() {
        let reason = if !stderr.trim().is_empty() { short_error(&stderr) } else { short_error(&stdout) };
        return Ok(TestResult {
            ok: false,
            duration_ms,
            reply: None,
            error: Some(if reason.is_empty() { format!("exit {}", out.status.code().unwrap_or(-1)) } else { reason }),
        });
    }
    match parse_test_reply(id, &stdout) {
        Ok(reply) => Ok(TestResult { ok: true, duration_ms, reply: Some(reply.chars().take(80).collect()), error: None }),
        Err(error) => Ok(TestResult { ok: false, duration_ms, reply: None, error: Some(format!("{error:#}")) }),
    }
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

    #[test]
    fn probe_cache_round_trips_only_non_personal_fields() {
        let dir = std::env::temp_dir().join(format!("raiox-probe-cache-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        assert!(load_cached(&dir).is_none());

        let mut probes = vec![Probe::default(); KNOWN.len()];
        probes[0] = Probe {
            version: Some("2.1.0".into()),
            logged_in: Some(true),
            method: Some("claudeai".into()),
            plan: Some("max".into()),
        };
        save_cached(&dir, &probes);

        let raw = fs::read_to_string(dir.join(PROBE_CACHE_FILE)).unwrap();
        assert!(!raw.contains("email") && !raw.contains("account"), "cache must not store personal account fields");
        let loaded = load_cached(&dir).unwrap();
        assert_eq!(loaded.len(), KNOWN.len());
        assert_eq!(loaded[0], probes[0]);
        assert_eq!(loaded[1], Probe::default());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_command_configures_minimal_toolless_flags() {
        let dir = std::env::temp_dir();
        let (claude, prompt) = test_command("claude", &dir, &dir).unwrap();
        let args: Vec<String> = claude.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        assert_eq!(prompt, TEST_PROMPT);
        assert!(args.contains(&"haiku".to_string()));
        assert!(args.contains(&"--strict-mcp-config".to_string()));
        assert!(args.contains(&"--no-session-persistence".to_string()));
        assert!(args.windows(2).any(|w| w[0] == "--tools" && w[1].is_empty()));

        let (codex, _) = test_command("codex", &dir, &dir).unwrap();
        let cargs: Vec<String> = codex.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        assert!(cargs.contains(&"sandbox_mode=\"read-only\"".to_string()));
        assert!(cargs.contains(&"model_reasoning_effort=\"low\"".to_string()));

        assert!(test_command("copilot", &dir, &dir).is_err());
        assert!(test_command("rm -rf", &dir, &dir).is_err());
    }
}
