//! First-use milestones are kept beside the panel index and survive removal of the item that earned them.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::sync::Mutex;

use anyhow::Result;
use raiox_core::Store;
use serde::{Deserialize, Serialize};
use serde_json::Value;

const FILE: &str = "first-use.json";
static LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Milestone {
    Chat,
    Approval,
    Task,
    Link,
    Usage,
    Desk,
    Flow,
    Delivery,
    Wiki,
    Turn,
}

#[derive(Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Progress {
    #[serde(default)]
    legacy: bool,
    #[serde(default)]
    dismissed: bool,
    #[serde(default)]
    completed: BTreeSet<Milestone>,
}

fn load(state_dir: &Path) -> Result<Progress> {
    Ok(serde_json::from_slice(&fs::read(state_dir.join(FILE))?)?)
}

fn save(state_dir: &Path, progress: &Progress) -> Result<()> {
    fs::create_dir_all(state_dir)?;
    let file = state_dir.join(FILE);
    let temp = file.with_extension("tmp");
    fs::write(&temp, serde_json::to_vec_pretty(progress)?)?;
    fs::rename(temp, file)?;
    Ok(())
}

fn has_items(items: &Value) -> bool {
    items.as_array().is_some_and(|items| !items.is_empty())
}

/// Classifies an installation before the panel imports any CLI logs from this computer.
pub fn initialize(state_dir: &Path, store: &Store) -> Result<()> {
    let _guard = LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if state_dir.join(FILE).exists() {
        return Ok(());
    }
    let indexed: bool = store.conn().query_row("SELECT EXISTS(SELECT 1 FROM calls)", [], |row| row.get(0))?;
    let legacy = indexed
        || !crate::bots::list(state_dir).is_empty()
        || fs::read_dir(state_dir.join("bots")).is_ok_and(|mut files| files.any(|file| file.is_ok()))
        || fs::read_dir(state_dir.join("rooms")).is_ok_and(|mut files| files.any(|file| file.is_ok()))
        || !crate::channels::list(state_dir).is_empty()
        || !crate::workflows::list(state_dir).is_empty()
        || !crate::batches::list(state_dir).is_empty()
        || !crate::requests::list(state_dir).is_empty()
        || !crate::links::list(state_dir).is_empty()
        || has_items(&crate::tasks::state(state_dir)["tasks"])
        || has_items(&crate::wiki::list(state_dir)["pages"]);
    save(state_dir, &Progress { legacy, ..Progress::default() })
}

fn update(state_dir: &Path, edit: impl FnOnce(&mut Progress) -> bool) -> Result<()> {
    let _guard = LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    // The CLI and its MCP server can make changes before the panel has classified this installation.
    if !state_dir.join(FILE).exists() {
        return Ok(());
    }
    let mut progress = load(state_dir)?;
    if !progress.legacy && edit(&mut progress) {
        save(state_dir, &progress)?;
    }
    Ok(())
}

pub fn record(state_dir: &Path, milestones: &[Milestone]) {
    if let Err(error) = update(state_dir, |progress| {
        let before = progress.completed.len();
        progress.completed.extend(milestones.iter().copied());
        progress.completed.len() != before
    }) {
        eprintln!("raiox: could not save first-use progress: {error:#}");
    }
}

pub fn dismiss(state_dir: &Path) -> Result<()> {
    update(state_dir, |progress| !std::mem::replace(&mut progress.dismissed, true))
}

pub fn viewed_usage(state_dir: &Path) -> Result<()> {
    update(state_dir, |progress| progress.completed.insert(Milestone::Usage))
}

/// Unlocks sections whose contents are built elsewhere, including requests from the MCP process.
pub fn state(state_dir: &Path, has_requests: bool) -> Progress {
    let Ok(progress) = load(state_dir) else { return Progress::default() };
    if progress.legacy {
        return progress;
    }
    let mut unlocked = Vec::new();
    if has_requests && !progress.completed.contains(&Milestone::Desk) {
        unlocked.push(Milestone::Desk);
    }
    if progress.completed.contains(&Milestone::Turn)
        && !progress.completed.contains(&Milestone::Delivery)
        && !crate::deliveries::list(state_dir).is_empty()
    {
        unlocked.push(Milestone::Delivery);
    }
    if unlocked.is_empty() {
        return progress;
    }
    record(state_dir, &unlocked);
    load(state_dir).unwrap_or(progress)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("raiox-progress-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        path
    }

    #[test]
    fn a_new_installation_keeps_its_milestones_and_dismissal_after_reopening() {
        let state_dir = folder("new");
        let store = Store::open_in_memory().unwrap();
        initialize(&state_dir, &store).unwrap();
        assert_eq!(state(&state_dir, false), Progress::default());
        record(&state_dir, &[Milestone::Chat, Milestone::Turn]);
        record(&state_dir, &[Milestone::Desk, Milestone::Task]);
        record(&state_dir, &[Milestone::Task]);
        viewed_usage(&state_dir).unwrap();
        dismiss(&state_dir).unwrap();
        fs::create_dir_all(state_dir.join("bots")).unwrap();
        fs::write(state_dir.join("bots/old.json"), "[]").unwrap();
        initialize(&state_dir, &store).unwrap();
        let progress = state(&state_dir, false);
        assert!(!progress.legacy && progress.dismissed);
        assert_eq!(
            progress.completed,
            BTreeSet::from([Milestone::Chat, Milestone::Turn, Milestone::Desk, Milestone::Task, Milestone::Usage])
        );
        let _ = fs::remove_dir_all(state_dir);
    }

    #[test]
    fn a_pending_request_unlocks_the_desk_without_completing_the_task_mission() {
        let state_dir = folder("desk");
        initialize(&state_dir, &Store::open_in_memory().unwrap()).unwrap();
        assert_eq!(state(&state_dir, true).completed, BTreeSet::from([Milestone::Desk]));
        assert_eq!(state(&state_dir, false).completed, BTreeSet::from([Milestone::Desk]));
        let _ = fs::remove_dir_all(state_dir);
    }

    #[test]
    fn an_existing_conversation_keeps_the_full_menu() {
        let state_dir = folder("legacy");
        let store = Store::open_in_memory().unwrap();
        let message = crate::room::Message {
            ms: 1,
            author: "bot".into(),
            kind: "answer".into(),
            data: serde_json::json!({ "reply": { "text": "Hello." } }),
        };
        crate::room::append_messages(&state_dir.join("bots/old.json"), vec![message]).unwrap();
        initialize(&state_dir, &store).unwrap();
        assert!(state(&state_dir, false).legacy);
        let _ = fs::remove_dir_all(state_dir);
    }
}
