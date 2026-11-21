//! Bots of the person's own AI: Claude Code or Codex living in a project the
//! person turned on, one by one. A bot is the CLI itself run in that folder, so
//! it has the project's context (CLAUDE.md or AGENTS.md, and ai-memory when its
//! hooks are installed). It only reads while talking; it edits only after the
//! person approves its proposal. Nothing runs unless the person asks.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{bail, Context, Result};
use raiox_core::Store;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::bot_run::{self, Mode};
use crate::room::{self, Message};
use crate::threads::{self, MAIN};

mod recover;
mod review;
mod turn;
use turn::Turn;

/// Messages that come between an answer and the person's next step and leave the answer
/// current: its proposal can still be approved and its commands still run.
const ASIDE: [&str; 13] = ["failed", "sent", "stopped", "interrupted", "house", "remembered", "review", "review_asked", "carried", "shot", "turn_undone", "access_changed", "waiting_cancelled"];

/// What a new conversation is called until its first question names it.
pub const DEFAULT_THREAD_TITLE: &str = "Conversa";

/// Longest message the person can send to a bot, in characters.
pub const MAX_TEXT: usize = 4000;
/// Maximum length of a new project folder name created under `~/Projects/`.
pub const MAX_PROJECT_NAME: usize = 60;
/// Extra conversations a free (project-less) bot keeps per AI.
const MAX_FREE_THREADS: usize = 200;
/// Subfolder under `state_dir` holding empty working folders for free conversations.
const FREE_DIR: &str = "conversa-livre";
const CANDIDATES: usize = 200;

/// Serializes writes to the registry: answers finish on their own threads.
static REGISTRY: Mutex<()> = Mutex::new(());
/// Serializes a person's decision about a waiting card with approval and command execution.
static WAITING: Mutex<()> = Mutex::new(());

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
    #[serde(default)]
    pub persona: Option<Persona>,
    /// The model and effort the bot runs with; None keeps the CLI's default.
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub effort: Option<String>,
    pub created_ms: i64,
    /// Exact commands the person lets this bot run without asking.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed: Vec<String>,
    /// A lighter model for talk; changes still run on `model`. None uses `model` for both.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub talk_model: Option<String>,
    /// Another bot of the project that reviews every proposal of this one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reviewer: Option<String>,
    /// What it may do beyond reading its project (phone, web, MCP, weekly ceiling).
    #[serde(default)]
    pub grants: crate::grants::Grants,
    /// Built-in Dev/Infra auditor: audits scripts, workflows, routines and skills of this project.
    #[serde(default, skip_serializing_if = "crate::auditor::Config::is_default")]
    pub auditor: crate::auditor::Config,
    /// True for internal project-less bots backing free conversations (always read-only).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub free: bool,
}

/// Name, look and instructions the person chose for a bot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Persona {
    /// The preset it started from ("partner", "reviewer"…) or "custom".
    pub preset: String,
    pub name: String,
    pub instructions: String,
    /// "#rrggbb".
    pub color: String,
    /// An avatar shape id from the panel's gallery.
    pub shape: String,
    /// "solid", "flat" or "pixel".
    #[serde(default = "solid")]
    pub finish: String,
    /// Its role in the person's team ("Frontend", "Security review"…); empty when none was given.
    #[serde(default)]
    pub role: String,
}

fn solid() -> String {
    "solid".into()
}

pub const FINISHES: [&str; 3] = ["solid", "flat", "pixel"];
pub const CLAUDE_EFFORTS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];
pub const CODEX_EFFORTS: [&str; 4] = ["low", "medium", "high", "xhigh"];
const MAX_MODEL: usize = 64;

/// Effort levels a CLI takes; Cursor has none (its models carry the variant).
pub fn efforts_for(agent: &str) -> &'static [&'static str] {
    match agent {
        "claude" => &CLAUDE_EFFORTS,
        "codex" => &CODEX_EFFORTS,
        _ => &[],
    }
}

/// An empty model means the CLI's default; anything else is letters, digits, dots, dashes and
/// slashes (opencode names its models provider/model).
pub fn check_model(model: Option<&str>) -> Result<Option<String>> {
    let model = model.map(str::trim).filter(|m| !m.is_empty());
    if let Some(m) = model
        && (m.len() > MAX_MODEL || !m.bytes().all(|b| b.is_ascii_alphanumeric() || b"-._/".contains(&b)))
    {
        bail!("invalid model name");
    }
    Ok(model.map(str::to_string))
}

pub fn check_effort(agent: &str, effort: Option<&str>) -> Result<Option<String>> {
    let effort = effort.map(str::trim).filter(|e| !e.is_empty());
    let allowed = efforts_for(agent);
    if effort.is_some_and(|e| !allowed.contains(&e)) {
        bail!("{agent} takes no effort {}", effort.unwrap_or_default());
    }
    Ok(effort.map(str::to_string))
}

const MAX_NAME: usize = 32;
const MAX_INSTRUCTIONS: usize = 1200;
const MAX_ROLE: usize = 40;
const MAX_SLUG: usize = 40;

impl Persona {
    pub fn validate(&self) -> Result<()> {
        let slug = |s: &str| !s.is_empty() && s.len() <= MAX_SLUG && s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
        let color = self.color.len() == 7 && self.color.starts_with('#') && self.color[1..].bytes().all(|b| b.is_ascii_hexdigit());
        let name = self.name.trim().chars().count();
        if !(1..=MAX_NAME).contains(&name) {
            bail!("a bot name has 1 to {MAX_NAME} characters");
        }
        if self.instructions.chars().count() > MAX_INSTRUCTIONS {
            bail!("instructions have at most {MAX_INSTRUCTIONS} characters");
        }
        if self.role.trim().chars().count() > MAX_ROLE || self.role.contains('\n') {
            bail!("a role is one line of at most {MAX_ROLE} characters");
        }
        if !slug(&self.preset) || !slug(&self.shape) || !color || !FINISHES.contains(&self.finish.as_str()) {
            bail!("invalid preset, shape, color or finish");
        }
        Ok(())
    }

    /// What the CLI is told about its role here.
    fn prompt(&self) -> String {
        let role = self.role.trim();
        let role = if role.is_empty() { String::new() } else { format!(" Your role in the person's team: {role}.") };
        format!("Your name in this project is {}.{role} {}", self.name.trim(), self.instructions.trim())
    }
}

/// Snapshot of one running turn on a bot, exposed to the panel per conversation.
#[derive(Clone, Serialize)]
pub struct BusyThread {
    pub thread: String,
    pub step: Option<crate::bot_steps::Step>,
    pub trail: Option<crate::bot_trail::Trail>,
    pub started_ms: i64,
    pub editing: bool,
}

/// Active turns by `(bot_id, thread_id)`: each conversation runs on its own, while draft edits
/// (`Mode::Edit`) remain one at a time per bot because `drafts::prepare` shares one copy per bot.
#[derive(Clone, Default)]
pub struct Jobs(Arc<Mutex<HashMap<(String, String), Job>>>);

#[derive(Default)]
struct Job {
    stop: bool,
    is_edit: bool,
    step: Option<crate::bot_steps::Step>,
    /// The path of the running turn, as the CLI reports it.
    trail: Option<crate::bot_trail::Trail>,
    /// What the person wants changed: the turn stops and goes on with it.
    correction: Option<String>,
    started_ms: i64,
}

impl Jobs {
    /// Reserves `(id, thread)` for a turn of `mode`.
    /// Parallel `Mode::Talk` turns on different conversations of the same bot are allowed;
    /// a second `Mode::Edit` on the same bot is refused while one is already running.
    pub(crate) fn start_mode(&self, id: &str, thread: &str, mode: Mode) -> Result<()> {
        let mut map = self.0.lock().map_err(|_| anyhow::anyhow!("jobs lock poisoned"))?;
        let key = (id.to_string(), thread.to_string());
        if map.contains_key(&key) {
            bail!("this conversation is still answering");
        }
        if mode == Mode::Edit && map.iter().any(|((bot, _), job)| bot == id && job.is_edit) {
            bail!("another edit in a draft copy is already running for this project; wait for it to finish");
        }
        map.insert(key, Job { is_edit: mode == Mode::Edit, started_ms: room::now_ms(), ..Job::default() });
        Ok(())
    }

    /// Snapshot of every conversation currently running on `id`, oldest first.
    pub fn busy_threads(&self, id: &str) -> Vec<BusyThread> {
        let Ok(map) = self.0.lock() else { return Vec::new() };
        let mut list: Vec<BusyThread> = map
            .iter()
            .filter(|((bot, _), _)| bot == id)
            .map(|((_, thread), job)| BusyThread {
                thread: thread.clone(),
                step: job.step.clone(),
                trail: job.trail.clone(),
                started_ms: job.started_ms,
                editing: job.is_edit,
            })
            .collect();
        list.sort_by(|a, b| a.started_ms.cmp(&b.started_ms).then_with(|| a.thread.cmp(&b.thread)));
        list
    }

    fn set_step(&self, id: &str, thread: &str, step: crate::bot_steps::Step) {
        if let Ok(mut map) = self.0.lock()
            && let Some(job) = map.get_mut(&(id.to_string(), thread.to_string()))
        {
            job.step = Some(step);
        }
    }

    fn set_trail(&self, id: &str, thread: &str, trail: &crate::bot_trail::Trail) {
        if let Ok(mut map) = self.0.lock()
            && let Some(job) = map.get_mut(&(id.to_string(), thread.to_string()))
        {
            job.step = trail.current();
            job.trail = Some(trail.clone());
        }
    }

    /// Picks the target `(id, thread)` key when a caller passes an optional thread name.
    fn pick_key(map: &HashMap<(String, String), Job>, id: &str, thread: Option<&str>) -> Option<(String, String)> {
        if let Some(thread) = thread.filter(|t| !t.is_empty()) {
            let key = (id.to_string(), thread.to_string());
            return map.contains_key(&key).then_some(key);
        }
        let mut keys: Vec<_> = map.iter().filter(|((bot, _), _)| bot == id).collect();
        if keys.len() == 1 {
            return Some(keys[0].0.clone());
        }
        keys.sort_by_key(|((_, th), job)| (th != MAIN, job.started_ms));
        keys.first().map(|(k, _)| (*k).clone())
    }

