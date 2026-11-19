//! Seeing the app run. The person's dev command for a project (raiox suggests
//! one from package.json; it runs only when the person presses Run) starts in
//! the project folder; raiox reads the address it prints, the panel shows the
//! page, and headless Chrome takes screenshots of local pages. A dev server
//! stops with Stop, or the next time raiox opens if raiox closed first.

use std::collections::HashMap;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use base64::Engine;
use serde_json::{json, Value};

use crate::room;

/// Lines of the dev server's output the panel shows.
const LOG_LINES: usize = 40;
const SHOT_TIMEOUT: Duration = Duration::from_secs(30);
/// Time a page gets to run its scripts before the screenshot.
const SHOT_BUDGET_MS: &str = "4000";
const MAX_WIDTH: u32 = 2560;
const MAX_HEIGHT: u32 = 4000;
const PIDS: &str = "previews.json";
/// Hosts a screenshot may load: the person's own machine only.
const LOCAL_HOSTS: [&str; 4] = ["localhost", "127.0.0.1", "0.0.0.0", "[::1]"];
const CHROMES: [&str; 4] = [
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
    "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser",
    "/Applications/Chromium.app/Contents/MacOS/Chromium",
    "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
];

struct Running {
    child: Child,
    command: String,
    started_ms: i64,
}

static RUNNING: Mutex<Vec<(String, Running)>> = Mutex::new(Vec::new());

fn running() -> MutexGuard<'static, Vec<(String, Running)>> {
    RUNNING.lock().unwrap_or_else(PoisonError::into_inner)
}

fn log_file(state_dir: &Path, bot: &str) -> PathBuf {
    state_dir.join("previews").join(format!("{bot}.log"))
}

/// The last command the person ran for a project, offered again after Stop.
fn last_file(state_dir: &Path, bot: &str) -> PathBuf {
    state_dir.join("previews").join(format!("{bot}.cmd"))
}

/// The dev command a project's package.json suggests, with the package manager its lockfile names.
pub fn suggest(dir: &Path) -> Option<String> {
    let manifest: Value = serde_json::from_str(&fs::read_to_string(dir.join("package.json")).ok()?).ok()?;
    let scripts = manifest.get("scripts")?;
    let script = ["dev", "start", "serve", "preview"].into_iter().find(|name| scripts.get(*name).is_some())?;
    let runner = if dir.join("pnpm-lock.yaml").exists() {
        "pnpm"
    } else if dir.join("yarn.lock").exists() {
        "yarn"
    } else if dir.join("bun.lockb").exists() || dir.join("bun.lock").exists() {
        "bun run"
    } else {
        "npm run"
    };
    Some(format!("{runner} {script}"))
}

