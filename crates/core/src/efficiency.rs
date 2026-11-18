//! Where the tokens go: context size per request, fixed overhead, what stays
//! loaded, cache rewrites and what earlier compaction would save.

use std::collections::BTreeMap;

use anyhow::Result;
use rusqlite::Connection;
use serde::Serialize;

use crate::report::{rows, single};

/// Prices relative to fresh input: cache read 0.1x, 5-minute cache write 1.25x,
/// 1-hour cache write 2x, output 5x.
const WEIGHTS: [(&str, f64); 5] =
    [("input", 1.0), ("cache_read", 0.1), ("cache_write_5m", 1.25), ("cache_write_1h", 2.0), ("output", 5.0)];
/// Rough size of a token in characters of mixed code, prose and JSON.
const CHARS_PER_TOKEN: f64 = 3.6;
/// Rough cost of one screenshot-sized image.
const IMAGE_TOKENS: f64 = 1_500.0;
const BUCKET_EDGES: [u64; 6] = [50_000, 100_000, 200_000, 400_000, 600_000, 800_000];
pub const COMPACTION_CAPS: [u64; 3] = [200_000, 300_000, 400_000];
const DEFAULT_POST_COMPACTION: u64 = 40_000;

#[derive(Debug, Serialize)]
pub struct Distribution {
    pub calls: u64,
    pub total: u64,
    pub mean: f64,
    pub median: u64,
    pub p90: u64,
}

#[derive(Debug, Serialize)]
pub struct Bucket {
    pub up_to: Option<u64>,
    pub share_of_calls: f64,
    pub share_of_tokens: f64,
}

#[derive(Debug, Serialize)]
pub struct FixedOverhead {
    pub kind: &'static str,
    pub median_first_request: u64,
    pub files: u64,
    pub calls: u64,
    pub share_of_context: f64,
}

#[derive(Debug, Serialize)]
pub struct Rewrite {
    pub reason: String,
    pub count: u64,
    pub tokens: u64,
}

#[derive(Debug, Serialize)]
pub struct Carried {
    pub name: String,
    pub results: u64,
    pub avg_tokens: f64,
    pub max_tokens: f64,
    pub images: u64,
    /// Estimated tokens re-read because this stayed in the context.
    pub carried_tokens: f64,
    pub share_of_context: f64,
}

#[derive(Debug, Serialize)]
pub struct Efficiency {
    pub source: String,
    pub cost_shares: Vec<(String, f64)>,
    pub context: Distribution,
    pub buckets: Vec<Bucket>,
    pub fixed: Vec<FixedOverhead>,
    pub rewrites: Vec<Rewrite>,
    pub compactions: u64,
    pub compaction_pre_median: u64,
    pub compaction_post_median: u64,
    /// Share of context tokens saved if compaction happened at each cap (upper bound).
    pub simulation: Vec<(u64, f64)>,
    pub carried: Vec<Carried>,
    pub skills_used: Vec<(String, u64)>,
    pub snapshots: BTreeMap<String, serde_json::Value>,
    pub threads_by_model: Vec<(String, u64)>,
    /// Tokens re-read that a shell output limit would have cut: (tool, limit in characters, tokens).
    pub output_trim: Vec<(String, u64, f64)>,
}