    /// Stops the running turn in `thread` so it goes on with the person's correction.
    pub fn correct(&self, id: &str, thread: Option<&str>, text: &str) -> Result<()> {
        let text = text.trim();
        if text.is_empty() || text.chars().count() > MAX_TEXT {
            bail!("a correction has 1 to {MAX_TEXT} characters");
        }
        let mut map = self.0.lock().map_err(|_| anyhow::anyhow!("jobs lock poisoned"))?;
        let key = Self::pick_key(&map, id, thread).context("the bot is not answering")?;
        let job = map.get_mut(&key).context("the bot is not answering")?;
        job.stop = true;
        job.correction = Some(text.to_string());
        Ok(())
    }

    /// The correction a stopped turn goes on with, and the CLI session it had reached;
    /// the turn is running again from here.
    fn take_correction(&self, id: &str, thread: &str) -> Option<(String, Option<String>)> {
        let mut map = self.0.lock().ok()?;
        let job = map.get_mut(&(id.to_string(), thread.to_string()))?;
        let correction = job.correction.take()?;
        let session = job.trail.take().and_then(|trail| trail.session);
        job.stop = false;
        job.step = None;
        Some((correction, session))
    }

    fn finish(&self, id: &str, thread: &str) {
        if let Ok(mut map) = self.0.lock() {
            map.remove(&(id.to_string(), thread.to_string()));
        }
    }

    /// How many turns are running right now across all bots and conversations.
    pub fn busy_count(&self) -> usize {
        self.0.lock().map_or(0, |map| map.len())
    }

    /// True when any conversation of `id` is currently running a turn.
    pub fn is_busy(&self, id: &str) -> bool {
        self.0.lock().is_ok_and(|map| map.keys().any(|(bot, _)| bot == id))
    }

    /// True when the specific `(id, thread)` conversation is currently running a turn.
    pub fn is_thread_busy(&self, id: &str, thread: &str) -> bool {
        self.0.lock().is_ok_and(|map| map.contains_key(&(id.to_string(), thread.to_string())))
    }

    /// True when another conversation of `id` besides `thread` is currently running.
    fn has_other_busy(&self, id: &str, thread: &str) -> bool {
        self.0.lock().is_ok_and(|map| map.keys().any(|(bot, th)| bot == id && th != thread))
    }

    fn should_stop(&self, id: &str, thread: &str) -> bool {
        self.0.lock().is_ok_and(|map| map.get(&(id.to_string(), thread.to_string())).is_some_and(|job| job.stop))
    }

    /// Marks the running turn in `thread` to stop; its worker kills the CLI on its next check.
    pub fn stop(&self, id: &str, thread: Option<&str>) -> Result<()> {
        let mut map = self.0.lock().map_err(|_| anyhow::anyhow!("jobs lock poisoned"))?;
        let Some(key) = Self::pick_key(&map, id, thread) else {
            bail!("the bot is not answering");
        };
        if let Some(job) = map.get_mut(&key) {
            job.stop = true;
        }
        Ok(())
    }
}

fn registry_file(state_dir: &Path) -> PathBuf {
    state_dir.join("bots.json")
}

/// The main conversation's file (ai-memory mail lands there).
pub fn chat_file(state_dir: &Path, id: &str) -> PathBuf {
    threads::chat_file(state_dir, id, MAIN)
}

pub fn list(state_dir: &Path) -> Vec<Bot> {
    fs::read_to_string(registry_file(state_dir)).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
}

/// Only project bots, excluding internal free-conversation bots.
pub fn project_list(state_dir: &Path) -> Vec<Bot> {
    list(state_dir).into_iter().filter(|b| !b.free).collect()
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
pub(crate) fn update(state_dir: &Path, change: impl FnOnce(Vec<Bot>) -> Result<Vec<Bot>>) -> Result<()> {
    let _guard = REGISTRY.lock().map_err(|_| anyhow::anyhow!("registry lock poisoned"))?;
    save(state_dir, &change(list(state_dir))?)
}

pub fn find(state_dir: &Path, id: &str) -> Result<Bot> {
    list(state_dir).into_iter().find(|b| b.id == id).with_context(|| format!("no bot {id}"))
}

fn bot_id(agent: &str, dir: &Path) -> String {
    let key = format!("{agent}\0{}", dir.display());
    let hash = key.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3));
    format!("{hash:016x}")
}

/// Projects where the person used an AI CLI (newest first) followed by local git
/// folders found by setup, without duplicates. The home folder itself is never one.
#[derive(Debug, Serialize)]
pub struct Candidate {
    pub project: String,
    pub name: String,
    pub agent: Option<String>,
    pub requests: u64,
    pub last_ms: i64,
}

/// Maximum characters kept in a bot card's one-line preview of the main conversation.
const PREVIEW_CHARS: usize = 140;

/// True when `candidate_dir` is the raiox state folder itself or lives inside it.
fn inside_state_dir(candidate_dir: &Path, state_dir: &Path) -> bool {
    let Ok(cand) = candidate_dir.canonicalize() else { return false };
    let Ok(state) = state_dir.canonicalize() else { return false };
    cand.starts_with(&state)
}

/// Cleans a message into one short preview line for the Chats card.
fn preview_line(text: &str) -> Option<String> {
    let line = text
        .lines()
        .map(|raw| raw.trim().trim_start_matches(['#', '>', '-', '*', '`']).trim())
        .find(|l| !l.is_empty())?;
    let collapsed = line.split_whitespace().collect::<Vec<_>>().join(" ").replace('`', "");
    let trimmed = collapsed.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.chars().take(PREVIEW_CHARS).collect())
}

/// Latest timestamp and one-line preview from the tail of a bot's main conversation.
fn main_preview(messages: &[Message]) -> (Option<i64>, Option<String>) {
    let last_ms = messages.last().map(|m| m.ms);
    let last_line = messages.iter().rev().find_map(|m| {
        let raw = match m.kind.as_str() {
            "answer" | "draft" => m.data.pointer("/reply/text").and_then(Value::as_str),
            "ask" => m.data.get("text").and_then(Value::as_str),
            "failed" => m.data.get("error").and_then(Value::as_str),
            _ => None,
        }?;
        preview_line(raw)
    });
    (last_ms, last_line)
}

pub fn candidates(store: &Store, home: &Path, state_dir: &Path) -> Result<Vec<Candidate>> {
    let mut statement = store.conn().prepare(
        "SELECT project, source, COUNT(*), MAX(ts_ms) FROM calls
         WHERE is_sub = 0 AND project IS NOT NULL AND project NOT IN ('', '~') AND ts_ms IS NOT NULL
         GROUP BY project, source ORDER BY MAX(ts_ms) DESC",
    )?;
    let rows = statement.query_map([], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3)?))
    })?;
    let mut seen_dirs: HashMap<crate::setup::Identity, Vec<String>> = HashMap::new();
    let mut list = Vec::new();
    for (project, agent, requests, last_ms) in rows.flatten() {
        let dir = home.join(&project);
        // Hidden, Library/Caches and raiox's own state folder hold tool data, not projects.
        if !crate::connections::bot_agents().any(|a| a == agent) || crate::setup::is_non_project_path(&project) || inside_state_dir(&dir, state_dir) {
            continue;
        }
        let Some(id) = crate::setup::identity(&dir) else {
            continue;
        };
        let agents = seen_dirs.entry(id).or_default();
        if agents.contains(&agent) {
            continue;
        }
        agents.push(agent.clone());
        list.push(Candidate {
            name: project.rsplit('/').next().unwrap_or(&project).to_string(),
            project,
            agent: Some(agent),
            requests: requests.max(0) as u64,
            last_ms,
        });
        if list.len() >= CANDIDATES {
            return Ok(list);
        }
    }
    for project in crate::setup::projects(store, home)? {
        if list.len() >= CANDIDATES {
            break;
        }
        let dir = home.join(&project.project);
        if crate::setup::is_non_project_path(&project.project) || inside_state_dir(&dir, state_dir) {
            continue;
        }
        let Some(id) = crate::setup::identity(&dir) else {
            continue;
        };
        if seen_dirs.contains_key(&id) {
            continue;
        }
        seen_dirs.insert(id, Vec::new());
        list.push(Candidate {
            project: project.project,
            name: project.name,
            agent: None,
            requests: 0,
            last_ms: project.last_ms.unwrap_or(0),
        });
    }
    Ok(list)
}

/// Validates a new project folder name: 1 to 60 ASCII letters, digits, dashes or underscores,
/// never starting with a dash.
pub fn valid_project_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    let Some(first) = bytes.next() else { return false };
    name.len() <= MAX_PROJECT_NAME
        && (first.is_ascii_alphanumeric() || first == b'_')
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Creates `<home>/Projects/<name>` when valid and not already a non-empty folder.
/// Returns the relative project path (`Projects/<name>`) and whether this call created the directory.
pub fn create_project_dir(home: &Path, name: &str) -> Result<(String, bool)> {
    let name = name.trim();
    if !valid_project_name(name) {
        bail!("project name must be 1 to {MAX_PROJECT_NAME} letters, digits, dashes or underscores");
    }
    let parent = home.join("Projects");
    fs::create_dir_all(&parent)?;
    let target = parent.join(name);
    let created = if target.exists() {
        if !target.is_dir() {
            bail!("Projects/{name} already exists and is not a folder");
        }
        if fs::read_dir(&target)?.next().is_some() {
            bail!("Projects/{name} already exists and is not empty");
        }
        false
    } else {
        fs::create_dir(&target)?;
        true
    };
    Ok((format!("Projects/{name}"), created))
}

/// Creates `~/Projects/<name>` and activates a standard bot in it on the chosen AI.
pub fn create_project(state_dir: &Path, home: &Path, name: &str, agent: &str) -> Result<Bot> {
    if !crate::connections::bot_agents().any(|a| a == agent) {
        bail!("raiox cannot run {agent} as a bot");
    }
    let (project, created) = create_project_dir(home, name)?;
    match activate(state_dir, home, &project, agent, None) {
        Ok(bot) => Ok(bot),
        Err(error) => {
            if created {
                let _ = fs::remove_dir(home.join(&project));
            }
            Err(error)
        }
    }
}

