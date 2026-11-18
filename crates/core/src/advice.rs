//! What the agents Raio and Dieta answer: the context a session carries right
//! now, and ranked suggestions to carry less. Suggestions are text and
//! numbers only; nothing here changes a setting.

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;

use crate::efficiency::Efficiency;
use crate::report::rows;

const CHARS_PER_TOKEN: f64 = 3.6;
const IMAGE_TOKENS: f64 = 1_500.0;
const COMPACT_TARGET: u64 = 400_000;
const TOP_LOADED: usize = 6;
/// Suggestions below this share of the re-read context are not worth the noise.
const MIN_SHARE: f64 = 0.02;

#[derive(Debug, Serialize)]
pub struct Loaded {
    pub name: String,
    pub tokens: u64,
}

/// The context of the most recent session (or of `session`, when given).
#[derive(Debug, Serialize)]
pub struct ContextStatus {
    pub source: String,
    pub session: String,
    pub project: String,
    pub last_request_ms: Option<i64>,
    pub context_tokens: u64,
    pub requests_since_compaction: u64,
    pub startup_tokens: u64,
    pub loaded_since_compaction: Vec<Loaded>,
    pub usual_compaction_at: u64,
}

pub fn context_status(conn: &Connection, session: Option<&str>) -> Result<Option<ContextStatus>> {
    let latest = conn
        .query_row(
            "SELECT source, session, project, file, segment, ts_ms, input + cache_read + cache_write_5m + cache_write_1h
             FROM calls WHERE is_sub = 0 AND (?1 IS NULL OR session = ?1) ORDER BY ts_ms DESC LIMIT 1",
            [session],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, i64>(4)?,
                    r.get::<_, Option<i64>>(5)?,
                    r.get::<_, i64>(6)?,
                ))
            },
        )
        .optional()?;
    let Some((source, session, project, file, segment, ts, context)) = latest else { return Ok(None) };
    let requests = conn.query_row(
        "SELECT COUNT(*) FROM calls WHERE file = ?1 AND segment = ?2",
        rusqlite::params![file, segment],
        |r| r.get::<_, i64>(0),
    )?;
    let startup = conn
        .query_row(
            "SELECT input + cache_read + cache_write_5m + cache_write_1h FROM calls WHERE file = ?1 AND is_first = 1",
            [&file],
            |r| r.get::<_, i64>(0),
        )
        .optional()?
        .unwrap_or(0);
    let mut loaded = rows(
        conn,
        "SELECT tool, SUM(chars), SUM(images) FROM tool_results WHERE file = ?1 AND segment = ?2 GROUP BY tool
         UNION ALL
         SELECT CASE injected WHEN 1 THEN 'injected text' ELSE 'your prompts' END, SUM(chars), SUM(images)
         FROM prompts WHERE file = ?1 AND segment = ?2 GROUP BY injected",
        rusqlite::params![file, segment],
        |r| {
            let tokens = r.get::<_, i64>(1)? as f64 / CHARS_PER_TOKEN + r.get::<_, i64>(2)? as f64 * IMAGE_TOKENS;
            Ok(Loaded { name: r.get(0)?, tokens: tokens as u64 })
        },
    )?;
    loaded.sort_by_key(|item| std::cmp::Reverse(item.tokens));
    loaded.truncate(TOP_LOADED);
    let usual = conn
        .query_row(
            "SELECT pre FROM compactions WHERE source = ?1 AND pre > 0 ORDER BY pre
             LIMIT 1 OFFSET (SELECT COUNT(*) FROM compactions WHERE source = ?1 AND pre > 0) / 2",
            [&source],
            |r| r.get::<_, i64>(0),
        )
        .optional()?
        .unwrap_or(0);
    Ok(Some(ContextStatus {
        source,
        session,
        project,
        last_request_ms: ts,
        context_tokens: context.max(0) as u64,
        requests_since_compaction: requests.max(0) as u64,
        startup_tokens: startup.max(0) as u64,
        loaded_since_compaction: loaded,
        usual_compaction_at: usual.max(0) as u64,
    }))
}

/// One ranked suggestion from Dieta.
#[derive(Debug, Serialize)]
pub struct Suggestion {
    pub id: &'static str,
    pub source: String,
    pub title: &'static str,
    pub evidence: String,
    /// Tokens of re-read context saved over the analyzed period (an estimate).
    pub saving_tokens: u64,
    pub saving_share: f64,
    pub how: &'static str,
    /// "upper bound" or "estimate": how much to trust the number.
    pub confidence: &'static str,
}

