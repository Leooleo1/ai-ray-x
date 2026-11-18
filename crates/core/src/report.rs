//! Overview numbers per agent, computed from the index.

use std::collections::BTreeMap;

use anyhow::Result;
use chrono::NaiveDate;
use rusqlite::{Connection, Params, Row};
use serde::Serialize;

const ACTIVE_GAP_MS: i64 = 300_000;
const TOP_PROJECTS: usize = 12;
const TOP_TOOLS: usize = 15;

#[derive(Debug, Serialize)]
pub struct ModelUsage {
    pub model: String,
    pub calls: u64,
    pub input: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub output: u64,
    pub thinking: u64,
}

impl ModelUsage {
    pub fn total(&self) -> u64 {
        self.input + self.cache_read + self.cache_write + self.output
    }
}

#[derive(Debug, Serialize)]
pub struct Overview {
    pub source: String,
    pub models: Vec<ModelUsage>,
    pub sessions: u64,
    pub first_day: Option<String>,
    pub last_day: Option<String>,
    pub active_days: u64,
    pub longest_streak: u64,
    pub busiest_day: Option<(String, u64)>,
    pub active_hours: f64,
    pub by_hour: [u64; 24],
    pub by_weekday: [u64; 7],
    pub projects: Vec<(String, u64)>,
    pub tools: Vec<(String, u64)>,
    pub prompts: u64,
    pub lines_added: u64,
    pub lines_removed: u64,
    pub files_edited: u64,
    pub subagent_calls: u64,
    pub compactions: u64,
    pub counters: BTreeMap<String, i64>,
    /// Sum of the cost the agent itself reported, adding up each process run.
    pub reported_cost_usd: Option<f64>,
}

pub(crate) fn rows<T, P: Params>(
    conn: &Connection,
    sql: &str,
    params: P,
    map: impl FnMut(&Row) -> rusqlite::Result<T>,
) -> Result<Vec<T>> {
    let mut statement = conn.prepare(sql)?;
    let collected = statement.query_map(params, map)?.collect::<rusqlite::Result<Vec<T>>>()?;
    Ok(collected)
}

pub(crate) fn single<P: Params>(conn: &Connection, sql: &str, params: P) -> Result<u64> {
    let value: Option<i64> = conn.query_row(sql, params, |r| r.get(0))?;
    Ok(value.unwrap_or(0).max(0) as u64)
}

fn pairs(conn: &Connection, sql: &str, source: &str) -> Result<Vec<(String, u64)>> {
    rows(conn, sql, [source], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?.max(0) as u64)))
}

pub fn overview(conn: &Connection, source: &str) -> Result<Overview> {
    let models = rows(
        conn,
        "SELECT model, COUNT(*), SUM(input), SUM(cache_read), SUM(cache_write_5m + cache_write_1h), SUM(output), SUM(thinking)
         FROM calls WHERE source = ?1 GROUP BY model ORDER BY SUM(output) DESC",
        [source],
        |r| {
            Ok(ModelUsage {
                model: r.get(0)?,
                calls: r.get::<_, i64>(1)? as u64,
                input: r.get::<_, i64>(2)? as u64,
                cache_read: r.get::<_, i64>(3)? as u64,
                cache_write: r.get::<_, i64>(4)? as u64,
                output: r.get::<_, i64>(5)? as u64,
                thinking: r.get::<_, i64>(6)? as u64,
            })
        },
    )?;
    let days = pairs(
        conn,
        "SELECT local_date, COUNT(*) FROM calls WHERE source = ?1 AND local_date IS NOT NULL GROUP BY local_date ORDER BY local_date",
        source,
    )?;
    let (lines_added, lines_removed, files_edited) = conn.query_row(
        "SELECT COALESCE(SUM(added), 0), COALESCE(SUM(removed), 0), COUNT(DISTINCT path_hash)
         FROM tool_results WHERE source = ?1 AND (added > 0 OR removed > 0)",
        [source],
        |r| Ok((r.get::<_, i64>(0)? as u64, r.get::<_, i64>(1)? as u64, r.get::<_, i64>(2)? as u64)),
    )?;
    let counters = rows(conn, "SELECT key, SUM(value) FROM counters WHERE source = ?1 GROUP BY key", [source], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
    })?;
    Ok(Overview {
        source: source.to_string(),
        models,
        sessions: single(conn, "SELECT COUNT(DISTINCT session) FROM calls WHERE source = ?1", [source])?,
        first_day: days.first().map(|(d, _)| d.clone()),
        last_day: days.last().map(|(d, _)| d.clone()),
        active_days: days.len() as u64,
        longest_streak: longest_streak(&days),
        busiest_day: days.iter().max_by_key(|(_, calls)| *calls).cloned(),
        active_hours: active_hours(conn, source)?,
        by_hour: histogram(conn, "local_hour", source)?,
        by_weekday: histogram(conn, "weekday", source)?,
        projects: top(
            pairs(
                conn,
                "SELECT project, SUM(input + cache_read + cache_write_5m + cache_write_1h + output) AS t
             FROM calls WHERE source = ?1 GROUP BY project ORDER BY t DESC",
                source,
            )?,
            TOP_PROJECTS,
        ),
        tools: top(
            pairs(
                conn,
                "SELECT name, COUNT(*) AS c FROM tool_calls WHERE source = ?1 GROUP BY name ORDER BY c DESC",
                source,
            )?,
            TOP_TOOLS,
        ),
        prompts: single(conn, "SELECT COUNT(*) FROM prompts WHERE source = ?1 AND injected = 0", [source])?,
        lines_added,
        lines_removed,
        files_edited,
        subagent_calls: single(conn, "SELECT COUNT(*) FROM calls WHERE source = ?1 AND is_sub = 1", [source])?,
        compactions: single(conn, "SELECT COUNT(*) FROM compactions WHERE source = ?1", [source])?,
        counters: counters.into_iter().collect(),
        reported_cost_usd: reported_cost(conn, source)?,
    })
}

