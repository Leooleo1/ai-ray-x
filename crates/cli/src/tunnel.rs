//! The way in from the phone: Cloudflare Tunnel (`cloudflared`) carries
//! requests from a public address to this panel, which still listens on
//! 127.0.0.1 only. Either a temporary link (trycloudflare.com: no account, a
//! new address each time) or the person's own hostname behind Cloudflare
//! Access, whose login raiox checks on every request (access.rs).

use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, MutexGuard, PoisonError};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{access, procs, room};

const CONFIG: &str = "remote.json";
const LOG: &str = "tunnel.log";
const KEY: &str = "tunnel";
/// Lines of cloudflared's output the panel shows when something goes wrong.
const LOG_LINES: usize = 12;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// A trycloudflare.com address, new each time; only paired devices get in.
    #[default]
    Quick,
    /// The person's tunnel and hostname, behind Cloudflare Access.
    Named,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct Config {
    pub on: bool,
    pub mode: Mode,
    /// The named tunnel (from `cloudflared tunnel create`).
    #[serde(default)]
    pub tunnel: String,
    #[serde(default)]
    pub hostname: String,
    /// The Access team domain and the application's audience tag.
    #[serde(default)]
    pub team: String,
    #[serde(default)]
    pub aud: String,
}

struct Running {
    child: Child,
    mode: Mode,
    host: Option<String>,
    started_ms: i64,
}

static RUNNING: Mutex<Option<Running>> = Mutex::new(None);

fn running() -> MutexGuard<'static, Option<Running>> {
    RUNNING.lock().unwrap_or_else(PoisonError::into_inner)
}

fn log_file(state_dir: &Path) -> PathBuf {
    state_dir.join(LOG)
}

pub fn load(state_dir: &Path) -> Config {
    fs::read_to_string(state_dir.join(CONFIG)).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
}

fn save(state_dir: &Path, config: &Config) -> Result<()> {
    fs::write(state_dir.join(CONFIG), serde_json::to_string_pretty(config)?)?;
    Ok(())
}

fn installed() -> bool {
    Command::new("cloudflared").arg("--version").stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success())
}

