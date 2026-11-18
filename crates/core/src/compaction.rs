//! What compacting earlier would really save, from the person's own sessions.
//!
//! Each log is replayed as a work trajectory: the real growth of the context
//! from one request to the next (the drops of real compactions removed). A
//! simulated compaction at `cap` is charged in full:
//! - the summarizing request reads the whole context once;
//! - the context restarts at the measured floor (startup + summary), not at
//!   the summary size alone;
//! - plus the measured refill: what agents read again in the requests right
//!   after a compaction, above their normal pace.
//!
//! The result is a net estimate. It cannot see quality: earlier summaries
//! drop older detail sooner.

use anyhow::Result;
use rusqlite::Connection;
use serde::Serialize;

use crate::report::rows;

pub const CAPS: [u64; 5] = [200_000, 300_000, 400_000, 500_000, 600_000];
/// Requests compared on each side of a real compaction to measure the refill.
const REFILL_WINDOW: usize = 20;
/// Quality guard: at most one summary per 100 requests, so the agent does not
/// lose older detail every few dozen requests just to save tokens.
pub const MAX_COMPACTIONS_PER_100: f64 = 1.0;
/// Used when the logs have no compaction to measure the floor from.
const FALLBACK_FLOOR: u64 = 120_000;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Plan {
    pub cap: u64,
    /// Share of today's re-read tokens (context plus summarizing reads) saved.
    pub net_saving: f64,
    pub avg_context: u64,
    pub compactions_per_100: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Model {
    /// Context of the first request after a real compaction (median).
    pub floor: u64,
    /// Extra tokens read again after a compaction, over the normal pace (median).
    pub refill: u64,
    pub measured_compactions: u64,
    pub now_avg_context: u64,
    pub now_compactions_per_100: f64,
    /// The replay with today's compaction point, to check it matches reality.
    pub calibration: f64,
    pub plans: Vec<Plan>,
    pub best: Option<Plan>,
}

struct Segment {
    file: String,
    contexts: Vec<u64>,
}

fn load(conn: &Connection, source: &str) -> Result<Vec<Segment>> {
    let flat = rows(
        conn,
        "SELECT file, segment, input + cache_read + cache_write_5m + cache_write_1h FROM calls
         WHERE source = ?1 ORDER BY file, segment, seq",
        [source],
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?.max(0) as u64)),
    )?;
    let mut segments: Vec<Segment> = Vec::new();
    let mut key: Option<(String, i64)> = None;
    for (file, segment, ctx) in flat {
        if key.as_ref() != Some(&(file.clone(), segment)) {
            segments.push(Segment { file: file.clone(), contexts: Vec::new() });
            key = Some((file, segment));
        }
        if let Some(last) = segments.last_mut() {
            last.contexts.push(ctx);
        }
    }
    Ok(segments)
}

fn median(mut values: Vec<u64>) -> Option<u64> {
    values.sort_unstable();
    values.get(values.len() / 2).copied()
}

/// Floor and refill, from the pairs of segments split by a real compaction.
fn measure_restart(segments: &[Segment]) -> (u64, u64, u64) {
    let pairs: Vec<(&Segment, &Segment)> = segments.windows(2).filter(|w| w[0].file == w[1].file).map(|w| (&w[0], &w[1])).collect();
    let floor = median(pairs.iter().filter_map(|(_, after)| after.contexts.first().copied()).collect()).unwrap_or(FALLBACK_FLOOR);
    let k = REFILL_WINDOW;
    let refills: Vec<u64> = pairs
        .iter()
        .filter(|(before, after)| before.contexts.len() > k && after.contexts.len() > k)
        .map(|(before, after)| {
            let pace_before = before.contexts[before.contexts.len() - 1] as i64 - before.contexts[before.contexts.len() - 1 - k] as i64;
            let pace_after = after.contexts[k] as i64 - after.contexts[0] as i64;
            (pace_after - pace_before).max(0) as u64
        })
        .collect();
    (floor, median(refills).unwrap_or(0), pairs.len() as u64)
}

struct Replay {
    reread: f64,
    requests: u64,
    compactions: u64,
}

