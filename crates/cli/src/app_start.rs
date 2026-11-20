//! Starting a project's app by itself, for a bot that runs without asking: the
//! dev command See the app would run starts, and raiox waits until its page
//! answers, so the bot can look at it and tell the person it is up.

use std::net::{TcpStream, ToSocketAddrs};
use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};

use crate::preview;

/// How long a dev server gets to print its address and answer.
const READY_WAIT: Duration = Duration::from_secs(90);
const POLL: Duration = Duration::from_millis(500);
const CONNECT_TIMEOUT: Duration = Duration::from_millis(500);

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