pub fn efficiency(conn: &Connection, source: &str) -> Result<Efficiency> {
    let contexts = rows(
        conn,
        "SELECT input + cache_read + cache_write_5m + cache_write_1h, is_sub FROM calls WHERE source = ?1",
        [source],
        |r| Ok((r.get::<_, i64>(0)?.max(0) as u64, r.get::<_, bool>(1)?)),
    )?;
    let mut sizes: Vec<u64> = contexts.iter().map(|(ctx, _)| *ctx).collect();
    sizes.sort_unstable();
    let context = distribution(&sizes);
    let (pre, post) = compaction_medians(conn, source)?;
    Ok(Efficiency {
        source: source.to_string(),
        cost_shares: cost_shares(conn, source)?,
        buckets: buckets(&sizes, context.total),
        fixed: fixed_overhead(conn, source, &contexts, context.total)?,
        rewrites: rows(
            conn,
            "SELECT rewrite_reason, COUNT(*), SUM(cache_write_5m + cache_write_1h) FROM calls
             WHERE source = ?1 AND rewrite_reason IS NOT NULL GROUP BY 1 ORDER BY 3 DESC",
            [source],
            |r| {
                Ok(Rewrite { reason: r.get(0)?, count: r.get::<_, i64>(1)? as u64, tokens: r.get::<_, i64>(2)? as u64 })
            },
        )?,
        compactions: single(conn, "SELECT COUNT(*) FROM compactions WHERE source = ?1", [source])?,
        compaction_pre_median: pre,
        compaction_post_median: post,
        simulation: COMPACTION_CAPS
            .iter()
            .map(|cap| (*cap, simulate(&sizes, *cap, post.max(1).min(cap / 2))))
            .collect(),
        carried: carried(conn, source, context.total)?,
        skills_used: rows(
            conn,
            "SELECT skill, COUNT(*) AS c FROM tool_calls WHERE source = ?1 AND skill IS NOT NULL GROUP BY skill ORDER BY c DESC",
            [source],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64)),
        )?,
        snapshots: snapshots(conn, source)?,
        threads_by_model: rows(
            conn,
            "SELECT model, SUM(tokens) AS t FROM threads WHERE source = ?1 GROUP BY model ORDER BY t DESC",
            [source],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64)),
        )?,
        output_trim: output_trim(conn, source)?,
        context,
    })
}

/// Shell output limits worth simulating, in characters.
pub const OUTPUT_LIMITS: [u64; 2] = [15_000, 10_000];

/// Exact for the indexed logs: each output's characters above the limit, times
/// the requests that re-read it before the next compaction.
fn output_trim(conn: &Connection, source: &str) -> Result<Vec<(String, u64, f64)>> {
    let tool = if source == "claude" { "Bash" } else { "exec" };
    let sql = "WITH last AS (SELECT file, segment, MAX(seq) AS last_seq FROM calls WHERE source = ?1 GROUP BY file, segment)
        SELECT COALESCE(SUM(MAX(r.chars - ?3, 0) * MAX(COALESCE(l.last_seq, r.seq_at) - r.seq_at, 0)), 0)
        FROM tool_results r LEFT JOIN last l ON l.file = r.file AND l.segment = r.segment
        WHERE r.source = ?1 AND r.tool = ?2";
    OUTPUT_LIMITS
        .iter()
        .map(|limit| {
            let chars: i64 = conn.query_row(sql, rusqlite::params![source, tool, *limit as i64], |r| r.get(0))?;
            Ok((tool.to_string(), *limit, chars.max(0) as f64 / CHARS_PER_TOKEN))
        })
        .collect()
}

fn distribution(sorted: &[u64]) -> Distribution {
    let total: u64 = sorted.iter().sum();
    let at = |q: f64| sorted.get(((sorted.len() as f64) * q) as usize).copied().unwrap_or(0);
    Distribution {
        calls: sorted.len() as u64,
        total,
        mean: if sorted.is_empty() { 0.0 } else { total as f64 / sorted.len() as f64 },
        median: at(0.5),
        p90: at(0.9),
    }
}

fn buckets(sorted: &[u64], total: u64) -> Vec<Bucket> {
    let ratio = |part: f64, whole: f64| if whole > 0.0 { part / whole } else { 0.0 };
    let edges: Vec<Option<u64>> = BUCKET_EDGES.iter().copied().map(Some).chain([None]).collect();
    let mut lower = 0;
    edges
        .into_iter()
        .map(|upper| {
            let inside: Vec<u64> =
                sorted.iter().copied().filter(|v| *v >= lower && upper.is_none_or(|u| *v < u)).collect();
            lower = upper.unwrap_or(u64::MAX);
            Bucket {
                up_to: upper,
                share_of_calls: ratio(inside.len() as f64, sorted.len() as f64),
                share_of_tokens: ratio(inside.iter().sum::<u64>() as f64, total as f64),
            }
        })
        .collect()
}

