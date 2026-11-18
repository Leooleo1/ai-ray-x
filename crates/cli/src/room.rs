//! The agents room of one project. It only works in projects that opted in by
//! installing the token-diet skill (`.claude/skills/token-diet`), and agents
//! only speak when the person clicks or runs `raiox ask` (from the skill).
//! Answers are data; the panel renders them in each agent's voice.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Result};
use raiox_core::outcome::{self, Outcome};
use raiox_core::{advice, apply, efficiency, Roots, Store};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const OPT_IN: &str = ".claude/skills/token-diet";
const TOP: usize = 3;
const MAX_MESSAGES: usize = 200;
/// The person's own words are kept short; agents answer from data, not from the text.
const MAX_TEXT: usize = 280;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub ms: i64,
    /// "you", "raio", "dieta" or "memo".
    pub author: String,
    /// What happened: "ask", "answer", "applied", "undone", "kept", "kept_failed".
    pub kind: String,
    pub data: Value,
}

pub fn opted_in(project_dir: &Path) -> bool {
    project_dir.join(OPT_IN).exists()
}

/// Refuses to act outside projects that opted in.
pub fn require_opt_in(project_dir: &Path) -> Result<()> {
    if !opted_in(project_dir) {
        bail!("raiox agents only act in projects with {OPT_IN}; {} has none", project_dir.display());
    }
    Ok(())
}

fn room_file(state_dir: &Path, project_dir: &Path) -> PathBuf {
    let hash = project_dir.to_string_lossy().bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3));
    state_dir.join("rooms").join(format!("{hash:016x}.json"))
}

pub fn messages(state_dir: &Path, project_dir: &Path) -> Vec<Message> {
    fs::read_to_string(room_file(state_dir, project_dir))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn append(state_dir: &Path, project_dir: &Path, new: Vec<Message>) -> Result<()> {
    let all: Vec<Message> = messages(state_dir, project_dir).into_iter().chain(new).collect();
    let kept = &all[all.len().saturating_sub(MAX_MESSAGES)..];
    let file = room_file(state_dir, project_dir);
    fs::create_dir_all(file.parent().unwrap_or(state_dir))?;
    let temp = file.with_extension("tmp");
    fs::write(&temp, serde_json::to_string_pretty(kept)? + "\n")?;
    fs::rename(temp, file)?;
    Ok(())
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

/// Claude Code keeps a project's transcripts in `projects/<folder with every
/// non-alphanumeric character turned into '-'>`; the newest file is the
/// session being written now.
pub fn session_of_dir(claude: &Path, dir: &Path) -> Option<String> {
    let encoded: String = dir.to_string_lossy().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    fs::read_dir(claude.join("projects").join(encoded))
        .ok()?
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "jsonl"))
        .filter_map(|entry| Some((entry.metadata().ok()?.modified().ok()?, entry.path())))
        .max_by_key(|(modified, _)| *modified)
        .and_then(|(_, path)| Some(path.file_stem()?.to_string_lossy().into_owned()))
}

/// The session working in this project now: the latest main-session request
/// whose folder is this project (a session may have started elsewhere), else
/// the newest transcript stored for the folder.
pub fn current_session(store: &Store, roots: &Roots, project_dir: &Path) -> Option<String> {
    let label = raiox_core::project_label(project_dir, &roots.home);
    store
        .conn()
        .query_row(
            "SELECT session FROM calls WHERE project = ?1 AND is_sub = 0 ORDER BY ts_ms DESC LIMIT 1",
            [label],
            |r| r.get::<_, String>(0),
        )
        .ok()
        .or_else(|| session_of_dir(&roots.claude, project_dir))
}

/// The person asks one agent; the answer is computed now from this project.
pub fn ask(store: &Store, roots: &Roots, state_dir: &Path, project_dir: &Path, agent: &str, text: Option<&str>) -> Result<Vec<Message>> {
    require_opt_in(project_dir)?;
    let data = match agent {
        "raio" => raio(store, roots, project_dir)?,
        "dieta" => dieta(store)?,
        "memo" => memo(store, roots, state_dir, project_dir)?,
        other => bail!("unknown agent {other}"),
    };
    let now = now_ms();
    let new = vec![
        Message {
            ms: now,
            author: "you".into(),
            kind: "ask".into(),
            data: json!({ "agent": agent, "text": text.map(|t| t.trim().chars().take(MAX_TEXT).collect::<String>()).filter(|t| !t.is_empty()) }),
        },
        Message { ms: now, author: agent.into(), kind: "answer".into(), data },
    ];
    append(state_dir, project_dir, new.clone())?;
    Ok(new)
}

