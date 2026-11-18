//! Finds log files and indexes only what changed since the last run.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{Instant, UNIX_EPOCH};

use anyhow::{Context, Result};
use rayon::prelude::*;
use rusqlite::Transaction;

use crate::model::{Batch, CLAUDE, CODEX, FileState};
use crate::sources::{claude, codex, history};
use crate::store::{self, FileUpdate, Store, StoredFile};

/// Where each agent keeps its data.
#[derive(Debug, Clone)]
pub struct Roots {
    pub home: PathBuf,
    pub claude: PathBuf,
    pub codex: PathBuf,
}

impl Roots {
    /// Honors `CLAUDE_CONFIG_DIR` and `CODEX_HOME`, like the agents themselves.
    pub fn detect() -> Result<Self> {
        let home = dirs::home_dir().context("home folder not found")?;
        let from_env =
            |key: &str, fallback: &str| std::env::var_os(key).map(PathBuf::from).unwrap_or_else(|| home.join(fallback));
        let (claude, codex) = (from_env("CLAUDE_CONFIG_DIR", ".claude"), from_env("CODEX_HOME", ".codex"));
        Ok(Self { home, claude, codex })
    }
}

#[derive(Debug, Clone, Default)]
pub struct ScanStats {
    pub files: usize,
    pub changed: usize,
    pub bytes_read: u64,
    pub lines_parsed: u64,
    pub parse_ms: u128,
    pub write_ms: u128,
    pub elapsed_ms: u128,
    pub warnings: Vec<String>,
}

/// Bytes read from a log at a time; each window is decoded in parallel blocks.
const WINDOW_BYTES: usize = 64 * 1024 * 1024;

struct Job {
    path: PathBuf,
    key: String,
    source: &'static str,
    is_subagent: bool,
    size: u64,
    mtime_ms: i64,
    offset: u64,
    state: FileState,
    reset: bool,
}

struct Parsed {
    job: Job,
    offset: u64,
    batch: Batch,
}

/// Reads new lines of every log in parallel and writes them in one transaction.
pub fn refresh(store: &mut Store, roots: &Roots) -> Result<ScanStats> {
    let started = Instant::now();
    let known = store.files()?;
    let found = discover(roots);
    let mut jobs: Vec<Job> = found.iter().filter_map(|(path, source)| plan(path, source, &known)).collect();
    jobs.sort_by_key(|job| std::cmp::Reverse(job.size.saturating_sub(job.offset)));
    let parse_started = Instant::now();
    let parsed: Vec<(PathBuf, Result<Parsed>)> =
        jobs.into_par_iter().map(|job| (job.path.clone(), parse(job, &roots.home))).collect();
    let parse_ms = parse_started.elapsed().as_millis();

    let write_started = Instant::now();
    let mut stats = ScanStats { files: found.len(), parse_ms, ..ScanStats::default() };
    let tx = store.transaction()?;
    for (path, result) in parsed {
        let Parsed { job, offset, batch } = match result {
            Ok(parsed) => parsed,
            Err(error) => {
                stats.warnings.push(format!("skipped {}: {error:#}", path.display()));
                continue;
            }
        };
        if job.reset {
            store::forget_file(&tx, &job.key)?;
        }
        stats.changed += 1;
        stats.bytes_read += offset - job.offset;
        stats.lines_parsed += batch.lines_parsed;
        let update = FileUpdate {
            path: &job.key,
            source: job.source,
            size: job.size,
            mtime_ms: job.mtime_ms,
            offset,
            state: &job.state,
        };
        store::write_batch(&tx, &update, &batch)?;
    }
    stats.warnings.extend(refresh_side_sources(&tx, roots));
    tx.commit()?;
    stats.write_ms = write_started.elapsed().as_millis();
    stats.elapsed_ms = started.elapsed().as_millis();
    Ok(stats)
}

fn discover(roots: &Roots) -> Vec<(PathBuf, &'static str)> {
    let mut found = Vec::new();
    collect_jsonl(&roots.claude.join("projects"), CLAUDE, &mut found);
    for dir in ["sessions", "archived_sessions"] {
        collect_jsonl(&roots.codex.join(dir), CODEX, &mut found);
    }
    found
}

