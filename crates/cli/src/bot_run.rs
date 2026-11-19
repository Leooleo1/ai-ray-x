//! Runs the person's own CLI (Claude Code, Codex, Cursor, Antigravity or opencode) in a project folder and
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

use crate::bot_marks::{self, OPEN_MARK, PROPOSAL_MARK, RUN_MARK, SHOT_MARK, TO_MARK};
use crate::bot_trail::{self, Trail};

/// A bot that thinks longer than this is stopped.
const TIMEOUT: Duration = Duration::from_secs(10 * 60);
const POLL: Duration = Duration::from_millis(200);
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
    /// Messages for other projects' bots, as the bot wrote them.
    #[serde(default)]
    pub to: Vec<crate::bot_marks::Note>,
    /// Panel places the answer points to, drawn as cards with Open.
    #[serde(default)]
    pub open: Vec<String>,
    /// Commands the bot asks raiox to run, each with why raiox refuses it, if it does.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub run: Vec<crate::commands::Request>,
    /// The path the turn took: its steps with their results, and what the AI said between them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trail: Option<Trail>,
    /// A reviewer's conclusion about another bot's proposal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verdict: Option<String>,
    /// In a channel: requests for other members, passed on only when the person says so.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ask: Vec<crate::bot_marks::Note>,
    /// Screenshots the bot asked for: a path of the running app, a local URL or "simulator".
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shots: Vec<String>,
    #[serde(skip)]
    pub session: Option<String>,
    /// Model rounds or actions the CLI took, when it says (Claude Code: turns; Codex: commands and edits).
    #[serde(default)]
    pub steps: Option<u64>,
    /// Tokens the CLI read to answer (fresh input, cache reads and writes).
    pub read_tokens: u64,
    pub output_tokens: u64,
    /// What the API would charge; on a subscription it is only a yardstick.
    pub cost_usd: Option<f64>,
    pub duration_ms: u64,
}

/// Who runs a turn: the CLI, where, with which guide, continuing which session.
pub struct Target<'a> {
    pub agent: &'a str,
    pub dir: &'a Path,
    pub guide: String,
    pub session: Option<&'a str>,
    /// None keeps the CLI's own default.
    pub model: Option<&'a str>,
    pub effort: Option<&'a str>,
    /// A folder of screenshots the CLI may read (Claude Code's --add-dir).
    pub shots: Option<&'a Path>,
    /// Images handed to the CLI with the prompt (Codex's --image).
    pub images: &'a [std::path::PathBuf],
    /// The bot may read the web (its grants).
    pub web: bool,
    /// The bot may use the person's own MCP servers (its grants).
    pub mcp: bool,
}

