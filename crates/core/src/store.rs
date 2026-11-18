//! Local index in SQLite. Holds only numbers and names, and is rebuilt from the
//! logs whenever the schema version changes. Rows of deleted logs are kept, so
//! the index outlives the agents' own log cleanup.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

use crate::model::{Batch, FileState};
use crate::sources::history::{StatsCache, Thread};

const SCHEMA_VERSION: &str = "1";

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS files(path TEXT PRIMARY KEY, source TEXT NOT NULL, size INTEGER NOT NULL,
  mtime_ms INTEGER NOT NULL, offset INTEGER NOT NULL, state TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS calls(id TEXT PRIMARY KEY, source TEXT NOT NULL, file TEXT NOT NULL,
  session TEXT NOT NULL, project TEXT NOT NULL, model TEXT NOT NULL, is_sub INTEGER NOT NULL, ts_ms INTEGER,
  local_date TEXT, local_hour INTEGER, weekday INTEGER, input INTEGER NOT NULL, cache_read INTEGER NOT NULL,
  cache_write_5m INTEGER NOT NULL, cache_write_1h INTEGER NOT NULL, output INTEGER NOT NULL,
  thinking INTEGER NOT NULL, web_searches INTEGER NOT NULL, effort TEXT, seq INTEGER NOT NULL,
  segment INTEGER NOT NULL, is_first INTEGER NOT NULL, rewrite_reason TEXT);
CREATE INDEX IF NOT EXISTS calls_by_segment ON calls(file, segment, seq);
CREATE INDEX IF NOT EXISTS calls_by_session ON calls(source, session, ts_ms);
CREATE TABLE IF NOT EXISTS tool_calls(id TEXT PRIMARY KEY, source TEXT NOT NULL, file TEXT NOT NULL,
  session TEXT NOT NULL, name TEXT NOT NULL, ts_ms INTEGER, skill TEXT, secret TEXT);
CREATE TABLE IF NOT EXISTS tool_results(id TEXT PRIMARY KEY, source TEXT NOT NULL, file TEXT NOT NULL,
  tool TEXT NOT NULL, segment INTEGER NOT NULL, seq_at INTEGER NOT NULL, chars INTEGER NOT NULL,
  images INTEGER NOT NULL, added INTEGER NOT NULL, removed INTEGER NOT NULL, path_hash INTEGER, ext TEXT);
CREATE TABLE IF NOT EXISTS prompts(id TEXT PRIMARY KEY, source TEXT NOT NULL, file TEXT NOT NULL,
  session TEXT NOT NULL, ts_ms INTEGER, segment INTEGER NOT NULL, seq_at INTEGER NOT NULL,
  chars INTEGER NOT NULL, images INTEGER NOT NULL, injected INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS compactions(id TEXT PRIMARY KEY, source TEXT NOT NULL, file TEXT NOT NULL,
  segment INTEGER NOT NULL, ts_ms INTEGER, pre INTEGER NOT NULL, post INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS costs(file TEXT NOT NULL, line INTEGER NOT NULL, session TEXT NOT NULL,
  usd REAL NOT NULL, PRIMARY KEY(file, line));
CREATE TABLE IF NOT EXISTS counters(file TEXT NOT NULL, source TEXT NOT NULL, key TEXT NOT NULL, value INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS snapshots(source TEXT NOT NULL, key TEXT NOT NULL, ts_ms INTEGER NOT NULL,
  value TEXT NOT NULL, PRIMARY KEY(source, key));
CREATE TABLE IF NOT EXISTS history_tokens(source TEXT NOT NULL, date TEXT NOT NULL, model TEXT NOT NULL,
  tokens INTEGER NOT NULL, PRIMARY KEY(source, date, model));
CREATE TABLE IF NOT EXISTS history_days(source TEXT NOT NULL, date TEXT NOT NULL, messages INTEGER NOT NULL,
  sessions INTEGER NOT NULL, tool_calls INTEGER NOT NULL, PRIMARY KEY(source, date));
CREATE TABLE IF NOT EXISTS threads(source TEXT NOT NULL, id TEXT NOT NULL, model TEXT NOT NULL,
  tokens INTEGER NOT NULL, created_ms INTEGER, updated_ms INTEGER, PRIMARY KEY(source, id));
";

const DATA_TABLES: [&str; 12] = [
    "files",
    "calls",
    "tool_calls",
    "tool_results",
    "prompts",
    "compactions",
    "costs",
    "counters",
    "snapshots",
    "history_tokens",
    "history_days",
    "threads",
];
const FILE_TABLES: [&str; 7] = ["calls", "tool_calls", "tool_results", "prompts", "compactions", "costs", "counters"];

/// What the index remembers about one log file.
#[derive(Debug, Clone, Default)]
pub struct StoredFile {
    pub size: u64,
    pub mtime_ms: i64,
    pub offset: u64,
    pub state: FileState,
}

/// New position of a file after parsing one more chunk of it.
pub struct FileUpdate<'a> {
    pub path: &'a str,
    pub source: &'static str,
    pub size: u64,
    pub mtime_ms: i64,
    pub offset: u64,
    pub state: &'a FileState,
}

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        let conn = Connection::open(path).with_context(|| format!("opening {}", path.display()))?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA temp_store=MEMORY;")?;
        Self::ready(conn)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::ready(Connection::open_in_memory()?)
    }

    fn ready(conn: Connection) -> Result<Self> {
        conn.execute_batch("CREATE TABLE IF NOT EXISTS meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);")?;
        let store = Self { conn };
        if store.meta("schema")?.as_deref() != Some(SCHEMA_VERSION) {
            store.reset()?;
            store.set_meta("schema", SCHEMA_VERSION)?;
        }
        store.conn.execute_batch(SCHEMA)?;
        Ok(store)
    }

    /// Drops every indexed row; the next refresh reads all logs again.
    pub fn reset(&self) -> Result<()> {
        for table in DATA_TABLES {
            self.conn.execute_batch(&format!("DROP TABLE IF EXISTS {table};"))?;
        }
        self.conn.execute_batch(SCHEMA)?;
        Ok(())
    }

    pub fn meta(&self, key: &str) -> Result<Option<String>> {
        let value = self.conn.query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0)).optional()?;
        Ok(value)
    }

    pub fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute("INSERT OR REPLACE INTO meta(key, value) VALUES(?1, ?2)", [key, value])?;
        Ok(())
    }

    pub fn files(&self) -> Result<HashMap<String, StoredFile>> {
        let mut statement = self.conn.prepare("SELECT path, size, mtime_ms, offset, state FROM files")?;
        let rows = statement.query_map([], |r| {
            let state: String = r.get(4)?;
            let stored = StoredFile {
                size: r.get::<_, i64>(1)? as u64,
                mtime_ms: r.get(2)?,
                offset: r.get::<_, i64>(3)? as u64,
                state: serde_json::from_str(&state).unwrap_or_default(),
            };
            Ok((r.get::<_, String>(0)?, stored))
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn transaction(&mut self) -> Result<Transaction<'_>> {
        Ok(self.conn.transaction()?)
    }
}

/// Clamps counts into SQLite's signed integers.
fn n(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

/// Forgets a file whose log was truncated or rewritten, before reading it again.
pub fn forget_file(tx: &Transaction, path: &str) -> Result<()> {
    for table in FILE_TABLES {
        tx.execute(&format!("DELETE FROM {table} WHERE file = ?1"), [path])?;
    }
    tx.execute("DELETE FROM files WHERE path = ?1", [path])?;
    Ok(())
}

pub fn write_batch(tx: &Transaction, file: &FileUpdate, batch: &Batch) -> Result<()> {
    write_calls(tx, file, batch)?;
    write_tools(tx, file, batch)?;
    write_context(tx, file, batch)?;
    let state = serde_json::to_string(file.state)?;
    tx.prepare_cached(
        "INSERT OR REPLACE INTO files(path, source, size, mtime_ms, offset, state) VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
    )?
    .execute(params![file.path, file.source, n(file.size), file.mtime_ms, n(file.offset), state])?;
    Ok(())
}

fn write_calls(tx: &Transaction, file: &FileUpdate, batch: &Batch) -> Result<()> {
    let mut insert = tx.prepare_cached(
        "INSERT INTO calls(id, source, file, session, project, model, is_sub, ts_ms, local_date, local_hour,
           weekday, input, cache_read, cache_write_5m, cache_write_1h, output, thinking, web_searches, effort,
           seq, segment, is_first, rewrite_reason)
         VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23)
         ON CONFLICT(id) DO UPDATE SET input = excluded.input, cache_read = excluded.cache_read,
           cache_write_5m = excluded.cache_write_5m, cache_write_1h = excluded.cache_write_1h,
           output = excluded.output, thinking = excluded.thinking, web_searches = excluded.web_searches",
    )?;
    for c in &batch.calls {
        let at = c.at.as_ref();
        insert.execute(params![
            c.id,
            c.source,
            file.path,
            c.session,
            c.project,
            c.model,
            c.is_subagent,
            at.map(|m| m.ms),
            at.map(|m| m.date.as_str()),
            at.map(|m| m.hour),
            at.map(|m| m.weekday),
            n(c.input),
            n(c.cache_read),
            n(c.cache_write_5m),
            n(c.cache_write_1h),
            n(c.output),
            n(c.thinking),
            n(c.web_searches),
            c.effort,
            n(c.seq),
            n(c.segment),
            c.is_first,
            c.rewrite_reason,
        ])?;
    }
    Ok(())
}