/// Terminal colors and cursor codes out of a dev server's output.
fn plain(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for code in chars.by_ref() {
                    if code.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

/// Whether a URL points at this machine.
fn is_local(url: &str) -> bool {
    let rest = url.strip_prefix("http://").or_else(|| url.strip_prefix("https://")).unwrap_or("");
    LOCAL_HOSTS.iter().any(|host| rest.strip_prefix(host).is_some_and(|after| after.starts_with(':') || after.starts_with('/') || after.is_empty()))
}

/// Whether a URL's path ends in a secrets file (`/.env`, `/config/.dev.vars`), which a dev server may serve.
fn opens_secret(url: &str) -> bool {
    let path = url.split(['?', '#']).next().unwrap_or("");
    path.rsplit('/').find(|part| !part.is_empty()).is_some_and(raiox_core::secrets::is_secret_file)
}

/// The first local address a dev server printed (0.0.0.0 read as localhost).
fn address(log: &str) -> Option<String> {
    log.split_whitespace()
        .map(|word| word.trim_start_matches(|c: char| !c.is_ascii_alphanumeric()).trim_end_matches(|c: char| !(c.is_ascii_alphanumeric() || c == '/')))
        .find(|word| is_local(word) && word.split("://").nth(1).is_some_and(|rest| rest.contains(':')))
        .map(|url| url.replacen("0.0.0.0", "localhost", 1))
}

/// The address of a project's app while raiox runs it.
pub fn url_of(state_dir: &Path, bot: &str) -> Option<String> {
    let mut open = running();
    let (_, run) = open.iter_mut().find(|(b, _)| b == bot)?;
    if run.child.try_wait().ok().flatten().is_some() {
        return None;
    }
    address(&plain(&fs::read_to_string(log_file(state_dir, bot)).ok()?))
}

/// Starts the person's dev command for a project. One per project at a time.
pub fn start(state_dir: &Path, bot: &str, dir: &Path, command: &str) -> Result<()> {
    let words = crate::commands::check(command).map_err(|why| anyhow::anyhow!("raiox will not run this command ({why})"))?;
    let mut open = running();
    if let Some(index) = open.iter().position(|(b, _)| b == bot) {
        if open[index].1.child.try_wait()?.is_none() {
            bail!("the app is already running; stop it first");
        }
        open.remove(index);
    }
    let log = log_file(state_dir, bot);
    fs::create_dir_all(log.parent().context("no log folder")?)?;
    let out = File::create(&log)?;
    fs::write(last_file(state_dir, bot), command)?;
    let mut process = Command::new(&words[0]);
    // BROWSER=none keeps dev servers from opening a tab of their own.
    process.args(&words[1..]).current_dir(dir).stdin(Stdio::null()).stdout(out.try_clone()?).stderr(out).env("BROWSER", "none").env("FORCE_COLOR", "0").env("PYTHONUNBUFFERED", "1");
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut process, 0);
    let child = process.spawn().with_context(|| format!("could not start {}", words[0]))?;
    remember(state_dir, bot, child.id());
    open.push((bot.to_string(), Running { child, command: command.to_string(), started_ms: room::now_ms() }));
    Ok(())
}

fn kill_group(pid: u32) {
    #[cfg(unix)]
    // SAFETY: kill(2) on the process group the dev server leads (set with process_group(0)).
    unsafe {
        libc::kill(-(pid as i32), libc::SIGTERM);
    }
}

pub fn stop(state_dir: &Path, bot: &str) -> Result<()> {
    let mut open = running();
    let index = open.iter().position(|(b, _)| b == bot).context("the app is not running")?;
    let (_, mut run) = open.remove(index);
    kill_group(run.child.id());
    let started = Instant::now();
    while run.child.try_wait()?.is_none() && started.elapsed() < Duration::from_secs(3) {
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = run.child.kill();
    let _ = run.child.wait();
    forget(state_dir, bot);
    Ok(())
}

/// What the panel shows of a project's app: the suggestion, and whether it runs, where, and its last lines.
pub fn status(state_dir: &Path, bot: &str, dir: &Path) -> Result<Value> {
    let mut open = running();
    let current = open.iter_mut().find(|(b, _)| b == bot).map(|(_, run)| run);
    let log = fs::read_to_string(log_file(state_dir, bot)).map(|text| plain(&text)).unwrap_or_default();
    let tail: Vec<&str> = log.lines().filter(|l| !l.trim().is_empty()).collect();
    let tail = tail[tail.len().saturating_sub(LOG_LINES)..].join("\n");
    let (running, command, started, code) = match current {
        Some(run) => match run.child.try_wait()? {
            None => (true, Some(run.command.clone()), Some(run.started_ms), None),
            Some(status) => (false, Some(run.command.clone()), Some(run.started_ms), status.code()),
        },
        None => (false, fs::read_to_string(last_file(state_dir, bot)).ok(), None, None),
    };
    Ok(json!({
        "suggestion": suggest(dir),
        "running": running,
        "command": command,
        "started_ms": started,
        "exit_code": code,
        "url": if running { address(&log) } else { None },
        "log": tail,
        "simulator": crate::simulator::booted(),
    }))
}

fn chrome() -> Option<&'static str> {
    CHROMES.into_iter().find(|path| Path::new(path).exists())
}

/// A screenshot of a local page, saved for the bot, as a PNG file.
pub fn shot(state_dir: &Path, bot: &str, url: &str, width: u32, height: u32) -> Result<PathBuf> {
    if !is_local(url) {
        bail!("raiox only takes screenshots of pages on this machine");
    }
    if opens_secret(url) {
        bail!("raiox takes no screenshot of a secrets file");
    }
    let (width, height) = (width.clamp(320, MAX_WIDTH), height.clamp(320, MAX_HEIGHT));
    let browser = chrome().context("no Chrome, Brave, Chromium or Edge found for screenshots")?;
    let file = shots_dir(state_dir, bot).join(format!("{:x}.png", room::now_ms()));
    fs::create_dir_all(file.parent().context("no screenshot folder")?)?;
    let mut child = Command::new(browser)
        .args(["--headless=new", "--disable-gpu", "--hide-scrollbars", "--disable-threaded-animation", "--no-first-run", "--no-default-browser-check"])
        .arg(format!("--virtual-time-budget={SHOT_BUDGET_MS}"))
        .arg(format!("--user-data-dir={}", state_dir.join("chrome-shots").display()))
        .arg(format!("--window-size={width},{height}"))
        .arg(format!("--screenshot={}", file.display()))
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("could not start the browser for the screenshot")?;
    let started = Instant::now();
    while child.try_wait()?.is_none() {
        if started.elapsed() > SHOT_TIMEOUT {
            let _ = child.kill();
            bail!("the page took too long to load for a screenshot");
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    if !file.exists() {
        bail!("the browser made no screenshot of {url}");
    }
    Ok(file)
}

/// Where a bot's screenshots are kept; its CLI may read them.
pub fn shots_dir(state_dir: &Path, bot: &str) -> PathBuf {
    state_dir.join("shots").join(bot)
}

/// The folder a bot's CLI may read: only the screenshots of the current question, so it never
/// mistakes an older one (of another app, or of a page since changed) for what it asked.
pub fn ai_dir(state_dir: &Path, bot: &str) -> PathBuf {
    shots_dir(state_dir, bot).join("now")
}

/// Empties the folder the CLI reads, at the start of a question (the folder itself stays: a live session keeps its path).
pub fn clear_ai_dir(state_dir: &Path, bot: &str) {
    let dir = ai_dir(state_dir, bot);
    let _ = fs::create_dir_all(&dir);
    for entry in fs::read_dir(&dir).into_iter().flatten().flatten() {
        let _ = fs::remove_file(entry.path());
    }
}

/// Hands a screenshot to the CLI: a link in the folder it reads (a copy when links are not possible).
pub fn hand_over(state_dir: &Path, bot: &str, file: &Path) -> Result<PathBuf> {
    let to = ai_dir(state_dir, bot).join(file.file_name().context("no file name")?);
    if fs::hard_link(file, &to).is_err() {
        fs::copy(file, &to)?;
    }
    Ok(to)
}

/// Whether a project builds an iOS app (native, React Native/Expo or Capacitor): only then is the simulator mentioned.
fn builds_ios(dir: &Path) -> bool {
    let marked = ["ios", "app.json", "capacitor.config.ts", "capacitor.config.json", "Package.swift"].iter().any(|name| dir.join(name).exists());
    marked
        || fs::read_dir(dir).into_iter().flatten().flatten().any(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.ends_with(".xcodeproj") || name.ends_with(".xcworkspace")
        })
}

/// What a question is told about the app and the simulator, when either is running.
pub fn for_cli(state_dir: &Path, bot: &str, dir: &Path) -> String {
    let mut seen = Vec::new();
    if let Some(url) = url_of(state_dir, bot) {
        seen.push(format!("the project's app is running at {url} (`{} /path` for a page)", crate::bot_marks::SHOT_MARK));
    }
    if let Some(device) = crate::simulator::booted().filter(|_| builds_ios(dir)) {
        seen.push(format!("an iOS simulator ({device}) is open (`{} simulator`)", crate::bot_marks::SHOT_MARK));
    }
    if seen.is_empty() {
        return String::new();
    }
    format!("(raiox: {}. To see it, ask for a screenshot; do not look for older ones.)\n\n", seen.join("; "))
}

/// A saved screenshot of a bot, as a data URL for the panel (only files in its own folder).
pub fn image(state_dir: &Path, bot: &str, name: &str) -> Result<Value> {
    let valid = name.ends_with(".png") && name.len() <= 40 && name.trim_end_matches(".png").bytes().all(|b| b.is_ascii_hexdigit());
    if !valid {
        bail!("no such screenshot");
    }
    let bytes = fs::read(shots_dir(state_dir, bot).join(name)).context("no such screenshot")?;
    Ok(json!({ "image": format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)) }))
}

