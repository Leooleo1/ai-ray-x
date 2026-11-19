//! Antigravity CLI (`agy`, Google's) and opencode (any provider, free models
//! included) as bots, next to Claude Code, Codex and Cursor.
//!
//! Talk only reads: agy runs in its plan mode (edits and commands are denied
//! without a person to approve them) and opencode as its plan agent with the
//! shell and edits denied. An approved edit runs in the draft copy: agy in
//! accept-edits, opencode as its build agent, both still without a shell.
//! Neither has a system-prompt flag, so the guide opens a new session's first
//! message, as with Codex.

use std::process::Command;

use anyhow::{bail, Result};
use serde_json::{json, Value};

use crate::bot_run::{reply_from, Mode, Reply, Target};

/// opencode's settings for a turn, merged over the person's own (never a shell or the web).
/// Other folders are denied outright: left to "ask", the run would stop with an error.
const OPENCODE_TALK: &str = r#"{"permission":{"bash":"deny","edit":"deny","webfetch":"deny","external_directory":"deny"}}"#;
const OPENCODE_EDIT: &str = r#"{"permission":{"bash":"deny","webfetch":"deny","external_directory":"deny"}}"#;
/// Models opencode serves without an account (its Zen free tier), offered when it lists none.
pub const OPENCODE_FREE: [&str; 2] = ["opencode/deepseek-v4.1-flash-free", "opencode/mimo-v2.6-flash-free"];

pub fn antigravity(target: &Target, mode: Mode) -> Command {
    let mut command = crate::procs::command("agy");
    // PWD too: the panel's own would point it at the folder raiox was opened in.
    command.current_dir(target.dir).env("PWD", target.dir).args(["--input-format", "stream-json", "--output-format", "stream-json", "--sandbox"]);
    command.args(["--mode", if mode == Mode::Talk { "plan" } else { "accept-edits" }]);
    if let Some(id) = target.session {
        command.args(["--conversation", id]);
    }
    if let Some(model) = target.model {
        command.args(["--model", model]);
    }
    if let Some(shots) = target.shots {
        command.arg("--add-dir").arg(shots);
    }
    command
}

/// Said on every agy turn: a denied command ends its turn silently, so it must not try one.
const ANTIGRAVITY_RULES: &str = "(raiox: terminal commands are denied here and would end your turn without an answer. \
     Look at the project with your file tools only: list a folder, read a file, search. Then answer.)";

/// agy reads its prompt as one stream-json message on stdin.
pub fn antigravity_input(prompt: &str) -> String {
    let content = format!("{ANTIGRAVITY_RULES}\n\n{prompt}");
    format!("{}\n", json!({ "event": "user", "message": { "role": "user", "content": content } }))
}

pub fn opencode(target: &Target, mode: Mode) -> Command {
    let mut command = crate::procs::command("opencode");
    // Its own private server: no background service to depend on.
    // PWD too: opencode takes its project folder from it, and the panel's points elsewhere.
    command.current_dir(target.dir).env("PWD", target.dir).args(["run", "--standalone", "--format", "json"]);
    command.args(["--agent", if mode == Mode::Talk { "plan" } else { "build" }]);
    command.env("OPENCODE_CONFIG_CONTENT", if mode == Mode::Talk { OPENCODE_TALK } else { OPENCODE_EDIT });
    if let Some(id) = target.session {
        command.args(["--session", id]);
    }
    if let Some(model) = target.model {
        command.args(["-m", model]);
    }
    for image in target.images {
        command.arg("--file").arg(image);
    }
    command
}

fn number(value: Option<&Value>) -> u64 {
    value.and_then(Value::as_u64).unwrap_or(0)
}

/// agy's last event is its result: the answer, the conversation, the usage.
pub fn parse_antigravity(out: &str) -> Result<Reply> {
    let result = out
        .lines()
        .rev()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find(|event| event.get("event").and_then(Value::as_str) == Some("result"))
        .and_then(|event| event.get("result").cloned());
    let Some(result) = result else { bail!("Antigravity gave no result") };
    let status = result.get("status").and_then(Value::as_str).unwrap_or("");
    let text = result.get("response").and_then(Value::as_str).unwrap_or("").trim();
    if status != "SUCCESS" {
        bail!("Antigravity: {}", result.get("error").and_then(Value::as_str).unwrap_or(status));
    }
    if text.is_empty() {
        let denied = result.get("denied_actions").and_then(Value::as_array).is_some_and(|d| !d.is_empty());
        bail!("Antigravity gave no answer{}", if denied { " (it tried an action raiox does not allow here)" } else { "" });
    }
    let usage = result.get("usage");
    let field = |name: &str| number(usage.and_then(|u| u.get(name)));
    Ok(Reply {
        session: result.get("conversation_id").and_then(Value::as_str).map(str::to_string),
        steps: result.get("num_turns").and_then(Value::as_u64),
        read_tokens: field("input_tokens") + field("cache_read_tokens"),
        output_tokens: field("output_tokens") + field("thinking_tokens"),
        ..reply_from(text)
    })
}

