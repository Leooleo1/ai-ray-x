//! Apply and undo, shared by the panel and the terminal. Each step is
//! reported back, including when Memo could not keep the lesson.

use std::path::Path;

use anyhow::{anyhow, Result};
use raiox_core::{advice, apply, efficiency, Store};
use serde_json::{json, Value};

use crate::{memo, room};

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as i64)
}

fn evidence(store: &Store, suggestion: &str, source: &str) -> String {
    efficiency::efficiency(store.conn(), source)
        .map(|e| advice::suggestions(&e, None))
        .ok()
        .and_then(|list| list.into_iter().find(|s| s.id == suggestion))
        .map_or_else(String::new, |s| s.evidence)
}

pub fn apply_suggestion(store: &Store, state_dir: &Path, project_dir: &Path, suggestion: &str, source: &str) -> Result<Value> {
    room::require_opt_in(project_dir)?;
    let plan = apply::plan(suggestion, source, project_dir)
        .ok_or_else(|| anyhow!("{suggestion} for {source} has no setting to apply; it is a habit"))?;
    let change = apply::apply(&plan, state_dir, now_ms())?;
    let memo = match memo::keep_lesson(project_dir, &change, &evidence(store, suggestion, source)) {
        Ok(path) => {
            apply::set_lesson(&change.id, &path, state_dir)?;
            json!({ "kept": path })
        }
        Err(error) => json!({ "error": format!("{error:#}") }),
    };
    log(state_dir, project_dir, "dieta", "applied", json!({ "suggestion": change.suggestion, "key": change.key, "value": change.after }), &memo)?;
    Ok(json!({ "change": change, "effect": plan.effect, "memo": memo }))
}

pub fn undo_change(store: &Store, state_dir: &Path, project_dir: &Path, change_id: &str) -> Result<Value> {
    room::require_opt_in(project_dir)?;
    let change = apply::undo(change_id, state_dir, now_ms())?;
    let memo = match memo::keep_lesson(project_dir, &change, &evidence(store, &change.suggestion, "claude")) {
        Ok(path) => json!({ "kept": path }),
        Err(error) => json!({ "error": format!("{error:#}") }),
    };
    log(state_dir, project_dir, "dieta", "undone", json!({ "suggestion": change.suggestion, "key": change.key }), &memo)?;
    Ok(json!({ "change": change, "memo": memo }))
}

/// Puts an action and Memo's outcome in the project's room.
fn log(state_dir: &Path, project_dir: &Path, author: &str, kind: &str, data: Value, memo: &Value) -> Result<()> {
    let now = room::now_ms();
    let memo_kind = if memo.get("kept").is_some() { "kept" } else { "kept_failed" };
    room::append(
        state_dir,
        project_dir,
        vec![
            room::Message { ms: now, author: author.into(), kind: kind.into(), data },
            room::Message { ms: now, author: "memo".into(), kind: memo_kind.into(), data: memo.clone() },
        ],
    )
}
