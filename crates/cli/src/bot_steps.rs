//! What a bot is doing right now, read from the CLI's live events: the file
//! it reads, the text it searches, the command it runs. Only the step's kind
//! and target are kept (a path relative to the project, a short command);
//! the panel words it in the reader's language.

use std::path::Path;

use serde::Serialize;
use serde_json::Value;

/// Targets longer than this are cut (a long command, a long pattern).
const MAX_TARGET: usize = 80;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Step {
    /// "read", "search", "list", "edit", "run", "web", "think" or "tool".
    pub kind: &'static str,
    pub target: String,
}

fn step(kind: &'static str, target: &str, dir: &Path) -> Step {
    let base = dir.to_string_lossy();
    let relative = target.strip_prefix(base.as_ref()).map_or(target, |rest| rest.trim_start_matches('/'));
    let short: String = relative.trim().chars().take(MAX_TARGET).collect();
    Step { kind, target: short }
}

fn text<'a>(value: &'a Value, keys: &[&str]) -> &'a str {
    keys.iter().find_map(|k| value.get(*k).and_then(Value::as_str)).unwrap_or("")
}

/// Codex wraps commands as `/bin/zsh -lc "…"`; the step shows what is inside.
fn unwrap_shell(command: &str) -> &str {
    let inner = command.split_once(" -lc ").map_or(command, |(_, rest)| rest);
    inner.trim().trim_matches(['"', '\''])
}

fn claude(event: &Value, dir: &Path) -> Option<Step> {
    if event.get("type").and_then(Value::as_str) != Some("assistant") {
        return None;
    }
    let blocks = event.pointer("/message/content")?.as_array()?;
    blocks.iter().rev().find_map(|block| {
        if block.get("type").and_then(Value::as_str) != Some("tool_use") {
            return None;
        }
        let input = block.get("input").cloned().unwrap_or(Value::Null);
        let name = block.get("name").and_then(Value::as_str).unwrap_or("");
        Some(match name {
            "Read" => step("read", text(&input, &["file_path"]), dir),
            "Grep" => step("search", text(&input, &["pattern"]), dir),
            "Glob" => step("list", text(&input, &["pattern"]), dir),
            "Edit" | "Write" | "MultiEdit" | "NotebookEdit" => step("edit", text(&input, &["file_path", "notebook_path"]), dir),
            "Bash" => step("run", text(&input, &["command"]), dir),
            "WebFetch" | "WebSearch" => step("web", text(&input, &["url", "query"]), dir),
            other => step("tool", other, dir),
        })
    })
}

fn codex(event: &Value, dir: &Path) -> Option<Step> {
    if event.get("type").and_then(Value::as_str) != Some("item.started") {
        return None;
    }
    let item = event.get("item")?;
    Some(match item.get("type").and_then(Value::as_str)? {
        "command_execution" => step("run", unwrap_shell(text(item, &["command"])), dir),
        "file_change" => {
            let path = item.pointer("/changes/0/path").and_then(Value::as_str).unwrap_or("");
            step("edit", path, dir)
        }
        "web_search" => step("web", text(item, &["query"]), dir),
        "reasoning" => step("think", "", dir),
        "mcp_tool_call" => step("tool", text(item, &["tool"]), dir),
        _ => return None,
    })
}

fn cursor(event: &Value, dir: &Path) -> Option<Step> {
    if event.get("type").and_then(Value::as_str) != Some("tool_call") || event.get("subtype").and_then(Value::as_str) != Some("started") {
        return None;
    }
    let call = event.get("tool_call")?.as_object()?;
    let (name, body) = call.iter().find(|(name, _)| name.ends_with("ToolCall"))?;
    let args = body.get("args").cloned().unwrap_or(Value::Null);
    Some(match name.as_str() {
        "readToolCall" => step("read", text(&args, &["path"]), dir),
        "grepToolCall" | "semSearchToolCall" => step("search", text(&args, &["pattern", "query"]), dir),
        "globToolCall" | "lsToolCall" => step("list", text(&args, &["globPattern", "path"]), dir),
        "editToolCall" | "deleteToolCall" | "applyAgentDiffToolCall" => step("edit", text(&args, &["path"]), dir),
        "shellToolCall" => step("run", text(&args, &["command"]), dir),
        "webSearchToolCall" | "webFetchToolCall" => step("web", text(&args, &["searchTerm", "query", "url"]), dir),
        other => step("tool", other.trim_end_matches("ToolCall"), dir),
    })
}

/// The step one event line announces, if any.
pub fn from_line(agent: &str, line: &str, dir: &Path) -> Option<Step> {
    let event: Value = serde_json::from_str(line).ok()?;
    match agent {
        "claude" => claude(&event, dir),
        "codex" => codex(&event, dir),
        "cursor" => cursor(&event, dir),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_tool_use_becomes_a_relative_read() {
        let line = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Read","input":{"file_path":"/p/src/app.ts"}}]}}"#;
        assert_eq!(from_line("claude", line, Path::new("/p")), Some(Step { kind: "read", target: "src/app.ts".into() }));
    }

    #[test]
    fn codex_commands_lose_their_shell_wrapper() {
        let line = r#"{"type":"item.started","item":{"type":"command_execution","command":"/bin/zsh -lc \"npm test\""}}"#;
        assert_eq!(from_line("codex", line, Path::new("/p")), Some(Step { kind: "run", target: "npm test".into() }));
    }

    #[test]
    fn cursor_started_calls_only() {
        let started = r#"{"type":"tool_call","subtype":"started","tool_call":{"grepToolCall":{"args":{"pattern":"TODO"}}}}"#;
        let done = r#"{"type":"tool_call","subtype":"completed","tool_call":{"grepToolCall":{"args":{"pattern":"TODO"}}}}"#;
        assert_eq!(from_line("cursor", started, Path::new("/p")), Some(Step { kind: "search", target: "TODO".into() }));
        assert_eq!(from_line("cursor", done, Path::new("/p")), None);
    }
}
