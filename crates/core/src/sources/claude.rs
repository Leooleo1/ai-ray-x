//! Claude Code transcripts: `~/.claude/projects/**/*.jsonl`.
//!
//! Content (messages, thinking, tool input and output) stays as raw JSON slices
//! and is only measured. The single exception is a tool call's input, read in
//! memory to spot secrets files and then dropped.

use std::collections::BTreeMap;
use std::path::Path;

use memchr::memmem::Finder;
use rayon::prelude::*;
use serde::Deserialize;
use serde_json::json;
use serde_json::value::RawValue;

use super::record_call;
use crate::model::{Batch, CLAUDE, Call, Compaction, Cost, FileState, Prompt, Snapshot, ToolCall, ToolResult};
use crate::util::lenient;
use crate::{secrets, util};

/// Bytes checked at each end of a line before falling back to a full scan.
const EDGE_BYTES: usize = 1024;
/// Large transcripts are decoded in blocks of about this size, in parallel.
const BLOCK_BYTES: usize = 8 * 1024 * 1024;

const SYSTEM_LABELS: [(&str, &[&str]); 6] = [
    ("skills", &["skills are available", "Skill tool"]),
    ("instructions", &["Contents of /"]),
    ("memory", &["MEMORY.md", "auto-memory"]),
    ("mcp_instructions", &["MCP Server Instructions"]),
    ("deferred_tools", &["deferred tools"]),
    ("output_style", &["Output Style"]),
];

/// Parses complete lines of one transcript chunk. JSON decoding runs in
/// parallel blocks; the timeline (order, segments, rewrites) is then applied
/// in file order.
pub fn parse_chunk(data: &[u8], state: &mut FileState, is_subagent: bool, home: &Path) -> Batch {
    let decoded: Vec<(u64, Vec<(u64, Line)>)> =
        util::blocks(data, BLOCK_BYTES).into_par_iter().map(decode_block).collect();
    let mut parser = Parser { state, is_subagent, home, batch: Batch::default() };
    for (line_count, records) in decoded {
        let base = parser.state.line;
        for (position, record) in records {
            parser.state.line = base + position;
            parser.batch.lines_parsed += 1;
            parser.handle(record);
        }
        parser.state.line = base + line_count;
    }
    parser.batch
}