fn write_tools(tx: &Transaction, file: &FileUpdate, batch: &Batch) -> Result<()> {
    let mut calls = tx.prepare_cached(
        "INSERT OR IGNORE INTO tool_calls(id, source, file, session, name, ts_ms, skill, secret)
         VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )?;
    for t in &batch.tool_calls {
        let secret = t.secret.map(|kind| kind.as_str());
        calls.execute(params![t.id, file.source, file.path, t.session, t.name, t.ms, t.skill, secret])?;
    }
    let mut results = tx.prepare_cached(
        "INSERT OR IGNORE INTO tool_results(id, source, file, tool, segment, seq_at, chars, images, added, removed,
           path_hash, ext) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
    )?;
    for r in &batch.results {
        let path_hash = r.path_hash.map(|h| h as i64);
        results.execute(params![
            r.id,
            file.source,
            file.path,
            r.tool,
            n(r.segment),
            n(r.seq_at),
            n(r.chars),
            n(r.images),
            n(r.added),
            n(r.removed),
            path_hash,
            r.ext,
        ])?;
    }
    Ok(())
}

fn write_context(tx: &Transaction, file: &FileUpdate, batch: &Batch) -> Result<()> {
    let mut prompts = tx.prepare_cached(
        "INSERT OR IGNORE INTO prompts(id, source, file, session, ts_ms, segment, seq_at, chars, images, injected)
         VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
    )?;
    for p in &batch.prompts {
        prompts.execute(params![
            p.id,
            file.source,
            file.path,
            p.session,
            p.ms,
            n(p.segment),
            n(p.seq_at),
            n(p.chars),
            n(p.images),
            p.injected,
        ])?;
    }
    let mut compactions = tx.prepare_cached(
        "INSERT OR IGNORE INTO compactions(id, source, file, segment, ts_ms, pre, post) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )?;
    for c in &batch.compactions {
        compactions.execute(params![c.id, file.source, file.path, n(c.segment), c.ms, n(c.pre), n(c.post)])?;
    }
    let mut costs =
        tx.prepare_cached("INSERT OR REPLACE INTO costs(file, line, session, usd) VALUES(?1, ?2, ?3, ?4)")?;
    for c in &batch.costs {
        costs.execute(params![file.path, n(c.line), c.session, c.usd])?;
    }
    let mut counters = tx.prepare_cached("INSERT INTO counters(file, source, key, value) VALUES(?1, ?2, ?3, ?4)")?;
    for (key, value) in &batch.counters {
        counters.execute(params![file.path, file.source, key, value])?;
    }
    for s in &batch.snapshots {
        save_snapshot(tx, file.source, s.key, s.ms, &s.value)?;
    }
    Ok(())
}

/// Keeps the newest value of a snapshot key.
pub fn save_snapshot(tx: &Transaction, source: &str, key: &str, ms: i64, value: &str) -> Result<()> {
    tx.prepare_cached(
        "INSERT INTO snapshots(source, key, ts_ms, value) VALUES(?1, ?2, ?3, ?4)
         ON CONFLICT(source, key) DO UPDATE SET ts_ms = excluded.ts_ms, value = excluded.value
         WHERE excluded.ts_ms >= snapshots.ts_ms",
    )?
    .execute(params![source, key, ms, value])?;
    Ok(())
}

pub fn replace_history(tx: &Transaction, source: &str, cache: &StatsCache, now_ms: i64) -> Result<()> {
    tx.execute("DELETE FROM history_tokens WHERE source = ?1", [source])?;
    tx.execute("DELETE FROM history_days WHERE source = ?1", [source])?;
    let mut tokens =
        tx.prepare_cached("INSERT OR REPLACE INTO history_tokens(source, date, model, tokens) VALUES(?1, ?2, ?3, ?4)")?;
    for day in &cache.tokens {
        tokens.execute(params![source, day.date, day.model, n(day.tokens)])?;
    }
    let mut days = tx.prepare_cached(
        "INSERT OR REPLACE INTO history_days(source, date, messages, sessions, tool_calls) VALUES(?1, ?2, ?3, ?4, ?5)",
    )?;
    for day in &cache.activity {
        days.execute(params![source, day.date, n(day.messages), n(day.sessions), n(day.tool_calls)])?;
    }
    save_snapshot(tx, source, "model_totals", now_ms, &cache.model_totals)
}

pub fn replace_threads(tx: &Transaction, source: &str, threads: &[Thread]) -> Result<()> {
    tx.execute("DELETE FROM threads WHERE source = ?1", [source])?;
    let mut insert = tx.prepare_cached(
        "INSERT OR REPLACE INTO threads(source, id, model, tokens, created_ms, updated_ms) VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
    )?;
    for t in threads {
        insert.execute(params![source, t.id, t.model, n(t.tokens), t.created_ms, t.updated_ms])?;
    }
    Ok(())
}
