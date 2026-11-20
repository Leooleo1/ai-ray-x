//! How full a conversation's context is: what its CLI re-reads on every request, and when it
//! will compact. Read from the CLI's own logs (the index), for the session the conversation
//! continues.

use std::fs;
use std::path::Path;

use anyhow::Result;
use raiox_core::{Roots, Store};
use serde_json::{json, Value};

use crate::bots;

/// Claude Code's setting that moves the compaction point (code.claude.com/docs/en/env-vars).
const COMPACT_KEY: &str = "CLAUDE_CODE_AUTO_COMPACT_WINDOW";
/// The context window of the current Claude models (Opus 5.5, Sonnet 5.5, Haiku 5.5, Fable).
const CLAUDE_WINDOW: u64 = 1_000_000;

/// The compaction point Claude Code is told to use: the project's settings first, then the person's.
fn compact_setting(project: &Path, home: &Path) -> Option<u64> {
    let files = [project.join(".claude/settings.local.json"), project.join(".claude/settings.json"), home.join(".claude/settings.json")];
    files.iter().find_map(|file| {
        let settings: Value = serde_json::from_str(&fs::read_to_string(file).ok()?).ok()?;
        let value = settings.get("env")?.get(COMPACT_KEY)?;
        value.as_str().and_then(|v| v.trim().parse().ok()).or_else(|| value.as_u64())
    })
}

/// The context of one conversation of a bot, or null before its first turn is in the logs.
pub fn of_conversation(store: &mut Store, roots: &Roots, state_dir: &Path, bot_id: &str, thread: &str) -> Result<Value> {
    let bot = bots::find(state_dir, bot_id)?;
    let Some(session) = bots::conversation_session(state_dir, &bot, thread)? else { return Ok(Value::Null) };
    // Only the logs that changed since the last look are read again.
    raiox_core::refresh(store, roots)?;
    let Some(status) = raiox_core::advice::context_status(store.conn(), Some(&session))? else { return Ok(Value::Null) };
    let setting = (bot.agent == "claude").then(|| compact_setting(&bot.dir, &roots.home)).flatten();
    let compact_at = setting.or((status.usual_compaction_at > 0).then_some(status.usual_compaction_at));
    Ok(json!({
        "status": status,
        "compact_at": compact_at,
        "compact_from": if setting.is_some() { "setting" } else if compact_at.is_some() { "usual" } else { "none" },
        "window": (bot.agent == "claude").then_some(CLAUDE_WINDOW),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_project_setting_wins_over_the_persons() {
        let dir = std::env::temp_dir().join(format!("raiox-contexto-{}", std::process::id()));
        let (project, home) = (dir.join("p"), dir.join("h"));
        fs::create_dir_all(project.join(".claude")).unwrap();
        fs::create_dir_all(home.join(".claude")).unwrap();
        fs::write(home.join(".claude/settings.json"), r#"{"env":{"CLAUDE_CODE_AUTO_COMPACT_WINDOW":"400000"}}"#).unwrap();
        assert_eq!(compact_setting(&project, &home), Some(400_000));
        fs::write(project.join(".claude/settings.json"), r#"{"env":{"CLAUDE_CODE_AUTO_COMPACT_WINDOW":"300000"}}"#).unwrap();
        assert_eq!(compact_setting(&project, &home), Some(300_000));
        fs::write(home.join(".claude/settings.json"), "{}").unwrap();
        fs::remove_file(project.join(".claude/settings.json")).unwrap();
        assert_eq!(compact_setting(&project, &home), None);
    }
}
