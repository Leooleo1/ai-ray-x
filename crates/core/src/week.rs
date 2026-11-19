//! A project's week, for Raio's weekly report: the tokens of the last seven
//! days against the seven before, and what weighed most, the tool whose
//! outputs were re-read the most in this week's sessions.

use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

const DAY_MS: i64 = 86_400_000;
const CHARS_PER_TOKEN: f64 = 3.6;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Week {
    /// Tokens read and written in the last seven days.
    pub this: u64,
    /// The seven days before.
    pub last: u64,
    pub top: Option<Weight>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Weight {
    /// The tool whose outputs the sessions re-read the most ("Bash", "Read", "exec"…).
    pub tool: String,
    /// Tokens re-read because those outputs stayed in the context.
    pub tokens: u64,
}

pub fn week(conn: &Connection, project: &str, now_ms: i64) -> Result<Week> {
    let (start, before) = (now_ms - 7 * DAY_MS, now_ms - 14 * DAY_MS);
    let (this, last): (i64, i64) = conn.query_row(
        "SELECT COALESCE(SUM(CASE WHEN ts_ms >= ?2 THEN t END), 0), COALESCE(SUM(CASE WHEN ts_ms < ?2 THEN t END), 0)
         FROM (SELECT ts_ms, input + cache_read + cache_write_5m + cache_write_1h + output AS t
               FROM calls WHERE project = ?1 AND ts_ms >= ?3 AND ts_ms <= ?4)",
        params![project, start, before, now_ms],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    // Outputs that arrived this week: each one's characters times the requests that re-read it before the next compaction.
    let top: Option<(String, i64)> = conn
        .query_row(
            "WITH last AS (SELECT file, segment, MAX(seq) AS last_seq FROM calls WHERE project = ?1 GROUP BY file, segment)
             SELECT r.tool, SUM(r.chars * MAX(COALESCE(l.last_seq, r.seq_at) - r.seq_at, 0)) AS weight
             FROM tool_results r
             JOIN calls c ON c.file = r.file AND c.segment = r.segment AND c.seq = r.seq_at AND c.project = ?1 AND c.ts_ms >= ?2
             LEFT JOIN last l ON l.file = r.file AND l.segment = r.segment
             GROUP BY r.tool ORDER BY weight DESC LIMIT 1",
            params![project, start],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    Ok(Week {
        this: this.max(0) as u64,
        last: last.max(0) as u64,
        top: top.filter(|(_, weight)| *weight > 0).map(|(tool, weight)| Weight { tool, tokens: (weight as f64 / CHARS_PER_TOKEN) as u64 }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(store: &crate::Store, id: &str, project: &str, file: &str, ts_ms: i64, seq: i64, tokens: i64) {
        store
            .conn()
            .execute(
                "INSERT INTO calls(id, source, file, session, project, model, is_sub, ts_ms, input, cache_read, cache_write_5m,
                 cache_write_1h, output, thinking, web_searches, seq, segment, is_first) VALUES (?1, 'claude', ?2, 's', ?3, 'm', 0, ?4, ?5, 0, 0, 0, 0, 0, 0, ?6, 0, 0)",
                params![id, file, project, ts_ms, tokens, seq],
            )
            .unwrap();
    }

    #[test]
    fn this_week_against_the_last_and_the_heaviest_tool() {
        let store = crate::Store::open_in_memory().unwrap();
        let now = 100 * DAY_MS;
        call(&store, "a", "Projects/app", "f1", now - DAY_MS, 1, 600);
        call(&store, "b", "Projects/app", "f1", now - DAY_MS + 1, 5, 400);
        call(&store, "c", "Projects/app", "f0", now - 10 * DAY_MS, 1, 500);
        call(&store, "d", "Projects/app", "f9", now - 30 * DAY_MS, 1, 9_000);
        call(&store, "e", "Projects/other", "f2", now - DAY_MS, 1, 7_000);
        let output = |id: &str, tool: &str, chars: i64| {
            store
                .conn()
                .execute(
                    "INSERT INTO tool_results(id, source, file, tool, segment, seq_at, chars, images, added, removed) VALUES (?1, 'claude', 'f1', ?2, 0, 1, ?3, 0, 0, 0)",
                    params![id, tool, chars],
                )
                .unwrap();
        };
        output("r1", "Bash", 3600);
        output("r2", "Read", 360);
        let week = week(store.conn(), "Projects/app", now).unwrap();
        assert_eq!((week.this, week.last), (1000, 500));
        // 3600 characters re-read by the 4 requests after it: 4000 tokens.
        assert_eq!(week.top, Some(Weight { tool: "Bash".into(), tokens: 4000 }));
    }
}
