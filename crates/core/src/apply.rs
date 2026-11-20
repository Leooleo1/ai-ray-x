//! Applies a Dieta suggestion to one project, with a backup and an undo.
//!
//! Only documented Claude Code settings are touched, and only in the
//! project's `.claude/settings.local.json` (personal, per project, kept out of
//! git by Claude Code). Every change is logged in a ledger next to the index.
//! Undo restores the previous value only if the file still holds the value
//! raiox wrote; a value the person changed afterwards is left alone.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// What applying a suggestion would change.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Plan {
    pub suggestion: String,
    pub file: PathBuf,
    pub key: String,
    pub value: String,
    pub effect: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Change {
    pub id: String,
    pub suggestion: String,
    pub file: String,
    pub key: String,
    pub before: Option<String>,
    pub after: String,
    pub backup: Option<String>,
    pub applied_ms: i64,
    pub undone_ms: Option<i64>,
    pub lesson: Option<String>,
    /// When Memo kept the measured before/after result (the lesson's wiki path).
    #[serde(default)]
    pub result_kept: Option<String>,
}

struct Spec {
    id: &'static str,
    source: &'static str,
    key: &'static str,
    default: &'static str,
    min: u64,
    max: u64,
    effect: &'static str,
}

/// Documented settings behind each applicable suggestion (code.claude.com/docs/en/env-vars).
const PLANS: [Spec; 2] = [
    Spec {
        id: "compact-earlier",
        source: "claude",
        key: "CLAUDE_CODE_AUTO_COMPACT_WINDOW",
        default: "400000",
        min: 100_000,
        max: 1_000_000,
        effect: "Claude Code compacts when the context reaches {value} tokens instead of near the model limit",
    },
    Spec {
        id: "trim-tool-output",
        source: "claude",
        key: "BASH_MAX_OUTPUT_LENGTH",
        default: "15000",
        min: 5_000,
        max: 100_000,
        effect: "Claude Code trims Bash output after {value} characters instead of 30000",
    },
];

/// The plan for a suggestion; `value` (computed by Dieta or chosen in the panel)
/// replaces the default when it is inside the setting's documented range.
pub fn plan(suggestion: &str, source: &str, project_dir: &Path, value: Option<&str>) -> Option<Plan> {
    PLANS.iter().find(|s| s.id == suggestion && s.source == source).map(|s| {
        let chosen = value
            .filter(|v| v.parse::<u64>().is_ok_and(|n| (s.min..=s.max).contains(&n)))
            .unwrap_or(s.default)
            .to_string();
        Plan {
            suggestion: s.id.to_string(),
            file: project_dir.join(".claude").join("settings.local.json"),
            key: s.key.to_string(),
            effect: s.effect.replace("{value}", &chosen),
            value: chosen,
        }
    })
}

/// The agent a suggestion's setting belongs to.
pub fn source_of(suggestion: &str) -> Option<&'static str> {
    PLANS.iter().find(|s| s.id == suggestion).map(|s| s.source)
}

/// Ids of suggestions that can be applied for a source.
pub fn applicable(source: &str) -> Vec<&'static str> {
    PLANS.iter().filter(|s| s.source == source).map(|s| s.id).collect()
}

pub fn ledger(path: &Path) -> Result<Vec<Change>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

fn save_ledger(path: &Path, changes: &[Change]) -> Result<()> {
    write_atomic(path, &(serde_json::to_string_pretty(changes)? + "\n"))
}

fn write_atomic(path: &Path, text: &str) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let temp = path.with_extension("raiox-tmp");
    fs::write(&temp, text).with_context(|| format!("writing {}", temp.display()))?;
    fs::rename(&temp, path).with_context(|| format!("replacing {}", path.display()))?;
    Ok(())
}

fn read_settings(file: &Path) -> Result<Map<String, Value>> {
    if !file.exists() {
        return Ok(Map::new());
    }
    let text = fs::read_to_string(file)?;
    match serde_json::from_str::<Value>(&text) {
        Ok(Value::Object(map)) => Ok(map),
        // Never overwrite a file we cannot read faithfully.
        _ => bail!("{} is not a JSON object; fix it before applying", file.display()),
    }
}

