//! What a bot's project really runs on: the models and effort levels its
//! Claude Code or Codex sessions used lately (from the index), the CLI's own
//! defaults (from its settings), and the choices the panel offers.

use std::fs;
use std::path::Path;

use anyhow::Result;
use raiox_core::{Roots, Store};
use serde_json::{json, Value};

use crate::bots;

/// How far back "lately" goes.
const DAYS: i64 = 30;
const DAY_MS: i64 = 86_400_000;
const MAX_ROWS: usize = 8;
/// Claude Code's own model aliases; each points at the latest of its family.
pub(crate) const CLAUDE_ALIASES: [&str; 4] = ["opus", "sonnet", "haiku", "fable"];

fn grouped(store: &Store, sql: &str, source: &str, project: &str, since: i64) -> Result<Vec<Value>> {
    let mut statement = store.conn().prepare(sql)?;
    let rows = statement.query_map((source, project, since), |r| {
        Ok(json!({ "name": r.get::<_, String>(0)?, "tokens": r.get::<_, i64>(1)?.max(0), "requests": r.get::<_, i64>(2)?.max(0) }))
    })?;
    Ok(rows.flatten().take(MAX_ROWS).collect())
}

/// Top-level `key = "value"` of a TOML file, before any [section].
fn toml_value(text: &str, key: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .take_while(|line| !line.starts_with('['))
        .find_map(|line| {
            let (name, value) = line.split_once('=')?;
            (name.trim() == key).then(|| value.trim().trim_matches('"').to_string())
        })
        .filter(|v| !v.is_empty())
}

fn defaults(roots: &Roots, agent: &str) -> Value {
    if matches!(agent, "antigravity" | "opencode") {
        return json!({ "model": null, "effort": null });
    }
    if agent == "cursor" {
        let config: Value = fs::read_to_string(roots.home.join(".cursor/cli-config.json"))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or(Value::Null);
        return json!({ "model": config.pointer("/model/displayModelId"), "effort": null });
    }
    if agent == "codex" {
        let config = fs::read_to_string(roots.codex.join("config.toml")).unwrap_or_default();
        return json!({ "model": toml_value(&config, "model"), "effort": toml_value(&config, "model_reasoning_effort") });
    }
    let settings: Value = fs::read_to_string(roots.claude.join("settings.json"))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or(Value::Null);
    json!({ "model": settings.get("model"), "effort": settings.get("effortLevel") })
}

/// The Codex models this person has used, newest first.
pub(crate) fn codex_models(store: &Store) -> Result<Vec<String>> {
    let mut statement = store.conn().prepare(
        "SELECT model FROM calls WHERE source = 'codex' AND is_sub = 0 AND model IS NOT NULL
         GROUP BY model ORDER BY MAX(ts_ms) DESC LIMIT 6",
    )?;
    let found = statement.query_map([], |r| r.get::<_, String>(0))?.flatten().collect();
    Ok(found)
}

/// Cursor's model ids, from `cursor-agent models` ("id - Name" per line).
pub(crate) fn cursor_models() -> Vec<String> {
    let Some(out) = crate::connections::probe("cursor-agent", &["models"]) else { return Vec::new() };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| line.split_once(" - ").map(|(id, _)| id.trim().to_string()))
        .filter(|id| bots::check_model(Some(id)).is_ok_and(|m| m.is_some()))
        .collect()
}

/// Antigravity's model ids, from `agy models` ("id<TAB>Name" per line; the id carries its level).
pub(crate) fn antigravity_models() -> Vec<String> {
    let Some(out) = crate::connections::probe("agy", &["models"]) else { return Vec::new() };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| line.split_once('\t').map(|(id, _)| id.trim().to_string()))
        .filter(|id| bots::check_model(Some(id)).is_ok_and(|m| m.is_some()))
        .collect()
}

/// opencode's models (provider/model), or its free ones when it lists none in time.
pub(crate) fn opencode_models() -> Vec<String> {
    let listed: Vec<String> = crate::connections::probe("opencode", &["models"])
        .map(|out| String::from_utf8_lossy(&out.stdout).lines().map(str::trim).filter(|l| l.contains('/')).map(str::to_string).collect())
        .unwrap_or_default();
    let listed: Vec<String> = listed.into_iter().filter(|id| bots::check_model(Some(id)).is_ok_and(|m| m.is_some())).collect();
    if listed.is_empty() { crate::bot_clis::OPENCODE_FREE.iter().map(|m| m.to_string()).collect() } else { listed }
}

pub fn usage(store: &Store, roots: &Roots, state_dir: &Path, id: &str) -> Result<Value> {
    let bot = bots::list(state_dir).into_iter().find(|b| b.id == id).ok_or_else(|| anyhow::anyhow!("no bot {id}"))?;
    let project = raiox_core::project_label(&bot.dir, &roots.home);
    let since = crate::room::now_ms() - DAYS * DAY_MS;
    let models = grouped(
        store,
        "SELECT model, SUM(input + cache_read + cache_write_5m + cache_write_1h), COUNT(*) FROM calls
         WHERE source = ?1 AND project = ?2 AND is_sub = 0 AND ts_ms >= ?3 AND model IS NOT NULL
         GROUP BY model ORDER BY 2 DESC",
        &bot.agent,
        &project,
        since,
    )?;
    let efforts = grouped(
        store,
        "SELECT effort, SUM(input + cache_read + cache_write_5m + cache_write_1h), COUNT(*) FROM calls
         WHERE source = ?1 AND project = ?2 AND is_sub = 0 AND ts_ms >= ?3 AND effort IS NOT NULL
         GROUP BY effort ORDER BY 3 DESC",
        &bot.agent,
        &project,
        since,
    )?;
    // Models to offer: Claude Code's aliases, or the Codex models this person has used.
    let offered: Vec<String> = match bot.agent.as_str() {
        "codex" => codex_models(store)?,
        "cursor" => cursor_models(),
        "antigravity" => antigravity_models(),
        "opencode" => opencode_models(),
        _ => CLAUDE_ALIASES.iter().map(|m| m.to_string()).collect(),
    };
    let efforts_allowed = bots::efforts_for(&bot.agent);
    Ok(json!({
        "days": DAYS,
        "project": project,
        "models": models,
        "efforts": efforts,
        "default": defaults(roots, &bot.agent),
        "offered_models": offered,
        "offered_efforts": efforts_allowed,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_top_level_toml_values_only() {
        let text = "model = \"gpt-x\"\nmodel_reasoning_effort = \"medium\"\n[profiles.fast]\nmodel = \"other\"\n";
        assert_eq!(toml_value(text, "model").as_deref(), Some("gpt-x"));
        assert_eq!(toml_value(text, "model_reasoning_effort").as_deref(), Some("medium"));
        assert_eq!(toml_value("[a]\nmodel = \"x\"", "model"), None);
    }
}
