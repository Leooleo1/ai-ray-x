//! Codex rollouts: `~/.codex/sessions/**/*.jsonl`.
//!
//! Message text and reasoning lines are skipped before parsing; tool outputs
//! are only measured; shell commands are read in memory to spot secrets files.

use std::path::Path;

use memchr::memmem::Finder;
use serde::{Deserialize, Serialize};
use serde_json::json;
use serde_json::value::RawValue;

use super::record_call;
use crate::model::{Batch, CODEX, Call, Compaction, FileState, Moment, Snapshot, ToolCall, ToolResult};
use crate::util::lenient;
use crate::{secrets, util};

/// Parses complete lines of one rollout chunk.
pub fn parse_chunk(data: &[u8], state: &mut FileState, home: &Path) -> Batch {
    let markers = Markers::new();
    let mut parser = Parser { state, home, batch: Batch::default() };
    for line in util::lines(data) {
        parser.state.line += 1;
        if !markers.wanted(line) {
            continue;
        }
        let Ok(record) = serde_json::from_slice::<Line>(line) else { continue };
        parser.batch.lines_parsed += 1;
        parser.handle(record);
    }
    parser.batch
}

struct Markers {
    response_item: Finder<'static>,
    tool_items: [Finder<'static>; 4],
}

impl Markers {
    fn new() -> Self {
        Self {
            response_item: Finder::new(b"\"type\":\"response_item\""),
            tool_items: [
                Finder::new(b"\"type\":\"function_call\""),
                Finder::new(b"\"type\":\"custom_tool_call\""),
                Finder::new(b"\"type\":\"function_call_output\""),
                Finder::new(b"\"type\":\"custom_tool_call_output\""),
            ],
        }
    }

    /// Response items are mostly message text and reasoning: only tool calls and outputs matter.
    fn wanted(&self, line: &[u8]) -> bool {
        self.response_item.find(line).is_none() || self.tool_items.iter().any(|f| f.find(line).is_some())
    }
}

#[derive(Deserialize)]
struct Line<'a> {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default, deserialize_with = "lenient")]
    timestamp: Option<String>,
    #[serde(borrow)]
    payload: Option<&'a RawValue>,
}

#[derive(Deserialize)]
struct SessionMeta<'a> {
    #[serde(default, deserialize_with = "lenient")]
    id: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    cwd: Option<String>,
    #[serde(borrow)]
    source: Option<&'a RawValue>,
    #[serde(borrow)]
    base_instructions: Option<BaseInstructions<'a>>,
    #[serde(borrow)]
    dynamic_tools: Option<&'a RawValue>,
}

#[derive(Deserialize)]
struct BaseInstructions<'a> {
    #[serde(borrow)]
    text: Option<&'a RawValue>,
}

#[derive(Deserialize)]
struct TurnContext {
    #[serde(default, deserialize_with = "lenient")]
    model: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    effort: Option<String>,
}

#[derive(Deserialize)]
struct UsageRecord {
    #[serde(default, deserialize_with = "lenient")]
    response_id: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    usage: Option<Usage>,
}

#[derive(Deserialize)]
struct Usage {
    input_tokens: Option<u64>,
    cached_input_tokens: Option<u64>,
    cache_write_input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    reasoning_output_tokens: Option<u64>,
}

#[derive(Deserialize)]
struct Event<'a> {
    #[serde(rename = "type", default, deserialize_with = "lenient")]
    kind: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    duration_ms: Option<u64>,
    #[serde(default, deserialize_with = "lenient")]
    time_to_first_token_ms: Option<u64>,
    #[serde(borrow)]
    error: Option<&'a RawValue>,
    #[serde(borrow)]
    item: Option<&'a RawValue>,
    #[serde(borrow)]
    info: Option<&'a RawValue>,
    #[serde(default, deserialize_with = "lenient")]
    rate_limits: Option<Limits>,
}

#[derive(Deserialize)]
struct Item<'a> {
    #[serde(default, deserialize_with = "lenient")]
    exit_code: Option<i64>,
    #[serde(borrow)]
    command: Option<&'a RawValue>,
}

