//! Parallel conversations with one bot. The main one is the chat every bot
//! already had (its file and its session stay where they were); each new one
//! keeps its own messages and its own CLI session, so a short question does
//! not re-read a long conversation.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::room;

pub const MAIN: &str = "main";
/// Conversations a bot keeps besides the main one.
const MAX_THREADS: usize = 12;
const MAX_TITLE: usize = 40;
static INDEX: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Thread {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub session: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub created_ms: i64,
    /// Task conversations do not use up the slots for parallel conversations.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub task: bool,
}

fn folder(state_dir: &Path, bot: &str) -> PathBuf {
    state_dir.join("bots").join(bot)
}

fn index(state_dir: &Path, bot: &str) -> PathBuf {
    folder(state_dir, bot).join("threads.json")
}

pub fn valid(thread: &str) -> bool {
    thread == MAIN || (!thread.is_empty() && thread.len() <= 24 && thread.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit()))
}

/// Where a conversation's messages live; the main one keeps the old path.
pub fn chat_file(state_dir: &Path, bot: &str, thread: &str) -> PathBuf {
    if thread == MAIN {
        state_dir.join("bots").join(format!("{bot}.json"))
    } else {
        folder(state_dir, bot).join(format!("{thread}.json"))
    }
}

pub fn list(state_dir: &Path, bot: &str) -> Vec<Thread> {
    fs::read_to_string(index(state_dir, bot)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn save(state_dir: &Path, bot: &str, threads: &[Thread]) -> Result<()> {
    fs::create_dir_all(folder(state_dir, bot))?;
    let file = index(state_dir, bot);
    let temp = file.with_extension("tmp");
    fs::write(&temp, serde_json::to_string_pretty(threads)? + "\n")?;
    fs::rename(temp, file)?;
    Ok(())
}

pub fn create(state_dir: &Path, bot: &str, title: &str) -> Result<Thread> {
    create_with_limit(state_dir, bot, title, MAX_THREADS)
}

pub fn create_with_limit(state_dir: &Path, bot: &str, title: &str, max_threads: usize) -> Result<Thread> {
    let _guard = INDEX.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let threads = list(state_dir, bot);
    if threads.iter().filter(|thread| !thread.task).count() >= max_threads {
        bail!("a bot keeps at most {max_threads} extra conversations; close one first");
    }
    let now = room::now_ms();
    // The id comes from the time; two made in the same millisecond take the next free one.
    let mut stamp = now;
    while threads.iter().any(|t| t.id == format!("t{stamp:x}")) {
        stamp += 1;
    }
    let thread = Thread { id: format!("t{stamp:x}"), title: title.chars().take(MAX_TITLE).collect(), session: None, model: None, created_ms: now, task: false };
    save(state_dir, bot, &threads.into_iter().chain([thread.clone()]).collect::<Vec<_>>())?;
    Ok(thread)
}

/// A task id gives its conversation a stable name across starts and retries.
pub(crate) fn create_for_task(state_dir: &Path, bot: &str, id: &str, title: &str) -> Result<bool> {
    if !id.starts_with('k') || !valid(id) {
        bail!("invalid task conversation");
    }
    let _guard = INDEX.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut threads = list(state_dir, bot);
    if let Some(thread) = threads.iter().find(|thread| thread.id == id) {
        if !thread.task {
            bail!("the task conversation id is already in use");
        }
        return Ok(false);
    }
    threads.push(Thread { id: id.to_string(), title: title.chars().take(MAX_TITLE).collect(), session: None, model: None, created_ms: room::now_ms(), task: true });
    save(state_dir, bot, &threads)?;
    Ok(true)
}

pub fn delete(state_dir: &Path, bot: &str, thread: &str) -> Result<()> {
    if thread == MAIN || !valid(thread) {
        bail!("the main conversation stays");
    }
    let _guard = INDEX.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let threads = list(state_dir, bot);
    if !threads.iter().any(|t| t.id == thread) {
        bail!("no conversation {thread}");
    }
    save(state_dir, bot, &threads.into_iter().filter(|t| t.id != thread).collect::<Vec<_>>())?;
    let _ = fs::remove_file(chat_file(state_dir, bot, thread));
    Ok(())
}

/// The CLI session of a side conversation (the main one's lives on the bot).
pub fn session(state_dir: &Path, bot: &str, thread: &str) -> Result<Option<String>> {
    list(state_dir, bot).into_iter().find(|t| t.id == thread).map(|t| t.session).context("no such conversation")
}

pub fn set_session(state_dir: &Path, bot: &str, thread: &str, session: Option<String>) -> Result<()> {
    let _guard = INDEX.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let threads = list(state_dir, bot).into_iter().map(|t| if t.id == thread { Thread { session: session.clone(), ..t } } else { t }).collect::<Vec<_>>();
    save(state_dir, bot, &threads)
}

/// Optional per-conversation model override (used by free conversations).
pub fn model(state_dir: &Path, bot: &str, thread: &str) -> Option<String> {
    if thread == MAIN {
        return None;
    }
    list(state_dir, bot).into_iter().find(|t| t.id == thread).and_then(|t| t.model)
}

/// Sets or clears a conversation's own model override.
pub fn set_model(state_dir: &Path, bot: &str, thread: &str, model: Option<String>) -> Result<()> {
    if thread == MAIN || !valid(thread) {
        bail!("invalid conversation");
    }
    let _guard = INDEX.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let threads = list(state_dir, bot);
    if !threads.iter().any(|t| t.id == thread) {
        bail!("no such conversation");
    }
    let updated: Vec<Thread> = threads.into_iter().map(|t| if t.id == thread { Thread { model: model.clone(), ..t } } else { t }).collect();
    save(state_dir, bot, &updated)
}

/// A new conversation is named by its first question.
pub fn name_after(state_dir: &Path, bot: &str, thread: &str, text: &str, default_prefix: &str) -> Result<()> {
    let _guard = INDEX.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let title: String = text.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(MAX_TITLE).collect();
    let threads = list(state_dir, bot)
        .into_iter()
        .map(|t| if t.id == thread && !t.task && t.title.starts_with(default_prefix) { Thread { title: title.clone(), ..t } } else { t })
        .collect::<Vec<_>>();
    save(state_dir, bot, &threads)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversations_made_in_the_same_millisecond_get_their_own_ids() {
        let state = std::env::temp_dir().join(format!("raiox-thread-ids-{}", std::process::id()));
        let _ = fs::remove_dir_all(&state);
        let ids: Vec<String> = (0..MAX_THREADS).map(|n| create(&state, "b1", &format!("c{n}")).unwrap().id).collect();
        let unique: std::collections::HashSet<&String> = ids.iter().collect();
        assert_eq!(unique.len(), ids.len(), "every conversation has its own id: {ids:?}");
        assert!(ids.iter().all(|id| valid(id)));
        let _ = fs::remove_dir_all(&state);
    }

    #[test]
    fn task_conversations_do_not_use_parallel_conversation_slots() {
        let state = std::env::temp_dir().join(format!("raiox-task-thread-limit-{}", std::process::id()));
        for index in 0..=MAX_THREADS {
            assert!(create_for_task(&state, "b1", &format!("k{index}"), "Task").unwrap());
        }
        assert!(!create_for_task(&state, "b1", "k0", "Same task").unwrap());
        for _ in 0..MAX_THREADS {
            create(&state, "b1", "Conversation").unwrap();
        }
        assert!(create(&state, "b1", "Too many parallel conversations").is_err());
        let _ = fs::remove_dir_all(state);
    }

    #[test]
    fn ids_cannot_reach_other_folders() {
        assert!(valid("main") && valid("t19a2b"));
        assert!(!valid("../x") && !valid("") && !valid("T1"));
    }

    #[test]
    fn the_main_conversation_keeps_its_old_file() {
        let state = Path::new("/s");
        assert_eq!(chat_file(state, "b1", MAIN), PathBuf::from("/s/bots/b1.json"));
        assert_eq!(chat_file(state, "b1", "t1"), PathBuf::from("/s/bots/b1/t1.json"));
    }
}