/// Tool calls that touched secrets files, by how they touched them. Counts only.
pub fn secret_touches(conn: &Connection, source: &str) -> Result<Vec<(String, u64)>> {
    let mut found = pairs(
        conn,
        "SELECT secret, COUNT(*) FROM tool_calls WHERE source = ?1 AND secret IS NOT NULL GROUP BY secret",
        source,
    )?;
    found.extend(pairs(
        conn,
        "SELECT substr(key, 8), SUM(value) FROM counters WHERE source = ?1 AND key LIKE 'secret:%' GROUP BY key",
        source,
    )?);
    found.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    Ok(found)
}

fn top(items: Vec<(String, u64)>, limit: usize) -> Vec<(String, u64)> {
    items.into_iter().take(limit).collect()
}

fn histogram<const N: usize>(conn: &Connection, column: &str, source: &str) -> Result<[u64; N]> {
    let sql =
        format!("SELECT {column}, COUNT(*) FROM calls WHERE source = ?1 AND {column} IS NOT NULL GROUP BY {column}");
    let counts = rows(conn, &sql, [source], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?;
    Ok(std::array::from_fn(|slot| {
        counts.iter().find(|(key, _)| *key == slot as i64).map_or(0, |(_, count)| *count as u64)
    }))
}

/// Time between consecutive requests of a session, ignoring gaps over five minutes.
fn active_hours(conn: &Connection, source: &str) -> Result<f64> {
    let ms = single(
        conn,
        &format!(
            "SELECT SUM(gap) FROM (SELECT ts_ms - LAG(ts_ms) OVER (PARTITION BY session ORDER BY ts_ms) AS gap
             FROM calls WHERE source = ?1 AND ts_ms IS NOT NULL) WHERE gap <= {ACTIVE_GAP_MS}"
        ),
        [source],
    )?;
    Ok(ms as f64 / 3_600_000.0)
}

fn longest_streak(days: &[(String, u64)]) -> u64 {
    let dates: Vec<NaiveDate> = days.iter().filter_map(|(d, _)| d.parse().ok()).collect();
    let (best, _) = dates.windows(2).fold((u64::from(!dates.is_empty()), 1u64), |(best, run), pair| {
        let run = if (pair[1] - pair[0]).num_days() == 1 { run + 1 } else { 1 };
        (best.max(run), run)
    });
    best
}

/// The running cost restarts with every process, so each run's peak is added up.
fn reported_cost(conn: &Connection, source: &str) -> Result<Option<f64>> {
    let points = rows(
        conn,
        "SELECT costs.session, costs.usd FROM costs JOIN files ON files.path = costs.file
         WHERE files.source = ?1 ORDER BY costs.file, costs.line",
        [source],
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, f64>(1)?)),
    )?;
    if points.is_empty() {
        return Ok(None);
    }
    let mut runs: BTreeMap<&str, (f64, f64, f64)> = BTreeMap::new();
    for (session, usd) in &points {
        let (total, peak, last) = runs.get(session.as_str()).copied().unwrap_or((0.0, 0.0, 0.0));
        let next = if *usd < last { (total + peak, *usd, *usd) } else { (total, peak.max(*usd), *usd) };
        runs.insert(session, next);
    }
    Ok(Some(runs.values().map(|(total, peak, _)| total + peak).sum()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streak_counts_consecutive_days() {
        let days: Vec<(String, u64)> =
            ["2026-11-01", "2026-11-02", "2026-11-03", "2026-11-05"].iter().map(|d| (d.to_string(), 1)).collect();
        assert_eq!(longest_streak(&days), 3);
        assert_eq!(longest_streak(&[]), 0);
    }
}