/// Turns a bot on for one project (a folder under the home, never the home itself).
/// Turning on an existing bot again only replaces its persona.
pub fn activate(state_dir: &Path, home: &Path, project: &str, agent: &str, persona: Option<Persona>) -> Result<Bot> {
    let home = home.canonicalize()?;
    let dir = home.join(project).canonicalize().with_context(|| format!("no folder {project} in the home folder"))?;
    let hidden = dir.strip_prefix(&home).ok().and_then(|rest| rest.components().next()).is_some_and(|first| first.as_os_str().to_string_lossy().starts_with('.'));
    if !dir.is_dir() || !dir.starts_with(&home) || dir == home || hidden {
        bail!("bots live in project folders under the home folder");
    }
    activate_dir(state_dir, dir, project, agent, persona, true)
}

/// Validates an explicitly chosen folder, including projects outside the home.
pub(crate) fn picked_project_dir(state_dir: &Path, home: &Path, path: &Path) -> Result<PathBuf> {
    if !path.is_absolute() {
        bail!("select an absolute project folder");
    }
    let dir = path.canonicalize().context("the selected project folder no longer exists")?;
    let home = home.canonicalize()?;
    // A hidden folder in the home (.ssh, .aws, .config…) holds keys and settings, never a project.
    let hidden = dir.strip_prefix(&home).ok().and_then(|rest| rest.components().next()).is_some_and(|first| first.as_os_str().to_string_lossy().starts_with('.'));
    if !dir.is_dir() || dir.parent().is_none() || home.starts_with(&dir) || inside_state_dir(&dir, state_dir) || hidden {
        bail!("select a project folder");
    }
    Ok(dir)
}

/// Opens a bot in the chosen folder, keeping its persona unless one is supplied.
pub(crate) fn activate_folder(state_dir: &Path, home: &Path, project: &str, agent: &str, persona: Option<Persona>) -> Result<Bot> {
    let dir = picked_project_dir(state_dir, home, Path::new(project))?;
    activate_dir(state_dir, dir, project, agent, persona, false)
}

fn activate_dir(state_dir: &Path, dir: PathBuf, project: &str, agent: &str, persona: Option<Persona>, replace_persona: bool) -> Result<Bot> {
    if let Some(p) = &persona {
        p.validate()?;
    }
    if !crate::connections::bot_agents().any(|a| a == agent) {
        bail!("raiox cannot run {agent} as a bot");
    }
    let bot = Bot {
        id: bot_id(agent, &dir),
        agent: agent.to_string(),
        name: dir.file_name().map_or_else(|| project.to_string(), |n| n.to_string_lossy().into_owned()),
        dir,
        session: None,
        persona,
        model: None,
        effort: None,
        created_ms: room::now_ms(),
        allowed: Vec::new(),
        talk_model: None,
        reviewer: None,
        grants: crate::grants::Grants::default(),
        auditor: crate::auditor::Config::default(),
        free: false,
    };
    let added = bot.clone();
    update(state_dir, |bots| {
        Ok(if bots.iter().any(|b| b.id == added.id) {
            bots.into_iter()
                .map(|b| if b.id == added.id && (replace_persona || added.persona.is_some()) { Bot { persona: added.persona.clone(), ..b } } else { b })
                .collect()
        } else {
            bots.into_iter().chain([added]).collect()
        })
    })?;
    Ok(bot)
}

/// Returns the single internal free-conversation bot for `agent`, creating it once under
/// `<state_dir>/conversa-livre/<agent>` in strictly read-only mode.
pub fn ensure_free(state_dir: &Path, agent: &str) -> Result<Bot> {
    if !crate::connections::bot_agents().any(|a| a == agent) {
        bail!("raiox cannot run {agent} as a bot");
    }
    let dir = state_dir.join(FREE_DIR).join(agent);
    fs::create_dir_all(&dir)?;
    let dir = dir.canonicalize().unwrap_or(dir);
    let mut kept: Option<Bot> = None;
    update(state_dir, |bots| {
        if let Some(existing) = bots.iter().find(|b| b.free && b.agent == agent) {
            kept = Some(existing.clone());
            return Ok(bots);
        }
        let bot = Bot {
            id: bot_id(agent, &dir),
            agent: agent.to_string(),
            name: crate::connections::known(agent).map_or(agent, |k| k.name).to_string(),
            dir: dir.clone(),
            session: None,
            persona: None,
            model: None,
            effort: None,
            created_ms: room::now_ms(),
            allowed: Vec::new(),
            talk_model: None,
            reviewer: None,
            grants: crate::grants::Grants {
                web: crate::grants::web_possible(agent),
                full_access: false,
                bypass: false,
                auto_run: false,
                auto_edit: false,
                ..crate::grants::Grants::default()
            },
            auditor: crate::auditor::Config::default(),
            free: true,
        };
        kept = Some(bot.clone());
        Ok(bots.into_iter().chain([bot]).collect())
    })?;
    kept.context("could not create free bot")
}

/// Starts a new free conversation on `agent` with an optional per-conversation `model`.
pub fn start_free_chat(state_dir: &Path, jobs: &Jobs, agent: &str, model: Option<&str>, text: &str, images: &[String]) -> Result<(Bot, threads::Thread)> {
    let trimmed = text.trim();
    let validated = crate::uploads::validate_all(images)?;
    if (trimmed.is_empty() && validated.is_empty()) || trimmed.chars().count() > MAX_TEXT {
        bail!("a message has 1 to {MAX_TEXT} characters");
    }
    let model = check_model(model)?;
    let bot = ensure_free(state_dir, agent)?;
    let thread = threads::create_with_limit(state_dir, &bot.id, DEFAULT_THREAD_TITLE, MAX_FREE_THREADS)?;
    if let Some(m) = model {
        threads::set_model(state_dir, &bot.id, &thread.id, Some(m))?;
    }
    if let Err(error) = ask(state_dir, jobs, &bot.id, &thread.id, text, images) {
        let _ = threads::delete(state_dir, &bot.id, &thread.id);
        return Err(error);
    }
    let updated = threads::list(state_dir, &bot.id).into_iter().find(|t| t.id == thread.id).unwrap_or(thread);
    Ok((bot, updated))
}

/// Updates the model (and optionally effort on the bot) for a specific conversation thread.
pub fn set_thread_model(state_dir: &Path, id: &str, thread: &str, model: Option<&str>, effort: Option<&str>) -> Result<()> {
    let bot = find(state_dir, id)?;
    let model = check_model(model)?;
    if thread == MAIN {
        return configure(state_dir, id, model.as_deref(), effort.or(bot.effort.as_deref()), None);
    }
    threads::set_model(state_dir, id, thread, model)?;
    if let Some(effort) = effort {
        let effort = check_effort(&bot.agent, Some(effort))?;
        update(state_dir, |bots| Ok(bots.into_iter().map(|b| if b.id == id { Bot { effort: effort.clone(), ..b } } else { b }).collect()))?;
    }
    Ok(())
}

pub fn deactivate(state_dir: &Path, jobs: &Jobs, id: &str) -> Result<()> {
    if jobs.is_busy(id) {
        bail!("the bot is answering; wait for it to finish");
    }
    update(state_dir, |bots| Ok(bots.into_iter().filter(|b| b.id != id).collect()))
}

/// Changes how a bot runs and looks. Empty model or effort means the CLI's default.
pub fn configure(state_dir: &Path, id: &str, model: Option<&str>, effort: Option<&str>, persona: Option<Persona>) -> Result<()> {
    // A setting for the next messages: it may change while a turn runs (that turn keeps what it started with).
    let bot = find(state_dir, id)?;
    let (model, effort) = (check_model(model)?, check_effort(&bot.agent, effort)?);
    if let Some(p) = &persona {
        p.validate()?;
    }
    update(state_dir, |bots| {
        Ok(bots
            .into_iter()
            .map(|b| {
                if b.id != id {
                    return b;
                }
                let persona = persona.clone().or(b.persona.clone());
                Bot { model: model.clone(), effort: effort.clone(), persona, ..b }
            })
            .collect())
    })
}

/// Scans every bot conversation at panel startup and marks any turn cut off when raiox closed.
pub fn recover_interrupted(state_dir: &Path) {
    recover::all(state_dir);
}

/// Resumes the CLI session recovered from an interrupted turn without re-running completed work.
pub fn continue_turn(state_dir: &Path, jobs: &Jobs, id: &str, thread: &str) -> Result<()> {
    if !threads::valid(thread) {
        bail!("invalid conversation");
    }
    let bot = find(state_dir, id)?;
    let messages = room::read_messages(&threads::chat_file(state_dir, id, thread));
    let last = messages.last().filter(|m| m.kind == "interrupted").context("no interrupted turn to continue")?;
    let session = last
        .data
        .get("session")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .context("no session to continue")?;
    if bot.agent == "cursor"
        && let Some(was_full) = last.data.get("full_access").and_then(Value::as_bool)
        && was_full != bot.grants.full_access
    {
        bail!("access mode changed since the interrupted turn; use Try again instead");
    }
    set_thread_session(state_dir, id, thread, Some(session.to_string()))?;
    crate::live::end(&format!("{id}:{thread}"));
    let prompt = "(raiox: the app closed in the middle of your turn; continue from where you stopped without redoing what you already did. Answer in the same language as the person's message.)";
    let asked = Some(said("ask", json!({ "text": "", "continued": true })));
    start(state_dir, jobs, id, thread, Turn { mode: Mode::Talk, text: prompt.to_string(), asked, home: None, command: None, images: Vec::new(), keep_session: true, is_audit: false, auditor_agent: None })
}

/// Re-sends the question from an interrupted turn as a fresh ask.
pub fn retry_turn(state_dir: &Path, jobs: &Jobs, id: &str, thread: &str) -> Result<()> {
    if !threads::valid(thread) {
        bail!("invalid conversation");
    }
    let messages = room::read_messages(&threads::chat_file(state_dir, id, thread));
    let last = messages.last().filter(|m| m.kind == "interrupted").context("no interrupted turn to retry")?;
    let text = last
        .data
        .get("retry_text")
        .and_then(Value::as_str)
        .filter(|t| !t.trim().is_empty())
        .or_else(|| {
            messages
                .iter()
                .rev()
                .find(|m| m.author == "you" && m.kind == "ask")
                .and_then(|m| m.data.get("text").and_then(Value::as_str))
                .filter(|t| !t.trim().is_empty())
        })
        .context("no question to retry")?;
    if last.data.get("aside").and_then(Value::as_bool) == Some(true) {
        ask_aside(state_dir, jobs, id, thread, text)
    } else {
        ask(state_dir, jobs, id, thread, text, &[])
    }
}