fn env_value(settings: &Map<String, Value>, key: &str) -> Option<String> {
    settings.get("env")?.get(key)?.as_str().map(str::to_string)
}

fn with_env(settings: &Map<String, Value>, key: &str, value: Option<&str>) -> Map<String, Value> {
    let mut env = settings.get("env").and_then(Value::as_object).cloned().unwrap_or_default();
    match value {
        Some(v) => {
            env.insert(key.to_string(), Value::String(v.to_string()));
        }
        None => {
            env.shift_remove(key);
        }
    }
    let mut next = settings.clone();
    if env.is_empty() {
        next.shift_remove("env");
    } else {
        next.insert("env".to_string(), Value::Object(env));
    }
    next
}

fn write_settings(file: &Path, settings: &Map<String, Value>) -> Result<()> {
    write_atomic(file, &(serde_json::to_string_pretty(&Value::Object(settings.clone()))? + "\n"))
}

/// Applies a plan: backs the file up, writes the setting and logs the change.
pub fn apply(plan: &Plan, state_dir: &Path, now_ms: i64) -> Result<Change> {
    let settings = read_settings(&plan.file)?;
    let before = env_value(&settings, &plan.key);
    if before.as_deref() == Some(plan.value.as_str()) {
        bail!("{} is already {} in {}", plan.key, plan.value, plan.file.display());
    }
    let backup = if plan.file.exists() {
        let path = state_dir.join("backups").join(format!("{now_ms}-settings.local.json"));
        fs::create_dir_all(path.parent().ok_or_else(|| anyhow!("bad backup path"))?)?;
        fs::copy(&plan.file, &path).with_context(|| format!("backing up {}", plan.file.display()))?;
        Some(path.to_string_lossy().into_owned())
    } else {
        None
    };
    write_settings(&plan.file, &with_env(&settings, &plan.key, Some(&plan.value)))?;
    let change = Change {
        id: format!("{}-{now_ms}", plan.suggestion),
        suggestion: plan.suggestion.clone(),
        file: plan.file.to_string_lossy().into_owned(),
        key: plan.key.clone(),
        before,
        after: plan.value.clone(),
        backup,
        applied_ms: now_ms,
        undone_ms: None,
        lesson: None,
        result_kept: None,
    };
    let ledger_path = state_dir.join("changes.json");
    let mut changes = ledger(&ledger_path)?;
    changes.push(change.clone());
    save_ledger(&ledger_path, &changes)?;
    Ok(change)
}

/// Undoes a change if the file still holds the value raiox wrote.
pub fn undo(change_id: &str, state_dir: &Path, now_ms: i64) -> Result<Change> {
    let ledger_path = state_dir.join("changes.json");
    let changes = ledger(&ledger_path)?;
    let change = changes
        .iter()
        .find(|c| c.id == change_id && c.undone_ms.is_none())
        .ok_or_else(|| anyhow!("no open change {change_id}"))?;
    let file = PathBuf::from(&change.file);
    let settings = read_settings(&file)?;
    let current = env_value(&settings, &change.key);
    if current.as_deref() != Some(change.after.as_str()) {
        bail!(
            "{} is now {:?} in {}, not the {} raiox wrote; leaving it as you set it",
            change.key,
            current,
            change.file,
            change.after
        );
    }
    write_settings(&file, &with_env(&settings, &change.key, change.before.as_deref()))?;
    let undone = Change { undone_ms: Some(now_ms), ..change.clone() };
    let updated: Vec<Change> = changes.iter().map(|c| if c.id == change_id { undone.clone() } else { c.clone() }).collect();
    save_ledger(&ledger_path, &updated)?;
    Ok(undone)
}

/// Records that Memo kept the measured result for a change.
pub fn set_result_kept(change_id: &str, lesson: &str, state_dir: &Path) -> Result<()> {
    let ledger_path = state_dir.join("changes.json");
    let updated: Vec<Change> = ledger(&ledger_path)?
        .into_iter()
        .map(|c| if c.id == change_id { Change { result_kept: Some(lesson.to_string()), ..c } } else { c })
        .collect();
    save_ledger(&ledger_path, &updated)
}

