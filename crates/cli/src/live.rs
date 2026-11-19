//! Live sessions: one Claude Code process kept open per conversation and fed
//! each message on its standard input (`--input-format stream-json`), so a
//! follow-up skips the CLI's start. Measured on Haiku: 0.8 s to a short answer
//! against 1.4 to 1.7 s with a new process, with the same tokens (the prompt
//! cache already works across processes). Only Claude Code talk turns: Cursor
//! reads one prompt per run, Codex's app server is still experimental, and
//! edits run once in a draft. A process closes after ten idle minutes, when
//! the conversation's setup changes, when the person stops it, and with the
//! panel (its input closes).

use std::collections::hash_map::DefaultHasher;
use std::fs::{self, File};
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Stdio};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

use crate::bot_run::{self, Mode, Reply, Target};
use crate::bot_trail::{self, Trail};

/// A process unused this long closes.
const IDLE: Duration = Duration::from_secs(10 * 60);
/// At most this many processes stay open; the one unused longest closes first.
const MAX_LIVE: usize = 4;
/// How often a turn looks at the output: faster than a new process would, as speed is the point.
const POLL: Duration = Duration::from_millis(50);
const TIMEOUT: Duration = Duration::from_secs(10 * 60);

struct Live {
    conversation: String,
    child: Child,
    stdin: ChildStdin,
    out: PathBuf,
    err: PathBuf,
    /// What the process was started with: the stable guide, model and effort.
    setup: u64,
    /// The CLI session the process carries.
    session: Option<String>,
    used: Instant,
}

static LIVES: Mutex<Vec<Live>> = Mutex::new(Vec::new());

fn lives() -> MutexGuard<'static, Vec<Live>> {
    LIVES.lock().unwrap_or_else(PoisonError::into_inner)
}

fn hash(parts: &[&str]) -> u64 {
    let mut hasher = DefaultHasher::new();
    parts.hash(&mut hasher);
    hasher.finish()
}

fn close(mut live: Live) {
    let _ = live.child.kill();
    let _ = live.child.wait();
}

/// Which open processes should close now: the idle ones, then the oldest past the cap.
fn to_close(used: &[Instant], now: Instant) -> Vec<usize> {
    let mut closing: Vec<usize> = (0..used.len()).filter(|&i| now.duration_since(used[i]) > IDLE).collect();
    let mut kept: Vec<usize> = (0..used.len()).filter(|i| !closing.contains(i)).collect();
    kept.sort_by_key(|&i| used[i]);
    while kept.len() > MAX_LIVE {
        closing.push(kept.remove(0));
    }
    closing
}

/// Closes idle processes and the oldest past the cap; runs on every turn and from the scheduler.
pub fn reap() {
    let mut open = lives();
    let used: Vec<Instant> = open.iter().map(|live| live.used).collect();
    let mut closing = to_close(&used, Instant::now());
    closing.sort_unstable_by(|a, b| b.cmp(a));
    for index in closing {
        close(open.remove(index));
    }
}

/// The line that hands the process a message.
fn message(text: &str) -> String {
    json!({ "type": "user", "message": { "role": "user", "content": text } }).to_string() + "\n"
}

fn spawn(conversation: &str, target: &Target, scratch: &Path, setup: u64) -> Result<Live> {
    fs::create_dir_all(scratch)?;
    let name = format!("{:x}", hash(&[conversation]));
    let (out, err) = (scratch.join(format!("live-{name}.out")), scratch.join(format!("live-{name}.err")));
    let mut command = bot_run::claude_command(target, Mode::Talk);
    command.args(["--input-format", "stream-json"]);
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(File::create(&out)?)
        .stderr(File::create(&err)?)
        .spawn()
        .context("could not start claude; is it installed and on the PATH?")?;
    let stdin = child.stdin.take().context("no input for claude")?;
    Ok(Live { conversation: conversation.to_string(), child, stdin, out, err, setup, session: target.session.map(str::to_string), used: Instant::now() })
}

