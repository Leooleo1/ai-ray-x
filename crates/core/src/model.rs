//! Normalized records extracted from agent logs.
//!
//! Only numbers and names live here: conversation text, command text and file
//! contents are measured while parsing and then dropped, never stored.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

pub const CLAUDE: &str = "claude";
pub const CODEX: &str = "codex";

/// Local-time breakdown of a log timestamp.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Moment {
    pub ms: i64,
    pub date: String,
    pub hour: u8,
    /// 0 = Monday.
    pub weekday: u8,
}

/// One model request, deduplicated by the provider's response id.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Call {
    pub id: String,
    pub source: &'static str,
    pub session: String,
    pub project: String,
    pub model: String,
    pub is_subagent: bool,
    pub at: Option<Moment>,
    pub input: u64,
    pub cache_read: u64,
    pub cache_write_5m: u64,
    pub cache_write_1h: u64,
    pub output: u64,
    pub thinking: u64,
    pub web_searches: u64,
    pub effort: Option<String>,
    /// Position of this request inside its log file (1-based).
    pub seq: u64,
    /// Compaction segment inside the file; increases after every compaction.
    pub segment: u64,
    pub is_first: bool,
    pub rewrite_reason: Option<&'static str>,
}

impl Call {
    pub fn cache_write(&self) -> u64 {
        self.cache_write_5m + self.cache_write_1h
    }

    /// Tokens the model had to read for this request (fresh + cached).
    pub fn context(&self) -> u64 {
        self.input + self.cache_read + self.cache_write()
    }

    /// Keeps identity and position, takes the token counts of a later copy.
    pub fn with_usage_of(&self, newer: &Call) -> Call {
        Call {
            input: newer.input,
            cache_read: newer.cache_read,
            cache_write_5m: newer.cache_write_5m,
            cache_write_1h: newer.cache_write_1h,
            output: newer.output,
            thinking: newer.thinking,
            web_searches: newer.web_searches,
            ..self.clone()
        }
    }
}

/// How a tool call touched a secrets file such as `.env`. Counted, never shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SecretKind {
    /// Content likely printed into the conversation (`cat`, Read tool, ...).
    Printed,
    /// Searched with grep-like tools; matching lines may have been printed.
    Searched,
    /// Loaded into a process without printing (`--env-file`, `source`).
    Loaded,
    /// Written or copied.
    Written,
    /// Mentioned in some other way.
    Mentioned,
}

impl SecretKind {
    pub fn as_str(self) -> &'static str {
        match self {
            SecretKind::Printed => "printed",
            SecretKind::Searched => "searched",
            SecretKind::Loaded => "loaded",
            SecretKind::Written => "written",
            SecretKind::Mentioned => "mentioned",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolCall {
    pub id: String,
    pub session: String,
    pub name: String,
    pub ms: Option<i64>,
    pub skill: Option<String>,
    pub secret: Option<SecretKind>,
}

/// Size of what a tool returned into the context, plus edit stats.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ToolResult {
    pub id: String,
    pub tool: String,
    pub segment: u64,
    pub seq_at: u64,
    pub chars: u64,
    pub images: u64,
    pub added: u64,
    pub removed: u64,
    /// Stable hash of the edited path, to count distinct files without storing paths.
    pub path_hash: Option<u64>,
    pub ext: Option<String>,
}

/// Text or images that entered the context from the user side.
#[derive(Debug, Clone, PartialEq)]
pub struct Prompt {
    pub id: String,
    pub session: String,
    pub ms: Option<i64>,
    pub segment: u64,
    pub seq_at: u64,
    pub chars: u64,
    pub images: u64,
    /// True for text injected by the harness (meta messages, subagent tasks, tags).
    pub injected: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Compaction {
    pub id: String,
    pub segment: u64,
    pub ms: Option<i64>,
    pub pre: u64,
    pub post: u64,
}

/// Running session cost as reported by the agent itself (resets per process).
#[derive(Debug, Clone, PartialEq)]
pub struct Cost {
    pub line: u64,
    pub session: String,
    pub usd: f64,
}

/// Latest-wins value, such as the size of the skills listing.
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub key: &'static str,
    pub ms: i64,
    pub value: String,
}

/// Everything parsed from one chunk of one file.
#[derive(Debug, Default)]
pub struct Batch {
    pub calls: Vec<Call>,
    pub tool_calls: Vec<ToolCall>,
    pub results: Vec<ToolResult>,
    pub prompts: Vec<Prompt>,
    pub compactions: Vec<Compaction>,
    pub costs: Vec<Cost>,
    pub counters: BTreeMap<String, i64>,
    pub snapshots: Vec<Snapshot>,
    pub lines_parsed: u64,
}

impl Batch {
    pub fn count(&mut self, key: impl Into<String>, by: i64) {
        *self.counters.entry(key.into()).or_insert(0) += by;
    }

    /// Appends the records of a later chunk of the same file.
    pub fn absorb(&mut self, later: Batch) {
        self.calls.extend(later.calls);
        self.tool_calls.extend(later.tool_calls);
        self.results.extend(later.results);
        self.prompts.extend(later.prompts);
        self.compactions.extend(later.compactions);
        self.costs.extend(later.costs);
        self.snapshots.extend(later.snapshots);
        for (key, value) in later.counters {
            self.count(key, value);
        }
        self.lines_parsed += later.lines_parsed;
    }
}

/// Parser position inside a file, persisted so the next run reads only new lines.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FileState {
    /// Size of each attachment type loaded before the first request of a session.
    pub startup_chars: BTreeMap<String, u64>,
    pub line: u64,
    pub seq: u64,
    pub segment: u64,
    pub seen_first: bool,
    pub after_compact: bool,
    pub last_ms: Option<i64>,
    pub last_model: Option<String>,
    pub last_call_id: Option<String>,
    /// Tool calls still waiting for their result: id -> tool name.
    pub pending_tools: HashMap<String, String>,
    pub session: Option<String>,
    pub project: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub is_subagent: bool,
}
