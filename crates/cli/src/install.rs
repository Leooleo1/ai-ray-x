//! Installing and signing in to the AI CLIs from the panel, so nobody has to
//! open a terminal for it. Codex downloads its official native binary from
//! GitHub releases (no Node.js needed) and falls back to npm if needed; other
//! CLIs run their official installers. Sign in runs in the background, captures
//! the OAuth URL and opens the default browser (falling back to a terminal if
//! no URL is printed); raiox never sees the account.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::{mpsc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use serde::Serialize;
use serde_json::{json, Value};

use crate::connections::{self, Known};
use crate::{procs, room};

/// What the panel shows of an installer's output: its last lines.
const MAX_LOG: usize = 3000;
/// Installer job id used for Python 3 (for the self-improving agent template).
pub const PYTHON_ID: &str = "python";
const PYTHON_URL: &str = "https://www.python.org/downloads/";
const PYTHON_WINDOWS: &str = "winget install -e --id Python.Python.3.12 --accept-package-agreements --accept-source-agreements";
const PYTHON_MACOS_BREW: &str = "brew install python@3.12";
/// Official GitHub releases endpoint for OpenAI Codex native binaries.
const CODEX_RELEASE_API: &str = "https://api.github.com/repos/openai/codex/releases/latest";
const CODEX_DOWNLOAD_PREFIX: &str = "https://github.com/openai/codex/releases/download/";
const CODEX_FETCH_TIMEOUT: &str = "180";
/// How long background sign-in waits for a CLI to print its OAuth URL before falling back to Terminal.
const LOGIN_URL_WAIT: Duration = Duration::from_secs(5);
/// Maximum time a background sign-in process stays alive waiting for the browser callback.
const LOGIN_MAX_WAIT: Duration = Duration::from_secs(5 * 60);
const LOGIN_POLL: Duration = Duration::from_millis(80);

#[derive(Clone, Serialize)]
struct Job {
    /// "running", "done" or "failed".
    state: &'static str,
    log: String,
    started_ms: i64,
}

static JOBS: OnceLock<Mutex<BTreeMap<String, Job>>> = OnceLock::new();
/// Active background sign-in process ids by CLI id, so a second attempt replaces the previous one.
static LOGIN_PIDS: OnceLock<Mutex<HashMap<String, u32>>> = OnceLock::new();

fn jobs() -> std::sync::MutexGuard<'static, BTreeMap<String, Job>> {
    JOBS.get_or_init(Default::default).lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn login_pids() -> std::sync::MutexGuard<'static, HashMap<String, u32>> {
    LOGIN_PIDS.get_or_init(Default::default).lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn tail(text: &str) -> String {
    let start = text.char_indices().rev().nth(MAX_LOG).map_or(0, |(at, _)| at);
    text[start..].trim().to_string()
}

/// A CLI raiox works with (for bots or for the x-ray).
fn usable(id: &str) -> Result<&'static Known> {
    connections::known(id).filter(|k| k.bots || k.xray).with_context(|| format!("raiox does not install {id}"))
}

fn installer(known: &Known) -> &'static str {
    if cfg!(windows) { known.install_windows } else { known.install }
}

fn has_program(program: &str) -> bool {
    procs::command(program).arg("--version").stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success())
}

/// Selects the automatic Python 3 install command for the given platform capabilities.
fn python_installer_for(is_windows: bool, is_macos: bool, has_brew: bool) -> Option<&'static str> {
    if is_windows {
        Some(PYTHON_WINDOWS)
    } else if is_macos && has_brew {
        Some(PYTHON_MACOS_BREW)
    } else {
        None
    }
}

/// The command that installs Python 3 automatically on this machine, if supported.
pub fn python_installer() -> Option<&'static str> {
    python_installer_for(cfg!(windows), cfg!(target_os = "macos"), cfg!(target_os = "macos") && has_program("brew"))
}

/// Whether this machine can install Python 3 automatically from the panel.
pub fn can_install_python() -> bool {
    python_installer().is_some()
}