/// Records where Memo kept the lesson for a change.
pub fn set_lesson(change_id: &str, lesson: &str, state_dir: &Path) -> Result<()> {
    let ledger_path = state_dir.join("changes.json");
    let updated: Vec<Change> = ledger(&ledger_path)?
        .into_iter()
        .map(|c| if c.id == change_id { Change { lesson: Some(lesson.to_string()), ..c } } else { c })
        .collect();
    save_ledger(&ledger_path, &updated)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("raiox-apply-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("project/.claude")).unwrap();
        dir
    }

    #[test]
    fn apply_keeps_other_keys_and_undo_restores_them() {
        let dir = temp_dir("roundtrip");
        let file = dir.join("project/.claude/settings.local.json");
        fs::write(&file, r#"{"permissions":{"allow":["Bash(ls)"]},"env":{"FOO":"1"}}"#).unwrap();
        let plan = plan("compact-earlier", "claude", &dir.join("project"), None).unwrap();
        let change = apply(&plan, &dir.join("state"), 1).unwrap();
        let applied: Value = serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(applied["env"]["CLAUDE_CODE_AUTO_COMPACT_WINDOW"], "400000");
        assert_eq!(applied["permissions"]["allow"][0], "Bash(ls)");
        undo(&change.id, &dir.join("state"), 2).unwrap();
        let restored: Value = serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(restored["env"], serde_json::json!({"FOO": "1"}));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn undo_leaves_a_value_the_person_changed() {
        let dir = temp_dir("edited");
        let plan = plan("compact-earlier", "claude", &dir.join("project"), None).unwrap();
        let change = apply(&plan, &dir.join("state"), 1).unwrap();
        fs::write(&plan.file, r#"{"env":{"CLAUDE_CODE_AUTO_COMPACT_WINDOW":"300000"}}"#).unwrap();
        assert!(undo(&change.id, &dir.join("state"), 2).is_err());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_broken_settings_file_is_never_overwritten() {
        let dir = temp_dir("broken");
        let file = dir.join("project/.claude/settings.local.json");
        fs::write(&file, "{ not json").unwrap();
        let plan = plan("compact-earlier", "claude", &dir.join("project"), None).unwrap();
        assert!(apply(&plan, &dir.join("state"), 1).is_err());
        assert_eq!(fs::read_to_string(&file).unwrap(), "{ not json");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn trim_tool_output_applies_and_undoes_cleanly() {
        let dir = temp_dir("trim");
        let file = dir.join("project/.claude/settings.local.json");
        let plan = plan("trim-tool-output", "claude", &dir.join("project"), Some("10000")).unwrap();
        assert_eq!(plan.key, "BASH_MAX_OUTPUT_LENGTH");
        assert_eq!(plan.value, "10000");
        let change = apply(&plan, &dir.join("state"), 10).unwrap();
        let applied: Value = serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(applied["env"]["BASH_MAX_OUTPUT_LENGTH"], "10000");
        undo(&change.id, &dir.join("state"), 20).unwrap();
        let restored: Value = serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
        assert!(restored.get("env").is_none());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn custom_values_are_clamped_by_each_settings_documented_range() {
        let project = Path::new("/tmp/fake-project");
        assert_eq!(plan("compact-earlier", "claude", project, Some("300000")).unwrap().value, "300000");
        assert_eq!(plan("compact-earlier", "claude", project, Some("50")).unwrap().value, "400000");
        assert_eq!(plan("trim-tool-output", "claude", project, Some("15000")).unwrap().value, "15000");
        assert_eq!(plan("trim-tool-output", "claude", project, Some("500")).unwrap().value, "15000");
        assert_eq!(plan("trim-tool-output", "claude", project, Some("999999")).unwrap().value, "15000");
        assert!(plan("trim-tool-output", "codex", project, Some("15000")).is_none());
    }
}
