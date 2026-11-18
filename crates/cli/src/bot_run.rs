//! Runs the person's own CLI (Claude Code or Codex) in a project folder and
//! reads its answer. The prompt goes through stdin, never as an argument, so
//! text that starts with "-" cannot turn into a flag.
//!
//! Talk turns can only read: Claude Code gets Read, Grep and Glob and no MCP
//! servers; Codex runs in its read-only sandbox. An approved proposal resumes
//! the same session with edits allowed: Claude Code adds Edit and Write (no
//! shell), Codex switches to its workspace-write sandbox.

use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use serde::Serialize;
use serde_json::Value;

/// A bot that thinks longer than this is stopped.
const TIMEOUT: Duration = Duration::from_secs(10 * 60);
const POLL: Duration = Duration::from_millis(200);
const PROPOSAL_MARK: &str = "RAIOX-PROPOSAL:";
/// Characters of the CLI's error output kept for the chat.
const ERROR_TAIL: usize = 400;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Mode {
    Talk,
    Edit,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Reply {
    pub text: String,
    pub proposal: Option<String>,
    #[serde(skip)]
    pub session: Option<String>,
    /// Tokens the CLI read to answer (fresh input, cache reads and writes).
    pub read_tokens: u64,
    pub output_tokens: u64,
    /// What the API would charge; on a subscription it is only a yardstick.
    pub cost_usd: Option<f64>,
    pub duration_ms: u64,
}

/// What the bot is told on every turn: how raiox works and how to propose a change.
pub fn guide(project: &str) -> String {
    format!(
        "You are answering through raiox, a local panel where the person talks to the AI of the project \"{project}\". \
         In this turn you can only read: do not edit files or run commands that change anything. \
         Answer in the person's language, briefly. \
         When the person asks for a change, describe in a few lines what you would change and end with exactly one line:\n\
         {PROPOSAL_MARK} <the change in one line>\n\
         raiox then shows an Approve button; only after approval are you resumed with edit permission."
    )
}

fn edit_prompt(proposal: &str) -> String {
    format!(
        "Approved by the person: {proposal}\n\
         Make this change now, only inside this project. End with a short list of the files you changed."
    )
}

pub fn prompt_for(mode: Mode, text: &str) -> String {
    match mode {
        Mode::Talk => text.to_string(),
        Mode::Edit => edit_prompt(text),
    }
}

fn claude(dir: &Path, project: &str, session: Option<&str>, mode: Mode) -> Command {
    let tools = match mode {
        Mode::Talk => "Read,Grep,Glob",
        Mode::Edit => "Read,Grep,Glob,Edit,Write",
    };
    let mut command = Command::new("claude");
    command
        .current_dir(dir)
        .args(["-p", "--output-format", "json", "--tools", tools, "--strict-mcp-config"])
        .args(["--append-system-prompt", &guide(project)]);
    if mode == Mode::Edit {
        command.args(["--permission-mode", "acceptEdits"]);
    }
    if let Some(id) = session {
        command.args(["--resume", id]);
    }
    command
}

fn codex(dir: &Path, session: Option<&str>, mode: Mode) -> Command {
    let sandbox = match mode {
        Mode::Talk => "sandbox_mode=\"read-only\"",
        Mode::Edit => "sandbox_mode=\"workspace-write\"",
    };
    let mut command = Command::new("codex");
    command.current_dir(dir).arg("exec");
    if let Some(id) = session {
        command.args(["resume", id]);
    }
    command.args(["--json", "--skip-git-repo-check", "-c", sandbox, "-"]);
    command
}

/// Runs one turn and waits for it, up to the timeout.
pub fn run(agent: &str, dir: &Path, project: &str, session: Option<&str>, mode: Mode, text: &str, scratch: &Path) -> Result<Reply> {
    let (mut command, prompt) = match agent {
        "claude" => (claude(dir, project, session, mode), prompt_for(mode, text)),
        // Codex has no system-prompt flag: the guide opens the first message of a thread.
        "codex" if session.is_none() => (codex(dir, None, mode), format!("{}\n\n---\n\n{}", guide(project), prompt_for(mode, text))),
        "codex" => (codex(dir, session, mode), prompt_for(mode, text)),
        other => bail!("unknown agent {other}"),
    };
    fs::create_dir_all(scratch)?;
    let out_path = scratch.join("out.txt");
    let err_path = scratch.join("err.txt");
    let started = Instant::now();
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(File::create(&out_path)?)
        .stderr(File::create(&err_path)?)
        .spawn()
        .with_context(|| format!("could not start {agent}; is it installed and on the PATH?"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(prompt.as_bytes())?;
    }
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if started.elapsed() > TIMEOUT {
            let _ = child.kill();
            bail!("{agent} took longer than {} minutes and was stopped", TIMEOUT.as_secs() / 60);
        }
        std::thread::sleep(POLL);
    };
    let out = fs::read_to_string(&out_path).unwrap_or_default();
    let duration_ms = started.elapsed().as_millis() as u64;
    let parsed = match agent {
        "claude" => parse_claude(&out),
        _ => parse_codex(&out),
    };
    match parsed {
        Ok(reply) => Ok(Reply { duration_ms, ..reply }),
        Err(error) if status.success() => Err(error),
        Err(_) => Err(anyhow!("{agent} failed: {}", tail(&fs::read_to_string(&err_path).unwrap_or_default()))),
    }
}

fn tail(text: &str) -> String {
    let trimmed = text.trim();
    let start = trimmed.char_indices().rev().nth(ERROR_TAIL).map_or(0, |(i, _)| i);
    trimmed[start..].to_string()
}

fn number(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or(0)
}

pub fn parse_claude(out: &str) -> Result<Reply> {
    let value: Value = serde_json::from_str(out.trim()).context("Claude Code did not answer in JSON")?;
    let text = value.get("result").and_then(Value::as_str).unwrap_or("").to_string();
    if value.get("is_error").and_then(Value::as_bool).unwrap_or(false) {
        bail!("Claude Code: {}", if text.is_empty() { "error without a message" } else { &text });
    }
    let usage = value.get("usage").cloned().unwrap_or(Value::Null);
    let (clean, proposal) = split_proposal(&text);
    Ok(Reply {
        text: clean,
        proposal,
        session: value.get("session_id").and_then(Value::as_str).map(str::to_string),
        read_tokens: number(&usage, "input_tokens") + number(&usage, "cache_read_input_tokens") + number(&usage, "cache_creation_input_tokens"),
        output_tokens: number(&usage, "output_tokens"),
        cost_usd: value.get("total_cost_usd").and_then(Value::as_f64),
        duration_ms: 0,
    })
}

/// Codex prints one JSON event per line; the answer is the last agent message.
pub fn parse_codex(out: &str) -> Result<Reply> {
    let events: Vec<Value> = out.lines().filter_map(|line| serde_json::from_str(line).ok()).collect();
    let failure = events.iter().find_map(|e| match e.get("type").and_then(Value::as_str) {
        Some("turn.failed") => e.pointer("/error/message").and_then(Value::as_str),
        Some("error") => e.get("message").and_then(Value::as_str),
        _ => None,
    });
    let text = events
        .iter()
        .rev()
        .filter(|e| e.get("type").and_then(Value::as_str) == Some("item.completed"))
        .filter_map(|e| e.get("item"))
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("agent_message"))
        .find_map(|item| item.get("text").and_then(Value::as_str));
    let Some(text) = text else {
        bail!("Codex: {}", failure.unwrap_or("no answer"));
    };
    let usage = events
        .iter()
        .filter(|e| e.get("type").and_then(Value::as_str) == Some("turn.completed"))
        .filter_map(|e| e.get("usage"))
        .fold((0, 0), |(read, wrote), u| (read + number(u, "input_tokens"), wrote + number(u, "output_tokens")));
    let (clean, proposal) = split_proposal(text);
    Ok(Reply {
        text: clean,
        proposal,
        session: events.iter().find_map(|e| e.get("thread_id").and_then(Value::as_str)).map(str::to_string),
        read_tokens: usage.0,
        output_tokens: usage.1,
        cost_usd: None,
        duration_ms: 0,
    })
}