/// Official Codex native release asset for macOS and Windows; returns `(asset_name, is_tar_gz)`.
pub fn codex_asset_name(os: &str, arch: &str) -> Option<(&'static str, bool)> {
    match (os, arch) {
        ("macos", "aarch64") => Some(("codex-aarch64-apple-darwin.tar.gz", true)),
        ("macos", "x86_64") => Some(("codex-x86_64-apple-darwin.tar.gz", true)),
        ("windows", "x86_64") => Some(("codex-x86_64-pc-windows-msvc.exe", false)),
        ("windows", "aarch64") => Some(("codex-aarch64-pc-windows-msvc.exe", false)),
        _ => None,
    }
}

#[derive(Debug, PartialEq, Eq)]
struct ReleaseAsset {
    tag: String,
    url: String,
    sha256: Option<String>,
}

/// Extracts the download URL and published SHA-256 digest (if present) for `asset_name` from a GitHub release JSON.
fn parse_codex_release(release: &Value, asset_name: &str) -> Result<ReleaseAsset> {
    let tag = release.get("tag_name").and_then(Value::as_str).unwrap_or("latest").to_string();
    let asset = release
        .get("assets")
        .and_then(Value::as_array)
        .and_then(|list| list.iter().find(|item| item.get("name").and_then(Value::as_str) == Some(asset_name)))
        .with_context(|| format!("release {tag} has no asset {asset_name}"))?;
    let url = asset.get("browser_download_url").and_then(Value::as_str).context("missing asset download URL")?;
    if !url.starts_with(CODEX_DOWNLOAD_PREFIX) {
        bail!("unexpected Codex download URL");
    }
    let sha256 = asset
        .get("digest")
        .and_then(Value::as_str)
        .and_then(|d| d.strip_prefix("sha256:"))
        .filter(|hex| hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
        .map(str::to_lowercase);
    Ok(ReleaseAsset { tag, url: url.to_string(), sha256 })
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = ring::digest::digest(&ring::digest::SHA256, bytes);
    let mut out = String::with_capacity(64);
    for byte in digest.as_ref() {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// Finds the extracted Codex executable inside a temporary extraction folder.
fn find_extracted_codex(dir: &Path) -> Option<PathBuf> {
    let entries = fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name == "codex" || name.starts_with("codex-") {
                return Some(path);
            }
        } else if path.is_dir()
            && let Some(found) = find_extracted_codex(&path)
        {
            return Some(found);
        }
    }
    None
}

/// Downloads the official native Codex binary from GitHub's latest release into `bin_dir`,
/// verifying the release's published SHA-256 digest when present.
pub fn download_codex_to(bin_dir: &Path) -> Result<String> {
    let (asset_name, is_tar_gz) = codex_asset_name(std::env::consts::OS, std::env::consts::ARCH)
        .context("no native Codex binary configured for this OS/architecture")?;
    let meta = procs::command("curl")
        .args(["-fsSL", "--max-time", "30", "-H", "Accept: application/vnd.github+json", "-H", "User-Agent: raiox", CODEX_RELEASE_API])
        .stdin(Stdio::null())
        .output()
        .context("could not run curl to inspect latest Codex release")?;
    if !meta.status.success() {
        bail!("failed to fetch Codex release metadata: {}", String::from_utf8_lossy(&meta.stderr).trim());
    }
    let release: Value = serde_json::from_slice(&meta.stdout).context("invalid GitHub release JSON")?;
    let asset = parse_codex_release(&release, asset_name)?;
    fs::create_dir_all(bin_dir)?;
    let stamp = room::now_ms();
    let download_path = bin_dir.join(format!(".codex-download-{stamp}"));
    let extract_dir = bin_dir.join(format!(".codex-extract-{stamp}"));
    let cleanup = || {
        let _ = fs::remove_file(&download_path);
        let _ = fs::remove_dir_all(&extract_dir);
    };
    let fetch = procs::command("curl")
        .args(["-fsSL", "--max-time", CODEX_FETCH_TIMEOUT, "-H", "User-Agent: raiox", "-o"])
        .arg(&download_path)
        .arg(&asset.url)
        .stdin(Stdio::null())
        .output();
    match fetch {
        Ok(out) if out.status.success() => {}
        Ok(out) => {
            cleanup();
            bail!("failed to download {asset_name}: {}", String::from_utf8_lossy(&out.stderr).trim());
        }
        Err(error) => {
            cleanup();
            bail!("could not run curl to download Codex: {error}");
        }
    }
    let bytes = fs::read(&download_path).inspect_err(|_| cleanup())?;
    let actual_sha = sha256_hex(&bytes);
    // Without the digest the release publishes, nothing proves the file is Codex's: the npm way is used instead.
    let Some(expected) = &asset.sha256 else {
        cleanup();
        bail!("the Codex release publishes no SHA-256 digest for {}", asset.url);
    };
    if actual_sha != *expected {
        cleanup();
        bail!("Codex download checksum mismatch (expected {expected}, got {actual_sha})");
    }
    let final_path = if cfg!(windows) { bin_dir.join("codex.exe") } else { bin_dir.join("codex") };
    if is_tar_gz {
        fs::create_dir_all(&extract_dir).inspect_err(|_| cleanup())?;
        let untar = procs::command("tar")
            .args(["-xzf"])
            .arg(&download_path)
            .arg("-C")
            .arg(&extract_dir)
            .stdin(Stdio::null())
            .output();
        if !untar.is_ok_and(|o| o.status.success()) {
            cleanup();
            bail!("failed to extract {asset_name}");
        }
        let Some(extracted) = find_extracted_codex(&extract_dir) else {
            cleanup();
            bail!("extracted archive contained no codex binary");
        };
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&extracted, fs::Permissions::from_mode(0o755));
        }
        fs::rename(&extracted, &final_path).inspect_err(|_| cleanup())?;
    } else {
        fs::rename(&download_path, &final_path).inspect_err(|_| cleanup())?;
    }
    cleanup();
    let check = procs::command(&final_path.to_string_lossy()).arg("--version").stdin(Stdio::null()).output();
    let version = match check {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).trim().to_string(),
        _ => {
            let _ = fs::remove_file(&final_path);
            bail!("downloaded Codex binary did not run cleanly");
        }
    };
    let verified = "SHA-256 verified";
    Ok(format!("Installed {version} ({}, {verified}) to {}\n", asset.tag, final_path.display()))
}