fn raio(store: &Store, roots: &Roots, project_dir: &Path) -> Result<Value> {
    let Some(session) = current_session(store, roots, project_dir) else {
        return Ok(json!({ "found": false }));
    };
    let Some(status) = advice::context_status(store.conn(), Some(&session))? else {
        return Ok(json!({ "found": false }));
    };
    Ok(json!({
        "found": true,
        "context_tokens": status.context_tokens,
        "startup_tokens": status.startup_tokens,
        "requests": status.requests_since_compaction,
        "compaction_at": status.usual_compaction_at,
        "top": status.loaded_since_compaction.iter().take(TOP).collect::<Vec<_>>(),
    }))
}

fn dieta(store: &Store) -> Result<Value> {
    let eff = efficiency::efficiency(store.conn(), "claude")?;
    let listed = eff.snapshots.get("skill_listing_items").and_then(|v| v.as_u64().or_else(|| v.as_str()?.parse().ok()));
    let top: Vec<Value> = advice::suggestions(&eff, listed)
        .into_iter()
        .take(TOP)
        .map(|s| json!({ "id": s.id, "title": s.title, "share": s.saving_share, "confidence": s.confidence }))
        .collect();
    Ok(json!({ "suggestions": top, "applicable": apply::applicable("claude") }))
}

/// Before/after for every change applied in this project.
pub fn outcomes(store: &Store, roots: &Roots, state_dir: &Path, project_dir: &Path) -> Result<Vec<(apply::Change, Outcome)>> {
    let here = project_dir.to_string_lossy();
    let label = raiox_core::project_label(project_dir, &roots.home);
    apply::ledger(&state_dir.join("changes.json"))?
        .into_iter()
        .filter(|c| c.file.starts_with(here.as_ref()))
        .map(|c| {
            let source = apply::source_of(&c.suggestion).unwrap_or("claude");
            let measured = outcome::measure(store.conn(), &c, source, &label, now_ms())?;
            Ok((c, measured))
        })
        .collect()
}

/// Memo reports the lessons and, being asked, keeps each conclusive result in ai-memory once.
fn memo(store: &Store, roots: &Roots, state_dir: &Path, project_dir: &Path) -> Result<Value> {
    let mut lessons = Vec::new();
    for (change, measured) in outcomes(store, roots, state_dir, project_dir)? {
        let kept_now = if measured.conclusive && change.result_kept.is_none() {
            match crate::memo::keep_lesson(project_dir, &change, "", Some(&measured)) {
                Ok(path) => apply::set_result_kept(&change.id, &path, state_dir).map(|_| true).unwrap_or(false),
                Err(_) => false,
            }
        } else {
            false
        };
        lessons.push(json!({
            "suggestion": change.suggestion,
            "lesson": change.lesson,
            "applied_ms": change.applied_ms,
            "undone": change.undone_ms.is_some(),
            "outcome": measured,
            "result_kept": change.result_kept.is_some() || kept_now,
            "kept_now": kept_now,
        }));
    }
    Ok(json!({ "lessons": lessons, "ai_memory": ai_memory_state(project_dir), "min_requests": outcome::MIN_REQUESTS, "min_sessions": outcome::MIN_SESSIONS }))
}

/// Whether ai-memory knows this project, asked through its own CLI.
fn ai_memory_state(project_dir: &Path) -> &'static str {
    match Command::new("ai-memory").args(["message", "list"]).current_dir(project_dir).output() {
        Err(_) => "missing",
        Ok(out) if out.status.success() => "known",
        Ok(out) if String::from_utf8_lossy(&out.stderr).contains("not found") => "new_project",
        Ok(_) => "unreachable",
    }
}
