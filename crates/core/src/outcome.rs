//! Before and after: did an applied change make the project carry less?
//!
//! Compares the main Claude Code sessions of one project in the week before a
//! change with the time since it (until it was undone). The benefit is the
//! average context per request; the price is how often compaction runs.
//! Work varies from week to week, so this is evidence, not proof.

use anyhow::Result;
use rusqlite::{params, Connection};
use serde::Serialize;

use crate::apply::Change;

/// How far back "before" looks.
pub const BEFORE_MS: i64 = 7 * 86_400_000;
/// Requests needed on each side before the result counts.
pub const MIN_REQUESTS: u64 = 200;
/// Sessions needed on each side: one session grows heavier as it ages, so a
/// single session split in two would read as the change making things worse.
pub const MIN_SESSIONS: u64 = 2;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Period {
    pub requests: u64,
    pub sessions: u64,
    pub avg_context: u64,
    pub compactions_per_100: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Outcome {
    pub change_id: String,
    pub suggestion: String,
    pub before: Period,
    pub after: Period,
    /// Enough requests on both sides to read the result.
    pub conclusive: bool,
    /// (after - before) / before for the average context; negative is better.
    pub context_change: f64,
}

fn period(conn: &Connection, source: &str, project: &str, from_ms: i64, to_ms: i64) -> Result<Period> {
    let (requests, sessions, avg): (i64, i64, f64) = conn.query_row(
        "SELECT COUNT(*), COUNT(DISTINCT session), COALESCE(AVG(input + cache_read + cache_write_5m + cache_write_1h), 0)
         FROM calls WHERE source = ?1 AND project = ?2 AND is_sub = 0 AND ts_ms >= ?3 AND ts_ms < ?4",
        params![source, project, from_ms, to_ms],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    let compactions: i64 = conn.query_row(
        "SELECT COUNT(*) FROM compactions WHERE source = ?1 AND ts_ms >= ?3 AND ts_ms < ?4
           AND file IN (SELECT DISTINCT file FROM calls WHERE source = ?1 AND project = ?2 AND is_sub = 0)",
        params![source, project, from_ms, to_ms],
        |r| r.get(0),
    )?;
    let requests = requests.max(0) as u64;
    let per_100 = if requests > 0 { compactions as f64 * 100.0 / requests as f64 } else { 0.0 };
    Ok(Period { requests, sessions: sessions.max(0) as u64, avg_context: avg.max(0.0) as u64, compactions_per_100: per_100 })
}

/// Measures one change in one project (`project` is the index label, e.g. `Projects/app`).
pub fn measure(conn: &Connection, change: &Change, source: &str, project: &str, now_ms: i64) -> Result<Outcome> {
    let applied = change.applied_ms;
    let until = change.undone_ms.unwrap_or(now_ms);
    let before = period(conn, source, project, applied - BEFORE_MS, applied)?;
    let after = period(conn, source, project, applied, until)?;
    let enough = |p: &Period| p.requests >= MIN_REQUESTS && p.sessions >= MIN_SESSIONS;
    let conclusive = enough(&before) && enough(&after);
    let context_change = if before.avg_context > 0 {
        (after.avg_context as f64 - before.avg_context as f64) / before.avg_context as f64
    } else {
        0.0
    };
    Ok(Outcome { change_id: change.id.clone(), suggestion: change.suggestion.clone(), before, after, conclusive, context_change })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;

    fn change(applied_ms: i64) -> Change {
        Change {
            id: "c1".into(),
            suggestion: "compact-earlier".into(),
            file: "/p/.claude/settings.local.json".into(),
            key: "K".into(),
            before: None,
            after: "1".into(),
            backup: None,
            applied_ms,
            undone_ms: None,
            lesson: None,
            result_kept: None,
        }
    }

    #[test]
    fn compares_the_week_before_with_the_time_after() {
        let store = Store::open_in_memory().unwrap();
        let conn = store.conn();
        let insert = |id: &str, ts: i64, ctx: i64| {
            conn.execute(
                "INSERT INTO calls(id, source, file, session, project, model, is_sub, ts_ms, input, cache_read,
                   cache_write_5m, cache_write_1h, output, thinking, web_searches, seq, segment, is_first)
                 VALUES(?1, 'claude', 'f', ?4, 'Projects/app', 'm', 0, ?2, 0, ?3, 0, 0, 0, 0, 0, 1, 0, 0)",
                params![id, ts, ctx, format!("s{}", ts % 2)],
            )
            .unwrap();
        };
        let applied = 10 * 86_400_000;
        for i in 0..250 {
            insert(&format!("b{i}"), applied - 1000 - i, 600_000);
            insert(&format!("a{i}"), applied + 1000 + i, 300_000);
        }
        insert("old", applied - BEFORE_MS - 1, 999_999);
        let outcome = measure(conn, &change(applied), "claude", "Projects/app", applied + 10_000_000).unwrap();
        assert_eq!((outcome.before.requests, outcome.after.requests), (250, 250));
        assert!(outcome.conclusive);
        assert!((outcome.context_change + 0.5).abs() < 1e-9);
    }

    #[test]
    fn too_few_requests_after_is_not_conclusive() {
        let store = Store::open_in_memory().unwrap();
        let outcome = measure(store.conn(), &change(1_000), "claude", "Projects/app", 2_000).unwrap();
        assert!(!outcome.conclusive);
    }
}
