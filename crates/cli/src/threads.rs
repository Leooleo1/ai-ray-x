//! Parallel conversations with one bot. The main one is the chat every bot
//! already had (its file and its session stay where they were); each new one
//! keeps its own messages and its own CLI session, so a short question does
//! not re-read a long conversation.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::room;

pub const MAIN: &str = "main";
/// Conversations a bot keeps besides the main one.
const MAX_THREADS: usize = 12;
const MAX_TITLE: usize = 40;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Thread {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub session: Option<String>,
    pub created_ms: i64,
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
    let threads = list(state_dir, bot);
    if threads.len() >= MAX_THREADS {
        bail!("a bot keeps at most {MAX_THREADS} extra conversations; close one first");
    }
    let now = room::now_ms();
    let thread = Thread { id: format!("t{now:x}"), title: title.chars().take(MAX_TITLE).collect(), session: None, created_ms: now };
    save(state_dir, bot, &threads.into_iter().chain([thread.clone()]).collect::<Vec<_>>())?;
    Ok(thread)
}

pub fn delete(state_dir: &Path, bot: &str, thread: &str) -> Result<()> {
    if thread == MAIN || !valid(thread) {
        bail!("the main conversation stays");
    }
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
    let threads = list(state_dir, bot).into_iter().map(|t| if t.id == thread { Thread { session: session.clone(), ..t } } else { t }).collect::<Vec<_>>();
    save(state_dir, bot, &threads)
}

/// A new conversation is named by its first question.
pub fn name_after(state_dir: &Path, bot: &str, thread: &str, text: &str, default_prefix: &str) -> Result<()> {
    let title: String = text.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(MAX_TITLE).collect();
    let threads = list(state_dir, bot)
        .into_iter()
        .map(|t| if t.id == thread && t.title.starts_with(default_prefix) { Thread { title: title.clone(), ..t } } else { t })
        .collect::<Vec<_>>();
    save(state_dir, bot, &threads)
}

#[cfg(test)]
mod tests {
    use super::*;

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
