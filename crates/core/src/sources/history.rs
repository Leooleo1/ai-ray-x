//! Aggregates the agents keep themselves. They cover periods whose logs were
//! already deleted (Claude Code removes old transcripts; Codex prunes rollouts).

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};

pub struct DayTokens {
    pub date: String,
    pub model: String,
    pub tokens: u64,
}

pub struct DayActivity {
    pub date: String,
    pub messages: u64,
    pub sessions: u64,
    pub tool_calls: u64,
}

pub struct StatsCache {
    pub tokens: Vec<DayTokens>,
    pub activity: Vec<DayActivity>,
    /// All-time token totals per model, as JSON.
    pub model_totals: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawStats {
    #[serde(default)]
    daily_activity: Vec<RawActivity>,
    #[serde(default)]
    daily_model_tokens: Vec<RawDayTokens>,
    #[serde(default)]
    model_usage: BTreeMap<String, ModelTotals>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawActivity {
    date: String,
    #[serde(default)]
    message_count: u64,
    #[serde(default)]
    session_count: u64,
    #[serde(default)]
    tool_call_count: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawDayTokens {
    date: String,
    #[serde(default)]
    tokens_by_model: BTreeMap<String, u64>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ModelTotals {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
    #[serde(default)]
    cache_read_input_tokens: u64,
    #[serde(default)]
    cache_creation_input_tokens: u64,
}

/// Claude Code's `stats-cache.json` (daily activity and tokens per model).
pub fn read_stats_cache(path: &Path) -> Result<Option<StatsCache>> {
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let raw: RawStats = serde_json::from_slice(&bytes).context("parsing stats-cache.json")?;
    let tokens = raw
        .daily_model_tokens
        .into_iter()
        .flat_map(|day| {
            let date = day.date;
            day.tokens_by_model.into_iter().map(move |(model, tokens)| DayTokens { date: date.clone(), model, tokens })
        })
        .collect();
    let activity = raw
        .daily_activity
        .into_iter()
        .map(|day| DayActivity {
            date: day.date,
            messages: day.message_count,
            sessions: day.session_count,
            tool_calls: day.tool_call_count,
        })
        .collect();
    let model_totals = serde_json::to_string(&raw.model_usage)?;
    Ok(Some(StatsCache { tokens, activity, model_totals }))
}

pub struct Thread {
    pub id: String,
    pub model: String,
    pub tokens: u64,
    pub created_ms: Option<i64>,
    pub updated_ms: Option<i64>,
}

/// Codex's thread index (`state_5.sqlite`): ids, model, token totals and dates.
/// Titles and first messages are never selected.
pub fn read_codex_threads(path: &Path) -> Result<Vec<Thread>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let conn = Connection::open_with_flags(path, flags).with_context(|| format!("opening {}", path.display()))?;
    let columns: Vec<String> = conn
        .prepare("SELECT name FROM pragma_table_info('threads')")?
        .query_map([], |row| row.get(0))?
        .collect::<Result<_, _>>()?;
    let has = |name: &str| columns.iter().any(|c| c == name);
    if !has("id") || !has("tokens_used") {
        return Ok(Vec::new());
    }
    let model = if has("model") { "COALESCE(model, '?')" } else { "'?'" };
    let created = if has("created_at") { "created_at" } else { "NULL" };
    let updated = if has("updated_at") { "updated_at" } else { "NULL" };
    let sql = format!("SELECT id, {model}, tokens_used, {created}, {updated} FROM threads");
    let mut statement = conn.prepare(&sql)?;
    let threads = statement
        .query_map([], |row| {
            Ok(Thread {
                id: row.get(0)?,
                model: row.get(1)?,
                tokens: row.get::<_, i64>(2)?.max(0) as u64,
                created_ms: row.get::<_, Option<i64>>(3)?.map(|s| s * 1000),
                updated_ms: row.get::<_, Option<i64>>(4)?.map(|s| s * 1000),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(threads)
}