fn valid_hostname(host: &str) -> bool {
    host.contains('.') && host.len() <= 253 && host.split('.').all(|label| !label.is_empty() && label.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'))
}

fn valid_tunnel(name: &str) -> bool {
    (1..=64).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// What a named tunnel needs before it opens: all four fields, well formed.
fn check(config: &Config) -> Result<()> {
    if config.mode == Mode::Quick {
        return Ok(());
    }
    if !valid_tunnel(&config.tunnel) {
        bail!("the tunnel name takes letters, digits, - and _ (as `cloudflared tunnel create` made it)");
    }
    if !valid_hostname(&config.hostname) {
        bail!("the address is a hostname such as raiox.yourdomain.com");
    }
    if !access::valid_team(&config.team) {
        bail!("the Access team domain looks like yourteam.cloudflareaccess.com");
    }
    if !access::valid_aud(&config.aud) {
        bail!("the application audience (AUD) tag is the long hex code in the Access application");
    }
    Ok(())
}

/// The public address cloudflared printed for a temporary link.
fn quick_host(log: &str) -> Option<String> {
    log.split(|c: char| c.is_whitespace() || c == '|')
        .filter_map(|word| word.strip_prefix("https://"))
        .map(|host| host.trim_end_matches('/'))
        .find(|host| host.ends_with(".trycloudflare.com") && host.split('.').count() == 3 && valid_hostname(host))
        .map(str::to_string)
}

fn spawn(state_dir: &Path, port: u16, config: &Config) -> Result<Running> {
    let out = File::create(log_file(state_dir))?;
    let mut command = Command::new("cloudflared");
    command.args(["tunnel", "--no-autoupdate"]);
    let origin = format!("http://127.0.0.1:{port}");
    match config.mode {
        Mode::Quick => command.args(["--url", &origin]),
        Mode::Named => command.args(["run", "--url", &origin, &config.tunnel]),
    };
    command.stdin(Stdio::null()).stdout(out.try_clone()?).stderr(out);
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    let child = command.spawn().context("could not start cloudflared")?;
    procs::remember(state_dir, KEY, child.id());
    let host = (config.mode == Mode::Named).then(|| config.hostname.clone());
    Ok(Running { child, mode: config.mode, host, started_ms: room::now_ms() })
}

fn halt(state_dir: &Path, current: &mut Option<Running>) {
    if let Some(mut run) = current.take() {
        procs::kill_group(run.child.id());
        let _ = run.child.wait();
        procs::forget(state_dir, KEY);
    }
}

/// Turns the way in on (with the given settings) or off; the choice is kept for the next start.
pub fn set(state_dir: &Path, port: u16, config: Config) -> Result<()> {
    check(&config)?;
    if config.on && !installed() {
        bail!("cloudflared is not installed (macOS: brew install cloudflared)");
    }
    let mut current = running();
    halt(state_dir, &mut current);
    if config.on {
        *current = Some(spawn(state_dir, port, &config)?);
    }
    save(state_dir, &config)
}

/// At startup: the way in opens again if the person left it on.
pub fn resume(state_dir: &Path, port: u16) {
    let config = load(state_dir);
    if config.on && check(&config).is_ok() && installed() {
        match spawn(state_dir, port, &config) {
            Ok(run) => *running() = Some(run),
            Err(error) => eprintln!("the tunnel did not start: {error:#}"),
        }
    }
}

/// The public hostname requests come in by, while the tunnel runs.
pub fn public_host(state_dir: &Path) -> Option<String> {
    let mut current = running();
    let run = current.as_mut()?;
    if run.child.try_wait().ok().flatten().is_some() {
        return None;
    }
    if run.host.is_none() && run.mode == Mode::Quick {
        run.host = quick_host(&fs::read_to_string(log_file(state_dir)).unwrap_or_default());
    }
    run.host.clone()
}

/// The Access team and audience every request must carry a login for, in the named mode.
pub fn access_rule(state_dir: &Path) -> Option<(String, String)> {
    let current = running();
    let run = current.as_ref()?;
    (run.mode == Mode::Named).then(|| {
        let config = load(state_dir);
        (config.team, config.aud)
    })
}

/// What the panel shows: the settings, whether it runs, at which address, and the last lines when it stopped.
pub fn status(state_dir: &Path) -> Value {
    let config = load(state_dir);
    let host = public_host(state_dir);
    let mut current = running();
    let (alive, started, code) = match current.as_mut() {
        Some(run) => match run.child.try_wait().ok().flatten() {
            None => (true, Some(run.started_ms), None),
            Some(status) => (false, Some(run.started_ms), status.code()),
        },
        None => (false, None, None),
    };
    let log = fs::read_to_string(log_file(state_dir)).unwrap_or_default();
    let lines: Vec<&str> = log.lines().filter(|l| !l.trim().is_empty()).collect();
    json!({
        "installed": installed(),
        "config": config,
        "running": alive,
        "started_ms": started,
        "exit_code": code,
        "host": host,
        "log": if alive && host.is_some() { String::new() } else { lines[lines.len().saturating_sub(LOG_LINES)..].join("\n") },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_temporary_address_is_read_from_cloudflared() {
        let log = "2026-11-19T18:01:02Z INF |  Your quick Tunnel has been created! Visit it at:  |\n2026-11-19T18:01:02Z INF |  https://lime-quiet-river-fox.trycloudflare.com                    |";
        assert_eq!(quick_host(log).as_deref(), Some("lime-quiet-river-fox.trycloudflare.com"));
        assert_eq!(quick_host("see https://developers.cloudflare.com/x and https://a.b.trycloudflare.com"), None);
    }

    #[test]
    fn a_named_tunnel_opens_only_behind_access() {
        let mut config = Config { on: true, mode: Mode::Named, tunnel: "raiox".into(), hostname: "raiox.exemplo.com".into(), team: "equipe.cloudflareaccess.com".into(), aud: "ab".repeat(32) };
        assert!(check(&config).is_ok());
        config.aud = String::new();
        assert!(check(&config).is_err(), "no Access application, no tunnel");
        config.aud = "ab".repeat(32);
        config.hostname = "raiox.exemplo.com/../x".into();
        assert!(check(&config).is_err());
    }
}