/// Decodes the lines worth keeping, with their 1-based position in the block.
fn decode_block(block: &[u8]) -> (u64, Vec<(u64, Line<'_>)>) {
    let markers = Markers::new();
    let mut count = 0;
    let mut records = Vec::new();
    for line in util::lines(block) {
        count += 1;
        if matches!(markers.kind(line), LineKind::Keep)
            && let Ok(record) = serde_json::from_slice::<Line>(line)
        {
            records.push((count, record));
        }
    }
    (count, records)
}

enum LineKind {
    Skip,
    Keep,
}

/// Byte markers that decide, before any JSON parsing, whether a line matters.
/// The bulky `api-request-blob` lines (most of the bytes) are never parsed.
struct Markers {
    skip: [Finder<'static>; 2],
    keep: [Finder<'static>; 6],
}

impl Markers {
    fn new() -> Self {
        Self {
            skip: [Finder::new(b"\"type\":\"api-request-blob\""), Finder::new(b"\"type\":\"api-request\"")],
            keep: [
                Finder::new(b"\"type\":\"assistant\""),
                Finder::new(b"\"type\":\"user\""),
                Finder::new(b"\"type\":\"system\""),
                Finder::new(b"\"type\":\"attachment\""),
                Finder::new(b"\"type\":\"cost-state\""),
                Finder::new(b"\"type\":\"api-request-shape\""),
            ],
        }
    }

    /// The top-level `type` sits at the start or the end of a line, so the
    /// edges decide almost every line without scanning megabytes of content.
    fn kind(&self, line: &[u8]) -> LineKind {
        let head = &line[..line.len().min(EDGE_BYTES)];
        let tail = &line[line.len().saturating_sub(EDGE_BYTES)..];
        for window in [head, tail] {
            if let Some(kind) = self.decide(window) {
                return kind;
            }
        }
        self.decide(line).unwrap_or(LineKind::Skip)
    }

    fn decide(&self, bytes: &[u8]) -> Option<LineKind> {
        if self.skip.iter().any(|f| f.find(bytes).is_some()) {
            return Some(LineKind::Skip);
        }
        self.keep.iter().any(|f| f.find(bytes).is_some()).then_some(LineKind::Keep)
    }
}

#[derive(Deserialize)]
struct Line<'a> {
    #[serde(rename = "type")]
    kind: String,
    #[serde(rename = "sessionId", default, deserialize_with = "lenient")]
    session_id: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    timestamp: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    cwd: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    uuid: Option<String>,
    #[serde(rename = "isSidechain", default, deserialize_with = "lenient")]
    is_sidechain: Option<bool>,
    #[serde(rename = "isMeta", default, deserialize_with = "lenient")]
    is_meta: Option<bool>,
    #[serde(default, deserialize_with = "lenient")]
    effort: Option<String>,
    #[serde(borrow)]
    message: Option<Message<'a>>,
    #[serde(rename = "toolUseResult", borrow)]
    tool_use_result: Option<&'a RawValue>,
    #[serde(rename = "permissionDecision", default, deserialize_with = "lenient")]
    permission: Option<Permission>,
    #[serde(default, deserialize_with = "lenient")]
    subtype: Option<String>,
    #[serde(rename = "durationMs", default, deserialize_with = "lenient")]
    duration_ms: Option<u64>,
    #[serde(rename = "compactMetadata", default, deserialize_with = "lenient")]
    compact: Option<CompactMeta>,
    #[serde(rename = "totalCostUSD", default, deserialize_with = "lenient")]
    total_cost_usd: Option<f64>,
    #[serde(borrow)]
    attachment: Option<&'a RawValue>,
    #[serde(borrow)]
    shape: Option<Shape<'a>>,
}

#[derive(Deserialize)]
struct Message<'a> {
    #[serde(default, deserialize_with = "lenient")]
    id: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    model: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    usage: Option<Usage>,
    #[serde(borrow)]
    content: Option<&'a RawValue>,
}

#[derive(Deserialize, Default)]
struct Usage {
    input_tokens: Option<u64>,
    cache_read_input_tokens: Option<u64>,
    cache_creation_input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cache_creation: Option<CacheCreation>,
    output_tokens_details: Option<OutputDetails>,
    server_tool_use: Option<ServerToolUse>,
}

#[derive(Deserialize)]
struct CacheCreation {
    ephemeral_1h_input_tokens: Option<u64>,
}

#[derive(Deserialize)]
struct OutputDetails {
    thinking_tokens: Option<u64>,
}

#[derive(Deserialize)]
struct ServerToolUse {
    web_search_requests: Option<u64>,
}

#[derive(Deserialize)]
struct Block<'a> {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default, deserialize_with = "lenient")]
    id: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    name: Option<String>,
    #[serde(borrow)]
    input: Option<&'a RawValue>,
    #[serde(default, deserialize_with = "lenient")]
    tool_use_id: Option<String>,
    #[serde(borrow)]
    content: Option<&'a RawValue>,
    #[serde(borrow)]
    text: Option<&'a RawValue>,
}

#[derive(Deserialize)]
struct Permission {
    decision: Option<String>,
    source: Option<String>,
}

#[derive(Deserialize)]
struct CompactMeta {
    #[serde(rename = "preTokens")]
    pre_tokens: Option<u64>,
    #[serde(rename = "postTokens")]
    post_tokens: Option<u64>,
}

#[derive(Deserialize)]
struct Shape<'a> {
    #[serde(borrow)]
    system: Option<Vec<ShapeText<'a>>>,
    #[serde(borrow)]
    tools: Option<Vec<ShapeTool<'a>>>,
}

#[derive(Deserialize)]
struct ShapeText<'a> {
    #[serde(borrow)]
    text: Option<&'a RawValue>,
}

#[derive(Deserialize)]
struct ShapeTool<'a> {
    name: Option<String>,
    #[serde(borrow)]
    description: Option<&'a RawValue>,
    #[serde(borrow)]
    input_schema: Option<&'a RawValue>,
}