enum Plan {
    Shell { name: &'static str, line: &'static str },
    CodexNative { fallback: &'static str },
}

fn resolve_install(id: &str) -> Result<Plan> {
    if id == PYTHON_ID {
        let line = python_installer().with_context(|| format!("install Python 3 from {PYTHON_URL}"))?;
        return Ok(Plan::Shell { name: "Python 3", line });
    }
    let known = usable(id)?;
    let line = installer(known);
    if id == "codex" && codex_asset_name(std::env::consts::OS, std::env::consts::ARCH).is_some() {
        return Ok(Plan::CodexNative { fallback: line });
    }
    if line.starts_with("npm ") && !has_program("npm") {
        bail!("{} installs with npm, which comes with Node.js: install Node.js from https://nodejs.org first", known.name);
    }
    Ok(Plan::Shell { name: known.name, line })
}

/// The installer line, run by the system's shell.
fn shell(line: &str) -> std::process::Command {
    if cfg!(windows) {
        let mut command = procs::command("powershell.exe");
        command.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", line]);
        command
    } else {
        let mut command = procs::command("/bin/sh");
        command.args(["-c", line]);
        command
    }
}

fn run_shell_installer(line: &str) -> Result<(bool, String)> {
    let mut child = shell(line).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
    // Both streams, without one filling up while the other is read.
    let mut errors = child.stderr.take().context("no stderr")?;
    let reader = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = errors.read_to_string(&mut text);
        text
    });
    let mut out = String::new();
    child.stdout.take().context("no stdout")?.read_to_string(&mut out)?;
    let status = child.wait()?;
    Ok((status.success(), format!("{out}{}", reader.join().unwrap_or_default())))
}

