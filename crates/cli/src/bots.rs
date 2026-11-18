//! Bots of the person's own AI: Claude Code or Codex living in a project the
//! person turned on, one by one. A bot is the CLI itself run in that folder, so
//! it has the project's context (CLAUDE.md or AGENTS.md, and ai-memory when its
//! hooks are installed). It only reads while talking; it edits only after the
//! person approves its proposal. Nothing runs unless the person asks.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{bail, Context, Result};
use raiox_core::Store;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::bot_run::{self, Mode};
use crate::room::{self, Message};

pub const AGENTS: [(&str, &str); 2] = [("claude", "Claude Code"), ("codex", "Codex")];
/// Longest message the person can send to a bot, in characters.
pub const MAX_TEXT: usize = 4000;
const CANDIDATES: usize = 24;

/// Serializes writes to the registry: answers finish on their own threads.
static REGISTRY: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bot {
    pub id: String,
    /// "claude" or "codex".
    pub agent: String,
    pub dir: PathBuf,
    /// The folder name, as the person knows the project.
    pub name: String,
    /// The CLI's session or thread, so the conversation continues.
    #[serde(default)]
    pub session: Option<String>,
    pub created_ms: i64,
}

/// Bots answering right now, by id.
#[derive(Clone, Default)]
pub struct Jobs(Arc<Mutex<HashSet<String>>>);

impl Jobs {
    fn start(&self, id: &str) -> bool {
        self.0.lock().is_ok_and(|mut set| set.insert(id.to_string()))
    }

    fn finish(&self, id: &str) {
        if let Ok(mut set) = self.0.lock() {
            set.remove(id);
        }
    }

    pub fn is_busy(&self, id: &str) -> bool {
        self.0.lock().is_ok_and(|set| set.contains(id))
    }
}

fn registry_file(state_dir: &Path) -> PathBuf {
    state_dir.join("bots.json")
}

fn chat_file(state_dir: &Path, id: &str) -> PathBuf {
    state_dir.join("bots").join(format!("{id}.json"))
}

pub fn list(state_dir: &Path) -> Vec<Bot> {
    fs::read_to_string(registry_file(state_dir)).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
}

fn save(state_dir: &Path, bots: &[Bot]) -> Result<()> {
    fs::create_dir_all(state_dir)?;
    let file = registry_file(state_dir);
    let temp = file.with_extension("tmp");
    fs::write(&temp, serde_json::to_string_pretty(bots)? + "\n")?;
    fs::rename(temp, file)?;
    Ok(())
}

/// Reads, changes and writes the registry under the lock.
fn update(state_dir: &Path, change: impl FnOnce(Vec<Bot>) -> Result<Vec<Bot>>) -> Result<()> {
    let _guard = REGISTRY.lock().map_err(|_| anyhow::anyhow!("registry lock poisoned"))?;
    save(state_dir, &change(list(state_dir))?)
}

fn find(state_dir: &Path, id: &str) -> Result<Bot> {
    list(state_dir).into_iter().find(|b| b.id == id).with_context(|| format!("no bot {id}"))
}

fn bot_id(agent: &str, dir: &Path) -> String {
    let key = format!("{agent}\0{}", dir.display());
    let hash = key.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3));
    format!("{hash:016x}")
}

/// Projects where the person used Claude Code or Codex, newest first; the
/// panel offers these for activation. The home folder itself is never one.
#[derive(Debug, Serialize)]
pub struct Candidate {
    pub project: String,
    pub name: String,
    pub agent: String,
    pub requests: u64,
    pub last_ms: i64,
}