/// What the bot is told on every turn: how raiox works, how to propose a
/// change, and the personality the person gave it (may be empty).
pub fn guide(project: &str, persona: &str, others: &[String], unwired: &[String]) -> String {
    let base = format!(
        "You are answering through raiox, a local panel where the person talks to the AI of the project \"{project}\". \
         In this turn you can only read: do not edit files or run commands that change anything. \
         When the answer needs a command's output (the git history, test results, line counts, a version) and you \
         cannot run it yourself, do not guess and do not tell the person to run it: end your answer with one line\n\
         {RUN_MARK} <command>\n\
         (at most two such lines). raiox shows it to the person, runs it in the project folder if they approve and \
         sends you the output. One program with its arguments: no pipes, redirections, `;`, `&&`, `$` or shells, and \
         nothing that changes files or prints secrets. \
         Answer in the person's language, briefly. \
         When the person asks for a change, describe in a few lines what you would change and end with exactly one line:\n\
         {PROPOSAL_MARK} <the change in one line>\n\
         raiox then shows an Approve button; only after approval are you resumed with edit permission. \
         To look at the project's app as it runs (when the person started it in raiox's See the app) or at the iOS \
         simulator, end with one line {SHOT_MARK} <a path such as /login, a local URL, or simulator> (at most two); \
         raiox takes a screenshot and hands it to you. \
         When you compare numbers, you can show them as bars: a fenced code block with the language raiox-chart \
         holding JSON like {{\"title\": \"Build steps\", \"subtitle\": \"run of 14/11\", \"unit\": \"s\", \
         \"rows\": [{{\"label\": \"Install\", \"value\": 14}}]}}, at most 12 rows; the panel draws it."
    );
    let mail = if others.is_empty() {
        String::new()
    } else {
        format!(
            "\n\nOther projects with a raiox bot: {}. If something you found matters to one of them, end with one line \
             per project:\n{TO_MARK} <project>: <a self-contained message for that project's AI>\n\
             raiox shows it as a suggestion and sends it through ai-memory only if the person approves.",
            others.join(", ")
        )
    };
    let places = format!(
        "\n\nWhen the person asks about the model or effort you run on, your look, this project's ai-memory memories or \
         which AI tools are connected, you can end with one line {OPEN_MARK} model | settings | memories | connections; \
         raiox shows a card that opens that place."
    );
    // Projects with a bot that are not wired here: named so a request to talk to one gets the way to wire it.
    let wire = if unwired.is_empty() {
        String::new()
    } else {
        format!(
            "\n\nProjects with a raiox bot that are not wired to this one: {}. If the person asks you to talk to, ask or \
             tell one of them, say that once the two projects are wired in raiox you can send it a message (with their \
             approval), and end with one line {OPEN_MARK} wires",
            unwired.join(", ")
        )
    };
    let secrets = "\n\nNever open, print or quote secrets files (.env and its variants, keys, certificates, credential \
                   stores). If the person asks for one, say raiox keeps them out of reach.";
    let persona = persona.trim();
    format!("{base}{secrets}{mail}{places}{wire}{}", if persona.is_empty() { String::new() } else { format!("\n\n{persona}") })
}

fn edit_prompt(proposal: &str) -> String {
    format!(
        "Approved by the person: {proposal}\n\
         Make this change now in the files of this folder, a working copy of the project: the person reviews the \
         differences before they reach the project. End with a short list of the files you changed."
    )
}

/// Secrets files Claude Code may never open, by the names raiox keeps out of every draft copy.
const SECRET_PATTERNS: [&str; 14] = [
    "**/.env", "**/.env.*", "**/.envrc", "**/*.env", "**/.dev.vars", "**/.dev.vars.*", "**/*.pem", "**/*.key", "**/*.p12", "**/*.pfx",
    "**/id_rsa*", "**/id_ed25519*", "**/.npmrc", "**/credentials.json",
];

/// `--disallowedTools` rules: no reading secrets while talking, and no reading or writing them while editing.
fn secret_rules(mode: Mode) -> Vec<String> {
    let tools: &[&str] = match mode {
        Mode::Talk => &["Read"],
        Mode::Edit => &["Read", "Edit", "Write"],
    };
    tools.iter().flat_map(|tool| SECRET_PATTERNS.iter().map(move |pattern| format!("{tool}({pattern})"))).collect()
}

pub fn prompt_for(mode: Mode, text: &str) -> String {
    match mode {
        Mode::Talk => text.to_string(),
        Mode::Edit => edit_prompt(text),
    }
}

/// The Claude Code command for a turn (a live session adds its streaming input).
pub fn claude_command(target: &Target, mode: Mode) -> Command {
    let base = match mode {
        Mode::Talk => "Read,Grep,Glob",
        Mode::Edit => "Read,Grep,Glob,Edit,Write",
    };
    let tools = if target.web { format!("{base},WebFetch,WebSearch") } else { base.to_string() };
    let mut command = crate::procs::command("claude");
    command
        .current_dir(target.dir)
        // Live events (one JSON per line) so the panel can show each step; the last line is the result.
        .args(["-p", "--output-format", "stream-json", "--verbose", "--include-partial-messages", "--tools", &tools]);
    // MCP servers stay out unless the person let this bot use theirs.
    if !target.mcp {
        command.arg("--strict-mcp-config");
    }
    if target.web {
        command.args(["--allowedTools", "WebFetch", "WebSearch"]);
    }
    if let Some(shots) = target.shots {
        command.arg("--add-dir").arg(shots);
    }
    command
        .args(["--append-system-prompt", &target.guide]);
    command.arg("--disallowedTools").args(secret_rules(mode));
    if let Some(model) = target.model {
        command.args(["--model", model]);
    }
    if let Some(effort) = target.effort {
        command.args(["--effort", effort]);
    }
    // A print-mode call cannot ask: a bot allowed the person's MCP servers uses them without asking
    // (its built-in tools are still the list above, and the secrets rules still deny).
    if target.mcp {
        command.args(["--permission-mode", "bypassPermissions"]);
    } else if mode == Mode::Edit {
        command.args(["--permission-mode", "acceptEdits"]);
    }
    if let Some(id) = target.session {
        command.args(["--resume", id]);
    }
    command
}

