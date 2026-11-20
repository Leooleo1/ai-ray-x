//! What the AIs' use would cost at API prices: per day, per AI, per model and per project.
//! Plans (Pro, Max, ChatGPT) charge a flat fee, so this is an estimate of the value used, not a
//! bill. Models with no known price count their tokens but no cost.

use std::collections::BTreeMap;

use anyhow::Result;
use rusqlite::Connection;
use serde::Serialize;

use crate::report::rows;

const PER_MILLION: f64 = 1_000_000.0;
/// Cache writes cost this much of a fresh input token: five minutes, one hour (both providers).
const WRITE_5M: f64 = 1.25;
const WRITE_1H: f64 = 2.0;
/// When a price list gives no cache-read price, it is this share of the input price.
const READ_SHARE: f64 = 0.1;

/// Dollars per million tokens: (model prefix, input, cache read or None, output). Longest prefix wins.
/// Claude: Anthropic's list prices; GPT-6.1: OpenAI's list prices (November 2026).
const PRICES: &[(&str, f64, Option<f64>, f64)] = &[
    ("claude-fable-5-2", 8.0, Some(0.20), 40.0),
    ("claude-fable-5-1", 10.0, None, 50.0),
    ("claude-fable-5", 10.0, None, 50.0),
    ("claude-mythos-5-2", 8.0, Some(0.20), 40.0),
    ("claude-mythos-5-1", 10.0, None, 50.0),
    ("claude-opus-5-5", 4.0, Some(0.20), 20.0),
    ("claude-opus-5", 5.0, None, 25.0),
    ("claude-opus-4", 5.0, None, 25.0),
    ("claude-sonnet-5-5", 2.0, Some(0.10), 10.0),
    ("claude-sonnet-5", 2.0, None, 10.0),
    ("claude-sonnet-4", 3.0, None, 15.0),
    ("claude-haiku-5-5", 0.10, None, 0.50),
    ("claude-haiku-4", 1.0, None, 5.0),
    ("gpt-6.1-sol", 2.0, Some(0.10), 10.0),
    ("gpt-6.1-terra", 1.0, Some(0.05), 5.0),
];

/// One group of calls with its tokens by kind.
#[derive(Debug, Default, Clone, Copy, Serialize)]
pub struct Tokens {
    pub input: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub output: u64,
}

impl Tokens {
    pub fn total(&self) -> u64 {
        self.input + self.cache_read + self.cache_write + self.output
    }

    fn add(self, other: Tokens) -> Tokens {
        Tokens {
            input: self.input + other.input,
            cache_read: self.cache_read + other.cache_read,
            cache_write: self.cache_write + other.cache_write,
            output: self.output + other.output,
        }
    }
}

/// A slice of the use: its name, what it would cost (None when no model in it has a price) and its tokens.
#[derive(Debug, Serialize)]
pub struct Slice {
    pub key: String,
    pub source: Option<String>,
    pub usd: Option<f64>,
    pub tokens: u64,
    pub calls: u64,
}

/// One day's cost per AI, for the chart.
#[derive(Debug, Serialize)]
pub struct Day {
    pub date: String,
    pub usd: BTreeMap<String, f64>,
    pub tokens: u64,
}

#[derive(Debug, Serialize)]
pub struct Usage {
    pub since: String,
    pub usd: f64,
    pub sessions: u64,
    pub calls: u64,
    /// Calls of models with no known price: their tokens count, their cost does not.
    pub unpriced_calls: u64,
    pub totals: Tokens,
    pub daily: Vec<Day>,
    pub by_source: Vec<Slice>,
    pub by_model: Vec<Slice>,
    pub by_project: Vec<Slice>,
}

/// The price of a model, by the longest known prefix of its name.
fn price(model: &str) -> Option<(f64, f64, f64)> {
    PRICES
        .iter()
        .filter(|(prefix, ..)| model.starts_with(prefix))
        .max_by_key(|(prefix, ..)| prefix.len())
        .map(|&(_, input, read, output)| (input, read.unwrap_or(input * READ_SHARE), output))
}

/// What these tokens would cost on this model, in dollars.
fn cost(model: &str, input: u64, read: u64, write_5m: u64, write_1h: u64, output: u64) -> Option<f64> {
    let (fresh, cached, out) = price(model)?;
    let input_units = input as f64 + write_5m as f64 * WRITE_5M + write_1h as f64 * WRITE_1H;
    Some((input_units * fresh + read as f64 * cached + output as f64 * out) / PER_MILLION)
}

struct Row {
    date: String,
    source: String,
    model: String,
    project: String,
    calls: u64,
    tokens: Tokens,
    usd: Option<f64>,
}

#[derive(Default)]
struct Sum {
    source: Option<String>,
    usd: Option<f64>,
    tokens: u64,
    calls: u64,
}

fn slices(groups: BTreeMap<String, Sum>) -> Vec<Slice> {
    let mut out: Vec<Slice> =
        groups.into_iter().map(|(key, sum)| Slice { key, source: sum.source, usd: sum.usd, tokens: sum.tokens, calls: sum.calls }).collect();
    out.sort_by(|a, b| b.usd.unwrap_or(0.0).total_cmp(&a.usd.unwrap_or(0.0)).then(b.tokens.cmp(&a.tokens)));
    out
}

fn tally(groups: &mut BTreeMap<String, Sum>, key: &str, source: Option<&str>, row: &Row) {
    let sum = groups.entry(key.to_string()).or_default();
    sum.source = source.map(str::to_string);
    sum.tokens += row.tokens.total();
    sum.calls += row.calls;
    if let Some(usd) = row.usd {
        sum.usd = Some(sum.usd.unwrap_or(0.0) + usd);
    }
}

