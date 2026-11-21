//! Recovery of bot turns interrupted when raiox closed or crashed.
//!
//! At panel startup (before any jobs start), each bot's main and side
//! conversations are checked for a trailing unanswered turn (`ask`, `ran`,
//! `shot`, or `approve`). Any CLI output left in `<state>/bots/<id>.run/` is
//! parsed with `bot_trail` to recover the CLI session id and the steps already
//! taken, then an `interrupted` note is appended. Nothing runs automatically on
//! startup; the person chooses whether to Continue or Try again.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::{list, note, Bot};
use crate::bot_trail::Trail;
use crate::room::{self, Message};
use crate::threads::{self, MAIN};

/// Active turn metadata file written while a bot turn runs and removed on normal exit.
const ACTIVE_FILE: &str = "active.json";
/// Grace window when matching scratch output mtime against the interrupted message timestamp.
const MTIME_GRACE_MS: i64 = 2_000;

/// Messages that sit alongside a turn without ending or starting one.
const PASSIVE_KINDS: [&str; 10] = [
    "house",
    "remembered",
    "sent",
    "review",
    "review_asked",
    "carried",
    "turn_undone",
    "draft_applied",
    "draft_discarded",
    "draft_undone",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct ActiveTurn {
    pub thread: String,
    pub agent: String,
    pub full_access: bool,
    pub keep_session: bool,
    pub started_ms: i64,
}

/// Removes `active.json` when the turn finishes normally; left behind only if the process dies mid-turn.
pub(super) struct ActiveGuard(PathBuf);

impl ActiveGuard {
    pub(super) fn enter(scratch: &Path, info: &ActiveTurn) -> Self {
        let path = scratch.join(ACTIVE_FILE);
        if fs::create_dir_all(scratch).is_ok()
            && let Ok(body) = serde_json::to_string(info)
        {
            let _ = fs::write(&path, body);
        }
        Self(path)
    }
}

impl Drop for ActiveGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn read_active(scratch: &Path) -> Option<ActiveTurn> {
    let raw = fs::read_to_string(scratch.join(ACTIVE_FILE)).ok()?;
    serde_json::from_str(&raw).ok()
}

/// Returns the trailing message of an unfinished turn, or `None` when the conversation ended cleanly.
fn interrupted_trigger(messages: &[Message]) -> Option<&Message> {
    let last = messages.iter().rev().find(|m| !PASSIVE_KINDS.contains(&m.kind.as_str()))?;
    matches!(last.kind.as_str(), "ask" | "ran" | "shot" | "approve").then_some(last)
}

fn file_mtime_ms(path: &Path) -> Option<i64> {
    let modified = fs::metadata(path).ok()?.modified().ok()?;
    let ms = modified.duration_since(UNIX_EPOCH).ok()?.as_millis();
    i64::try_from(ms).ok()
}

fn is_claude_result_line(line: &str) -> bool {
    serde_json::from_str::<Value>(line).is_ok_and(|e| e.get("type").and_then(Value::as_str) == Some("result"))
}

fn has_final_result(agent: &str, lines: &[&str]) -> bool {
    let Some(last) = lines.iter().rev().find(|l| !l.trim().is_empty()) else { return false };
    let Ok(event) = serde_json::from_str::<Value>(last) else { return false };
    match agent {
        "claude" | "cursor" => event.get("type").and_then(Value::as_str) == Some("result"),
        "codex" => event.get("type").and_then(Value::as_str) == Some("turn.completed"),
        "antigravity" => event.get("event").and_then(Value::as_str) == Some("result"),
        "opencode" => event.get("type").and_then(Value::as_str) == Some("step_finish") && event.pointer("/part/reason").and_then(Value::as_str) == Some("stop"),
        _ => false,
    }
}

/// Extracts session id and partial trail from a Claude live output file (`live-*.out`),
/// slicing after the last completed `result` line so earlier turns are not replayed.
fn parse_live_output(path: &Path, dir: &Path) -> (Option<String>, Option<Trail>) {
    let Ok(raw) = fs::read_to_string(path) else { return (None, None) };
    let lines: Vec<&str> = raw.lines().collect();
    let mut last_session: Option<String> = None;
    for line in &lines {
        if let Ok(event) = serde_json::from_str::<Value>(line)
            && let Some(id) = event.get("session_id").and_then(Value::as_str).filter(|s| !s.is_empty())
        {
            last_session = Some(id.to_string());
        }
    }
    let after = lines.iter().rposition(|l| is_claude_result_line(l)).map_or(0, |idx| idx + 1);
    let mut trail = Trail::default();
    for line in &lines[after..] {
        trail.feed("claude", line, dir);
    }
    let session = trail.session.clone().or(last_session);
    let kept = (!trail.items.is_empty() || !trail.partial.trim().is_empty()).then_some(trail);
    (session, kept)
}

/// Extracts session id and partial trail from a single-run output file (`out.txt`).
fn parse_run_output(path: &Path, agent: &str, dir: &Path, check_complete: bool) -> (Option<String>, Option<Trail>) {
    let Ok(raw) = fs::read_to_string(path) else { return (None, None) };
    let lines: Vec<&str> = raw.lines().collect();
    if check_complete && has_final_result(agent, &lines) {
        return (None, None);
    }
    let mut trail = Trail::default();
    for line in &lines {
        trail.feed(agent, line, dir);
    }
    let session = trail.session.clone().filter(|s| !s.trim().is_empty());
    let kept = (!trail.items.is_empty() || !trail.partial.trim().is_empty()).then_some(trail);
    (session, kept)
}

fn recover_output(bot: &Bot, base_scratch: &Path, scratch: &Path, thread: &str, trigger_ms: i64, active: Option<&ActiveTurn>) -> (Option<String>, Option<Trail>) {
    let matches_active = active.is_some_and(|a| a.thread == thread);
    let fresh_enough = |path: &Path| matches_active || file_mtime_ms(path).is_some_and(|mtime| mtime + MTIME_GRACE_MS >= trigger_ms);

    if bot.agent == "claude" {
        let conv = format!("{}:{thread}", bot.id);
        for dir in [scratch, base_scratch] {
            let live_path = crate::live::out_file(dir, &conv);
            if live_path.is_file() && fresh_enough(&live_path) {
                let (session, trail) = parse_live_output(&live_path, &bot.dir);
                if session.is_some() || trail.is_some() {
                    return (session, trail);
                }
            }
        }
    }

    if active.is_some() && !matches_active {
        return (None, None);
    }
    let out_path = scratch.join("out.txt");
    if out_path.is_file() && fresh_enough(&out_path) {
        return parse_run_output(&out_path, &bot.agent, &bot.dir, !matches_active);
    }
    (None, None)
}

/// Checks one conversation and appends an `interrupted` marker if its last turn was cut off.
fn recover_thread(state_dir: &Path, bot: &Bot, base_scratch: &Path, scratch: &Path, thread: &str, active: Option<&ActiveTurn>) {
    let file = threads::chat_file(state_dir, &bot.id, thread);
    let messages = room::read_messages(&file);
    let Some(trigger) = interrupted_trigger(&messages) else { return };
    let matches_active = active.is_some_and(|a| a.thread == thread);
    let (mut session, trail) = recover_output(bot, base_scratch, scratch, thread, trigger.ms, active);

    // Non-session turns (draft edits, side questions, cross-CLI audits) cannot resume the conversation session.
    let aside = trigger.data.get("aside").and_then(Value::as_bool).unwrap_or(false);
    let audit = trigger.data.get("audit").and_then(Value::as_bool).unwrap_or(false);
    let keep_session = !aside && !audit && trigger.kind != "approve" && (!matches_active || active.is_none_or(|a| a.keep_session));
    if !keep_session {
        session = None;
    }

    let retry_source = messages.iter().rev().find(|m| m.author == "you" && m.kind == "ask");
    let retry_text = retry_source
        .and_then(|m| m.data.get("text").and_then(Value::as_str))
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string);
    let retry_aside = retry_source.and_then(|m| m.data.get("aside").and_then(Value::as_bool)).unwrap_or(false);
    let full_access = active.filter(|a| a.thread == thread).map_or(bot.grants.full_access, |a| a.full_access);

    let mut data = json!({ "full_access": full_access });
    if let Some(session) = session {
        data["session"] = json!(session);
    }
    if let Some(trail) = trail {
        data["trail"] = json!(trail);
    }
    if let Some(text) = retry_text {
        data["retry_text"] = json!(text);
    }
    if retry_aside {
        data["aside"] = json!(true);
    }
    let _ = room::append_messages(&file, vec![note("interrupted", data)]);
}

