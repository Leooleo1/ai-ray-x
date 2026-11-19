//! The agents room of one project. It only works in projects that opted in by
//! installing the token-diet skill (`.claude/skills/token-diet`), and agents
//! only speak when the person clicks or runs `raiox ask` (from the skill).
//! Answers are data; the panel renders them in each agent's voice.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Result};
use raiox_core::outcome::{self, Outcome};
use raiox_core::{advice, apply, efficiency, week, Roots, Store};
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
    /// "you", "raio", "dieta", "memo" or "bot".
    pub author: String,
    /// What happened: "ask", "answer", "applied", "undone", "kept", "kept_failed";
    /// in a bot chat also "approve", "failed" and "reset".
    pub kind: String,
    pub data: Value,
}

pub fn opted_in(project_dir: &Path) -> bool {
    project_dir.join(OPT_IN).exists()
}

/// A project opts in by installing the token-diet skill or by turning on a
/// bot in it: Raio, Dieta and Memo answer inside each bot's chat.
pub fn allowed(state_dir: &Path, project_dir: &Path) -> bool {
    opted_in(project_dir) || crate::bots::list(state_dir).iter().any(|bot| bot.dir == project_dir)
}

/// Refuses to act outside projects that opted in.
pub fn require_opt_in(state_dir: &Path, project_dir: &Path) -> Result<()> {
    if !allowed(state_dir, project_dir) {
        bail!("raiox agents only act in projects with {OPT_IN} or a bot; {} has neither", project_dir.display());
    }
    Ok(())
}

/// What one of the house agents says about a project, computed now from the logs.
pub fn answer(store: &Store, roots: &Roots, state_dir: &Path, project_dir: &Path, agent: &str) -> Result<Value> {
    require_opt_in(state_dir, project_dir)?;
    match agent {
        "raio" => raio(store, roots, project_dir),
        "dieta" => dieta(store, roots, project_dir),
        "memo" => memo(store, roots, state_dir, project_dir),
        other => bail!("unknown agent {other}"),
    }
}

fn room_file(state_dir: &Path, project_dir: &Path) -> PathBuf {
    let hash = project_dir.to_string_lossy().bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3));
    state_dir.join("rooms").join(format!("{hash:016x}.json"))
}

pub fn append(state_dir: &Path, project_dir: &Path, new: Vec<Message>) -> Result<()> {
    append_messages(&room_file(state_dir, project_dir), new)
}

/// One conversation file: the agents room of a project, or a bot's chat.
pub fn read_messages(file: &Path) -> Vec<Message> {
    fs::read_to_string(file).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
}

/// Appends and keeps the latest messages; written to a temporary file first, then renamed.
pub fn append_messages(file: &Path, new: Vec<Message>) -> Result<()> {
    let all: Vec<Message> = read_messages(file).into_iter().chain(new).collect();
    let kept = &all[all.len().saturating_sub(MAX_MESSAGES)..];
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent)?;
    }
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
    let data = answer(store, roots, state_dir, project_dir, agent)?;
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

/// How long a project's week is reused before it is read again from the index.
const WEEK_TTL_MS: i64 = 10 * 60_000;
static WEEKS: std::sync::Mutex<Vec<(String, i64, week::Week)>> = std::sync::Mutex::new(Vec::new());

/// A project's week for Raio's report and tips, read from the index at most every ten minutes.
pub fn week_of(store: &Store, home: &Path, project_dir: &Path) -> Option<week::Week> {
    let project = raiox_core::project_label(project_dir, home);
    let now = now_ms();
    let mut cache = WEEKS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some((_, _, cached)) = cache.iter().find(|(p, at, _)| *p == project && now - at < WEEK_TTL_MS) {
        return Some(cached.clone());
    }
    let fresh = week::week(store.conn(), &project, now).ok()?;
    cache.retain(|(p, _, _)| *p != project);
    cache.push((project, now, fresh.clone()));
    Some(fresh)
}

fn raio(store: &Store, roots: &Roots, project_dir: &Path) -> Result<Value> {
    let week = week_of(store, &roots.home, project_dir);
    let Some(session) = current_session(store, roots, project_dir) else {
        return Ok(json!({ "found": false, "week": week }));
    };
    let Some(status) = advice::context_status(store.conn(), Some(&session))? else {
        return Ok(json!({ "found": false, "week": week }));
    };
    Ok(json!({
        "found": true,
        "week": week,
        "context_tokens": status.context_tokens,
        "startup_tokens": status.startup_tokens,
        "requests": status.requests_since_compaction,
        "compaction_at": status.usual_compaction_at,
        "top": status.loaded_since_compaction.iter().take(TOP).collect::<Vec<_>>(),
    }))
}

/// The cuts as computed now for a bot's project, with what the Dieta card draws: the full
/// suggestions, the compaction model and where the numbers come from.
pub fn project_diet(store: &Store, roots: &Roots, project_dir: &Path) -> Result<Value> {
    let scoped = efficiency::for_project(store.conn(), "claude", &raiox_core::project_label(project_dir, &roots.home))?;
    let eff = &scoped.efficiency;
    let listed = eff.snapshots.get("skill_listing_items").and_then(|v| v.as_u64().or_else(|| v.as_str()?.parse().ok()));
    Ok(json!({
        "suggestions": advice::suggestions(eff, listed),
        "compaction": eff.compaction,
        "scope": { "own": scoped.own, "calls": scoped.project_calls, "minimum": efficiency::MIN_PROJECT_CALLS },
    }))
}