/// The use since a day (YYYY-MM-DD, inclusive), optionally of one AI only.
pub fn usage(conn: &Connection, since: &str, source: Option<&str>) -> Result<Usage> {
    let raw = rows(
        conn,
        "SELECT local_date, source, model, project, COUNT(*), SUM(input), SUM(cache_read), SUM(cache_write_5m), SUM(cache_write_1h), SUM(output)
         FROM calls WHERE local_date >= ?1 AND (?2 IS NULL OR source = ?2)
         GROUP BY local_date, source, model, project ORDER BY local_date",
        rusqlite::params![since, source],
        |r| {
            let n = |i: usize| r.get::<_, i64>(i).map(|v| v.max(0) as u64);
            let model: String = r.get(2)?;
            let (input, read, w5, w1, output) = (n(5)?, n(6)?, n(7)?, n(8)?, n(9)?);
            Ok(Row {
                date: r.get(0)?,
                source: r.get(1)?,
                usd: cost(&model, input, read, w5, w1, output),
                model,
                project: r.get(3)?,
                calls: n(4)?,
                tokens: Tokens { input, cache_read: read, cache_write: w5 + w1, output },
            })
        },
    )?;
    let sessions = rows(
        conn,
        "SELECT COUNT(DISTINCT source || session) FROM calls WHERE local_date >= ?1 AND (?2 IS NULL OR source = ?2)",
        rusqlite::params![since, source],
        |r| r.get::<_, i64>(0),
    )?
    .first()
    .copied()
    .unwrap_or(0)
    .max(0) as u64;
    let (mut by_source, mut by_model, mut by_project) = (BTreeMap::new(), BTreeMap::new(), BTreeMap::new());
    let mut daily: BTreeMap<String, Day> = BTreeMap::new();
    let mut totals = Tokens::default();
    let (mut usd, mut calls, mut unpriced_calls) = (0.0, 0, 0);
    for row in &raw {
        totals = totals.add(row.tokens);
        calls += row.calls;
        usd += row.usd.unwrap_or(0.0);
        if row.usd.is_none() {
            unpriced_calls += row.calls;
        }
        tally(&mut by_source, &row.source, Some(&row.source), row);
        tally(&mut by_model, &row.model, Some(&row.source), row);
        tally(&mut by_project, &row.project, None, row);
        let day = daily.entry(row.date.clone()).or_insert_with(|| Day { date: row.date.clone(), usd: BTreeMap::new(), tokens: 0 });
        day.tokens += row.tokens.total();
        *day.usd.entry(row.source.clone()).or_insert(0.0) += row.usd.unwrap_or(0.0);
    }
    Ok(Usage {
        since: since.to_string(),
        usd,
        sessions,
        calls,
        unpriced_calls,
        totals,
        daily: daily.into_values().collect(),
        by_source: slices(by_source),
        by_model: slices(by_model),
        by_project: slices(by_project),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_model_is_priced_by_its_longest_known_name() {
        assert_eq!(price("claude-opus-5-5"), Some((4.0, 0.20, 20.0)));
        assert_eq!(price("claude-opus-5-20260101"), Some((5.0, 0.5, 25.0)), "Opus 5, not 5.5");
        assert_eq!(price("gpt-6.1-terra"), Some((1.0, 0.05, 5.0)));
        assert_eq!(price("codex-auto-review"), None, "no price, no cost");
    }

    #[test]
    fn each_kind_of_token_costs_its_own_price() {
        // A million of each on Opus 5.5: 4 input + 0.20 read + 5 (5-minute write) + 8 (1-hour write) + 20 output.
        let usd = cost("claude-opus-5-5", 1_000_000, 1_000_000, 1_000_000, 1_000_000, 1_000_000).unwrap();
        assert!((usd - 37.2).abs() < 1e-9, "{usd}");
    }

    #[test]
    fn the_use_adds_up_by_day_ai_model_and_project() {
        let store = crate::store::Store::open_in_memory().unwrap();
        let conn = store.conn();
        let insert = |id: &str, source: &str, model: &str, project: &str, date: &str, input: i64, output: i64| {
            conn.execute(
                "INSERT INTO calls(id, source, file, session, project, model, is_sub, ts_ms, local_date, local_hour, weekday, input, cache_read,
                 cache_write_5m, cache_write_1h, output, thinking, web_searches, seq, segment, is_first)
                 VALUES (?1, ?2, 'f', 's', ?3, ?4, 0, 0, ?5, 0, 0, ?6, 0, 0, 0, ?7, 0, 0, 0, 0, 0)",
                rusqlite::params![id, source, project, model, date, input, output],
            )
            .unwrap();
        };
        insert("a", "claude", "claude-opus-5-5", "app", "2026-11-19", 1_000_000, 0);
        insert("b", "codex", "gpt-6.1-sol", "app", "2026-11-20", 0, 1_000_000);
        insert("c", "codex", "codex-auto-review", "site", "2026-11-20", 500, 0);
        insert("d", "claude", "claude-opus-5-5", "old", "2026-10-01", 9_000_000, 0);
        let used = usage(conn, "2026-11-01", None).unwrap();
        assert!((used.usd - 14.0).abs() < 1e-9, "4 (Opus input) + 10 (Sol output), the October day left out: {}", used.usd);
        assert_eq!((used.calls, used.unpriced_calls, used.daily.len()), (3, 1, 2));
        assert_eq!(used.by_project[0].key, "app");
        assert_eq!(used.by_model.iter().find(|m| m.key == "codex-auto-review").and_then(|m| m.usd), None);
        let codex = usage(conn, "2026-11-01", Some("codex")).unwrap();
        assert!((codex.usd - 10.0).abs() < 1e-9);
    }
}