fn cost_shares(conn: &Connection, source: &str) -> Result<Vec<(String, f64)>> {
    let sums = conn.query_row(
        "SELECT SUM(input), SUM(cache_read), SUM(cache_write_5m), SUM(cache_write_1h), SUM(output) FROM calls WHERE source = ?1",
        [source],
        |r| (0..5).map(|i| Ok(r.get::<_, Option<i64>>(i)?.unwrap_or(0) as f64)).collect::<rusqlite::Result<Vec<f64>>>(),
    )?;
    let weighted: Vec<(String, f64)> =
        WEIGHTS.iter().zip(&sums).map(|((name, w), sum)| (name.to_string(), w * sum)).collect();
    let total: f64 = weighted.iter().map(|(_, v)| v).sum();
    Ok(weighted.into_iter().map(|(name, v)| (name, if total > 0.0 { v / total } else { 0.0 })).collect())
}

/// Size of the first request of each log: what is loaded before any work.
fn fixed_overhead(conn: &Connection, source: &str, contexts: &[(u64, bool)], total: u64) -> Result<Vec<FixedOverhead>> {
    let firsts = rows(
        conn,
        "SELECT input + cache_read + cache_write_5m + cache_write_1h, is_sub FROM calls WHERE source = ?1 AND is_first = 1",
        [source],
        |r| Ok((r.get::<_, i64>(0)?.max(0) as u64, r.get::<_, bool>(1)?)),
    )?;
    let result = [("main", false), ("subagent", true)]
        .into_iter()
        .filter_map(|(kind, is_sub)| {
            let mut sizes: Vec<u64> = firsts.iter().filter(|(_, s)| *s == is_sub).map(|(c, _)| *c).collect();
            sizes.sort_unstable();
            let median = *sizes.get(sizes.len() / 2)?;
            let calls = contexts.iter().filter(|(_, s)| *s == is_sub).count() as u64;
            let share = if total > 0 { (median * calls) as f64 / total as f64 } else { 0.0 };
            Some(FixedOverhead {
                kind,
                median_first_request: median,
                files: sizes.len() as u64,
                calls,
                share_of_context: share,
            })
        })
        .collect();
    Ok(result)
}

fn compaction_medians(conn: &Connection, source: &str) -> Result<(u64, u64)> {
    let pairs = rows(conn, "SELECT pre, post FROM compactions WHERE source = ?1 AND pre > 0", [source], |r| {
        Ok((r.get::<_, i64>(0)? as u64, r.get::<_, i64>(1)? as u64))
    })?;
    let median = |mut values: Vec<u64>| {
        values.sort_unstable();
        values.get(values.len() / 2).copied()
    };
    let pre = median(pairs.iter().map(|(p, _)| *p).collect()).unwrap_or(0);
    let post = median(pairs.iter().map(|(_, p)| *p).collect()).unwrap_or(DEFAULT_POST_COMPACTION);
    Ok((pre, post))
}

/// Context re-read if compaction happened at `cap` and shrank it to `post`.
/// Upper bound: it ignores files the agent would read again after compacting.
fn simulate(sizes: &[u64], cap: u64, post: u64) -> f64 {
    let total: u64 = sizes.iter().sum();
    let simulated: u64 =
        sizes.iter().map(|ctx| if *ctx <= cap { *ctx } else { post + (ctx - cap) % (cap - post) }).sum();
    if total == 0 { 0.0 } else { 1.0 - simulated as f64 / total as f64 }
}