#[derive(Deserialize)]
struct Info {
    model_context_window: Option<u64>,
}

/// Only the plan and the usage percentages are kept, not balances or ids.
#[derive(Deserialize, Serialize)]
struct Limits {
    plan_type: Option<String>,
    primary: Option<Window>,
    secondary: Option<Window>,
}

#[derive(Deserialize, Serialize)]
struct Window {
    used_percent: Option<f64>,
    window_minutes: Option<u64>,
    resets_at: Option<i64>,
}

#[derive(Deserialize)]
struct ResponseItem<'a> {
    #[serde(rename = "type", default, deserialize_with = "lenient")]
    kind: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    name: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    call_id: Option<String>,
    #[serde(borrow)]
    output: Option<&'a RawValue>,
}

struct Parser<'s> {
    state: &'s mut FileState,
    home: &'s Path,
    batch: Batch,
}

impl Parser<'_> {
    fn handle(&mut self, record: Line<'_>) {
        let Some(payload) = record.payload else { return };
        let at = record.timestamp.as_deref().and_then(util::moment);
        let ms = at.as_ref().map(|m| m.ms);
        match record.kind.as_str() {
            "session_meta" => self.session_meta(payload, ms),
            "turn_context" => self.turn_context(payload),
            "token_usage_record" => self.usage(payload, at),
            "event_msg" => self.event(payload, ms),
            "compacted" => self.compacted(ms),
            "response_item" => self.response_item(payload, ms),
            _ => {}
        }
    }

    fn session_meta(&mut self, payload: &RawValue, ms: Option<i64>) {
        let Ok(meta) = serde_json::from_str::<SessionMeta>(payload.get()) else { return };
        if self.state.session.is_some() {
            return;
        }
        self.state.session = meta.id;
        self.state.project = meta.cwd.as_deref().map(|cwd| util::project_of(cwd, self.home));
        self.state.is_subagent = meta.source.is_some_and(|s| s.get().starts_with('{') && s.get().contains("subagent"));
        if !self.state.is_subagent {
            let instructions = meta.base_instructions.and_then(|b| b.text).map_or(0, util::raw_len);
            let tools = meta.dynamic_tools.map_or(0, |t| t.get().len() as u64);
            let value = json!({ "instructions_chars": instructions, "dynamic_tools_chars": tools }).to_string();
            self.batch.snapshots.push(Snapshot { key: "base_instructions", ms: ms.unwrap_or(0), value });
        }
    }

    fn turn_context(&mut self, payload: &RawValue) {
        let Ok(context) = serde_json::from_str::<TurnContext>(payload.get()) else { return };
        if let Some(model) = context.model {
            self.state.model = Some(model);
        }
        if let Some(effort) = context.effort {
            self.batch.count(format!("effort:{effort}"), 1);
            self.state.effort = Some(effort);
        }
    }

    fn usage(&mut self, payload: &RawValue, at: Option<Moment>) {
        let Ok(record) = serde_json::from_str::<UsageRecord>(payload.get()) else { return };
        let (Some(response_id), Some(usage)) = (record.response_id, record.usage) else { return };
        let input = usage.input_tokens.unwrap_or(0);
        let cached = usage.cached_input_tokens.unwrap_or(0).min(input);
        let call = Call {
            id: format!("codex:{response_id}"),
            source: CODEX,
            session: self.state.session.clone().unwrap_or_default(),
            project: self.state.project.clone().unwrap_or_else(|| "?".to_string()),
            model: self.state.model.clone().unwrap_or_else(|| "?".to_string()),
            is_subagent: self.state.is_subagent,
            at,
            input: input - cached,
            cache_read: cached,
            cache_write_5m: usage.cache_write_input_tokens.unwrap_or(0),
            output: usage.output_tokens.unwrap_or(0),
            thinking: usage.reasoning_output_tokens.unwrap_or(0),
            effort: self.state.effort.clone(),
            ..Call::default()
        };
        record_call(self.state, &mut self.batch, call);
    }

    fn event(&mut self, payload: &RawValue, ms: Option<i64>) {
        let Ok(event) = serde_json::from_str::<Event>(payload.get()) else { return };
        match event.kind.as_deref() {
            Some("task_complete") => {
                self.batch.count("turns", 1);
                self.batch.count("turn_ms", event.duration_ms.unwrap_or(0) as i64);
                self.batch.count("ttft_ms", event.time_to_first_token_ms.unwrap_or(0) as i64);
                if event.error.is_some() {
                    self.batch.count("turn_errors", 1);
                }
            }
            Some("task_started") => self.batch.count("turns_started", 1),
            Some("turn_aborted") => self.batch.count("turns_aborted", 1),
            Some("item_completed") => {
                if let Some(item) = event.item.and_then(|raw| serde_json::from_str::<Item>(raw.get()).ok()) {
                    self.command(item);
                }
            }
            Some("token_count") => self.token_count(event, ms.unwrap_or(0)),
            _ => {}
        }
    }

    fn command(&mut self, item: Item<'_>) {
        let Some(exit_code) = item.exit_code else { return };
        self.batch.count("commands", 1);
        if exit_code != 0 {
            self.batch.count("commands_failed", 1);
        }
        let command = item.command.map(command_text).unwrap_or_default();
        if let Some(kind) = secrets::classify_command(&command) {
            self.batch.count(format!("secret:{}", kind.as_str()), 1);
        }
    }

    fn token_count(&mut self, event: Event<'_>, ms: i64) {
        let window = event.info.and_then(|raw| serde_json::from_str::<Info>(raw.get()).ok()?.model_context_window);
        if let Some(window) = window {
            self.batch.snapshots.push(Snapshot { key: "context_window", ms, value: window.to_string() });
        }
        if let Some(value) = event.rate_limits.and_then(|limits| serde_json::to_string(&limits).ok()) {
            self.batch.snapshots.push(Snapshot { key: "rate_limits", ms, value });
        }
    }

    fn compacted(&mut self, ms: Option<i64>) {
        let id = format!("codex:{}:compact:{}", self.state.session.as_deref().unwrap_or("?"), self.state.line);
        self.batch.compactions.push(Compaction { id, segment: self.state.segment, ms, pre: 0, post: 0 });
        self.state.segment += 1;
        self.state.after_compact = true;
    }

    fn response_item(&mut self, payload: &RawValue, ms: Option<i64>) {
        let Ok(item) = serde_json::from_str::<ResponseItem>(payload.get()) else { return };
        let Some(call_id) = item.call_id else { return };
        match item.kind.as_deref() {
            Some("function_call" | "custom_tool_call") => {
                let name = item.name.unwrap_or_else(|| "?".to_string());
                self.state.pending_tools.insert(call_id.clone(), name.clone());
                let session = self.state.session.clone().unwrap_or_default();
                let id = format!("codex:{call_id}");
                self.batch.tool_calls.push(ToolCall { id, session, name, ms, skill: None, secret: None });
            }
            Some("function_call_output" | "custom_tool_call_output") => {
                let tool =
                    self.state.pending_tools.remove(&call_id).map_or_else(|| "?".to_string(), |n| util::tool_group(&n));
                let (chars, images) = item.output.map(util::measure_content).unwrap_or((0, 0));
                let (segment, seq_at) = (self.state.segment, self.state.seq);
                let id = format!("codex:{call_id}");
                self.batch.results.push(ToolResult {
                    id,
                    tool,
                    segment,
                    seq_at,
                    chars,
                    images,
                    ..ToolResult::default()
                });
            }
            _ => {}
        }
    }
}

/// `["/bin/zsh", "-lc", "cat .env"]` -> `cat .env`; plain strings pass through.
fn command_text(raw: &RawValue) -> String {
    if let Ok(parts) = serde_json::from_str::<Vec<String>>(raw.get()) {
        return match parts.as_slice() {
            [_, flag, body, ..] if flag == "-lc" || flag == "-c" => body.clone(),
            _ => parts.join(" "),
        };
    }
    serde_json::from_str::<String>(raw.get()).unwrap_or_default()
}

#[cfg(test)]
#[path = "codex_tests.rs"]
mod tests;