fn codex(target: &Target, mode: Mode) -> Command {
    let sandbox = match mode {
        Mode::Talk => "sandbox_mode=\"read-only\"",
        Mode::Edit => "sandbox_mode=\"workspace-write\"",
    };
    let mut command = crate::procs::command("codex");
    command.current_dir(target.dir).arg("exec");
    if let Some(id) = target.session {
        command.args(["resume", id]);
    }
    // Each image before an option, so the list ends there and the prompt still comes from stdin.
    for image in target.images {
        command.arg("--image").arg(image);
    }
    command.args(["--json", "--skip-git-repo-check", "-c", sandbox]);
    // Values are checked by the caller (letters, digits, dots and dashes), so quoting is safe.
    if let Some(model) = target.model {
        command.args(["-c", &format!("model=\"{model}\"")]);
    }
    if let Some(effort) = target.effort {
        command.args(["-c", &format!("model_reasoning_effort=\"{effort}\"")]);
    }
    command.arg("-");
    command
}

/// Cursor's agent: ask mode (read-only) for talk, resumed; an approved edit is
/// a fresh agent-mode session, because a resumed session keeps its mode.
/// Shell commands stay denied (no --force).
fn cursor(target: &Target, mode: Mode) -> Command {
    let mut command = crate::procs::command("cursor-agent");
    command.current_dir(target.dir).args(["-p", "--output-format", "stream-json", "--stream-partial-output", "--trust", "--workspace"]).arg(target.dir);
    if mode == Mode::Talk {
        command.args(["--mode", "ask"]);
    }
    // An edit resumes only its own session, when the person corrects it midway.
    if let Some(id) = target.session {
        command.args(["--resume", id]);
    }
    if let Some(model) = target.model {
        command.args(["--model", model]);
    }
    command
}

/// The CLI could not find the session to resume (its history was cleared or expired).
pub fn session_lost(error: &anyhow::Error) -> bool {
    let said = format!("{error:#}");
    said.contains("No conversation found") || (said.contains("session") && said.contains("not found"))
}

/// Whether a turn's session becomes the bot's conversation: an edit runs in a
/// throwaway session inside its draft copy, for every CLI.
pub fn keeps_session(mode: Mode) -> bool {
    mode == Mode::Talk
}

/// Runs one turn and waits for it, up to the timeout.
/// The error a turn ends with when the person pressed Stop.
pub const STOPPED: &str = "stopped by the person";