/// Starts the conversation over: the next message opens a fresh session.
pub fn reset(state_dir: &Path, jobs: &Jobs, id: &str, thread: &str) -> Result<()> {
    if jobs.is_thread_busy(id, thread) {
        bail!("this conversation is still answering; wait for it to finish");
    }
    set_thread_session(state_dir, id, thread, None)?;
    crate::live::end(&format!("{id}:{thread}"));
    room::append_messages(&threads::chat_file(state_dir, id, thread), vec![note("reset", json!({}))])
}

/// The CLI session a conversation continues (for the context meter).
pub(crate) fn conversation_session(state_dir: &Path, bot: &Bot, thread: &str) -> Result<Option<String>> {
    thread_session(state_dir, bot, thread)
}

/// The CLI session a conversation continues.
fn thread_session(state_dir: &Path, bot: &Bot, thread: &str) -> Result<Option<String>> {
    if thread == MAIN { Ok(bot.session.clone()) } else { threads::session(state_dir, &bot.id, thread) }
}

pub(crate) fn set_thread_session(state_dir: &Path, id: &str, thread: &str, session: Option<String>) -> Result<()> {
    if thread == MAIN { set_session(state_dir, id, session) } else { threads::set_session(state_dir, id, thread, session) }
}

/// Opens a parallel conversation with a bot, named after its first question.
pub fn new_thread(state_dir: &Path, id: &str, default_title: &str) -> Result<threads::Thread> {
    find(state_dir, id)?;
    threads::create(state_dir, id, default_title)
}

fn set_session(state_dir: &Path, id: &str, session: Option<String>) -> Result<()> {
    update(state_dir, |bots| {
        Ok(bots.into_iter().map(|b| if b.id == id { Bot { session: session.clone(), ..b } } else { b }).collect())
    })
}

/// Raio, Dieta or Memo answering in a bot's conversation: no CLI runs, the
/// answer is data about the bot's project, and the bot's session never sees it.
pub fn house(state_dir: &Path, id: &str, thread: &str, agent: &str, data: Value) -> Result<()> {
    if !threads::valid(thread) || !HOUSE.contains(&agent) {
        bail!("invalid conversation or agent");
    }
    let bot = find(state_dir, id)?;
    thread_session(state_dir, &bot, thread)?;
    let message = Message { ms: room::now_ms(), author: agent.into(), kind: "house".into(), data };
    room::append_messages(&threads::chat_file(state_dir, id, thread), vec![message])
}

/// What an edit left in the bot's working copy: the AI's answer and each changed file with its
/// diff, kept as a draft (no draft when nothing changed).
fn drafted(state_dir: &Path, bot: &str, work: &crate::drafts::Workspace, reply: &bot_run::Reply) -> Message {
    match crate::drafts::collect(state_dir, bot, work) {
        Ok((draft, found)) => note("draft", json!({ "draft": draft, "reply": reply, "files": found.files, "blocked": found.blocked })),
        Err(error) => note("failed", json!({ "error": format!("could not read the draft: {error:#}"), "reply": reply })),
    }
}

/// Applies, undoes or discards one of this bot's drafts, and says so in the conversation.
pub fn draft(state_dir: &Path, id: &str, thread: &str, draft: &str, action: &str) -> Result<()> {
    if !threads::valid(thread) {
        bail!("invalid conversation");
    }
    let bot = find(state_dir, id)?;
    let (kind, files) = match action {
        "apply" => ("draft_applied", crate::drafts::apply(state_dir, &bot.id, draft)?),
        "undo" => ("draft_undone", crate::drafts::undo(state_dir, &bot.id, draft)?),
        "discard" => {
            crate::drafts::discard(state_dir, &bot.id, draft)?;
            ("draft_discarded", Vec::new())
        }
        _ => bail!("unknown draft action"),
    };
    room::append_messages(&threads::chat_file(state_dir, id, thread), vec![said(kind, json!({ "draft": draft, "paths": files }))])
}

/// The project's house agents.
pub const HOUSE: [&str; 3] = ["raio", "dieta", "memo"];

/// The folder of a bot, for actions the house agents take in its project.
pub fn dir_of(state_dir: &Path, id: &str) -> Result<PathBuf> {
    find(state_dir, id).map(|bot| bot.dir)
}

/// The change a full-access answer made, with the bot it is in.
fn turn_changes(state_dir: &Path, id: &str, thread: &str, ms: i64) -> Result<(Bot, Vec<Message>, Value)> {
    let bot = find(state_dir, id)?;
    let messages = room::read_messages(&threads::chat_file(state_dir, id, thread));
    let changes = messages.iter().find(|m| m.ms == ms && m.author == "bot" && m.kind == "answer").map(|m| m.data["changes"].clone()).context("no such answer")?;
    if changes.is_null() {
        bail!("this answer changed no file");
    }
    Ok((bot, messages, changes))
}

/// The diff of what a full-access answer changed in its project.
pub fn turn_patch(state_dir: &Path, id: &str, thread: &str, ms: i64) -> Result<Value> {
    let (bot, _, changes) = turn_changes(state_dir, id, thread, ms)?;
    Ok(json!({ "patch": crate::turn_diff::patch(&bot.dir, &changes)?, "files": changes["files"], "head_moved": changes["head_moved"] }))
}

/// Puts back the files a full-access answer changed, once, when they were not changed since.
pub fn undo_turn(state_dir: &Path, id: &str, thread: &str, ms: i64) -> Result<()> {
    let (bot, messages, changes) = turn_changes(state_dir, id, thread, ms)?;
    if messages.iter().any(|m| m.kind == "turn_undone" && m.data["ms"].as_i64() == Some(ms)) {
        bail!("this turn is already undone");
    }
    crate::turn_diff::undo(&bot.dir, &changes)?;
    room::append_messages(&threads::chat_file(state_dir, id, thread), vec![note("turn_undone", json!({ "ms": ms }))])
}

fn note(kind: &str, data: Value) -> Message {
    Message { ms: room::now_ms(), author: "bot".into(), kind: kind.into(), data }
}

fn said(kind: &str, data: Value) -> Message {
    Message { ms: room::now_ms(), author: "you".into(), kind: kind.into(), data }
}

/// The person talks to a bot. The answer arrives later, on its own thread.
/// Asks with a prompt the person did not type (an ai-memory message); `shown` is what the chat displays.
pub fn ask_with(state_dir: &Path, jobs: &Jobs, id: &str, prompt: &str, shown: Value) -> Result<()> {
    start(state_dir, jobs, id, MAIN, Turn { mode: Mode::Talk, text: prompt.to_string(), asked: Some(said("ask", shown)), home: None, command: None, images: Vec::new(), keep_session: true, is_audit: false, auditor_agent: None })
}

pub fn ask(state_dir: &Path, jobs: &Jobs, id: &str, thread: &str, text: &str, images: &[String]) -> Result<()> {
    let text = text.trim();
    let validated = crate::uploads::validate_all(images)?;
    if (text.is_empty() && validated.is_empty()) || text.chars().count() > MAX_TEXT {
        bail!("a message has 1 to {MAX_TEXT} characters");
    }
    if !threads::valid(thread) {
        bail!("invalid conversation");
    }
    // Verify the bot exists before writing uploaded files to disk.
    find(state_dir, id)?;
    let saved = crate::uploads::save(state_dir, id, &validated)?;
    if thread != MAIN {
        let title = if text.is_empty() {
            if saved.len() == 1 { "Image" } else { "Images" }
        } else {
            text
        };
        if let Err(error) = threads::name_after(state_dir, id, thread, title, DEFAULT_THREAD_TITLE) {
            crate::uploads::remove(&saved);
            return Err(error);
        }
    }
    // What happened to a draft since the last question, so the conversation knows.
    let messages = room::read_messages(&threads::chat_file(state_dir, id, thread));
    let news = messages.iter().rev().take_while(|m| !(m.author == "you" && m.kind == "ask")).find_map(|m| match m.kind.as_str() {
        "draft_applied" => Some("applied to the project"),
        "draft_discarded" => Some("discarded"),
        "draft_undone" => Some("applied and then undid"),
        _ => None,
    });
    let for_cli = match news {
        Some(what) => format!("(raiox: the person {what} the change you made in a draft copy.)\n\n{text}"),
        None => text.to_string(),
    };
    // Context the person brought from another conversation goes with the first question after it.
    let brought: String = crate::carry::unread(&messages).into_iter().map(crate::carry::for_cli).collect();
    // The first question after the person switched this conversation to another AI carries what was said.
    let recap = crate::agent_switch::recap(&messages).unwrap_or_default();
    // The first question of a branch carries what was said up to where it branched.
    let branched = crate::fork::branch_recap(&messages).unwrap_or_default();
    let access = crate::fork::access_recap(&messages).unwrap_or_default();
    let for_cli = format!("{recap}{branched}{access}{brought}{for_cli}");
    let names: Vec<String> = saved.iter().map(|s| s.name.clone()).collect();
    let paths: Vec<PathBuf> = saved.iter().map(|s| s.path.clone()).collect();
    let shown = if names.is_empty() { json!({ "text": text }) } else { json!({ "text": text, "images": names }) };
    if let Err(error) = start(state_dir, jobs, id, thread, Turn { mode: Mode::Talk, text: for_cli, asked: Some(said("ask", shown)), home: None, command: None, images: paths, keep_session: true, is_audit: false, auditor_agent: None }) {
        crate::uploads::remove(&saved);
        return Err(error);
    }
    Ok(())
}

/// A side question: answered with the recent messages as context, in a session of its own, so the
/// conversation's session never sees it.
pub fn ask_aside(state_dir: &Path, jobs: &Jobs, id: &str, thread: &str, text: &str) -> Result<()> {
    let text = text.trim();
    if text.is_empty() || text.chars().count() > MAX_TEXT {
        bail!("a message has 1 to {MAX_TEXT} characters");
    }
    if !threads::valid(thread) {
        bail!("invalid conversation");
    }
    let messages = room::read_messages(&threads::chat_file(state_dir, id, thread));
    let for_cli = crate::fork::side_question(&messages, text);
    let asked = Some(said("ask", json!({ "text": text, "aside": true })));
    start(state_dir, jobs, id, thread, Turn { mode: Mode::Talk, text: for_cli, asked, home: None, command: None, images: Vec::new(), keep_session: false, is_audit: false, auditor_agent: None })
}