/// Scans every bot and conversation at panel startup and marks turns cut off when raiox closed.
pub fn all(state_dir: &Path) {
    for bot in list(state_dir) {
        let base_scratch = super::turn::scratch_dir(state_dir, &bot.id, MAIN);
        let thread_ids = std::iter::once(MAIN.to_string()).chain(threads::list(state_dir, &bot.id).into_iter().map(|t| t.id));
        for thread in thread_ids {
            let scratch = super::turn::scratch_dir(state_dir, &bot.id, &thread);
            let active = read_active(&scratch).or_else(|| read_active(&base_scratch).filter(|a| a.thread == thread));
            recover_thread(state_dir, &bot, &base_scratch, &scratch, &thread, active.as_ref());
            let _ = fs::remove_file(scratch.join(ACTIVE_FILE));
        }
        let _ = fs::remove_file(base_scratch.join(ACTIVE_FILE));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bots::{activate, continue_turn, find, said, set_grants, Jobs};

    fn temp_home(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("raiox-recover-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("Projects/app")).unwrap();
        dir
    }

    #[test]
    fn startup_recovery_extracts_codex_session_and_trail_and_is_idempotent() {
        let home = temp_home("codex");
        let state = home.join(".state");
        let bot = activate(&state, &home, "Projects/app", "codex", None).unwrap();
        let chat = threads::chat_file(&state, &bot.id, MAIN);
        room::append_messages(&chat, vec![said("ask", json!({ "text": "Analyze the build" }))]).unwrap();

        let scratch = state.join("bots").join(format!("{}.run", bot.id));
        fs::create_dir_all(&scratch).unwrap();
        fs::write(
            scratch.join("out.txt"),
            concat!(
                "{\"type\":\"thread.started\",\"thread_id\":\"th-cut-42\"}\n",
                "{\"type\":\"item.started\",\"item\":{\"id\":\"i1\",\"type\":\"command_execution\",\"command\":\"ls\"}}\n",
                "{\"type\":\"item.completed\",\"item\":{\"id\":\"i1\",\"type\":\"command_execution\",\"command\":\"ls\",\"aggregated_output\":\"Cargo.toml\\n\",\"exit_code\":0}}\n",
            ),
        )
        .unwrap();
        let active = ActiveTurn { thread: MAIN.into(), agent: "codex".into(), full_access: false, keep_session: true, started_ms: room::now_ms() };
        fs::write(scratch.join(ACTIVE_FILE), serde_json::to_string(&active).unwrap()).unwrap();

        all(&state);
        let messages = room::read_messages(&chat);
        assert_eq!(messages.len(), 2);
        let mark = &messages[1];
        assert_eq!(mark.kind, "interrupted");
        assert_eq!(mark.data["session"], "th-cut-42");
        assert_eq!(mark.data["retry_text"], "Analyze the build");
        assert_eq!(mark.data["full_access"], false);
        assert_eq!(mark.data["trail"]["items"].as_array().unwrap().len(), 1);
        assert!(!scratch.join(ACTIVE_FILE).exists(), "active.json is cleaned up after recovery");

        // Running recovery a second time does not append a duplicate marker.
        all(&state);
        assert_eq!(room::read_messages(&chat).len(), 2);
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn startup_recovery_slices_claude_live_output_after_last_result() {
        let home = temp_home("claude-live");
        let state = home.join(".state");
        let bot = activate(&state, &home, "Projects/app", "claude", None).unwrap();
        let chat = threads::chat_file(&state, &bot.id, MAIN);
        room::append_messages(&chat, vec![said("ask", json!({ "text": "Second question" }))]).unwrap();

        let scratch = state.join("bots").join(format!("{}.run", bot.id));
        fs::create_dir_all(&scratch).unwrap();
        let live_out = crate::live::out_file(&scratch, &format!("{}:{MAIN}", bot.id));
        fs::write(
            &live_out,
            concat!(
                "{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"s-live-1\"}\n",
                "{\"type\":\"assistant\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"Old answer.\"}]}}\n",
                "{\"type\":\"result\",\"result\":\"Old answer.\",\"session_id\":\"s-live-1\"}\n",
                "{\"type\":\"assistant\",\"message\":{\"content\":[{\"type\":\"tool_use\",\"id\":\"t1\",\"name\":\"Read\",\"input\":{\"file_path\":\"/p/a.rs\"}}]}}\n",
                "{\"type\":\"stream_event\",\"event\":{\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"Working on it\"}}}\n",
            ),
        )
        .unwrap();

        all(&state);
        let messages = room::read_messages(&chat);
        let mark = messages.last().unwrap();
        assert_eq!(mark.kind, "interrupted");
        assert_eq!(mark.data["session"], "s-live-1");
        assert_eq!(mark.data["trail"]["items"].as_array().unwrap().len(), 1, "only steps after the last completed result are kept");
        assert_eq!(mark.data["trail"]["partial"], "Working on it");
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn interrupted_turn_without_output_has_no_session_and_rejects_continue() {
        let home = temp_home("no-session");
        let state = home.join(".state");
        let bot = activate(&state, &home, "Projects/app", "claude", None).unwrap();
        let chat = threads::chat_file(&state, &bot.id, MAIN);
        room::append_messages(&chat, vec![said("ask", json!({ "text": "Hello" }))]).unwrap();

        all(&state);
        let messages = room::read_messages(&chat);
        let mark = messages.last().unwrap();
        assert_eq!(mark.kind, "interrupted");
        assert!(mark.data.get("session").is_none());
        assert_eq!(mark.data["retry_text"], "Hello");

        let err = continue_turn(&state, &Jobs::default(), &bot.id, MAIN).unwrap_err();
        assert!(err.to_string().contains("no session"), "{err:#}");
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn cursor_continue_is_refused_when_full_access_mode_changed() {
        let home = temp_home("cursor-mode");
        let state = home.join(".state");
        let bot = activate(&state, &home, "Projects/app", "cursor", None).unwrap();
        let chat = threads::chat_file(&state, &bot.id, MAIN);
        room::append_messages(
            &chat,
            vec![
                said("ask", json!({ "text": "Inspect repo" })),
                note("interrupted", json!({ "session": "cur-1", "retry_text": "Inspect repo", "full_access": false })),
            ],
        )
        .unwrap();

        // Turn on full access directly on the bot record (without restarting sessions yet) to verify continue_turn's guard.
        set_grants(&state, &bot.id, &json!({ "full_access": true })).unwrap();
        assert!(find(&state, &bot.id).unwrap().grants.full_access);
        let err = continue_turn(&state, &Jobs::default(), &bot.id, MAIN).unwrap_err();
        assert!(err.to_string().contains("access mode changed"), "{err:#}");
        let _ = fs::remove_dir_all(&home);
    }
}