/// Runs one turn and waits for it, up to the timeout or until `stop` says so.
pub fn run(target: &Target, mode: Mode, text: &str, scratch: &Path, stop: &dyn Fn() -> bool, on_trail: &dyn Fn(&Trail)) -> Result<Reply> {
    let agent = target.agent;
    let (mut command, prompt) = match agent {
        "claude" => (claude_command(target, mode), prompt_for(mode, text)),
        // Codex has no system-prompt flag: the guide opens the first message of a thread.
        "codex" if target.session.is_none() => (codex(target, mode), format!("{}\n\n---\n\n{}", target.guide, prompt_for(mode, text))),
        "codex" => (codex(target, mode), prompt_for(mode, text)),
        // Cursor, too: the guide opens the read-only talk session; an approved edit runs in its own session.
        "cursor" if mode == Mode::Talk && target.session.is_none() => (cursor(target, mode), format!("{}\n\n---\n\n{}", target.guide, text)),
        "cursor" => (cursor(target, mode), prompt_for(mode, text)),
        // Antigravity and opencode: the guide opens a new session, as with Codex.
        "antigravity" | "opencode" => {
            let prompt = if target.session.is_none() { format!("{}\n\n---\n\n{}", target.guide, prompt_for(mode, text)) } else { prompt_for(mode, text) };
            if agent == "antigravity" {
                (crate::bot_clis::antigravity(target, mode), crate::bot_clis::antigravity_input(&prompt))
            } else {
                (crate::bot_clis::opencode(target, mode), prompt)
            }
        }
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
    let mut events = Tail::new(&out_path);
    let mut trail = Trail::default();
    let status = loop {
        if bot_trail::read(&mut trail, agent, &events.read_new(), target.dir) {
            on_trail(&trail);
        }
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if stop() {
            let _ = child.kill();
            let _ = child.wait();
            bail!(STOPPED);
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
        "codex" => parse_codex(&out),
        "antigravity" => crate::bot_clis::parse_antigravity(&out),
        "opencode" => crate::bot_clis::parse_opencode(&out),
        _ => parse_result(agent, final_result(&out)),
    };
    match parsed {
        Ok(reply) => Ok(Reply { duration_ms, trail: Some(trail.finish()).filter(|t| !t.items.is_empty()), ..reply }),
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

/// The last `result` event of a stream (the whole output when it is a single JSON).
fn final_result(out: &str) -> &str {
    out.lines()
        .rev()
        .find(|line| serde_json::from_str::<Value>(line).is_ok_and(|e| e.get("type").and_then(Value::as_str) == Some("result")))
        .unwrap_or(out)
}

/// Reads the lines a running CLI appended to its output file since last time.
pub struct Tail {
    path: std::path::PathBuf,
    offset: u64,
    partial: String,
}

impl Tail {
    fn new(path: &Path) -> Tail {
        Tail::from(path, 0)
    }

    /// From a point of the file on: a live session's output holds every turn before this one.
    pub fn from(path: &Path, offset: u64) -> Tail {
        Tail { path: path.to_path_buf(), offset, partial: String::new() }
    }

    pub fn read_new(&mut self) -> Vec<String> {
        use std::io::{Read, Seek, SeekFrom};
        let Ok(mut file) = File::open(&self.path) else { return Vec::new() };
        if file.seek(SeekFrom::Start(self.offset)).is_err() {
            return Vec::new();
        }
        let mut chunk = String::new();
        let Ok(read) = file.read_to_string(&mut chunk) else { return Vec::new() };
        self.offset += read as u64;
        self.partial.push_str(&chunk);
        let complete = self.partial.rfind('\n').map_or(0, |end| end + 1);
        let lines: Vec<String> = self.partial[..complete].lines().map(str::to_string).collect();
        self.partial = self.partial[complete..].to_string();
        lines
    }
}

/// cursor-agent joins the messages of a turn with nothing between them ("I will read it.The
/// change…"): a sentence end right before a capital, outside code, starts a new paragraph.
fn separate_messages(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len() + 8);
    let mut in_code = false;
    for (i, c) in chars.iter().enumerate() {
        if *c == '`' {
            in_code = !in_code;
        }
        out.push(*c);
        let joined = !in_code
            && matches!(c, '.' | '!' | '?' | ':')
            && i > 0
            && (chars[i - 1].is_lowercase() || matches!(chars[i - 1], '`' | ')' | '"' | '*'))
            && chars.get(i + 1).is_some_and(|next| next.is_uppercase());
        if joined {
            out.push_str("\n\n");
        }
    }
    out
}

/// A reply from the AI's text, its markers read; the CLI's numbers come after.
pub fn reply_from(text: &str) -> Reply {
    let marked = bot_marks::split(text);
    Reply {
        text: marked.text,
        proposal: marked.proposal,
        to: marked.notes,
        open: marked.open,
        run: marked.run.iter().map(|c| crate::commands::request(c)).collect(),
        verdict: marked.verdict,
        ask: marked.asks,
        shots: marked.shots,
        trail: None,
        session: None,
        steps: None,
        read_tokens: 0,
        output_tokens: 0,
        cost_usd: None,
        duration_ms: 0,
    }
}

/// Claude Code and Cursor print one JSON result; Cursor names the usage fields in camelCase.
pub fn parse_result(agent: &str, out: &str) -> Result<Reply> {
    let value: Value = serde_json::from_str(out.trim()).with_context(|| format!("{agent} did not answer in JSON"))?;
    let text = value.get("result").and_then(Value::as_str).unwrap_or("").to_string();
    if value.get("is_error").and_then(Value::as_bool).unwrap_or(false) {
        bail!("{agent}: {}", if text.is_empty() { "error without a message" } else { &text });
    }
    let usage = value.get("usage").cloned().unwrap_or(Value::Null);
    let either = |a: &str, b: &str| number(&usage, a) + number(&usage, b);
    let text = if agent == "cursor" { separate_messages(&text) } else { text };
    Ok(Reply {
        steps: value.get("num_turns").and_then(Value::as_u64),
        session: value.get("session_id").and_then(Value::as_str).map(str::to_string),
        read_tokens: either("input_tokens", "inputTokens")
            + either("cache_read_input_tokens", "cacheReadTokens")
            + either("cache_creation_input_tokens", "cacheWriteTokens"),
        output_tokens: either("output_tokens", "outputTokens"),
        cost_usd: value.get("total_cost_usd").and_then(Value::as_f64),
        ..reply_from(&text)
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
    Ok(Reply {
        steps: Some(
            events
                .iter()
                .filter(|e| e.get("type").and_then(Value::as_str) == Some("item.completed"))
                .filter(|e| matches!(e.pointer("/item/type").and_then(Value::as_str), Some("command_execution" | "file_change" | "mcp_tool_call" | "web_search")))
                .count() as u64,
        ),
        session: events.iter().find_map(|e| e.get("thread_id").and_then(Value::as_str)).map(str::to_string),
        read_tokens: usage.0,
        output_tokens: usage.1,
        ..reply_from(text)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_messages_glued_together_get_a_paragraph_each() {
        assert_eq!(separate_messages("Vou ler `a.Js`.A proposta acerta: Sim."), "Vou ler `a.Js`.\n\nA proposta acerta: Sim.");
        assert_eq!(separate_messages("Use README.md e U.S.A ok."), "Use README.md e U.S.A ok.");
    }

    #[test]
    fn the_persona_follows_the_raiox_guide() {
        let plain = guide("app", "", &[], &[]);
        let with_persona = guide("app", "Your name in this project is Reviewer. Look for bugs.", &[], &[]);
        assert!(plain.contains(PROPOSAL_MARK) && !plain.contains("Reviewer"));
        assert!(with_persona.starts_with(&plain) && with_persona.ends_with("Look for bugs."));
    }

    #[test]
    fn cursor_json_counts_its_camel_case_usage() {
        let out = r#"{"type":"result","is_error":false,"result":"ok","session_id":"c1","usage":{"inputTokens":10,"outputTokens":4,"cacheReadTokens":90,"cacheWriteTokens":0}}"#;
        let reply = parse_result("cursor", out).unwrap();
        assert_eq!((reply.session.as_deref(), reply.read_tokens, reply.output_tokens), (Some("c1"), 100, 4));
    }

    #[test]
    fn the_guide_lists_the_other_bots_only_when_there_are_some() {
        assert!(!guide("app", "", &[], &[]).contains(TO_MARK));
        assert!(guide("app", "", &["shop".into()], &[]).contains("Other projects with a raiox bot: shop."));
        // A project with a bot but no wire comes with the way to wire it.
        let unwired = guide("app", "", &[], &["shop".into()]);
        assert!(unwired.contains("not wired to this one: shop") && unwired.contains(&format!("{OPEN_MARK} wires")));
    }

    #[test]
    fn claude_json_gives_text_session_and_tokens() {
        let out = r#"{"type":"result","is_error":false,"result":"ok","session_id":"s1","total_cost_usd":0.01,
            "usage":{"input_tokens":6,"cache_creation_input_tokens":100,"cache_read_input_tokens":900,"output_tokens":20}}"#;
        let reply = parse_result("claude", out).unwrap();
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