fn collect_jsonl(dir: &Path, source: &'static str, found: &mut Vec<(PathBuf, &'static str)>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        match entry.file_type() {
            Ok(kind) if kind.is_dir() => collect_jsonl(&path, source, found),
            Ok(kind) if kind.is_file() && path.extension().is_some_and(|ext| ext == "jsonl") => {
                found.push((path, source));
            }
            _ => {}
        }
    }
}

/// Decides how much of a file to read: nothing, the new tail, or all of it again.
fn plan(path: &Path, source: &'static str, known: &HashMap<String, StoredFile>) -> Option<Job> {
    let meta = fs::metadata(path).ok()?;
    let size = meta.len();
    let mtime_ms = meta
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |elapsed| elapsed.as_millis() as i64);
    let key = path.to_string_lossy().into_owned();
    let is_subagent = source == CLAUDE && path.components().any(|c| c.as_os_str() == "subagents");
    let fresh = |reset: bool| Job {
        path: path.to_path_buf(),
        key: key.clone(),
        source,
        is_subagent,
        size,
        mtime_ms,
        offset: 0,
        state: FileState::default(),
        reset,
    };
    match known.get(&key) {
        Some(stored) if stored.size == size && stored.mtime_ms == mtime_ms => None,
        Some(stored) if size >= stored.size && size >= stored.offset => {
            Some(Job { offset: stored.offset, state: stored.state.clone(), ..fresh(false) })
        }
        Some(_) => Some(fresh(true)),
        None => Some(fresh(false)),
    }
}

fn parse(mut job: Job, home: &Path) -> Result<Parsed> {
    let mut batch = Batch::default();
    let mut position = job.offset;
    if position < job.size {
        let mut file = File::open(&job.path).with_context(|| format!("opening {}", job.path.display()))?;
        let mut buffer = Vec::new();
        let mut window = WINDOW_BYTES;
        while position < job.size {
            let remaining = job.size - position;
            let want = usize::try_from(remaining).unwrap_or(usize::MAX).min(window);
            buffer.resize(want, 0);
            file.seek(SeekFrom::Start(position))?;
            file.read_exact(&mut buffer).with_context(|| format!("reading {}", job.path.display()))?;
            let Some(last_newline) = memchr::memrchr(b'\n', &buffer) else {
                if (want as u64) < remaining {
                    window = window.saturating_mul(2); // a single line longer than the window
                    continue;
                }
                break; // the last line is still being written; read it next time
            };
            let chunk = &buffer[..=last_newline];
            let part = match job.source {
                CLAUDE => claude::parse_chunk(chunk, &mut job.state, job.is_subagent, home),
                _ => codex::parse_chunk(chunk, &mut job.state, home),
            };
            batch.absorb(part);
            position += chunk.len() as u64;
            window = WINDOW_BYTES;
        }
    }
    Ok(Parsed { offset: position, batch, job })
}

/// Aggregates kept by the agents themselves. Failures become warnings.
fn refresh_side_sources(tx: &Transaction, roots: &Roots) -> Vec<String> {
    let now_ms = chrono::Utc::now().timestamp_millis();
    let mut warnings = Vec::new();
    let stats_cache = history::read_stats_cache(&roots.claude.join("stats-cache.json"))
        .and_then(|cache| cache.map_or(Ok(()), |cache| store::replace_history(tx, CLAUDE, &cache, now_ms)));
    if let Err(error) = stats_cache {
        warnings.push(format!("Claude Code stats-cache: {error:#}"));
    }
    if let Some(db) = newest_state_db(&roots.codex) {
        let threads = history::read_codex_threads(&db).and_then(|threads| store::replace_threads(tx, CODEX, &threads));
        if let Err(error) = threads {
            warnings.push(format!("Codex thread index: {error:#}"));
        }
    }
    warnings
}

/// Codex names its index `state_<version>.sqlite`; the highest version wins.
fn newest_state_db(codex: &Path) -> Option<PathBuf> {
    fs::read_dir(codex)
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let version = name.strip_prefix("state_")?.strip_suffix(".sqlite")?.parse::<u32>().ok()?;
            Some((version, entry.path()))
        })
        .max_by_key(|(version, _)| *version)
        .map(|(_, path)| path)
}