#[derive(Deserialize)]
struct EditResult<'a> {
    #[serde(rename = "filePath")]
    file_path: Option<String>,
    #[serde(rename = "structuredPatch", borrow)]
    structured_patch: Option<Vec<Hunk<'a>>>,
    #[serde(rename = "type")]
    kind: Option<String>,
    #[serde(borrow)]
    content: Option<&'a RawValue>,
}

#[derive(Deserialize)]
struct Hunk<'a> {
    #[serde(borrow, default)]
    lines: Vec<&'a RawValue>,
}

#[derive(Deserialize)]
struct Named {
    #[serde(rename = "type")]
    kind: Option<String>,
    skill: Option<String>,
}

struct Parser<'s> {
    state: &'s mut FileState,
    is_subagent: bool,
    home: &'s Path,
    batch: Batch,
}

impl Parser<'_> {
    fn handle(&mut self, record: Line<'_>) {
        match record.kind.as_str() {
            "assistant" => self.assistant(record),
            "user" => self.user(record),
            "system" => self.system(&record),
            "attachment" => self.attachment(&record),
            "cost-state" => self.cost(&record),
            "api-request-shape" => self.shape(&record),
            _ => {}
        }
    }

    fn assistant(&mut self, record: Line<'_>) {
        let Some(message) = record.message else { return };
        let session = record.session_id.unwrap_or_default();
        let at = record.timestamp.as_deref().and_then(util::moment);
        if let Some(content) = message.content {
            self.tool_uses(content, &session, at.as_ref().map(|m| m.ms));
        }
        let (Some(id), Some(model)) = (message.id, message.model) else { return };
        if model == "<synthetic>" {
            return;
        }
        let usage = message.usage.unwrap_or_default();
        let cache_write = usage.cache_creation_input_tokens.unwrap_or(0);
        let write_1h = usage.cache_creation.and_then(|c| c.ephemeral_1h_input_tokens).unwrap_or(0).min(cache_write);
        let call = Call {
            id,
            source: CLAUDE,
            session,
            project: record.cwd.as_deref().map_or_else(|| "?".to_string(), |cwd| util::project_of(cwd, self.home)),
            model,
            is_subagent: self.is_subagent || record.is_sidechain.unwrap_or(false),
            at,
            input: usage.input_tokens.unwrap_or(0),
            cache_read: usage.cache_read_input_tokens.unwrap_or(0),
            cache_write_5m: cache_write - write_1h,
            cache_write_1h: write_1h,
            output: usage.output_tokens.unwrap_or(0),
            thinking: usage.output_tokens_details.and_then(|d| d.thinking_tokens).unwrap_or(0),
            web_searches: usage.server_tool_use.and_then(|s| s.web_search_requests).unwrap_or(0),
            effort: record.effort,
            ..Call::default()
        };
        let (context, ms) = (call.context(), call.at.as_ref().map(|m| m.ms));
        let is_first = record_call(self.state, &mut self.batch, call);
        if is_first && !self.is_subagent && !self.state.startup_chars.is_empty() {
            let value = json!({ "attachments": &self.state.startup_chars, "first_call_context": context });
            self.batch.snapshots.push(Snapshot {
                key: "startup_context",
                ms: ms.unwrap_or(0),
                value: value.to_string(),
            });
            self.state.startup_chars.clear();
        }
    }

    fn tool_uses(&mut self, content: &RawValue, session: &str, ms: Option<i64>) {
        if !content.get().starts_with('[') {
            return;
        }
        let Ok(blocks) = serde_json::from_str::<Vec<Block>>(content.get()) else { return };
        for block in blocks.into_iter().filter(|b| b.kind == "tool_use") {
            let (Some(id), Some(name)) = (block.id, block.name) else { continue };
            let input = block.input.map(|raw| raw.get().as_bytes()).unwrap_or_default();
            let skill = (name == "Skill").then(|| serde_json::from_slice::<Named>(input).ok()?.skill).flatten();
            let secret = secrets::classify_tool(&name, input);
            self.state.pending_tools.insert(id.clone(), name.clone());
            self.batch.tool_calls.push(ToolCall { id, session: session.to_string(), name, ms, skill, secret });
        }
    }

    fn user(&mut self, record: Line<'_>) {
        if let Some(p) = &record.permission {
            let key =
                format!("permission:{}/{}", p.decision.as_deref().unwrap_or("?"), p.source.as_deref().unwrap_or("?"));
            self.batch.count(key, 1);
        }
        let edit_source = record.tool_use_result;
        if edit_source.is_some_and(|raw| util::contains(raw.get().as_bytes(), b"\"interrupted\":true")) {
            self.batch.count("interrupted", 1);
        }
        let Some(content) = record.message.and_then(|m| m.content) else { return };
        let session = record.session_id.unwrap_or_default();
        let ms = record.timestamp.as_deref().and_then(util::moment).map(|m| m.ms);
        let injected = record.is_meta.unwrap_or(false) || record.is_sidechain.unwrap_or(false) || self.is_subagent;
        let id = record.uuid.unwrap_or_else(|| format!("{session}:{}", self.state.line));
        if content.get().starts_with('"') {
            let tagged = util::raw_first_char(content) == Some(b'<');
            self.push_prompt(id, session, ms, (util::raw_len(content), 0), injected || tagged);
            return;
        }
        let Ok(blocks) = serde_json::from_str::<Vec<Block>>(content.get()) else { return };
        let (mut chars, mut images, mut tagged) = (0, 0, false);
        let mut edit_source = edit_source;
        for block in blocks {
            match block.kind.as_str() {
                "tool_result" => self.tool_result(block, edit_source.take()),
                "text" => {
                    if let Some(text) = block.text {
                        tagged |= chars == 0 && util::raw_first_char(text) == Some(b'<');
                        chars += util::raw_len(text);
                    }
                }
                "image" => images += 1,
                _ => {}
            }
        }
        if chars + images > 0 {
            self.push_prompt(id, session, ms, (chars, images), injected || tagged);
        }
    }

    fn push_prompt(&mut self, id: String, session: String, ms: Option<i64>, size: (u64, u64), injected: bool) {
        let (segment, seq_at) = (self.state.segment, self.state.seq);
        let (chars, images) = size;
        self.batch.prompts.push(Prompt { id, session, ms, segment, seq_at, chars, images, injected });
    }

    fn tool_result(&mut self, block: Block<'_>, edit_source: Option<&RawValue>) {
        let Some(id) = block.tool_use_id else { return };
        let tool = self.state.pending_tools.remove(&id).map_or_else(|| "?".to_string(), |name| util::tool_group(&name));
        let (chars, images) = block.content.map(util::measure_content).unwrap_or((0, 0));
        let result = ToolResult {
            id,
            tool,
            segment: self.state.segment,
            seq_at: self.state.seq,
            chars,
            images,
            ..ToolResult::default()
        };
        let result = match edit_source {
            Some(raw) => with_edit_stats(result, raw),
            None => result,
        };
        self.batch.results.push(result);
    }

    fn system(&mut self, record: &Line<'_>) {
        match record.subtype.as_deref() {
            Some("compact_boundary") => {
                let meta = record.compact.as_ref();
                self.batch.compactions.push(Compaction {
                    id: record.uuid.clone().unwrap_or_else(|| format!("compact:{}", self.state.line)),
                    segment: self.state.segment,
                    ms: record.timestamp.as_deref().and_then(util::moment).map(|m| m.ms),
                    pre: meta.and_then(|m| m.pre_tokens).unwrap_or(0),
                    post: meta.and_then(|m| m.post_tokens).unwrap_or(0),
                });
                self.state.segment += 1;
                self.state.after_compact = true;
            }
            Some("turn_duration") => {
                self.batch.count("turns", 1);
                self.batch.count("turn_ms", record.duration_ms.unwrap_or(0) as i64);
            }
            Some(other) => self.batch.count(format!("system:{other}"), 1),
            None => {}
        }
    }

    fn attachment(&mut self, record: &Line<'_>) {
        let Some(raw) = record.attachment else { return };
        let bytes = raw.get().as_bytes();
        let kind = serde_json::from_str::<Named>(raw.get()).ok().and_then(|n| n.kind).unwrap_or_default();
        let at_startup = self.state.seq == 0 && !self.is_subagent;
        if at_startup {
            *self.state.startup_chars.entry(kind.clone()).or_insert(0) += bytes.len() as u64;
        }
        let ms = record.timestamp.as_deref().and_then(util::moment).map(|m| m.ms);
        match kind.as_str() {
            // Later listings are deltas; only the startup one is the full list.
            "skill_listing" if at_startup => {
                let items = util::occurrences(bytes, b"\\n- ").to_string();
                self.batch.snapshots.push(Snapshot { key: "skill_listing_items", ms: ms.unwrap_or(0), value: items });
            }
            "queued_command" => {
                let images = util::occurrences(bytes, b"\"type\":\"image\"");
                if images > 0 {
                    let id = record.uuid.clone().unwrap_or_else(|| format!("queued:{}", self.state.line));
                    let session = record.session_id.clone().unwrap_or_default();
                    self.push_prompt(id, session, ms, (0, images), false);
                }
            }
            _ => {}
        }
    }

    fn cost(&mut self, record: &Line<'_>) {
        if let Some(usd) = record.total_cost_usd {
            let session = record.session_id.clone().unwrap_or_default();
            self.batch.costs.push(Cost { line: self.state.line, session, usd });
        }
    }

    fn shape(&mut self, record: &Line<'_>) {
        let ms = record.timestamp.as_deref().and_then(util::moment).map(|m| m.ms);
        let (Some(shape), Some(ms), false) = (&record.shape, ms, self.is_subagent) else { return };
        let mut system: BTreeMap<&str, u64> = BTreeMap::new();
        for text in shape.system.iter().flatten().filter_map(|block| block.text) {
            *system.entry(label_system(text.get())).or_insert(0) += util::raw_len(text);
        }
        let mut tools: BTreeMap<String, [u64; 2]> = BTreeMap::new();
        for tool in shape.tools.iter().flatten() {
            let name = tool.name.as_deref().unwrap_or("?");
            let group = if name.starts_with("mcp__") { util::tool_group(name) } else { "builtin".to_string() };
            let size =
                tool.description.map_or(0, util::raw_len) + tool.input_schema.map_or(0, |s| s.get().len() as u64);
            let entry = tools.entry(group).or_insert([0, 0]);
            entry[0] += 1;
            entry[1] += size;
        }
        let value = json!({ "system_chars": system, "tools": tools }).to_string();
        self.batch.snapshots.push(Snapshot { key: "request_shape", ms, value });
    }
}