/// A task done in a copy of the project of the bot's own: what it changes comes back as a draft
/// to apply, so it never touches the project, nor what another AI does in it meanwhile.
pub fn ask_in_copy(state_dir: &Path, home: &Path, jobs: &Jobs, id: &str, thread: &str, prompt: &str, shown: &str) -> Result<()> {
    if find(state_dir, id)?.free {
        bail!("free conversations are read-only");
    }
    let asked = Some(said("ask", json!({ "text": shown, "copy": true })));
    start(state_dir, jobs, id, thread, Turn { mode: Mode::Edit, text: prompt.to_string(), asked, home: Some(home.to_path_buf()), command: None, images: Vec::new(), keep_session: true, is_audit: false, auditor_agent: None })
}

#[derive(Clone, Copy, Deserialize)]
#[serde(tag = "waiting", rename_all = "snake_case")]
pub enum WaitingCard {
    Proposal,
    Command { index: usize },
    Question,
}

impl WaitingCard {
    fn key(self) -> (&'static str, Option<usize>) {
        match self {
            Self::Proposal => ("proposal", None),
            Self::Command { index } => ("command", Some(index)),
            Self::Question => ("question", None),
        }
    }

    fn cancelled(self, messages: &[Message], answer_ms: i64) -> bool {
        let (kind, index) = self.key();
        messages.iter().any(|message| {
            message.kind == "waiting_cancelled" && message.data["answer_ms"].as_i64() == Some(answer_ms)
                && message.data["waiting"].as_str() == Some(kind) && message.data["index"].as_u64() == index.map(|number| number as u64)
        })
    }

    fn exists(self, answer: &Message) -> bool {
        let reply = &answer.data["reply"];
        match self {
            Self::Proposal => reply["proposal"].as_str().is_some_and(|text| !text.is_empty()),
            Self::Question => reply["question"].as_str().is_some_and(|text| !text.is_empty()),
            Self::Command { index } => reply["run"][index]["command"].is_string() && reply["run"][index]["refused"].is_null(),
        }
    }
}

/// Closes a single waiting card in the conversation without starting a turn.
pub fn cancel_waiting(state_dir: &Path, jobs: &Jobs, id: &str, thread: &str, answer_ms: i64, card: WaitingCard) -> Result<()> {
    let _guard = WAITING.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if !threads::valid(thread) {
        bail!("invalid conversation");
    }
    find(state_dir, id)?;
    let file = threads::chat_file(state_dir, id, thread);
    let messages = room::read_messages(&file);
    if card.cancelled(&messages, answer_ms) {
        return Ok(());
    }
    if jobs.is_thread_busy(id, thread) {
        bail!("this conversation is still answering");
    }
    let current = messages.iter().rev().find(|m| !ASIDE.contains(&m.kind.as_str()));
    if !current.is_some_and(|answer| answer.ms == answer_ms && answer.author == "bot" && answer.kind == "answer" && card.exists(answer)) {
        bail!("this waiting card is no longer current");
    }
    let (kind, index) = card.key();
    room::append_messages(&file, vec![said("waiting_cancelled", json!({ "answer_ms": answer_ms, "waiting": kind, "index": index }))])
}

/// The person approves the proposal in the bot's latest answer: the AI makes it in a draft copy.
pub fn approve(state_dir: &Path, home: &Path, jobs: &Jobs, id: &str, thread: &str, answer_ms: Option<i64>) -> Result<()> {
    let _guard = WAITING.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if !threads::valid(thread) {
        bail!("invalid conversation");
    }
    if find(state_dir, id).is_ok_and(|b| b.free) {
        bail!("free conversations are read-only");
    }
    let messages = room::read_messages(&threads::chat_file(state_dir, id, thread));
    // A failed or interrupted approval can be retried, and a message sent on stays out of the way.
    let last = messages.iter().rev().find(|m| m.kind != "approve" && !ASIDE.contains(&m.kind.as_str()));
    if let Some(ms) = answer_ms
        && !last.is_some_and(|m| m.ms == ms)
    {
        bail!("this proposal is no longer current");
    }
    if last.is_some_and(|m| WaitingCard::Proposal.cancelled(&messages, m.ms)) {
        bail!("this proposal was cancelled");
    }
    let proposal = last
        .filter(|m| m.author == "bot" && m.kind == "answer")
        .and_then(|m| m.data.pointer("/reply/proposal").and_then(Value::as_str))
        .map(str::to_string)
        .context("the latest answer has no proposal to approve")?;
    // Every CLI edits in a fresh session inside the draft, so it gets its earlier answer as context.
    let answer = last.and_then(|m| m.data.pointer("/reply/text")).and_then(Value::as_str).unwrap_or("");
    let review = last.and_then(|m| review::for_answer(&messages, m.ms)).unwrap_or_default();
    let text = format!("{proposal}\n\nWhat you said before the approval:\n{answer}{review}\n\nAnswer in the same language as that earlier answer.");
    let asked = Some(said("approve", json!({ "proposal": proposal })));
    start(state_dir, jobs, id, thread, Turn { mode: Mode::Edit, text, asked, home: Some(home.to_path_buf()), command: None, images: Vec::new(), keep_session: true, is_audit: false, auditor_agent: None })
}

/// Most commands a bot may run without asking, per project.
const MAX_ALLOWED: usize = 40;

/// Runs a command the bot asked for in its latest answer, then lets it go on with the output.
/// With `always`, that exact command runs without asking from now on in this project.
pub fn run_command(state_dir: &Path, jobs: &Jobs, id: &str, thread: &str, answer_ms: Option<i64>, index: usize, always: bool) -> Result<()> {
    let _guard = WAITING.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if !threads::valid(thread) {
        bail!("invalid conversation");
    }
    if find(state_dir, id)?.free {
        bail!("free conversations cannot run commands");
    }
    if jobs.is_thread_busy(id, thread) {
        bail!("this conversation is still answering");
    }
    let messages = room::read_messages(&threads::chat_file(state_dir, id, thread));
    let last = messages.iter().rev().find(|m| !ASIDE.contains(&m.kind.as_str()));
    if !last.is_some_and(|m| answer_ms.is_none_or(|ms| m.ms == ms) && !(WaitingCard::Command { index }).cancelled(&messages, m.ms)) {
        bail!("this command request is no longer current");
    }
    let command = last
        .filter(|m| m.author == "bot" && m.kind == "answer")
        .and_then(|m| m.data.pointer(&format!("/reply/run/{index}/command")).and_then(Value::as_str))
        .map(str::to_string)
        .context("the latest answer asks for no such command")?;
    if always {
        allow(state_dir, id, &command, true)?;
    } else {
        crate::commands::check(&command).map_err(|why| anyhow::anyhow!("raiox will not run this command ({why})"))?;
    }
    start(state_dir, jobs, id, thread, Turn { mode: Mode::Talk, text: String::new(), asked: None, home: None, command: Some(command), images: Vec::new(), keep_session: true, is_audit: false, auditor_agent: None })
}

/// Sets the model the bot talks on (empty: the same as for changes).
pub fn set_talk_model(state_dir: &Path, id: &str, model: Option<&str>) -> Result<()> {
    // A setting for the next messages: it may change while a turn runs (that turn keeps what it started with).
    find(state_dir, id)?;
    let talk_model = check_model(model)?;
    update(state_dir, |bots| Ok(bots.into_iter().map(|b| if b.id == id { Bot { talk_model: talk_model.clone(), ..b } } else { b }).collect()))
}

/// A turn whose message for the CLI differs from what the conversation shows: a channel's
/// context goes to the CLI, the person's words to the screen.
pub fn ask_shown(state_dir: &Path, jobs: &Jobs, id: &str, thread: &str, shown: &str, cli: &str, channel: &str) -> Result<std::thread::JoinHandle<Option<Message>>> {
    let asked = Some(said("ask", json!({ "text": shown, "channel": channel })));
    start_turn(state_dir, jobs, id, thread, Turn { mode: Mode::Talk, text: cli.to_string(), asked, home: None, command: None, images: Vec::new(), keep_session: true, is_audit: false, auditor_agent: None })
}

/// Runs a Dev/Infra audit in `thread` using `audit_agent` (which may differ from the bot's own CLI).
/// Always starts in a fresh session, never uses personal MCP servers, and never triggers auto-edit.
pub(crate) fn run_audit_turn(state_dir: &Path, jobs: &Jobs, id: &str, thread: &str, audit_agent: &str, prompt: &str, shown: Value) -> Result<()> {
    if !threads::valid(thread) {
        bail!("invalid conversation");
    }
    let bot = find(state_dir, id)?;
    crate::budget::check(state_dir, &bot.name)?;
    crate::grants::check_week(state_dir, &bot)?;
    jobs.start_mode(id, thread, Mode::Talk)?;
    let asked = said("ask", shown);
    if let Err(error) = room::append_messages(&threads::chat_file(state_dir, id, thread), vec![asked]) {
        jobs.finish(id, thread);
        return Err(error);
    }
    let same_agent = audit_agent == bot.agent;
    let effective = Bot {
        agent: audit_agent.to_string(),
        model: if same_agent { bot.model.clone() } else { None },
        effort: if same_agent { bot.effort.clone() } else { None },
        talk_model: None,
        grants: crate::grants::Grants {
            mcp: false,
            web: bot.grants.web && crate::grants::web_possible(audit_agent),
            auto_edit: false,
            ..bot.grants
        },
        ..bot.clone()
    };
    let turn = Turn {
        mode: Mode::Talk,
        text: prompt.to_string(),
        asked: None,
        home: None,
        command: None,
        images: Vec::new(),
        keep_session: false,
        is_audit: true,
        auditor_agent: Some(audit_agent.to_string()),
    };
    let (state_dir, jobs, thread) = (state_dir.to_path_buf(), jobs.clone(), thread.to_string());
    std::thread::spawn(move || {
        turn::run(&state_dir, &jobs, &effective, &thread, turn, None);
        jobs.finish(&effective.id, &thread);
        // Deliberately skip auto_edit::after_turn: audit fixes always require explicit human approval.
        crate::push::after_turn(&state_dir, &effective, &thread);
        crate::team::refresh(&state_dir, vec![effective.dir.clone()]);
    });
    Ok(())
}

