//! Starting a project's app by itself, for a bot that runs without asking: the
//! project's locked dependencies are installed when they are missing, the dev
//! command See the app would run starts, and raiox waits until its page
//! answers, so the bot can look at it and tell the person it is up.

use std::net::{TcpStream, ToSocketAddrs};
use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};

use crate::preview;

/// How long a dev server gets to print its address and answer.
const READY_WAIT: Duration = Duration::from_secs(90);
const POLL: Duration = Duration::from_millis(500);
const CONNECT_TIMEOUT: Duration = Duration::from_millis(500);
/// How long installing a project's dependencies may take.
const INSTALL_WAIT: Duration = Duration::from_secs(10 * 60);
/// The end of a failed install's output kept for the bot, in characters.
const INSTALL_TAIL: usize = 2000;

/// The install a JavaScript project's lockfile asks for, when its dependencies are missing.
fn missing_install(dir: &Path) -> Option<(&'static str, &'static [&'static str])> {
    if !dir.join("package.json").exists() || dir.join("node_modules").exists() {
        return None;
    }
    Some(if dir.join("pnpm-lock.yaml").exists() {
        ("pnpm", &["install"])
    } else if dir.join("yarn.lock").exists() {
        ("yarn", &["install"])
    } else if dir.join("bun.lockb").exists() || dir.join("bun.lock").exists() {
        ("bun", &["install"])
    } else {
        ("npm", &["install"])
    })
}

/// Installs the project's dependencies when they are missing (the dev server would not find its tools).
fn install_if_missing(dir: &Path, stop: &dyn Fn() -> bool) -> Result<()> {
    let Some((program, args)) = missing_install(dir) else { return Ok(()) };
    let scratch = std::env::temp_dir().join(format!("raiox-install-{}.log", std::process::id()));
    let out = std::fs::File::create(&scratch)?;
    let mut child = crate::procs::command(program)
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(out.try_clone()?)
        .stderr(out)
        .spawn()
        .with_context(|| format!("could not run {program} install"))?;
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if stop() || started.elapsed() > INSTALL_WAIT {
            let _ = child.kill();
            let _ = std::fs::remove_file(&scratch);
            bail!("installing the dependencies ({program} install) was stopped");
        }
        std::thread::sleep(POLL);
    };
    let log = std::fs::read_to_string(&scratch).unwrap_or_default();
    let _ = std::fs::remove_file(&scratch);
    if !status.success() {
        let tail: String = log.chars().rev().take(INSTALL_TAIL).collect::<Vec<_>>().into_iter().rev().collect();
        bail!("{program} install failed:\n{tail}");
    }
    Ok(())
}

/// Whether something answers at a local URL (a dev server that is up).
pub fn answers(url: &str) -> bool {
    let rest = url.split("://").nth(1).unwrap_or(url);
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let with_port = if host.rsplit_once(':').is_some_and(|(_, port)| port.parse::<u16>().is_ok()) {
        host.to_string()
    } else if url.starts_with("https://") {
        format!("{host}:443")
    } else {
        format!("{host}:80")
    };
    let Ok(addresses) = with_port.to_socket_addrs() else { return false };
    addresses.into_iter().any(|address| TcpStream::connect_timeout(&address, CONNECT_TIMEOUT).is_ok())
}

/// The command See the app would run: the last one used for this bot, or the project's suggestion.
pub fn command_for(state_dir: &Path, bot: &str, dir: &Path) -> Option<String> {
    std::fs::read_to_string(preview::last_file(state_dir, bot)).ok().filter(|c| !c.trim().is_empty()).or_else(|| preview::suggest(dir))
}

/// Starts the project's app when it is not running and waits until its page answers; gives its URL.
pub fn start_and_wait(state_dir: &Path, bot: &str, dir: &Path, stop: &dyn Fn() -> bool) -> Result<String> {
    if !preview::is_running(bot) {
        let command = command_for(state_dir, bot, dir).context("raiox found no dev command for this project; the person can set one in See the app")?;
        install_if_missing(dir, stop)?;
        preview::start(state_dir, bot, dir, &command)?;
    }
    let started = Instant::now();
    loop {
        if let Some(url) = preview::url_of(state_dir, bot).filter(|url| answers(url)) {
            return Ok(url);
        }
        if !preview::is_running(bot) {
            bail!("the app stopped before its page answered");
        }
        if stop() {
            bail!("stopped");
        }
        if started.elapsed() > READY_WAIT {
            bail!("the app's page did not answer within {} s", READY_WAIT.as_secs());
        }
        std::thread::sleep(POLL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn missing_dependencies_are_installed_with_the_lockfile_s_manager() {
        let dir = std::env::temp_dir().join(format!("raiox-install-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(missing_install(&dir), None, "not a JavaScript project");
        std::fs::write(dir.join("package.json"), "{}").unwrap();
        assert_eq!(missing_install(&dir).map(|(p, _)| p), Some("npm"));
        std::fs::write(dir.join("pnpm-lock.yaml"), "").unwrap();
        assert_eq!(missing_install(&dir).map(|(p, _)| p), Some("pnpm"));
        std::fs::create_dir(dir.join("node_modules")).unwrap();
        assert_eq!(missing_install(&dir), None, "already installed");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_page_answers_only_while_something_listens() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(answers(&format!("http://127.0.0.1:{port}/")));
        assert!(answers(&format!("http://localhost:{port}/login?x=1")));
        drop(listener);
        // Port 1 (tcpmux) has nobody listening; a freed port could be taken by a test running beside this one.
        assert!(!answers("http://127.0.0.1:1"));
    }
}
