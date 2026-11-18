//! Messages the person pinned in a bot's conversations, so the important
//! answers stay one click away. Only where they are is kept (conversation
//! and time); the text stays in the conversation itself.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

use crate::threads;

const MAX_PINS: usize = 30;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pin {
    pub thread: String,
    pub ms: i64,
}

fn file(state_dir: &Path, bot: &str) -> PathBuf {
    state_dir.join("bots").join(bot).join("pins.json")
}

pub fn list(state_dir: &Path, bot: &str) -> Vec<Pin> {
    fs::read_to_string(file(state_dir, bot)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn save(state_dir: &Path, bot: &str, pins: &[Pin]) -> Result<()> {
    let path = file(state_dir, bot);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temp = path.with_extension("tmp");
    fs::write(&temp, serde_json::to_string_pretty(pins)? + "\n")?;
    fs::rename(temp, path)?;
    Ok(())
}

/// Pins a message, or unpins it when it already is.
pub fn toggle(state_dir: &Path, bot: &str, thread: &str, ms: i64) -> Result<()> {
    if !threads::valid(thread) {
        bail!("invalid conversation");
    }
    let pins = list(state_dir, bot);
    let pin = Pin { thread: thread.to_string(), ms };
    let next: Vec<Pin> = if pins.contains(&pin) {
        pins.into_iter().filter(|p| p != &pin).collect()
    } else if pins.len() >= MAX_PINS {
        bail!("at most {MAX_PINS} pinned messages per bot");
    } else {
        pins.into_iter().chain([pin]).collect()
    };
    save(state_dir, bot, &next)
}