/// Starts an official installer in the background (known CLI or Python 3); `state` follows it.
pub fn install(id: &str) -> Result<()> {
    let plan = resolve_install(id)?;
    let (name, initial_log) = match &plan {
        Plan::Shell { name, line } => (*name, format!("$ {line}\n")),
        Plan::CodexNative { .. } => ("Codex", "Downloading official Codex release…\n".to_string()),
    };
    {
        let mut all = jobs();
        if all.get(id).is_some_and(|job| job.state == "running") {
            bail!("{name} is already installing");
        }
        all.insert(id.to_string(), Job { state: "running", log: initial_log, started_ms: room::now_ms() });
    }
    let id = id.to_string();
    std::thread::spawn(move || {
        let outcome = match plan {
            Plan::Shell { line, .. } => run_shell_installer(line),
            Plan::CodexNative { fallback } => {
                let native = procs::managed_bin_dir()
                    .context("no home folder found")
                    .and_then(|dir| download_codex_to(&dir));
                match native {
                    Ok(log) => Ok((true, log)),
                    Err(native_error) => {
                        if fallback.starts_with("npm ") && !has_program("npm") {
                            Err(native_error)
                        } else {
                            let prefix = format!("Native download failed ({native_error:#}); trying fallback:\n$ {fallback}\n");
                            run_shell_installer(fallback).map(|(ok, text)| (ok, format!("{prefix}{text}")))
                        }
                    }
                }
            }
        };
        let mut all = jobs();
        let Some(job) = all.get_mut(&id) else { return };
        match outcome {
            Ok((ok, text)) => {
                job.state = if ok { "done" } else { "failed" };
                job.log = tail(&format!("{}{text}", job.log));
            }
            Err(error) => {
                job.state = "failed";
                job.log = tail(&format!("{}{error:#}", job.log));
            }
        }
    });
    Ok(())
}

/// Strips ANSI CSI and OSC escape sequences from terminal output.
fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            match chars.peek().copied() {
                Some('[') => {
                    chars.next();
                    for ch in chars.by_ref() {
                        if ch.is_ascii_alphabetic() {
                            break;
                        }
                    }
                }
                Some(']') => {
                    chars.next();
                    while let Some(ch) = chars.next() {
                        if ch == '\x07' || (ch == '\x1b' && chars.peek() == Some(&'\\')) {
                            if ch == '\x1b' {
                                chars.next();
                            }
                            break;
                        }
                    }
                }
                _ => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Extracts the first external OAuth/login URL printed by a CLI, skipping localhost callback URIs.
pub fn extract_login_url(text: &str) -> Option<String> {
    let clean = strip_ansi(text);
    for token in clean.split_whitespace() {
        let Some(pos) = token.find("https://").or_else(|| token.find("http://")) else { continue };
        let url = token[pos..].trim_end_matches(['.', ',', ';', ':', ')', '>', ']', '"', '\'']);
        let lower = url.to_ascii_lowercase();
        if lower.starts_with("http://localhost") || lower.starts_with("http://127.0.0.1") || lower.starts_with("http://[::1]") {
            continue;
        }
        let host = url.split_once("://").map(|(_, rest)| rest.split(['/', '?', '#']).next().unwrap_or("")).unwrap_or("");
        if host.contains('.') && !url.chars().any(char::is_control) {
            return Some(url.to_string());
        }
    }
    None
}

/// Outcome of starting a CLI sign-in flow: either the default browser opened directly, or a Terminal fallback opened.
pub struct SignInOutcome {
    pub mode: &'static str,
}

fn spawn_login_reader(stream: impl Read + Send + 'static, tx: mpsc::Sender<String>) {
    std::thread::spawn(move || {
        let mut reader = stream;
        let mut buf = [0u8; 1024];
        while let Ok(n) = reader.read(&mut buf) {
            if n == 0 {
                break;
            }
            let _ = tx.send(String::from_utf8_lossy(&buf[..n]).into_owned());
        }
    });
}

fn stop_previous_login(id: &str) {
    if let Some(pid) = login_pids().remove(id) {
        procs::kill_group(pid, true);
    }
}

fn watch_login_child(id: String, mut child: Child) {
    let pid = child.id();
    login_pids().insert(id.clone(), pid);
    std::thread::spawn(move || {
        let started = Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(_)) | Err(_) => break,
                Ok(None) if started.elapsed() >= LOGIN_MAX_WAIT => {
                    procs::kill_group(pid, true);
                    let _ = child.wait();
                    break;
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(500)),
            }
        }
        let mut map = login_pids();
        if map.get(&id) == Some(&pid) {
            map.remove(&id);
        }
    });
}