fn pids(state_dir: &Path) -> HashMap<String, (u32, String)> {
    fs::read_to_string(state_dir.join(PIDS)).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
}

fn save_pids(state_dir: &Path, map: &HashMap<String, (u32, String)>) {
    let _ = fs::write(state_dir.join(PIDS), serde_json::to_string(map).unwrap_or_default());
}

/// When a process started, as `ps` shows it: with the pid, it tells a dev server from a later process given the same pid.
fn started_at(pid: u32) -> String {
    Command::new("ps").args(["-o", "lstart=", "-p", &pid.to_string()]).output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default()
}

fn remember(state_dir: &Path, bot: &str, pid: u32) {
    let mut map = pids(state_dir);
    map.insert(bot.to_string(), (pid, started_at(pid)));
    save_pids(state_dir, &map);
}

fn forget(state_dir: &Path, bot: &str) {
    let mut map = pids(state_dir);
    map.remove(bot);
    save_pids(state_dir, &map);
}

/// At startup: dev servers an earlier raiox left running stop, when the process is still theirs.
pub fn cleanup(state_dir: &Path) {
    for (pid, started) in pids(state_dir).into_values() {
        // Only the same process (same pid, same start time): a pid can be reused by another program.
        if !started.is_empty() && started_at(pid) == started {
            kill_group(pid);
        }
    }
    save_pids(state_dir, &HashMap::new());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dev_server_address_is_read_from_its_output() {
        let vite = "\u{1b}[32m  VITE v8.0.0\u{1b}[39m  ready in 312 ms\n\n  ➜  Local:   http://localhost:5173/\n  ➜  Network: http://192.168.0.4:5173/";
        assert_eq!(address(&plain(vite)).as_deref(), Some("http://localhost:5173/"));
        assert_eq!(address("Listening on http://0.0.0.0:3000.").as_deref(), Some("http://localhost:3000"));
        assert_eq!(address("docs at https://example.com:443/x"), None);
        assert_eq!(address("see http://localhost/ for help"), None, "no port, no dev server");
        assert_eq!(address("Serving HTTP on 127.0.0.1 port 5199 (http://127.0.0.1:5199/) ...").as_deref(), Some("http://127.0.0.1:5199/"));
    }

    #[test]
    fn screenshots_stay_on_this_machine() {
        assert!(is_local("http://localhost:5173/a") && is_local("http://127.0.0.1:8080") && is_local("http://[::1]:3000/"));
        assert!(!is_local("http://localhost.evil.com/") && !is_local("https://example.com") && !is_local("file:///etc/passwd"));
        assert!(opens_secret("http://127.0.0.1:5199/.env") && opens_secret("http://localhost:3000/config/.dev.vars?x=1"));
        assert!(!opens_secret("http://localhost:3000/") && !opens_secret("http://localhost:3000/env.ts"));
    }

    #[test]
    fn the_simulator_is_mentioned_for_ios_projects_only() {
        let dir = std::env::temp_dir().join(format!("raiox-ios-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        assert!(!builds_ios(&dir));
        fs::create_dir_all(dir.join("Loja.xcodeproj")).unwrap();
        assert!(builds_ios(&dir));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_dev_command_follows_the_lockfile() {
        let dir = std::env::temp_dir().join(format!("raiox-preview-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        assert_eq!(suggest(&dir), None);
        fs::write(dir.join("package.json"), r#"{"scripts":{"build":"vite build","dev":"vite"}}"#).unwrap();
        assert_eq!(suggest(&dir).as_deref(), Some("npm run dev"));
        fs::write(dir.join("pnpm-lock.yaml"), "").unwrap();
        assert_eq!(suggest(&dir).as_deref(), Some("pnpm dev"));
        let _ = fs::remove_dir_all(&dir);
    }
}
