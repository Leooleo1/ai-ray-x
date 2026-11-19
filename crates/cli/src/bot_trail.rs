//! The whole path of a turn, read live from the CLI's events: each step with
//! what it touched and what came back (lines read, files found, a command's
//! last lines and exit code), what the AI said between steps, and the text
//! it is writing right now. The person sees the path while it runs, not only
//! once the answer is done, and the answer keeps it for later.

use std::path::Path;

use serde::Serialize;
use serde_json::Value;

use crate::bot_steps::{self, Step};

/// The path keeps this many items; older ones are dropped and counted.
const MAX_ITEMS: usize = 80;
/// What an item shows of a result, or of something the AI said.
const MAX_DETAIL: usize = 240;
const MAX_SAID: usize = 400;
/// Lines of a command's output shown in its item.
const OUTPUT_LINES: usize = 3;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Item {
    /// A step kind ("read", "search", "list", "edit", "run", "web", "think", "tool") or "say".
    pub kind: &'static str,
    /// What the step touched, or what the AI said.
    pub target: String,
    /// What came back, once it did.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Whether it worked, once it is known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ok: Option<bool>,
    /// Lines read, or files found or changed, when that is the result.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<usize>,
    #[serde(skip)]
    id: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Trail {
    pub items: Vec<Item>,
    /// Items dropped from the start of a long path.
    pub dropped: usize,
    /// The text the AI is writing now, before it is complete.
    pub partial: String,
    /// The CLI's session, as soon as it says which: a stopped turn can be corrected in it.
    #[serde(skip)]
    pub session: Option<String>,
}

fn cut(text: &str, max: usize) -> String {
    let text = text.trim();
    if text.chars().count() <= max {
        return text.to_string();
    }
    format!("{}…", text.chars().take(max).collect::<String>().trim_end())
}

/// The last lines of an output, in a few words.
fn last_lines(text: &str) -> String {
    let lines: Vec<&str> = text.lines().map(str::trim_end).filter(|l| !l.trim().is_empty()).collect();
    let start = lines.len().saturating_sub(OUTPUT_LINES);
    cut(&lines[start..].join(" · "), MAX_DETAIL)
}

/// Lines of a file read; Claude Code numbers them ("12\t…"), and a numbered empty line is no line.
fn lines_read(text: &str) -> usize {
    text.lines().map(|line| line.trim_start_matches(|c: char| c.is_ascii_digit()).trim_start_matches('\t')).filter(|l| !l.trim().is_empty()).count()
}

impl Trail {
    fn push(&mut self, item: Item) {
        self.items.push(item);
        if self.items.len() > MAX_ITEMS {
            self.items.remove(0);
            self.dropped += 1;
        }
    }

    fn step(&mut self, id: &str, step: Step) {
        self.flush();
        self.push(Item { kind: step.kind, target: step.target, detail: None, ok: None, count: None, id: id.to_string() });
    }

    fn said(&mut self, text: &str) {
        self.partial.clear();
        if !text.trim().is_empty() {
            self.push(Item { kind: "say", target: cut(text, MAX_SAID), detail: None, ok: None, count: None, id: String::new() });
        }
    }

    /// What was being written becomes something said, when a step interrupts it.
    fn flush(&mut self) {
        if !self.partial.trim().is_empty() {
            let text = std::mem::take(&mut self.partial);
            self.said(&text);
        }
    }

    fn result(&mut self, id: &str, text: &str, ok: bool) {
        if let Some(item) = self.items.iter_mut().rev().find(|item| item.id == id) {
            if item.kind == "read" && ok {
                item.count = Some(lines_read(text));
            } else {
                item.detail = Some(last_lines(text)).filter(|d| !d.is_empty());
            }
            item.ok = Some(ok);
        }
    }

    /// A result that is a number of files.
    fn files(&mut self, id: &str, files: usize, ok: bool) {
        if let Some(item) = self.items.iter_mut().rev().find(|item| item.id == id) {
            item.count = Some(files);
            item.ok = Some(ok);
        }
    }

    /// The step being done now, for the activity bar.
    pub fn current(&self) -> Option<Step> {
        self.items.iter().rev().find(|item| item.kind != "say").map(|item| Step { kind: item.kind, target: item.target.clone() })
    }

    /// The path as the answer keeps it: without what is half written, nor the closing text,
    /// which is the answer itself.
    pub fn finish(mut self) -> Trail {
        self.partial.clear();
        while self.items.last().is_some_and(|item| item.kind == "say") {
            self.items.pop();
        }
        self
    }

    /// Reads one event line of the CLI's output.
    pub fn feed(&mut self, agent: &str, line: &str, dir: &Path) {
        let Ok(event) = serde_json::from_str::<Value>(line) else { return };
        if self.session.is_none() {
            let id = event.get("session_id").or_else(|| event.get("thread_id")).and_then(Value::as_str);
            self.session = id.map(str::to_string);
        }
        match agent {
            "claude" => self.claude(&event, line, dir),
            "codex" => self.codex(&event, line, dir),
            "cursor" => self.cursor(&event, line, dir),
            _ => {}
        }
    }

    fn claude(&mut self, event: &Value, line: &str, dir: &Path) {
        match event.get("type").and_then(Value::as_str) {
            Some("stream_event") => {
                if let Some(delta) = event.pointer("/event/delta").filter(|d| d.get("type").and_then(Value::as_str) == Some("text_delta")) {
                    self.partial.push_str(delta.get("text").and_then(Value::as_str).unwrap_or(""));
                }
            }
            Some("assistant") => {
                for block in event.pointer("/message/content").and_then(Value::as_array).into_iter().flatten() {
                    match block.get("type").and_then(Value::as_str) {
                        Some("text") => self.said(block.get("text").and_then(Value::as_str).unwrap_or("")),
                        Some("tool_use") => {
                            if let Some(step) = bot_steps::from_line("claude", line, dir) {
                                self.step(block.get("id").and_then(Value::as_str).unwrap_or(""), step);
                            }
                        }
                        _ => {}
                    }
                }
            }
            Some("user") => {
                for block in event.pointer("/message/content").and_then(Value::as_array).into_iter().flatten() {
                    let Some(id) = block.get("tool_use_id").and_then(Value::as_str) else { continue };
                    let content = block.get("content");
                    let text = content.and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| {
                        content.and_then(Value::as_array).into_iter().flatten().filter_map(|part| part.get("text").and_then(Value::as_str)).collect::<Vec<_>>().join("\n")
                    });
                    let ok = !block.get("is_error").and_then(Value::as_bool).unwrap_or(false);
                    self.result(id, &text, ok);
                }
            }
            _ => {}
        }
    }

    fn codex(&mut self, event: &Value, line: &str, dir: &Path) {
        let Some(item) = event.get("item") else { return };
        let id = item.get("id").and_then(Value::as_str).unwrap_or("");
        match (event.get("type").and_then(Value::as_str), item.get("type").and_then(Value::as_str)) {
            (Some("item.started"), Some(_)) => {
                if let Some(step) = bot_steps::from_line("codex", line, dir).filter(|s| s.kind != "think") {
                    self.step(id, step);
                }
            }
            (Some("item.completed"), Some("agent_message")) => self.said(item.get("text").and_then(Value::as_str).unwrap_or("")),
            (Some("item.completed"), Some("command_execution")) => {
                let output = item.get("aggregated_output").and_then(Value::as_str).unwrap_or("");
                let code = item.get("exit_code").and_then(Value::as_i64);
                self.result(id, output, code == Some(0));
            }
            (Some("item.completed"), Some("file_change")) => {
                let paths = item.get("changes").and_then(Value::as_array).map_or(0, Vec::len);
                self.files(id, paths, true);
            }
            _ => {}
        }
    }

    fn cursor(&mut self, event: &Value, line: &str, dir: &Path) {
        match event.get("type").and_then(Value::as_str) {
            // Pieces of text carry a time; the last message, the whole answer, comes without one.
            Some("assistant") if event.get("timestamp_ms").is_some() => {
                let pieces = event.pointer("/message/content").and_then(Value::as_array).into_iter().flatten();
                self.partial.extend(pieces.filter_map(|part| part.get("text").and_then(Value::as_str)));
            }
            Some("tool_call") => {
                let id = event.get("call_id").and_then(Value::as_str).unwrap_or("");
                let call = event.get("tool_call").and_then(Value::as_object);
                let Some((name, body)) = call.and_then(|c| c.iter().find(|(name, _)| name.ends_with("ToolCall"))) else { return };
                // Cursor looks up its own tools as a call; that is not a step of the work.
                if name == "getMcpToolsToolCall" {
                    return;
                }
                match event.get("subtype").and_then(Value::as_str) {
                    Some("started") => {
                        if let Some(step) = bot_steps::from_line("cursor", line, dir) {
                            self.step(id, step);
                        }
                    }
                    Some("completed") => {
                        let success = body.pointer("/result/success");
                        let files = success.and_then(|s| s.get("files")).and_then(Value::as_array).map(Vec::len);
                        let text = success.and_then(|s| s.get("content").or_else(|| s.get("output")).or_else(|| s.get("stdout"))).and_then(Value::as_str);
                        match (files, text) {
                            (Some(files), _) => self.files(id, files, true),
                            (None, text) => self.result(id, text.unwrap_or(""), success.is_some()),
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

/// Reads the given new lines into the trail; says whether anything changed.
pub fn read(trail: &mut Trail, agent: &str, lines: &[String], dir: &Path) -> bool {
    let before = (trail.items.len(), trail.dropped, trail.partial.len(), trail.items.last().map(|i| i.detail.clone()));
    for line in lines {
        trail.feed(agent, line, dir);
    }
    before != (trail.items.len(), trail.dropped, trail.partial.len(), trail.items.last().map(|i| i.detail.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An item as the tests compare it: kind, target, detail, count, ok.
    type Shown<'a> = (&'a str, &'a str, Option<&'a str>, Option<usize>, Option<bool>);

    fn shown(t: &Trail) -> Vec<Shown<'_>> {
        t.items.iter().map(|i| (i.kind, i.target.as_str(), i.detail.as_deref(), i.count, i.ok)).collect()
    }

    fn trail(agent: &str, lines: &[&str]) -> Trail {
        let mut trail = Trail::default();
        for line in lines {
            trail.feed(agent, line, Path::new("/p"));
        }
        trail
    }

    #[test]
    fn claude_steps_their_results_and_what_it_writes() {
        let t = trail(
            "claude",
            &[
                r#"{"type":"system","subtype":"init","session_id":"s1"}"#,
                r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"Vou "}}}"#,
                r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"ler."}}}"#,
                r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Vou ler."}]}}"#,
                r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"Read","input":{"file_path":"/p/soma.js"}}]}}"#,
                r#"{"type":"user","message":{"content":[{"tool_use_id":"t1","type":"tool_result","content":"1\texport const somar\n2\t"}]}}"#,
                r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t2","name":"Bash","input":{"command":"npm test"}}]}}"#,
                r#"{"type":"user","message":{"content":[{"tool_use_id":"t2","type":"tool_result","is_error":true,"content":[{"type":"text","text":"a\nb\nc\nd failed"}]}]}}"#,
                r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"O teste"}}}"#,
            ],
        );
        assert_eq!(t.session.as_deref(), Some("s1"));
        assert_eq!(
            shown(&t),
            vec![("say", "Vou ler.", None, None, None), ("read", "soma.js", None, Some(1), Some(true)), ("run", "npm test", Some("b · c · d failed"), None, Some(false))]
        );
        assert_eq!(t.partial, "O teste");
        assert_eq!(t.current(), Some(Step { kind: "run", target: "npm test".into() }));
    }

    #[test]
    fn cursor_pieces_become_what_it_said_when_a_step_starts() {
        let t = trail(
            "cursor",
            &[
                r#"{"type":"assistant","timestamp_ms":1,"message":{"content":[{"type":"text","text":"Vou "}]}}"#,
                r#"{"type":"assistant","timestamp_ms":2,"message":{"content":[{"type":"text","text":"ler."}]}}"#,
                r#"{"type":"tool_call","subtype":"started","call_id":"c1","tool_call":{"readToolCall":{"args":{"path":"/p/soma.js"}}}}"#,
                r#"{"type":"tool_call","subtype":"started","call_id":"c2","tool_call":{"getMcpToolsToolCall":{"args":{}}}}"#,
                r#"{"type":"tool_call","subtype":"completed","call_id":"c1","tool_call":{"readToolCall":{"args":{"path":"/p/soma.js"},"result":{"success":{"content":"a\nb\n"}}}}}"#,
                r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Resposta inteira."}]}}"#,
            ],
        );
        let shown: Vec<(&str, &str, Option<usize>)> = t.items.iter().map(|i| (i.kind, i.target.as_str(), i.count)).collect();
        assert_eq!(shown, vec![("say", "Vou ler.", None), ("read", "soma.js", Some(2))]);
        assert!(t.partial.is_empty(), "the whole answer is not a piece");
    }

    #[test]
    fn codex_commands_with_their_output_and_the_path_without_the_answer() {
        let t = trail(
            "codex",
            &[
                r#"{"type":"thread.started","thread_id":"th1"}"#,
                r#"{"type":"item.completed","item":{"id":"i0","type":"agent_message","text":"Vou ler."}}"#,
                r#"{"type":"item.started","item":{"id":"i1","type":"command_execution","command":"/bin/zsh -lc 'cat soma.js'"}}"#,
                r#"{"type":"item.completed","item":{"id":"i1","type":"command_execution","command":"/bin/zsh -lc 'cat soma.js'","aggregated_output":"export const somar\n","exit_code":0}}"#,
                r#"{"type":"item.completed","item":{"id":"i2","type":"agent_message","text":"É uma soma."}}"#,
            ],
        );
        assert_eq!(t.session.as_deref(), Some("th1"));
        let kept = t.finish();
        assert_eq!(shown(&kept), vec![("say", "Vou ler.", None, None, None), ("run", "cat soma.js", Some("export const somar"), None, Some(true))]);
    }

    #[test]
    fn a_long_path_keeps_its_end() {
        let mut t = Trail::default();
        for n in 0..MAX_ITEMS + 5 {
            t.said(&format!("passo {n}"));
        }
        assert_eq!((t.items.len(), t.dropped), (MAX_ITEMS, 5));
        assert_eq!(t.items[0].target, "passo 5");
    }
}