/// opencode prints one event per line; the answer is the text of its last message.
pub fn parse_opencode(out: &str) -> Result<Reply> {
    let events: Vec<Value> = out.lines().filter_map(|line| serde_json::from_str(line).ok()).collect();
    let kind = |event: &Value| event.get("type").and_then(Value::as_str).unwrap_or("").to_string();
    let texts: Vec<(&str, &str)> = events
        .iter()
        .filter(|e| kind(e) == "text")
        .filter_map(|e| Some((e.pointer("/part/messageID")?.as_str()?, e.pointer("/part/text")?.as_str()?)))
        .collect();
    let last = texts.last().map(|(message, _)| *message);
    let text: Vec<&str> = texts.iter().filter(|(message, _)| Some(*message) == last).map(|(_, text)| *text).collect();
    let text = text.join("\n\n");
    if text.trim().is_empty() {
        let error = events.iter().find(|e| kind(e) == "error").and_then(|e| e.pointer("/error/message").or_else(|| e.pointer("/error/data/message"))).and_then(Value::as_str);
        bail!("opencode: {}", error.unwrap_or("no answer"));
    }
    let finished: Vec<&Value> = events.iter().filter(|e| kind(e) == "step_finish").filter_map(|e| e.get("part")).collect();
    let tokens = |path: &str| finished.iter().map(|part| number(part.pointer(path))).sum::<u64>();
    let cost: f64 = finished.iter().filter_map(|part| part.get("cost").and_then(Value::as_f64)).sum();
    Ok(Reply {
        session: events.iter().find_map(|e| e.get("sessionID").and_then(Value::as_str)).map(str::to_string),
        steps: Some(events.iter().filter(|e| kind(e) == "tool_use").count() as u64),
        read_tokens: tokens("/tokens/input") + tokens("/tokens/cache/read") + tokens("/tokens/cache/write"),
        output_tokens: tokens("/tokens/output") + tokens("/tokens/reasoning"),
        cost_usd: (cost > 0.0).then_some(cost),
        ..reply_from(text.trim())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_antigravity_result_gives_the_answer_and_its_usage() {
        let out = r#"{"event":"init","conversation_id":"c1","init":{"model":"m"}}
{"event":"result","result":{"conversation_id":"c1","status":"SUCCESS","response":"Soma dois números.\n","num_turns":1,"usage":{"input_tokens":200,"output_tokens":10,"thinking_tokens":5,"cache_read_tokens":50}}}"#;
        let reply = parse_antigravity(out).unwrap();
        assert_eq!((reply.text.as_str(), reply.session.as_deref(), reply.read_tokens, reply.output_tokens), ("Soma dois números.", Some("c1"), 250, 15));
        let refused = r#"{"event":"result","result":{"status":"SUCCESS","response":"","denied_actions":[{"action":"command"}]}}"#;
        assert!(parse_antigravity(refused).unwrap_err().to_string().contains("does not allow"));
        assert!(parse_antigravity(r#"{"event":"result","result":{"status":"ERROR","error":"quota"}}"#).unwrap_err().to_string().contains("quota"));
    }

    #[test]
    fn an_opencode_answer_is_its_last_message() {
        let out = r#"{"type":"step_start","sessionID":"s1","part":{"messageID":"m1"}}
{"type":"text","sessionID":"s1","part":{"messageID":"m1","type":"text","text":"Vou ler o arquivo."}}
{"type":"tool_use","sessionID":"s1","part":{"tool":"read","state":{"status":"completed","input":{"path":"/p/a.js"},"output":"x"}}}
{"type":"step_finish","sessionID":"s1","part":{"cost":0,"tokens":{"input":100,"output":5,"reasoning":0,"cache":{"read":20,"write":0}}}}
{"type":"text","sessionID":"s1","part":{"messageID":"m2","type":"text","text":"Soma dois números."}}
{"type":"step_finish","sessionID":"s1","part":{"cost":0.002,"tokens":{"input":10,"output":4,"reasoning":1,"cache":{"read":100,"write":0}}}}"#;
        let reply = parse_opencode(out).unwrap();
        assert_eq!((reply.text.as_str(), reply.session.as_deref(), reply.steps), ("Soma dois números.", Some("s1"), Some(1)));
        assert_eq!((reply.read_tokens, reply.output_tokens, reply.cost_usd), (230, 10, Some(0.002)));
        let failed = r#"{"type":"error","sessionID":"s","error":{"type":"provider","message":"Model is unavailable."}}"#;
        assert!(parse_opencode(failed).unwrap_err().to_string().contains("unavailable"));
    }
}
