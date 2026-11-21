//! One question to several bots at once. Each answers in a new parallel
//! conversation named by the question, so the answers sit side by side
//! without touching anyone's main conversation. A bot that cannot take it
//! (at work, or with no room for another conversation) is left out and said so.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

use crate::bots::{self, Jobs};
use crate::{room, threads};

const MAX_BOTS: usize = 12;
const MAX_KEPT: usize = 20;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub bot: String,
    pub thread: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Batch {
    pub id: String,
    pub text: String,
    pub created_ms: i64,
    pub items: Vec<Item>,
    /// Bots that could not take the question.
    #[serde(default)]
    pub skipped: Vec<String>,
}

fn file(state_dir: &Path) -> PathBuf {
    state_dir.join("batches.json")
}

pub fn list(state_dir: &Path) -> Vec<Batch> {
    fs::read_to_string(file(state_dir)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn save(state_dir: &Path, batches: &[Batch]) -> Result<()> {
    fs::create_dir_all(state_dir)?;
    let temp = file(state_dir).with_extension("tmp");
    fs::write(&temp, serde_json::to_string_pretty(batches)? + "\n")?;
    fs::rename(temp, file(state_dir))?;
    Ok(())
}

pub fn ask_many(state_dir: &Path, jobs: &Jobs, ids: &[String], text: &str) -> Result<()> {
    let text = text.trim();
    if text.is_empty() || text.chars().count() > bots::MAX_TEXT {
        bail!("a message has 1 to {} characters", bots::MAX_TEXT);
    }
    if ids.is_empty() || ids.len() > MAX_BOTS {
        bail!("pick 1 to {MAX_BOTS} bots");
    }
    let known: Vec<String> = bots::project_list(state_dir).into_iter().map(|b| b.id).collect();
    let (mut items, mut skipped) = (Vec::new(), Vec::new());
    for id in ids.iter().filter(|id| known.contains(id)) {
        let asked = threads::create(state_dir, id, text).and_then(|thread| bots::ask(state_dir, jobs, id, &thread.id, text, &[]).map(|()| thread.id));
        match asked {
            Ok(thread) => items.push(Item { bot: id.clone(), thread }),
            Err(_) => skipped.push(id.clone()),
        }
    }
    if items.is_empty() {
        bail!("no bot could take the question now");
    }
    let batch = Batch { id: format!("g{:x}", room::now_ms()), text: text.to_string(), created_ms: room::now_ms(), items, skipped };
    let kept: Vec<Batch> = [batch].into_iter().chain(list(state_dir)).take(MAX_KEPT).collect();
    save(state_dir, &kept)
}

pub fn delete(state_dir: &Path, id: &str) -> Result<()> {
    save(state_dir, &list(state_dir).into_iter().filter(|b| b.id != id).collect::<Vec<_>>())
}
