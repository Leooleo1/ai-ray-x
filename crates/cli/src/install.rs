//! Installing and signing in to the AI CLIs from the panel, so nobody has to
//! open a terminal for it. Install runs the CLI's official installer (the same
//! line the panel shows) and keeps its output for the panel. Sign in opens a
//! terminal window with the CLI's own sign-in, which then goes on in the
//! browser as the CLI does it; raiox never sees the account.

use std::collections::BTreeMap;
use std::io::Read;
use std::process::Stdio;
use std::sync::{Mutex, OnceLock};

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

#[derive(Clone, Serialize)]
struct Job {
    /// "running", "done" or "failed".
    state: &'static str,
    log: String,
    started_ms: i64,
}

static JOBS: OnceLock<Mutex<BTreeMap<String, Job>>> = OnceLock::new();

fn jobs() -> std::sync::MutexGuard<'static, BTreeMap<String, Job>> {
    JOBS.get_or_init(Default::default).lock().unwrap_or_else(std::sync::PoisonError::into_inner)
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

fn resolve_install(id: &str) -> Result<(&'static str, &'static str)> {
    if id == PYTHON_ID {
        let line = python_installer().with_context(|| format!("install Python 3 from {PYTHON_URL}"))?;
        return Ok(("Python 3", line));
    }
    let known = usable(id)?;
    let line = installer(known);
    if line.starts_with("npm ") && !has_program("npm") {
        bail!("{} installs with npm, which comes with Node.js: install Node.js from https://nodejs.org first", known.name);
    }
    Ok((known.name, line))
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

/// Starts an official installer in the background (known CLI or Python 3); `state` follows it.
pub fn install(id: &str) -> Result<()> {
    let (name, line) = resolve_install(id)?;
    {
        let mut all = jobs();
        if all.get(id).is_some_and(|job| job.state == "running") {
            bail!("{name} is already installing");
        }
        all.insert(id.to_string(), Job { state: "running", log: format!("$ {line}\n"), started_ms: room::now_ms() });
    }
    let (id, line) = (id.to_string(), line.to_string());
    std::thread::spawn(move || {
        let outcome = shell(&line).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(anyhow::Error::from).and_then(|mut child| {
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
        });
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

/// Opens a terminal window with the CLI's own sign-in (which goes on in the browser).
pub fn sign_in(id: &str) -> Result<()> {
    let known = usable(id)?;
    let line = known.login;
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
}