/// Asks another bot of the project to review a proposal in this conversation.
pub fn request_review(state_dir: &Path, jobs: &Jobs, id: &str, thread: &str, answer_ms: i64, reviewer: &str) -> Result<()> {
    review::request(state_dir, jobs, id, thread, answer_ms, reviewer)
}

/// Sets the bot that reviews every proposal of this one (empty: none).
pub fn set_reviewer(state_dir: &Path, id: &str, reviewer: &str) -> Result<()> {
    let bot = find(state_dir, id)?;
    let reviewer = (!reviewer.is_empty()).then(|| reviewer.to_string());
    if let Some(r) = &reviewer
        && !review::reviewers(state_dir, &bot).iter().any(|b| &b.id == r)
    {
        bail!("the reviewer has to be another AI of this project");
    }
    update(state_dir, |bots| Ok(bots.into_iter().map(|b| if b.id == id { Bot { reviewer: reviewer.clone(), ..b } } else { b }).collect()))
}

/// Sets or clears the daily token limit of this bot's project (0 clears it).
/// Changes what a bot may do (phone, web, MCP, weekly ceiling).
pub fn set_grants(state_dir: &Path, id: &str, input: &Value) -> Result<()> {
    // A setting for the next messages: it may change while a turn runs (that turn keeps what it started with).
    let bot = find(state_dir, id)?;
    if bot.free {
        bail!("free conversations are always read-only");
    }
    let grants = crate::grants::changed(&bot.grants, &bot.agent, input)?;
    let full_changed = grants.full_access != bot.grants.full_access;
    update(state_dir, |bots| Ok(bots.into_iter().map(|b| if b.id == id { Bot { grants: grants.clone(), ..b } } else { b }).collect()))?;
    // Cursor keeps a resumed session in the mode it began in (ask mode stays read-only, agent mode
    // keeps writing), so its conversations start new sessions, carrying what was said.
    if bot.agent == "cursor" && full_changed {
        restart_sessions(state_dir, id, json!({ "full": grants.full_access }))?;
    }
    Ok(())
}

/// Every conversation of a bot starts a new CLI session on its next question, after a mark.
fn restart_sessions(state_dir: &Path, id: &str, data: Value) -> Result<()> {
    let conversations = std::iter::once(MAIN.to_string()).chain(threads::list(state_dir, id).into_iter().map(|t| t.id));
    for thread in conversations {
        if thread_session(state_dir, &find(state_dir, id)?, &thread)?.is_none() {
            continue;
        }
        set_thread_session(state_dir, id, &thread, None)?;
        crate::live::end(&format!("{id}:{thread}"));
        room::append_messages(&threads::chat_file(state_dir, id, &thread), vec![note("access_changed", data.clone())])?;
    }
    Ok(())
}

pub fn set_limit(state_dir: &Path, id: &str, tokens: u64) -> Result<()> {
    let bot = find(state_dir, id)?;
    crate::budget::set(state_dir, &bot.name, (tokens > 0).then_some(tokens))
}

/// Lets an exact command run without asking in this bot's project, or stops letting it.
pub fn allow(state_dir: &Path, id: &str, command: &str, on: bool) -> Result<()> {
    let bot = find(state_dir, id)?;
    if bot.free {
        bail!("free conversations cannot run commands");
    }
    if on {
        crate::commands::check(command).map_err(|why| anyhow::anyhow!("raiox will not run this command ({why})"))?;
        if !bot.allowed.iter().any(|c| c == command) && bot.allowed.len() >= MAX_ALLOWED {
            bail!("a project can have up to {MAX_ALLOWED} allowed commands");
        }
    }
    update(state_dir, |bots| {
        Ok(bots
            .into_iter()
            .map(|b| {
                if b.id != id {
                    return b;
                }
                let mut allowed: Vec<String> = b.allowed.iter().filter(|c| *c != command).cloned().collect();
                if on {
                    allowed.push(command.to_string());
                }
                Bot { allowed, ..b }
            })
            .collect())
    })
}

/// Keeps one of the bot's answers as a decision of the project in ai-memory, and says so in the conversation.
pub fn remember(state_dir: &Path, id: &str, thread: &str, answer_ms: i64) -> Result<()> {
    if !threads::valid(thread) {
        bail!("invalid conversation");
    }
    let bot = find(state_dir, id)?;
    let file = threads::chat_file(state_dir, id, thread);
    let messages = room::read_messages(&file);
    if messages.iter().any(|m| m.kind == "remembered" && m.data.get("answer_ms").and_then(Value::as_i64) == Some(answer_ms)) {
        bail!("this answer is already kept in ai-memory");
    }
    let at = messages.iter().position(|m| m.ms == answer_ms && m.author == "bot" && m.kind == "answer").context("no such answer")?;
    let answer = messages[at].data.pointer("/reply/text").and_then(Value::as_str).context("the answer has no text")?;
    let question = messages[..at].iter().rev().find(|m| m.author == "you" && m.kind == "ask").and_then(|m| m.data.get("text")).and_then(Value::as_str).unwrap_or("");
    let agent = crate::connections::KNOWN.iter().find(|k| k.id == bot.agent).map_or(bot.agent.as_str(), |k| k.name);
    let path = crate::memory::remember(&bot.dir, question, answer, agent, &bot.name)?;
    room::append_messages(&file, vec![said("remembered", json!({ "answer_ms": answer_ms, "path": path }))])
}

fn start(state_dir: &Path, jobs: &Jobs, id: &str, thread: &str, turn: Turn) -> Result<()> {
    start_turn(state_dir, jobs, id, thread, turn).map(|_| ())
}

fn start_turn(state_dir: &Path, jobs: &Jobs, id: &str, thread: &str, turn: Turn) -> Result<std::thread::JoinHandle<Option<Message>>> {
    if !threads::valid(thread) {
        bail!("invalid conversation");
    }
    let bot = find(state_dir, id)?;
    if bot.free && (turn.mode == Mode::Edit || turn.command.is_some()) {
        bail!("free conversations are read-only");
    }
    crate::budget::check(state_dir, &bot.name)?;
    crate::grants::check_week(state_dir, &bot)?;
    // A turn that does not keep its session (a side question) does not continue the conversation's either.
    let session = if turn.keep_session { thread_session(state_dir, &bot, thread)? } else { None };
    jobs.start_mode(id, thread, turn.mode)?;
    if let Some(asked) = &turn.asked
        && let Err(error) = room::append_messages(&threads::chat_file(state_dir, id, thread), vec![asked.clone()])
    {
        jobs.finish(id, thread);
        return Err(error);
    }
    let (state_dir, jobs, thread) = (state_dir.to_path_buf(), jobs.clone(), thread.to_string());
    Ok(std::thread::spawn(move || {
        let answer = turn::run(&state_dir, &jobs, &bot, &thread, turn, session);
        jobs.finish(&bot.id, &thread);
        if !bot.free {
            crate::auto_edit::after_turn(&state_dir, &jobs, &bot.id, &thread);
            crate::team::refresh(&state_dir, vec![bot.dir.clone()]);
        }
        crate::push::after_turn(&state_dir, &bot, &thread);
        answer
    }))
}