/// Rules over the efficiency report, ranked by estimated saving.
pub fn suggestions(eff: &Efficiency, skills_listed: Option<u64>) -> Vec<Suggestion> {
    let total = eff.context.total.max(1) as f64;
    let carried = |name: &str| eff.carried.iter().find(|c| c.name == name).map_or(0.0, |c| c.carried_tokens);
    let source = eff.source.clone();
    let claude = source == "claude";
    let mut found = Vec::new();

    if eff.compaction_pre_median > COMPACT_TARGET {
        let saved = eff.simulation.iter().find(|(cap, _)| *cap == COMPACT_TARGET).map_or(0.0, |(_, s)| *s);
        found.push(Suggestion {
            id: "compact-earlier",
            source: source.clone(),
            title: "Compact earlier",
            evidence: format!(
                "compaction happens at about {}K tokens; half the requests carry over {}K",
                eff.compaction_pre_median / 1000,
                eff.context.median / 1000
            ),
            saving_tokens: (saved * total) as u64,
            saving_share: saved,
            how: if claude {
                "Lower the auto-compact window (Claude Code setting autoCompactWindow) or run /compact when a task ends"
            } else {
                "Set model_auto_compact_token_limit in ~/.codex/config.toml, or compact when a task ends"
            },
            confidence: "upper bound",
        });
    }

    let idle: u64 = eff.rewrites.iter().filter(|r| r.reason.starts_with("idle")).map(|r| r.tokens).sum();
    if idle > 0 {
        found.push(Suggestion {
            id: "fresh-after-breaks",
            source: source.clone(),
            title: "Start fresh after long breaks",
            evidence: format!(
                "{} returns to a big session after a pause rewrote {}M tokens of cache",
                eff.rewrites.iter().filter(|r| r.reason.starts_with("idle")).map(|r| r.count).sum::<u64>(),
                idle / 1_000_000
            ),
            saving_tokens: idle * 8 / 10,
            saving_share: idle as f64 * 0.8 / total,
            how: "After an hour away, start a new session with a short summary (or a handoff) instead of resuming a huge one",
            confidence: "estimate",
        });
    }

    let shell = if claude { "Bash" } else { "exec" };
    let shell_tokens = carried(shell);
    found.push(Suggestion {
        id: "trim-tool-output",
        source: source.clone(),
        title: "Trim tool output",
        evidence: format!("{shell} output stays loaded: {:.0}M tokens re-read", shell_tokens / 1e6),
        saving_tokens: (shell_tokens * 0.5) as u64,
        saving_share: shell_tokens * 0.5 / total,
        how: if claude {
            "Set BASH_MAX_OUTPUT_LENGTH, pipe long output through head/tail, and search with Grep instead of printing files"
        } else {
            "Set tool_output_token_limit in ~/.codex/config.toml and keep command output short"
        },
        confidence: "estimate",
    });

    let images: f64 = eff.carried.iter().map(|c| c.images as f64).sum::<f64>() * IMAGE_TOKENS;
    if images > 0.0 {
        found.push(Suggestion {
            id: "screenshots-in-subagents",
            source: source.clone(),
            title: "Take screenshots in a subagent",
            evidence: format!(
                "{} images entered the main context",
                eff.carried.iter().map(|c| c.images).sum::<u64>()
            ),
            saving_tokens: 0,
            saving_share: 0.0,
            how: "Let a subagent open and judge the screenshots and return a short verdict, so the images never reach the main session",
            confidence: "estimate",
        });
    }

    if let Some(fixed) = eff.fixed.iter().find(|f| f.kind == "main") {
        let used = eff.skills_used.len() as u64;
        let skills = skills_listed.filter(|listed| *listed > used * 3);
        if fixed.share_of_context >= MIN_SHARE {
            found.push(Suggestion {
                id: "lighter-startup",
                source: source.clone(),
                title: "Load less at startup",
                evidence: match skills {
                    Some(listed) => format!(
                        "{}K tokens load before any work; {listed} skills listed, {used} used",
                        fixed.median_first_request / 1000
                    ),
                    None => format!("{}K tokens load before any work", fixed.median_first_request / 1000),
                },
                saving_tokens: (fixed.share_of_context * total * 0.3) as u64,
                saving_share: fixed.share_of_context * 0.3,
                how: "Disable plugins, skills and MCP servers unused in the last 60 days; keep rules and memory indexes short",
                confidence: "estimate",
            });
        }
    }

    let mut ranked: Vec<Suggestion> = found
        .into_iter()
        .filter(|s| s.saving_share >= MIN_SHARE || s.id == "screenshots-in-subagents")
        .collect();
    ranked.sort_by_key(|s| std::cmp::Reverse(s.saving_tokens));
    ranked
}