/// The cuts for this project, from its own requests when it has enough of them.
fn dieta(store: &Store, roots: &Roots, project_dir: &Path) -> Result<Value> {
    let scoped = efficiency::for_project(store.conn(), "claude", &raiox_core::project_label(project_dir, &roots.home))?;
    let eff = &scoped.efficiency;
    let listed = eff.snapshots.get("skill_listing_items").and_then(|v| v.as_u64().or_else(|| v.as_str()?.parse().ok()));
    let top: Vec<Value> = advice::suggestions(eff, listed)
        .into_iter()
        .take(TOP)
        .map(|s| json!({ "id": s.id, "title": s.title, "share": s.saving_share, "confidence": s.confidence, "value": s.value }))
        .collect();
    let scope = json!({ "own": scoped.own, "calls": scoped.project_calls, "minimum": efficiency::MIN_PROJECT_CALLS });
    Ok(json!({ "suggestions": top, "applicable": apply::applicable("claude"), "scope": scope }))
}

/// Before/after for every change applied in this project.
pub fn outcomes(store: &Store, roots: &Roots, state_dir: &Path, project_dir: &Path) -> Result<Vec<(apply::Change, Outcome)>> {
    let label = raiox_core::project_label(project_dir, &roots.home);
    apply::ledger(&state_dir.join("changes.json"))?
        .into_iter()
        // Component-wise: a project named like the start of another (app, app-v2) keeps its own changes.
        .filter(|c| Path::new(&c.file).starts_with(project_dir))
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
    Ok(json!({
        "lessons": lessons,
        "ai_memory": ai_memory_state(project_dir),
        "care": care(project_dir),
        "min_requests": outcome::MIN_REQUESTS,
        "min_sessions": outcome::MIN_SESSIONS,
    }))
}

/// A durable page untouched this long is worth a look.
const STALE_DAYS: i64 = 120;

/// Memo's look at the project's durable pages in ai-memory: titles repeated across pages, and
/// pages untouched for months. Session transcripts are left out: ai-memory prunes those itself.
pub fn care(project_dir: &Path) -> Vec<Value> {
    let pages = crate::bot_mail::pages(project_dir).ok().flatten().unwrap_or_default();
    durable_findings(&pages, chrono::Utc::now())
}

fn durable_findings(pages: &[Value], now: chrono::DateTime<chrono::Utc>) -> Vec<Value> {
    let durable: Vec<&Value> = pages
        .iter()
        .filter(|p| p["path"].as_str().is_some_and(|path| !path.starts_with("sessions/") && path != crate::team::PAGE))
        .collect();
    let mut by_title: std::collections::BTreeMap<String, Vec<&str>> = std::collections::BTreeMap::new();
    for page in &durable {
        let title = page["title"].as_str().unwrap_or("").trim().to_lowercase();
        if !title.is_empty() {
            by_title.entry(title).or_default().push(page["path"].as_str().unwrap_or(""));
        }
    }
    let mut findings: Vec<Value> =
        by_title.into_iter().filter(|(_, paths)| paths.len() > 1).map(|(title, paths)| json!({ "kind": "duplicate", "message": title, "pages": paths })).collect();
    let limit = now - chrono::Duration::days(STALE_DAYS);
    let stale: Vec<&str> = durable
        .iter()
        .filter(|p| p["updated_at"].as_str().and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok()).is_some_and(|at| at < limit))
        .filter_map(|p| p["path"].as_str())
        .collect();
    if !stale.is_empty() {
        findings.push(json!({ "kind": "stale", "message": null, "pages": stale }));
    }
    findings
}

/// Deletes one durable page Memo pointed to in the project's ai-memory (only those it pointed to).
pub fn forget(project_dir: &Path, path: &str) -> Result<()> {
    let listed = care(project_dir).iter().any(|f| f["pages"].as_array().is_some_and(|pages| pages.iter().any(|p| p.as_str() == Some(path))));
    if !listed {
        bail!("Memo only deletes pages it pointed to as repeated or stale");
    }
    let output = crate::procs::command("ai-memory").args(["delete-page", "--path", path]).current_dir(project_dir).output()?;
    if !output.status.success() {
        bail!("ai-memory delete-page failed: {}", String::from_utf8_lossy(&output.stderr).lines().last().unwrap_or(""));
    }
    Ok(())
}

/// Whether ai-memory knows this project, asked through its own CLI.
fn ai_memory_state(project_dir: &Path) -> &'static str {
    match crate::procs::command("ai-memory").args(["message", "list"]).current_dir(project_dir).output() {
        Err(_) => "missing",
        Ok(out) if out.status.success() => "known",
        Ok(out) if String::from_utf8_lossy(&out.stderr).contains("not found") => "new_project",
        Ok(_) => "unreachable",
    }
}

#[cfg(test)]
mod care_tests {
    use super::*;

    #[test]
    fn memo_looks_only_at_durable_pages() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-11-19T12:00:00Z").unwrap().to_utc();
        let page = |path: &str, title: &str, at: &str| json!({ "path": path, "title": title, "updated_at": at });
        let pages = vec![
            page("sessions/a.md", "pre-tool-use", "2026-11-18T10:00:00Z"),
            page("sessions/b.md", "pre-tool-use", "2026-11-18T10:00:00Z"),
            page("notas/a.md", "Convenção de testes", "2026-11-18T10:00:00Z"),
            page("notas/b.md", "convenção de testes ", "2026-11-18T10:00:00Z"),
            page("notas/velha.md", "Deploy antigo", "2026-05-01T10:00:00Z"),
            page("raiox/equipe.md", "Equipe", "2026-01-01T10:00:00Z"),
        ];
        let found = durable_findings(&pages, now);
        assert_eq!(found.len(), 2, "{found:?}");
        assert_eq!(found[0]["pages"], json!(["notas/a.md", "notas/b.md"]));
        assert_eq!((found[1]["kind"].as_str(), &found[1]["pages"]), (Some("stale"), &json!(["notas/velha.md"])));
    }
}