/// What stayed in the context, weighted by how many later requests re-read it.
fn carried(conn: &Connection, source: &str, total: u64) -> Result<Vec<Carried>> {
    let remaining =
        "WITH last AS (SELECT file, segment, MAX(seq) AS last_seq FROM calls WHERE source = ?1 GROUP BY file, segment)";
    let tools = format!(
        "{remaining} SELECT r.tool, COUNT(*), SUM(r.chars), MAX(r.chars), SUM(r.images),
           SUM(r.chars * MAX(COALESCE(l.last_seq, r.seq_at) - r.seq_at, 0)),
           SUM(r.images * MAX(COALESCE(l.last_seq, r.seq_at) - r.seq_at, 0))
         FROM tool_results r LEFT JOIN last l ON l.file = r.file AND l.segment = r.segment
         WHERE r.source = ?1 GROUP BY r.tool"
    );
    let prompts = format!(
        "{remaining} SELECT CASE p.injected WHEN 1 THEN 'injected text' ELSE 'your prompts' END, COUNT(*),
           SUM(p.chars), MAX(p.chars), SUM(p.images),
           SUM(p.chars * MAX(COALESCE(l.last_seq, p.seq_at) - p.seq_at, 0)),
           SUM(p.images * MAX(COALESCE(l.last_seq, p.seq_at) - p.seq_at, 0))
         FROM prompts p LEFT JOIN last l ON l.file = p.file AND l.segment = p.segment
         WHERE p.source = ?1 GROUP BY p.injected"
    );
    let mut all = rows(conn, &tools, [source], |r| carried_row(r, total))?;
    all.extend(rows(conn, &prompts, [source], |r| carried_row(r, total))?);
    all.sort_by(|a, b| b.carried_tokens.total_cmp(&a.carried_tokens));
    Ok(all)
}

fn carried_row(r: &rusqlite::Row, total: u64) -> rusqlite::Result<Carried> {
    let get = |i: usize| r.get::<_, Option<i64>>(i).map(|v| v.unwrap_or(0).max(0) as f64);
    let (results, chars, max_chars, images) = (get(1)?, get(2)?, get(3)?, get(4)?);
    let carried_tokens = get(5)? / CHARS_PER_TOKEN + get(6)? * IMAGE_TOKENS;
    Ok(Carried {
        name: r.get(0)?,
        results: results as u64,
        avg_tokens: if results > 0.0 { chars / CHARS_PER_TOKEN / results } else { 0.0 },
        max_tokens: max_chars / CHARS_PER_TOKEN,
        images: images as u64,
        carried_tokens,
        share_of_context: if total > 0 { carried_tokens / total as f64 } else { 0.0 },
    })
}

fn snapshots(conn: &Connection, source: &str) -> Result<BTreeMap<String, serde_json::Value>> {
    let pairs = rows(conn, "SELECT key, value FROM snapshots WHERE source = ?1", [source], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })?;
    Ok(pairs
        .into_iter()
        .map(|(key, value)| {
            let parsed = serde_json::from_str(&value).unwrap_or(serde_json::Value::String(value));
            (key, parsed)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simulation_saves_nothing_below_the_cap() {
        assert_eq!(simulate(&[100, 200], 300, 40), 0.0);
    }

    #[test]
    fn simulation_folds_context_above_the_cap() {
        // 500 with cap 300 and post 100: 100 + (500 - 300) % 200 = 100.
        let saved = simulate(&[500], 300, 100);
        assert!((saved - 0.8).abs() < 1e-9);
    }

    #[test]
    fn buckets_split_calls_and_tokens() {
        let sizes = [10_000, 90_000, 900_000];
        let result = buckets(&sizes, sizes.iter().sum());
        assert_eq!(result.len(), BUCKET_EDGES.len() + 1);
        assert!((result[0].share_of_calls - 1.0 / 3.0).abs() < 1e-9);
        assert!(result.last().unwrap().share_of_tokens > 0.9);
    }
}