pub fn candidates(store: &Store, home: &Path) -> Result<Vec<Candidate>> {
    let mut statement = store.conn().prepare(
        "SELECT project, source, COUNT(*), MAX(ts_ms) FROM calls
         WHERE is_sub = 0 AND project IS NOT NULL AND project NOT IN ('', '~') AND ts_ms IS NOT NULL
         GROUP BY project, source ORDER BY MAX(ts_ms) DESC",
    )?;
    let rows = statement.query_map([], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3)?))
    })?;
    Ok(rows
        .flatten()
        // Hidden folders (.claude, .codex, .config…) hold the tools' own data, not projects.
        .filter(|(project, agent, ..)| {
            AGENTS.iter().any(|(a, _)| a == agent) && !project.starts_with('.') && home.join(project).is_dir()
        })
        .take(CANDIDATES)
        .map(|(project, agent, requests, last_ms)| Candidate {
            name: project.rsplit('/').next().unwrap_or(&project).to_string(),
            project,
            agent,
            requests: requests.max(0) as u64,
            last_ms,
        })
        .collect())
}

/// Turns a bot on for one project (a folder under the home, never the home itself).
pub fn activate(state_dir: &Path, home: &Path, project: &str, agent: &str) -> Result<Bot> {
    if !AGENTS.iter().any(|(a, _)| *a == agent) {
        bail!("unknown agent {agent}");
    }
    let home = home.canonicalize()?;
    let dir = home.join(project).canonicalize().with_context(|| format!("no folder {project} in the home folder"))?;
    let hidden = dir.strip_prefix(&home).ok().and_then(|rest| rest.components().next()).is_some_and(|first| first.as_os_str().to_string_lossy().starts_with('.'));
    if !dir.is_dir() || !dir.starts_with(&home) || dir == home || hidden {
        bail!("bots live in project folders under the home folder");
    }
    let bot = Bot {
        id: bot_id(agent, &dir),
        agent: agent.to_string(),
        name: dir.file_name().map_or_else(|| project.to_string(), |n| n.to_string_lossy().into_owned()),
        dir,
        session: None,
        created_ms: room::now_ms(),
    };
    let added = bot.clone();
    update(state_dir, |bots| {
        Ok(if bots.iter().any(|b| b.id == added.id) { bots } else { bots.into_iter().chain([added]).collect() })
    })?;
    Ok(bot)
}

pub fn deactivate(state_dir: &Path, jobs: &Jobs, id: &str) -> Result<()> {
    if jobs.is_busy(id) {
        bail!("the bot is answering; wait for it to finish");
    }
    update(state_dir, |bots| Ok(bots.into_iter().filter(|b| b.id != id).collect()))
}

/// Starts a new conversation: the next message opens a fresh session.
pub fn reset(state_dir: &Path, jobs: &Jobs, id: &str) -> Result<()> {
    if jobs.is_busy(id) {
        bail!("the bot is answering; wait for it to finish");
    }
    set_session(state_dir, id, None)?;
    room::append_messages(&chat_file(state_dir, id), vec![note("reset", json!({}))])
}

fn set_session(state_dir: &Path, id: &str, session: Option<String>) -> Result<()> {
    update(state_dir, |bots| {
        Ok(bots.into_iter().map(|b| if b.id == id { Bot { session: session.clone(), ..b } } else { b }).collect())
    })
}

fn note(kind: &str, data: Value) -> Message {
    Message { ms: room::now_ms(), author: "bot".into(), kind: kind.into(), data }
}

fn said(kind: &str, data: Value) -> Message {
    Message { ms: room::now_ms(), author: "you".into(), kind: kind.into(), data }
}

/// The person talks to a bot. The answer arrives later, on its own thread.
pub fn ask(state_dir: &Path, jobs: &Jobs, id: &str, text: &str) -> Result<()> {
    let text = text.trim();
    if text.is_empty() || text.chars().count() > MAX_TEXT {
        bail!("a message has 1 to {MAX_TEXT} characters");
    }
    start(state_dir, jobs, id, Mode::Talk, text, said("ask", json!({ "text": text })))
}

/// The person approves the proposal in the bot's latest answer.
pub fn approve(state_dir: &Path, jobs: &Jobs, id: &str) -> Result<()> {
    let messages = room::read_messages(&chat_file(state_dir, id));
    // A failed or interrupted approval can be retried: both are skipped.
    let last = messages.iter().rev().find(|m| m.kind != "failed" && m.kind != "approve");
    let proposal = last
        .filter(|m| m.author == "bot" && m.kind == "answer")
        .and_then(|m| m.data.pointer("/reply/proposal").and_then(Value::as_str))
        .map(str::to_string)
        .context("the latest answer has no proposal to approve")?;
    start(state_dir, jobs, id, Mode::Edit, &proposal, said("approve", json!({ "proposal": proposal })))
}