fn label_system(text: &str) -> &'static str {
    SYSTEM_LABELS.iter().find(|(_, keys)| keys.iter().any(|key| text.contains(key))).map_or("base", |(label, _)| label)
}

/// Lines added and removed by an Edit/Write result, without keeping the path.
fn with_edit_stats(result: ToolResult, raw: &RawValue) -> ToolResult {
    let bytes = raw.get().as_bytes();
    if !util::contains(bytes, b"\"structuredPatch\"") && !util::contains(bytes, b"\"type\":\"create\"") {
        return result;
    }
    let Ok(edit) = serde_json::from_str::<EditResult>(raw.get()) else { return result };
    let (mut added, mut removed) = (0, 0);
    for line in edit.structured_patch.iter().flatten().flat_map(|hunk| hunk.lines.iter()) {
        match util::raw_first_char(line) {
            Some(b'+') => added += 1,
            Some(b'-') => removed += 1,
            _ => {}
        }
    }
    if edit.kind.as_deref() == Some("create") {
        added += edit.content.map_or(0, |c| util::occurrences(c.get().as_bytes(), b"\\n") + 1);
    }
    if added + removed == 0 {
        return result;
    }
    let path = edit.file_path.unwrap_or_default();
    ToolResult {
        added,
        removed,
        path_hash: Some(util::stable_hash(path.as_bytes())),
        ext: Some(util::extension(&path)),
        ..result
    }
}

#[cfg(test)]
#[path = "claude_tests.rs"]
mod tests;