/// Everything the panel shows about bots.
pub fn state(store: &Store, home: &Path, state_dir: &Path, jobs: &Jobs) -> Result<Value> {
    let bots: Vec<Value> = list(state_dir)
        .into_iter()
        .map(|bot| {
            let messages = room::read_messages(&chat_file(state_dir, &bot.id));
            let (last_ms, last_line) = main_preview(&messages);
            let side: Vec<Value> = threads::list(state_dir, &bot.id)
                .into_iter()
                .map(|t| json!({ "id": t.id, "title": t.title, "has_session": t.session.is_some(), "model": t.model, "created_ms": t.created_ms, "messages": room::read_messages(&threads::chat_file(state_dir, &bot.id, &t.id)) }))
                .collect();
            let busy_threads = jobs.busy_threads(&bot.id);
            let primary = busy_threads.first();
            json!({
                "id": bot.id,
                "agent": bot.agent,
                "name": bot.name,
                "dir": bot.dir,
                "free": bot.free,
                "has_session": bot.session.is_some(),
                "persona": bot.persona,
                "model": bot.model,
                "effort": bot.effort,
                "allowed": bot.allowed,
                "grants": crate::grants::state(state_dir, &bot),
                "auditor": crate::auditor::state(state_dir, &bot),
                "week": crate::room::week_of(store, home, &bot.dir),
                "talk_model": bot.talk_model,
                "reviewer": bot.reviewer,
                "budget": { "limit": crate::budget::limit(state_dir, &bot.name), "used": crate::budget::used_today(state_dir, &bot.name), "choices": crate::budget::CHOICES },
                "busy": !busy_threads.is_empty(),
                "busy_thread": primary.map(|b| b.thread.as_str()),
                "busy_threads": busy_threads,
                "step": primary.and_then(|b| b.step.as_ref()),
                "trail": primary.and_then(|b| b.trail.as_ref()),
                "started_ms": primary.map(|b| b.started_ms),
                "last_ms": last_ms,
                "last_line": last_line,
                "messages": messages,
                "threads": side,
                "pins": crate::pins::list(state_dir, &bot.id),
                "routines": crate::routines::list(state_dir).into_iter().filter(|r| r.bot == bot.id).collect::<Vec<_>>(),
            })
        })
        .collect();
    // The CLIs the person turned on that raiox can run as bots, for the pickers.
    let settings = crate::settings::Settings::load(state_dir);
    let agents: Vec<Value> = crate::connections::KNOWN
        .iter()
        .filter(|k| k.bots && settings.is_enabled(k.id))
        .map(|k| json!({ "id": k.id, "name": k.name }))
        .collect();
    let requests = crate::requests::list(state_dir);
    Ok(json!({
        "bots": bots,
        "candidates": candidates(store, home, state_dir)?,
        "agents": agents,
        "links": crate::links::list(state_dir),
        "batches": crate::batches::list(state_dir),
        "channels": crate::channels::state(state_dir),
        "requests": requests,
        "rsi_python": crate::rsi::python(),
        "rsi_python_can_install": crate::install::can_install_python(),
        "progress": crate::progress::state(state_dir, !requests.is_empty()),
        "show_all_navigation": settings.show_all_navigation,
        "max_text": MAX_TEXT,
    }))
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
    fn an_approval_from_the_desk_is_refused_once_the_proposal_is_not_the_latest() {
        let dir = temp_dir("aprova-velha");
        let messages = vec![
            room::Message { ms: 100, author: "bot".into(), kind: "answer".into(), data: json!({ "reply": { "text": "Plan", "proposal": "Rename the helper" } }) },
            room::Message { ms: 200, author: "you".into(), kind: "ask".into(), data: json!({ "text": "and the tests?" }) },
        ];
        room::append_messages(&threads::chat_file(&dir, "b1", MAIN), messages).unwrap();
        let error = approve(&dir, &dir, &Jobs::default(), "b1", MAIN, Some(100)).unwrap_err();
        assert!(error.to_string().contains("no longer current"), "{error:#}");
    }

    #[test]
    fn cancelling_a_card_only_writes_one_local_note_and_blocks_the_cancelled_action() {
        let home = temp_dir("cancel-waiting");
        fs::create_dir_all(home.join("Projects/app")).unwrap();
        let state = home.join(".state");
        let bot = activate(&state, &home, "Projects/app", "claude", None).unwrap();
        let jobs = Jobs::default();
        let file = threads::chat_file(&state, &bot.id, MAIN);
        let reply = json!({ "proposal": "Rename the helper", "question": "Which name?", "run": [{ "command": "git status" }] });
        room::append_messages(&file, vec![Message { ms: 100, author: "bot".into(), kind: "answer".into(), data: json!({ "reply": reply }) }]).unwrap();
        for _ in 0..2 {
            cancel_waiting(&state, &jobs, &bot.id, MAIN, 100, WaitingCard::Proposal).unwrap();
        }
        assert!(approve(&state, &home, &jobs, &bot.id, MAIN, Some(100)).unwrap_err().to_string().contains("cancelled"));
        assert!(cancel_waiting(&state, &jobs, &bot.id, MAIN, 99, WaitingCard::Question).is_err());
        cancel_waiting(&state, &jobs, &bot.id, MAIN, 100, WaitingCard::Question).unwrap();
        cancel_waiting(&state, &jobs, &bot.id, MAIN, 100, WaitingCard::Command { index: 0 }).unwrap();
        assert!(run_command(&state, &jobs, &bot.id, MAIN, Some(100), 0, false).is_err());
        assert!(!jobs.is_busy(&bot.id));
        let messages = room::read_messages(&file);
        assert_eq!(messages.len(), 4);
        assert_eq!(messages[1..].iter().map(|m| m.kind.as_str()).collect::<Vec<_>>(), vec!["waiting_cancelled"; 3]);
        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn a_cursor_bot_starts_new_sessions_when_its_full_access_changes() {
        let home = temp_dir("cursor-acesso");
        fs::create_dir_all(home.join("Projects/app")).unwrap();
        let state = home.join(".state");
        let cursor = activate(&state, &home, "Projects/app", "cursor", None).unwrap();
        let claude = activate(&state, &home, "Projects/app", "claude", None).unwrap();
        for bot in [&cursor, &claude] {
            set_session(&state, &bot.id, Some("s-ask".into())).unwrap();
            room::append_messages(&threads::chat_file(&state, &bot.id, MAIN), vec![said("ask", json!({ "text": "create the .env" }))]).unwrap();
            set_grants(&state, &bot.id, &json!({ "full_access": true })).unwrap();
        }
        let marks = |id: &str| room::read_messages(&threads::chat_file(&state, id, MAIN)).iter().filter(|m| m.kind == "access_changed").count();
        assert_eq!((find(&state, &cursor.id).unwrap().session, marks(&cursor.id)), (None, 1), "Cursor's ask-mode session is left behind");
        assert_eq!((find(&state, &claude.id).unwrap().session.as_deref(), marks(&claude.id)), (Some("s-ask"), 0), "Claude Code takes its mode on each turn");
        set_grants(&state, &cursor.id, &json!({ "devices": false })).unwrap();
        assert_eq!(marks(&cursor.id), 1, "only a change of full access restarts");
    }

    #[test]
    fn activation_needs_a_project_folder_under_the_home() {
        let home = temp_dir("home");
        fs::create_dir_all(home.join("Projects/app")).unwrap();
        let state = home.join(".state");
        assert!(activate(&state, &home, "", "claude", None).is_err());
        assert!(activate(&state, &home, "../", "claude", None).is_err());
        fs::create_dir_all(home.join(".claude/projects")).unwrap();
        assert!(activate(&state, &home, ".claude/projects", "claude", None).is_err());
        assert!(activate(&state, &home, "Projects/app", "copilot", None).is_err());
        let bot = activate(&state, &home, "Projects/app", "claude", None).unwrap();
        assert_eq!(bot.name, "app");
        activate(&state, &home, "Projects/app", "claude", None).unwrap();
        assert_eq!(list(&state).len(), 1);
    }

    #[test]
    fn a_picked_local_folder_can_be_opened_without_git_or_a_home_relative_path() {
        let root = temp_dir("picked-folder");
        let home = root.join("home");
        let state = home.join(".state");
        let project = root.join("Pasta de projetos").join("Aplicação");
        fs::create_dir_all(&state).unwrap();
        fs::create_dir_all(&project).unwrap();
        let source_file = project.join("source.txt");
        fs::write(&source_file, "original").unwrap();

        let project_path = project.to_str().unwrap();
        assert!(activate(&state, &home, project_path, "claude", None).is_err());
        let persona = Persona {
            preset: "custom".into(),
            name: "Parceira".into(),
            instructions: String::new(),
            color: "#5599ff".into(),
            shape: "claude".into(),
            finish: "solid".into(),
            role: String::new(),
        };
        let bot = activate_folder(&state, &home, project_path, "claude", Some(persona.clone())).unwrap();
        assert_eq!(bot.dir, project.canonicalize().unwrap());
        assert_eq!(bot.name, "Aplicação");
        set_session(&state, &bot.id, Some("first-session".into())).unwrap();
        assert_eq!(activate_folder(&state, &home, project_path, "claude", None).unwrap().id, bot.id);
        assert_eq!(list(&state).len(), 1);
        let reopened = find(&state, &bot.id).unwrap();
        assert_eq!(reopened.session.as_deref(), Some("first-session"));
        assert_eq!(reopened.persona, Some(persona));
        assert_eq!(fs::read_dir(&project).unwrap().count(), 1);
        assert_eq!(fs::read_to_string(&source_file).unwrap(), "original");

        fs::create_dir_all(home.join(".ssh")).unwrap();
        for invalid in [&home, &state, &source_file, &project.join("missing"), &PathBuf::from("relative"), &home.join(".ssh")] {
            assert!(picked_project_dir(&state, &home, invalid).is_err());
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn approval_needs_a_proposal_in_the_latest_answer() {
        let state = temp_dir("approve");
        let jobs = Jobs::default();
        let answer = |proposal: Value| note("answer", json!({ "mode": "talk", "reply": { "text": "t", "proposal": proposal } }));
        room::append_messages(&chat_file(&state, "b1"), vec![answer(Value::Null)]).unwrap();
        assert!(approve(&state, Path::new("/home"), &jobs, "b1", MAIN, None).unwrap_err().to_string().contains("no proposal"));
        room::append_messages(&chat_file(&state, "b1"), vec![answer(json!("rename"))]).unwrap();
        // Keeping the answer and a review of it leave the proposal approvable.
        let aside = vec![said("remembered", json!({})), said("review_asked", json!({})), note("review", json!({ "reply": { "text": "ok" } }))];
        room::append_messages(&chat_file(&state, "b1"), aside).unwrap();
        // With a proposal it gets past the check and fails only because no bot b1 is registered.
        assert!(approve(&state, Path::new("/home"), &jobs, "b1", MAIN, None).unwrap_err().to_string().contains("no bot"));
    }

    #[test]
    fn ask_validates_images_and_leaves_no_orphans_on_failure() {
        use base64::Engine;
        let home = temp_dir("ask-images");
        fs::create_dir_all(home.join("Projects/app")).unwrap();
        let state = home.join(".state");
        let bot = activate(&state, &home, "Projects/app", "claude", None).unwrap();
        let jobs = Jobs::default();

        // Empty text and no images is rejected.
        assert!(ask(&state, &jobs, &bot.id, MAIN, "   ", &[]).is_err());

        // Invalid image bytes (e.g. text disguised as PNG data URL) are rejected before writing anything.
        let fake = format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(b"not a png"));
        assert!(ask(&state, &jobs, &bot.id, MAIN, "look", &[fake]).is_err());
        assert!(!crate::uploads::dir(&state, &bot.id).exists());

        // If the conversation is busy, validated images saved before start are cleaned up.
        assert!(jobs.start_mode(&bot.id, MAIN, Mode::Talk).is_ok());
        let valid_png = base64::engine::general_purpose::STANDARD.encode([0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0]);
        assert!(ask(&state, &jobs, &bot.id, MAIN, "", &[valid_png]).unwrap_err().to_string().contains("still answering"));
        let remaining: Vec<_> = fs::read_dir(crate::uploads::dir(&state, &bot.id)).into_iter().flatten().flatten().collect();
        assert!(remaining.is_empty(), "failed ask must not leave orphan upload files");
        jobs.finish(&bot.id, MAIN);
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn parallel_talk_threads_run_stop_and_aggregate_independently() {
        let jobs = Jobs::default();
        assert!(!jobs.is_busy("b1"));
        assert_eq!(jobs.busy_count(), 0);

        // Two conversations of the same bot start at the same time in Mode::Talk.
        jobs.start_mode("b1", MAIN, Mode::Talk).unwrap();
        jobs.start_mode("b1", "side1", Mode::Talk).unwrap();
        assert!(jobs.start_mode("b1", MAIN, Mode::Talk).is_err(), "same conversation cannot start twice");
        assert!(jobs.is_busy("b1"));
        assert!(jobs.is_thread_busy("b1", MAIN));
        assert!(jobs.is_thread_busy("b1", "side1"));
        assert!(!jobs.is_thread_busy("b1", "side2"));
        assert_eq!(jobs.busy_count(), 2);

        // Steps and trails stay isolated per conversation.
        jobs.set_step("b1", MAIN, crate::bot_steps::Step { kind: "read", target: "a.rs".into() });
        jobs.set_step("b1", "side1", crate::bot_steps::Step { kind: "search", target: "b.rs".into() });
        let threads = jobs.busy_threads("b1");
        assert_eq!(threads.len(), 2);
        let main_step = threads.iter().find(|t| t.thread == MAIN).and_then(|t| t.step.as_ref()).unwrap();
        let side_step = threads.iter().find(|t| t.thread == "side1").and_then(|t| t.step.as_ref()).unwrap();
        assert_eq!(main_step.target, "a.rs");
        assert_eq!(side_step.target, "b.rs");

        // Stopping one conversation does not stop the other.
        jobs.stop("b1", Some(MAIN)).unwrap();
        assert!(jobs.should_stop("b1", MAIN));
        assert!(!jobs.should_stop("b1", "side1"));

        // Finishing one keeps bot busy until the other finishes too.
        jobs.finish("b1", MAIN);
        assert!(jobs.is_busy("b1"));
        assert!(!jobs.is_thread_busy("b1", MAIN));
        assert!(jobs.is_thread_busy("b1", "side1"));
        assert_eq!(jobs.busy_count(), 1);

        jobs.finish("b1", "side1");
        assert!(!jobs.is_busy("b1"));
        assert_eq!(jobs.busy_count(), 0);
    }

    #[test]
    fn edit_mode_stays_one_per_bot_while_talk_can_run_alongside() {
        let jobs = Jobs::default();
        jobs.start_mode("b1", MAIN, Mode::Edit).unwrap();
        // A second Edit on another conversation of the same bot is refused with a clear message.
        let err = jobs.start_mode("b1", "side1", Mode::Edit).unwrap_err();
        assert!(err.to_string().contains("draft copy"), "{err:#}");
        // Another bot's Edit is unaffected, and Talk on another thread of the same bot is allowed.
        jobs.start_mode("b2", MAIN, Mode::Edit).unwrap();
        jobs.start_mode("b1", "side1", Mode::Talk).unwrap();
        assert_eq!(jobs.busy_threads("b1").len(), 2);
        assert!(jobs.busy_threads("b1").iter().any(|t| t.thread == MAIN && t.editing));
        assert!(jobs.busy_threads("b1").iter().any(|t| t.thread == "side1" && !t.editing));
        // Once the first Edit finishes, another conversation of b1 can run an Edit.
        jobs.finish("b1", MAIN);
        jobs.start_mode("b1", "side2", Mode::Edit).unwrap();
    }

    #[test]
    fn candidates_include_local_git_folders_after_usage_without_duplicates() {
        let home = temp_dir("candidates-git");
        let state = home.join("raiox-state");
        fs::create_dir_all(home.join("Projects/used-app/.git")).unwrap();
        fs::create_dir_all(home.join("Projects/fresh-repo/.git")).unwrap();
        fs::create_dir_all(home.join("Projects/plain-dir")).unwrap();
        fs::create_dir_all(home.join("Projects/.secret/.git")).unwrap();
        fs::create_dir_all(home.join(".hidden/.git")).unwrap();
        fs::create_dir_all(home.join("Library/Caches/raiox")).unwrap();
        fs::create_dir_all(home.join("Caches/scratch")).unwrap();
        fs::create_dir_all(&state).unwrap();
        let store = Store::open_in_memory().unwrap();
        for (id, project, ts) in [
            ("c1", "Projects/used-app", 5000),
            ("c2", "Library/Caches/raiox", 9000),
            ("c3", "Caches/scratch", 8500),
            ("c4", "Projects/.secret", 8000),
            ("c5", ".hidden", 7500),
            ("c6", "raiox-state", 7000),
        ] {
            store
                .conn()
                .execute(
                    "INSERT INTO calls(id, source, file, session, project, model, is_sub, ts_ms, input, cache_read, cache_write_5m, cache_write_1h, output, thinking, web_searches, seq, segment, is_first)
                     VALUES(?1, 'claude', 'f1', 's1', ?2, 'm', 0, ?3, 1, 0, 0, 0, 1, 0, 0, 1, 0, 1)",
                    rusqlite::params![id, project, ts],
                )
                .unwrap();
        }
        let list = candidates(&store, &home, &state).unwrap();
        let names: Vec<&str> = list.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["used-app", "fresh-repo"], "usage projects come first, git projects after, excluding hidden/cache/state dirs");
        assert_eq!(list[0].agent.as_deref(), Some("claude"));
        assert_eq!(list[0].requests, 1);
        assert_eq!(list[1].agent, None);
        assert_eq!(list[1].requests, 0);
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn main_preview_extracts_last_timestamp_and_clean_line() {
        assert_eq!(main_preview(&[]), (None, None));
        let mut first = said("ask", json!({ "text": "  First question  " }));
        first.ms = 1000;
        let mut second = note("answer", json!({ "reply": { "text": "   \n## Resumo do `fix`   aqui\nSegunda linha" } }));
        second.ms = 2000;
        let mut third = note("remembered", json!({}));
        third.ms = 3000;
        let (last_ms, last_line) = main_preview(&[first, second, third]);
        assert_eq!(last_ms, Some(3000));
        assert_eq!(last_line.as_deref(), Some("Resumo do fix aqui"));

        let long = "a".repeat(PREVIEW_CHARS + 20);
        let mut fourth = said("ask", json!({ "text": long }));
        fourth.ms = 4000;
        let (last_ms, last_line) = main_preview(&[fourth]);
        assert_eq!(last_ms, Some(4000));
        assert_eq!(last_line.as_deref().map(|s| s.chars().count()), Some(PREVIEW_CHARS));
    }

    #[test]
    fn project_name_validation_accepts_safe_names_only() {
        assert!(valid_project_name("my-app"));
        assert!(valid_project_name("MyApp_2"));
        assert!(valid_project_name("_scratch"));
        assert!(valid_project_name(&"a".repeat(MAX_PROJECT_NAME)));
        assert!(!valid_project_name(""));
        assert!(!valid_project_name(&"a".repeat(MAX_PROJECT_NAME + 1)));
        assert!(!valid_project_name("-flag"));
        assert!(!valid_project_name("../escape"));
        assert!(!valid_project_name("a/b"));
        assert!(!valid_project_name("with space"));
        assert!(!valid_project_name("dot.name"));
    }

    #[test]
    fn create_project_creates_folder_and_refuses_non_empty() {
        let home = temp_dir("create-project");
        let state = home.join(".state");

        // Fresh project folder is created and activated.
        let bot = create_project(&state, &home, "My_App-1", "claude").unwrap();
        assert_eq!(bot.name, "My_App-1");
        assert!(home.join("Projects/My_App-1").is_dir());
        assert_eq!(list(&state).len(), 1);

        // An existing empty folder is accepted.
        fs::create_dir_all(home.join("Projects/empty-dir")).unwrap();
        let bot2 = create_project(&state, &home, "empty-dir", "codex").unwrap();
        assert_eq!(bot2.name, "empty-dir");

        // An existing non-empty folder is refused.
        fs::create_dir_all(home.join("Projects/busy-dir")).unwrap();
        fs::write(home.join("Projects/busy-dir/README.md"), "hi").unwrap();
        let err = create_project(&state, &home, "busy-dir", "claude").unwrap_err();
        assert!(err.to_string().contains("not empty"), "{err:#}");

        // A file at the same path is refused.
        fs::write(home.join("Projects/taken-file"), "x").unwrap();
        assert!(create_project(&state, &home, "taken-file", "claude").is_err());

        // An unsupported agent is refused without creating the folder.
        assert!(create_project(&state, &home, "bad-agent", "copilot").is_err());
        assert!(!home.join("Projects/bad-agent").exists());

        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn free_bot_is_created_once_per_agent_excluded_from_projects_and_strictly_read_only() {
        let home = temp_dir("free-bot");
        fs::create_dir_all(home.join("Projects/real-app/.git")).unwrap();
        let state = home.join("state");
        let jobs = Jobs::default();

        // A real project bot alongside a free bot.
        let real = activate(&state, &home, "Projects/real-app", "claude", None).unwrap();
        assert!(!real.free);

        // Creating the free bot for an AI creates an empty folder under <state>/conversa-livre/<agent>
        // and reuses the exact same bot on repeat calls.
        let first = ensure_free(&state, "claude").unwrap();
        let second = ensure_free(&state, "claude").unwrap();
        assert_eq!(first.id, second.id);
        assert!(first.free);
        assert!(first.dir.is_dir());
        assert!(first.dir.starts_with(state.canonicalize().unwrap().join(FREE_DIR)));
        assert!(!first.grants.full_access && !first.grants.auto_run && !first.grants.auto_edit && !first.grants.bypass);
        let codex_free = ensure_free(&state, "codex").unwrap();
        assert_ne!(first.id, codex_free.id);
        assert_eq!(list(&state).len(), 3, "registry has 1 project bot + 2 free bots");

        // Free bots never appear in project_list, candidates, links, or reviewers.
        let projects = project_list(&state);
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].id, real.id);
        let store = Store::open_in_memory().unwrap();
        let cands = candidates(&store, &home, &state).unwrap();
        assert!(!cands.iter().any(|c| c.project.contains(FREE_DIR)));
        assert!(review::reviewers(&state, &first).is_empty());

        // Free bots stay strictly read-only: grants, commands, approvals and copy-edits are all refused.
        assert!(set_grants(&state, &first.id, &json!({ "full_access": true })).is_err());
        assert!(allow(&state, &first.id, "ls", true).is_err());
        assert!(approve(&state, &home, &jobs, &first.id, MAIN, None).is_err());
        assert!(ask_in_copy(&state, &home, &jobs, &first.id, MAIN, "edit", "edit").is_err());
        assert!(run_command(&state, &jobs, &first.id, MAIN, None, 0, false).is_err());

        // Per-conversation model override works cleanly for free conversation threads.
        let thread = threads::create_with_limit(&state, &first.id, DEFAULT_THREAD_TITLE, MAX_FREE_THREADS).unwrap();
        set_thread_model(&state, &first.id, &thread.id, Some("haiku"), None).unwrap();
        assert_eq!(threads::model(&state, &first.id, &thread.id).as_deref(), Some("haiku"));

        // Free guide never mentions a project folder or edit proposals.
        let guide = bot_run::free_guide("");
        assert!(guide.contains("no project attached"));
        assert!(!guide.contains("RAIOX-PROPOSAL"));
        assert!(!guide.contains("RAIOX-RUN"));

        let _ = fs::remove_dir_all(&home);
    }
}