/// Takes the proposal line out of the answer; the panel shows it as a card.
pub fn split_proposal(text: &str) -> (String, Option<String>) {
    let proposal = text
        .lines()
        .rev()
        .find_map(|line| line.trim().strip_prefix(PROPOSAL_MARK))
        .map(|p| p.trim().trim_matches('`').trim().to_string())
        .filter(|p| !p.is_empty());
    let clean: Vec<&str> = text.lines().filter(|line| !line.trim().starts_with(PROPOSAL_MARK)).collect();
    (clean.join("\n").trim().to_string(), proposal)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_proposal_line_becomes_a_card() {
        let (text, proposal) = split_proposal("I would rename it.\n\nRAIOX-PROPOSAL: rename soma to add\n");
        assert_eq!(text, "I would rename it.");
        assert_eq!(proposal.as_deref(), Some("rename soma to add"));
    }

    #[test]
    fn claude_json_gives_text_session_and_tokens() {
        let out = r#"{"type":"result","is_error":false,"result":"ok","session_id":"s1","total_cost_usd":0.01,
            "usage":{"input_tokens":6,"cache_creation_input_tokens":100,"cache_read_input_tokens":900,"output_tokens":20}}"#;
        let reply = parse_claude(out).unwrap();
        assert_eq!((reply.text.as_str(), reply.session.as_deref(), reply.read_tokens, reply.output_tokens), ("ok", Some("s1"), 1006, 20));
    }

    #[test]
    fn codex_events_give_the_last_message_and_usage() {
        let out = concat!(
            "{\"type\":\"thread.started\",\"thread_id\":\"t1\"}\n",
            "{\"type\":\"item.completed\",\"item\":{\"type\":\"agent_message\",\"text\":\"looking\"}}\n",
            "{\"type\":\"item.completed\",\"item\":{\"type\":\"agent_message\",\"text\":\"done\"}}\n",
            "{\"type\":\"turn.completed\",\"usage\":{\"input_tokens\":500,\"cached_input_tokens\":400,\"output_tokens\":9}}\n",
        );
        let reply = parse_codex(out).unwrap();
        assert_eq!((reply.text.as_str(), reply.session.as_deref(), reply.read_tokens), ("done", Some("t1"), 500));
    }

    #[test]
    fn a_failed_codex_turn_is_an_error() {
        let out = "{\"type\":\"turn.failed\",\"error\":{\"message\":\"usage limit\"}}\n";
        assert!(parse_codex(out).unwrap_err().to_string().contains("usage limit"));
    }
}