/// Runs the CLI's login command in the background, captures its OAuth URL and opens the default browser.
/// Returns `None` if no URL is printed in time (so the caller falls back to opening a Terminal window).
fn sign_in_browser(known: &Known) -> Option<String> {
    let words = shell_words::split(known.login).ok()?;
    let args = words.get(1..)?;
    stop_previous_login(known.id);
    let mut cmd = procs::command(known.binary);
    procs::own_group(&mut cmd);
    cmd.args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    // Prevent the CLI from also launching a second browser tab while still printing its login URL.
    #[cfg(unix)]
    cmd.env("BROWSER", "true");
    #[cfg(windows)]
    cmd.env("NO_BROWSER", "1");
    let mut child = cmd.spawn().ok()?;
    let (tx, rx) = mpsc::channel();
    if let Some(stdout) = child.stdout.take() {
        spawn_login_reader(stdout, tx.clone());
    }
    if let Some(stderr) = child.stderr.take() {
        spawn_login_reader(stderr, tx);
    }
    let started = Instant::now();
    let mut collected = String::new();
    while started.elapsed() < LOGIN_URL_WAIT {
        while let Ok(chunk) = rx.try_recv() {
            collected.push_str(&chunk);
            if let Some(url) = extract_login_url(&collected) {
                if open::that(&url).is_ok() {
                    watch_login_child(known.id.to_string(), child);
                    return Some(url);
                }
                procs::kill_group(child.id(), true);
                let _ = child.wait();
                return None;
            }
        }
        if child.try_wait().ok().flatten().is_some() {
            // Drain any final buffered output after process exit.
            while let Ok(chunk) = rx.try_recv() {
                collected.push_str(&chunk);
            }
            if let Some(url) = extract_login_url(&collected)
                && open::that(&url).is_ok()
            {
                return Some(url);
            }
            return None;
        }
        std::thread::sleep(LOGIN_POLL);
    }
    procs::kill_group(child.id(), true);
    let _ = child.wait();
    None
}

fn sign_in_terminal(line: &str) -> Result<()> {
    let status = if cfg!(target_os = "macos") {
        // The line is raiox's own constant, never text from a request.
        procs::command("osascript").args(["-e", "tell application \"Terminal\" to activate", "-e", &format!("tell application \"Terminal\" to do script \"{line}\"")]).status()
    } else if cfg!(windows) {
        windows_terminal(line)
    } else {
        procs::command("x-terminal-emulator").args(["-e", "sh", "-c", &format!("{line}; exec \"${{SHELL:-sh}}\"")]).status()
    };
    match status {
        Ok(done) if done.success() => Ok(()),
        _ => bail!("could not open a terminal here: run `{line}` in one"),
    }
}

/// Signs in to a CLI: captures its OAuth URL and opens the default browser on macOS/Windows,
/// falling back to a terminal window if the CLI prints no URL (or on Linux).
pub fn sign_in(id: &str) -> Result<SignInOutcome> {
    let known = usable(id)?;
    if (cfg!(target_os = "macos") || cfg!(windows)) && sign_in_browser(known).is_some() {
        return Ok(SignInOutcome { mode: "browser" });
    }
    sign_in_terminal(known.login)?;
    Ok(SignInOutcome { mode: "terminal" })
}