fn start(state_dir: &Path, jobs: &Jobs, id: &str, mode: Mode, text: &str, asked: Message) -> Result<()> {
    let bot = find(state_dir, id)?;
    if mode == Mode::Edit && bot.session.is_none() {
        bail!("this conversation was reset; ask the bot again");
    }
    if !jobs.start(id) {
        bail!("the bot is still answering");
    }
    let file = chat_file(state_dir, id);
    if let Err(error) = room::append_messages(&file, vec![asked]) {
        jobs.finish(id);
        return Err(error);
    }
    let (state_dir, jobs, text) = (state_dir.to_path_buf(), jobs.clone(), text.to_string());
    std::thread::spawn(move || {
        let scratch = state_dir.join("bots").join(format!("{}.run", bot.id));
        let outcome = bot_run::run(&bot.agent, &bot.dir, &bot.name, bot.session.as_deref(), mode, &text, &scratch);
        let message = match outcome {
            Ok(reply) => {
                if reply.session.is_some() && reply.session != bot.session {
                    let _ = set_session(&state_dir, &bot.id, reply.session.clone());
                }
                let mode_name = if mode == Mode::Edit { "edit" } else { "talk" };
                note("answer", json!({ "mode": mode_name, "reply": reply }))
            }
            Err(error) => note("failed", json!({ "error": format!("{error:#}") })),
        };
        let _ = room::append_messages(&file, vec![message]);
        jobs.finish(&bot.id);
    });
    Ok(())
}

/// Everything the panel shows about bots.
pub fn state(store: &Store, home: &Path, state_dir: &Path, jobs: &Jobs) -> Result<Value> {
    let bots: Vec<Value> = list(state_dir)
        .into_iter()
        .map(|bot| {
            let messages = room::read_messages(&chat_file(state_dir, &bot.id));
            json!({
                "id": bot.id,
                "agent": bot.agent,
                "name": bot.name,
                "dir": bot.dir,
                "has_session": bot.session.is_some(),
                "busy": jobs.is_busy(&bot.id),
                "messages": messages,
            })
        })
        .collect();
    Ok(json!({ "bots": bots, "candidates": candidates(store, home)?, "max_text": MAX_TEXT }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("raiox-bots-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn activation_needs_a_project_folder_under_the_home() {
        let home = temp_dir("home");
        fs::create_dir_all(home.join("Projects/app")).unwrap();
        let state = home.join(".state");
        assert!(activate(&state, &home, "", "claude").is_err());
        assert!(activate(&state, &home, "../", "claude").is_err());
        fs::create_dir_all(home.join(".claude/projects")).unwrap();
        assert!(activate(&state, &home, ".claude/projects", "claude").is_err());
        assert!(activate(&state, &home, "Projects/app", "gemini").is_err());
        let bot = activate(&state, &home, "Projects/app", "claude").unwrap();
        assert_eq!(bot.name, "app");
        activate(&state, &home, "Projects/app", "claude").unwrap();
        assert_eq!(list(&state).len(), 1);
    }

    #[test]
    fn approval_needs_a_proposal_in_the_latest_answer() {
        let state = temp_dir("approve");
        let jobs = Jobs::default();
        let answer = |proposal: Value| note("answer", json!({ "mode": "talk", "reply": { "text": "t", "proposal": proposal } }));
        room::append_messages(&chat_file(&state, "b1"), vec![answer(Value::Null)]).unwrap();
        assert!(approve(&state, &jobs, "b1").unwrap_err().to_string().contains("no proposal"));
        room::append_messages(&chat_file(&state, "b1"), vec![answer(json!("rename"))]).unwrap();
        // With a proposal it gets past the check and fails only because no bot b1 is registered.
        assert!(approve(&state, &jobs, "b1").unwrap_err().to_string().contains("no bot"));
    }
}