/// One turn on an open process: hands it the message and reads until its result.
fn turn(live: &mut Live, target: &Target, text: &str, stop: &dyn Fn() -> bool, on_trail: &dyn Fn(&Trail)) -> Result<Reply> {
    let started = Instant::now();
    let mut events = bot_run::Tail::from(&live.out, fs::metadata(&live.out).map_or(0, |m| m.len()));
    live.stdin.write_all(message(text).as_bytes())?;
    live.stdin.flush()?;
    let mut trail = Trail::default();
    loop {
        let lines = events.read_new();
        if bot_trail::read(&mut trail, "claude", &lines, target.dir) {
            on_trail(&trail);
        }
        if let Some(result) = lines.iter().find(|line| serde_json::from_str::<Value>(line).is_ok_and(|e| e.get("type").and_then(Value::as_str) == Some("result"))) {
            let reply = bot_run::parse_result("claude", result)?;
            let duration_ms = started.elapsed().as_millis() as u64;
            return Ok(Reply { duration_ms, trail: Some(trail.finish()).filter(|t| !t.items.is_empty()), ..reply });
        }
        if live.child.try_wait()?.is_some() {
            let said = fs::read_to_string(&live.err).unwrap_or_default();
            bail!("claude failed: {}", said.lines().last().unwrap_or("it closed without answering"));
        }
        if stop() {
            bail!(bot_run::STOPPED);
        }
        if started.elapsed() > TIMEOUT {
            bail!("claude took longer than {} minutes and was stopped", TIMEOUT.as_secs() / 60);
        }
        std::thread::sleep(POLL);
    }
}

/// A talk turn on the conversation's open process, or on a new one when there is none fit for it.
/// `stable` is the part of the guide that does not change between messages.
pub fn ask(conversation: &str, stable: &str, target: &Target, text: &str, scratch: &Path, stop: &dyn Fn() -> bool, on_trail: &dyn Fn(&Trail)) -> Result<Reply> {
    reap();
    // A change of grants (web, MCP) changes the process's tools: it opens again.
    let grants = format!("{}{}", target.web, target.mcp);
    let setup = hash(&[stable, target.model.unwrap_or(""), target.effort.unwrap_or(""), &grants]);
    let previous = {
        let mut open = lives();
        open.iter().position(|live| live.conversation == conversation).map(|index| open.remove(index))
    };
    let fit = |live: &mut Live| live.setup == setup && live.session.as_deref() == target.session && live.child.try_wait().ok().flatten().is_none();
    let reusable = previous.and_then(|mut live| {
        if fit(&mut live) {
            return Some(live);
        }
        close(live);
        None
    });
    let mut live = match reusable {
        Some(live) => live,
        None => spawn(conversation, target, scratch, setup)?,
    };
    match turn(&mut live, target, text, stop, on_trail) {
        Ok(reply) => {
            live.session = reply.session.clone().or(live.session);
            live.used = Instant::now();
            lives().push(live);
            Ok(reply)
        }
        Err(error) => {
            close(live);
            Err(error)
        }
    }
}

/// Closes a conversation's process (when the conversation starts over or is closed).
pub fn end(conversation: &str) {
    let mut open = lives();
    if let Some(index) = open.iter().position(|live| live.conversation == conversation) {
        close(open.remove(index));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_processes_and_the_oldest_past_the_cap_close() {
        let now = Instant::now();
        let ago = |secs: u64| now - Duration::from_secs(secs);
        // One idle for 11 minutes; six recent ones, two too many for the cap.
        let used = [ago(660), ago(50), ago(10), ago(40), ago(30), ago(20), ago(60)];
        let mut closing = to_close(&used, now);
        closing.sort_unstable();
        assert_eq!(closing, vec![0, 1, 6]);
        assert!(to_close(&[ago(5), ago(1)], now).is_empty());
    }

    #[test]
    fn a_message_is_one_json_line() {
        let line = message("oi \"você\"\nlinha 2");
        assert!(line.ends_with('\n') && line.matches('\n').count() == 1);
        let value: Value = serde_json::from_str(line.trim()).unwrap();
        assert_eq!(value["message"]["content"], "oi \"você\"\nlinha 2");
    }
}