/// Replays every log with compaction at `cap`, from the real growth.
fn replay(segments: &[Segment], cap: u64, floor: u64, refill: u64) -> Replay {
    let restart = (floor + refill).min(cap / 2) as f64;
    let mut total = Replay { reread: 0.0, requests: 0, compactions: 0 };
    let mut i = 0;
    while i < segments.len() {
        let file = &segments[i].file;
        let mut level: Option<f64> = None;
        let mut previous: Option<u64> = None;
        while i < segments.len() && &segments[i].file == file {
            for (n, &ctx) in segments[i].contexts.iter().enumerate() {
                // Growth inside a segment is real work; the drop at a real compaction is not.
                let growth = match (previous, n) {
                    (Some(prev), n) if n > 0 => ctx as f64 - prev as f64,
                    _ => 0.0,
                };
                previous = Some(ctx);
                let next = level.map_or(ctx as f64, |l| (l + growth).max(0.0));
                let next = if next > cap as f64 {
                    total.compactions += 1;
                    total.reread += next; // the summarizing request reads it all once
                    restart
                } else {
                    next
                };
                total.reread += next;
                total.requests += 1;
                level = Some(next);
            }
            i += 1;
        }
    }
    total
}

pub fn model(conn: &Connection, source: &str) -> Result<Model> {
    let segments = load(conn, source)?;
    let (floor, refill, measured) = measure_restart(&segments);
    let requests: u64 = segments.iter().map(|s| s.contexts.len() as u64).sum();
    let context_sum: f64 = segments.iter().flat_map(|s| s.contexts.iter()).map(|c| *c as f64).sum();
    // Today: every request's context plus each real compaction's summarizing read.
    let summarizing: f64 =
        segments.windows(2).filter(|w| w[0].file == w[1].file).filter_map(|w| w[0].contexts.last()).map(|c| *c as f64).sum();
    let today = context_sum + summarizing;
    let per_100 = |compactions: u64, requests: u64| if requests > 0 { compactions as f64 * 100.0 / requests as f64 } else { 0.0 };
    let pre = median(segments.windows(2).filter(|w| w[0].file == w[1].file).filter_map(|w| w[0].contexts.last().copied()).collect());
    let calibration = pre.map_or(1.0, |cap| replay(&segments, cap, floor, refill).reread / today.max(1.0));
    let plans: Vec<Plan> = CAPS
        .iter()
        .map(|&cap| {
            let run = replay(&segments, cap, floor, refill);
            Plan {
                cap,
                net_saving: if today > 0.0 { 1.0 - run.reread / today } else { 0.0 },
                avg_context: if run.requests > 0 { (run.reread / run.requests as f64) as u64 } else { 0 },
                compactions_per_100: per_100(run.compactions, run.requests),
            }
        })
        .collect();
    let best = plans
        .iter()
        .filter(|p| p.net_saving > 0.0 && p.compactions_per_100 <= MAX_COMPACTIONS_PER_100 + 1e-9)
        .max_by(|a, b| a.net_saving.total_cmp(&b.net_saving))
        .cloned();
    Ok(Model {
        floor,
        refill,
        measured_compactions: measured,
        now_avg_context: if requests > 0 { (context_sum / requests as f64) as u64 } else { 0 },
        now_compactions_per_100: per_100(measured, requests),
        calibration,
        plans,
        best,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segment(file: &str, contexts: &[u64]) -> Segment {
        Segment { file: file.into(), contexts: contexts.to_vec() }
    }

    #[test]
    fn a_cap_above_every_context_changes_nothing() {
        let segs = [segment("f", &[100, 200, 300])];
        let run = replay(&segs, 1_000, 50, 0);
        assert_eq!((run.reread as u64, run.compactions), (600, 0));
    }

    #[test]
    fn each_compaction_pays_the_summarizing_read_and_restarts_at_the_floor() {
        // Growth of 100 per request; cap 250, floor 50: 100, 200, (300 > cap: summarize 300, restart 50), 150.
        let segs = [segment("f", &[100, 200, 300, 400])];
        let run = replay(&segs, 250, 50, 0);
        assert_eq!(run.compactions, 1);
        assert_eq!(run.reread as u64, 100 + 200 + 300 + 50 + 150);
    }

    #[test]
    fn real_compaction_drops_are_not_counted_as_work() {
        // A real compaction between 900 and 120: the replay keeps growing from 900, not from 120.
        let segs = [segment("f", &[800, 900]), segment("f", &[120, 220])];
        let run = replay(&segs, 10_000, 50, 0);
        assert_eq!(run.reread as u64, 800 + 900 + 900 + 1000);
    }
}