/// A new console window running the line. `start` takes its first quoted argument as the
/// window's title, so the title is passed as it is, quotes and all.
#[cfg(windows)]
fn windows_terminal(line: &str) -> std::io::Result<std::process::ExitStatus> {
    use std::os::windows::process::CommandExt;
    procs::command("cmd").raw_arg(format!("/c start \"raiox\" cmd /k {line}")).status()
}

#[cfg(not(windows))]
fn windows_terminal(_line: &str) -> std::io::Result<std::process::ExitStatus> {
    Err(std::io::Error::other("not Windows"))
}

/// Installs going on or finished since the panel started.
pub fn state() -> Value {
    json!(jobs().clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_known_clis_are_installed() {
        assert!(usable("claude").is_ok() && usable("opencode").is_ok());
        assert!(usable("copilot").is_err(), "a CLI raiox does not run");
        assert!(usable(PYTHON_ID).is_err(), "Python is not a CLI sign-in target");
        assert!(usable("rm -rf").is_err());
        assert_eq!(tail(&"x".repeat(MAX_LOG + 50)).len(), MAX_LOG + 1);
    }

    #[test]
    fn python_installer_matches_each_platform() {
        assert_eq!(python_installer_for(true, false, false), Some(PYTHON_WINDOWS));
        assert_eq!(python_installer_for(false, true, true), Some(PYTHON_MACOS_BREW));
        assert_eq!(python_installer_for(false, true, false), None);
        assert_eq!(python_installer_for(false, false, false), None);
        assert_eq!(python_installer_for(false, false, true), None);
    }

    #[test]
    fn codex_asset_name_covers_macos_and_windows() {
        assert_eq!(codex_asset_name("macos", "aarch64"), Some(("codex-aarch64-apple-darwin.tar.gz", true)));
        assert_eq!(codex_asset_name("macos", "x86_64"), Some(("codex-x86_64-apple-darwin.tar.gz", true)));
        assert_eq!(codex_asset_name("windows", "x86_64"), Some(("codex-x86_64-pc-windows-msvc.exe", false)));
        assert_eq!(codex_asset_name("windows", "aarch64"), Some(("codex-aarch64-pc-windows-msvc.exe", false)));
        assert_eq!(codex_asset_name("linux", "x86_64"), None);
    }

    #[test]
    fn parse_codex_release_reads_url_and_sha256_digest() {
        let digest = sha256_hex(b"hello codex");
        let payload = json!({
            "tag_name": "rust-v0.171.0",
            "assets": [{
                "name": "codex-aarch64-apple-darwin.tar.gz",
                "browser_download_url": "https://github.com/openai/codex/releases/download/rust-v0.171.0/codex-aarch64-apple-darwin.tar.gz",
                "digest": format!("sha256:{digest}")
            }]
        });
        let asset = parse_codex_release(&payload, "codex-aarch64-apple-darwin.tar.gz").unwrap();
        assert_eq!(asset.tag, "rust-v0.171.0");
        assert_eq!(asset.sha256.as_deref(), Some(digest.as_str()));

        let untrusted = json!({
            "tag_name": "rust-v0.171.0",
            "assets": [{
                "name": "codex-aarch64-apple-darwin.tar.gz",
                "browser_download_url": "https://evil.example/codex.tar.gz"
            }]
        });
        assert!(parse_codex_release(&untrusted, "codex-aarch64-apple-darwin.tar.gz").is_err());
    }

    #[test]
    fn extract_login_url_handles_ansi_and_skips_localhost() {
        let output = "Listening on http://localhost:1455/auth/callback\nOpen \x1b[36mhttps://auth.openai.com/oauth/authorize?client_id=abc&state=123\x1b[0m.\n";
        assert_eq!(
            extract_login_url(output).as_deref(),
            Some("https://auth.openai.com/oauth/authorize?client_id=abc&state=123")
        );
        assert_eq!(extract_login_url("Listening on http://127.0.0.1:8080/callback only"), None);
        assert_eq!(extract_login_url("No link printed here"), None);
    }
}
