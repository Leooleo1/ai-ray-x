//! OpenBot's local database (`<config dir>/OpenBot/openbot.db`), read-only.
//!
//! OpenBot runs Claude Code and Codex underneath, so its tokens are already in
//! those logs. This reader adds attribution only: which OpenBot agent spent
//! what, its estimated cost, and what loading each skill cost. It selects
//! ids, names, models and numbers; instructions, memories and messages are
//! never read. No OpenBot code is used: only its database, through SQLite.

use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Agent {
    pub id: String,
    pub name: String,
    pub model: String,
    pub records: u64,
    pub tokens: u64,
    pub cost_usd: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SkillLoad {
    pub skill: String,
    pub loads: u64,
    pub tokens: u64,
    pub cost_usd: f64,
}

#[derive(Debug, Default)]
pub struct Usage {
    pub agents: Vec<Agent>,
    pub skill_loads: Vec<SkillLoad>,
}

const AGENTS_SQL: &str = "
  WITH usage AS (
    SELECT agent_id, COUNT(*) AS records, SUM(estimated_cost_usd) AS cost,
      SUM(COALESCE(json_extract(tokens_json, '$.uncachedInput'), 0) + COALESCE(json_extract(tokens_json, '$.cachedInput'), 0)
        + COALESCE(json_extract(tokens_json, '$.cacheCreation'), 0) + COALESCE(json_extract(tokens_json, '$.output'), 0)) AS tokens
    FROM agent_usage_records GROUP BY agent_id)
  SELECT a.agent_id, COALESCE(json_extract(a.agent_json, '$.name'), a.agent_id), COALESCE(a.model, '?'),
    COALESCE(u.records, 0), COALESCE(u.tokens, 0), COALESCE(u.cost, 0)
  FROM projection_agents a LEFT JOIN usage u ON u.agent_id = a.agent_id
  ORDER BY 5 DESC";

const SKILLS_SQL: &str = "
  SELECT skill, COUNT(*), SUM(COALESCE(tokens, 0)), SUM(COALESCE(estimated_cost_usd, 0))
  FROM agent_usage_skill_loads GROUP BY skill ORDER BY 3 DESC";

/// Reads agent attribution and skill-load costs. Missing tables give empty lists,
/// so a newer or older OpenBot never breaks the scan.
pub fn read(path: &Path) -> Result<Usage> {
    if !path.exists() {
        return Ok(Usage::default());
    }
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    // immutable=1: never take locks or touch the WAL of a database the app is using.
    let conn = Connection::open_with_flags(format!("file:{}?mode=ro&immutable=1", uri_path(path)), flags)
        .with_context(|| format!("opening {}", path.display()))?;
    let has = |table: &str| -> bool {
        conn.query_row("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1", [table], |_| Ok(()))
            .is_ok()
    };
    let agents = if has("projection_agents") && has("agent_usage_records") {
        let mut statement = conn.prepare(AGENTS_SQL)?;
        let rows = statement.query_map([], |r| {
            Ok(Agent {
                id: r.get(0)?,
                name: r.get(1)?,
                model: r.get(2)?,
                records: r.get::<_, i64>(3)?.max(0) as u64,
                tokens: r.get::<_, i64>(4)?.max(0) as u64,
                cost_usd: r.get(5)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    } else {
        Vec::new()
    };
    let skill_loads = if has("agent_usage_skill_loads") {
        let mut statement = conn.prepare(SKILLS_SQL)?;
        let rows = statement.query_map([], |r| {
            Ok(SkillLoad {
                skill: r.get(0)?,
                loads: r.get::<_, i64>(1)?.max(0) as u64,
                tokens: r.get::<_, i64>(2)?.max(0) as u64,
                cost_usd: r.get(3)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    } else {
        Vec::new()
    };
    Ok(Usage { agents, skill_loads })
}

/// Percent-encodes the characters that would end or confuse a SQLite URI path.
fn uri_path(path: &Path) -> String {
    path.to_string_lossy()
        .chars()
        .map(|c| match c {
            ' ' => "%20".to_string(),
            '?' => "%3F".to_string(),
            '#' => "%23".to_string(),
            '%' => "%25".to_string(),
            other => other.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(dir: &Path) -> std::path::PathBuf {
        let path = dir.join("openbot test.db");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE projection_agents(agent_id TEXT, model TEXT, agent_json TEXT);
             CREATE TABLE agent_usage_records(agent_id TEXT, tokens_json TEXT, estimated_cost_usd REAL);
             CREATE TABLE agent_usage_skill_loads(skill TEXT, tokens INTEGER, estimated_cost_usd REAL);
             INSERT INTO projection_agents VALUES('a1', 'claude-opus-5-5', '{\"name\":\"Writer\",\"description\":\"secret\"}');
             INSERT INTO agent_usage_records VALUES('a1', '{\"uncachedInput\":10,\"cachedInput\":80,\"cacheCreation\":5,\"output\":5}', 0.5);
             INSERT INTO agent_usage_records VALUES('a1', '{\"uncachedInput\":0,\"cachedInput\":100,\"output\":0}', 0.25);
             INSERT INTO agent_usage_skill_loads VALUES('pdf', 1200, 0.01);",
        )
        .unwrap();
        path
    }

    #[test]
    fn reads_attribution_and_skill_costs_from_a_path_with_spaces() {
        let dir = std::env::temp_dir().join(format!("raiox-openbot-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let usage = read(&fixture(&dir)).unwrap();
        assert_eq!(usage.agents[0].name, "Writer");
        assert_eq!((usage.agents[0].records, usage.agents[0].tokens), (2, 200));
        assert!((usage.agents[0].cost_usd - 0.75).abs() < 1e-9);
        assert_eq!(usage.skill_loads[0].skill, "pdf");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_missing_database_is_empty() {
        let usage = read(Path::new("/nonexistent/openbot.db")).unwrap();
        assert!(usage.agents.is_empty() && usage.skill_loads.is_empty());
    }
}
