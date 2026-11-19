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

mod review;
mod turn;
use turn::Turn;

/// Messages that come between an answer and the person's next step and leave the answer
/// current: its proposal can still be approved and its commands still run.
const ASIDE: [&str; 7] = ["failed", "sent", "stopped", "house", "remembered", "review", "review_asked"];

/// What a new conversation is called until its first question names it.
pub const DEFAULT_THREAD_TITLE: &str = "Conversa";

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

/// An empty model means the CLI's default; anything else is letters, digits, dots and dashes.
pub fn check_model(model: Option<&str>) -> Result<Option<String>> {
    let model = model.map(str::trim).filter(|m| !m.is_empty());
    if let Some(m) = model
        && (m.len() > MAX_MODEL || !m.bytes().all(|b| b.is_ascii_alphanumeric() || b"-._".contains(&b)))
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
        if !slug(&self.preset) || !slug(&self.shape) || !color || !FINISHES.contains(&self.finish.as_str()) {
            bail!("invalid preset, shape, color or finish");
        }
        Ok(())
    }

    /// What the CLI is told about its role here.
    fn prompt(&self) -> String {
        format!("Your name in this project is {}. {}", self.name.trim(), self.instructions.trim())
    }
}

/// Bots answering right now, by id: whether the person asked them to stop, and in which conversation.
#[derive(Clone, Default)]
pub struct Jobs(Arc<Mutex<HashMap<String, Job>>>);

#[derive(Default)]
struct Job {
    stop: bool,
    thread: String,
    step: Option<crate::bot_steps::Step>,
}

impl Jobs {
    /// One turn at a time per bot, whichever conversation it is in.
    fn start(&self, id: &str, thread: &str) -> bool {
        self.0.lock().is_ok_and(|mut map| map.insert(id.to_string(), Job { thread: thread.to_string(), ..Job::default() }).is_none())
    }

    fn thread_of(&self, id: &str) -> Option<String> {
        self.0.lock().ok().and_then(|map| map.get(id).map(|job| job.thread.clone()))
    }

    fn step_of(&self, id: &str) -> Option<crate::bot_steps::Step> {
        self.0.lock().ok().and_then(|map| map.get(id).and_then(|job| job.step.clone()))
    }

    fn set_step(&self, id: &str, step: crate::bot_steps::Step) {
        if let Ok(mut map) = self.0.lock()
            && let Some(job) = map.get_mut(id)
        {
            job.step = Some(step);
        }
    }

    fn finish(&self, id: &str) {
        if let Ok(mut map) = self.0.lock() {
            map.remove(id);
        }
    }

    pub fn is_busy(&self, id: &str) -> bool {
        self.0.lock().is_ok_and(|map| map.contains_key(id))
    }

    fn should_stop(&self, id: &str) -> bool {
        self.0.lock().is_ok_and(|map| map.get(id).is_some_and(|job| job.stop))
    }

    /// Marks a running turn to stop; its thread kills the CLI on its next check.
    pub fn stop(&self, id: &str) -> Result<()> {
        let mut map = self.0.lock().map_err(|_| anyhow::anyhow!("jobs lock poisoned"))?;
        match map.get_mut(id) {
            Some(job) => {
                job.stop = true;
                Ok(())
            }
            None => bail!("the bot is not answering"),
        }
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

pub fn find(state_dir: &Path, id: &str) -> Result<Bot> {
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
            crate::connections::bot_agents().any(|a| a == agent) && !project.starts_with('.') && home.join(project).is_dir()
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
/// Turning on an existing bot again only replaces its persona.
pub fn activate(state_dir: &Path, home: &Path, project: &str, agent: &str, persona: Option<Persona>) -> Result<Bot> {
    if let Some(p) = &persona {
        p.validate()?;
    }
    if !crate::connections::bot_agents().any(|a| a == agent) {
        bail!("raiox cannot run {agent} as a bot");
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
        persona,
        model: None,
        effort: None,
        created_ms: room::now_ms(),
        allowed: Vec::new(),
        talk_model: None,
        reviewer: None,
    };
    let added = bot.clone();
    update(state_dir, |bots| {
        Ok(if bots.iter().any(|b| b.id == added.id) {
            bots.into_iter().map(|b| if b.id == added.id { Bot { persona: added.persona.clone(), ..b } } else { b }).collect()
        } else {
            bots.into_iter().chain([added]).collect()
        })
    })?;
    Ok(bot)
}

pub fn deactivate(state_dir: &Path, jobs: &Jobs, id: &str) -> Result<()> {
    if jobs.is_busy(id) {
        bail!("the bot is answering; wait for it to finish");
    }
    update(state_dir, |bots| Ok(bots.into_iter().filter(|b| b.id != id).collect()))
}

/// Changes how a bot runs and looks. Empty model or effort means the CLI's default.
pub fn configure(state_dir: &Path, jobs: &Jobs, id: &str, model: Option<&str>, effort: Option<&str>, persona: Option<Persona>) -> Result<()> {
    if jobs.is_busy(id) {
        bail!("the bot is answering; wait for it to finish");
    }
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

/// Starts the conversation over: the next message opens a fresh session.
pub fn reset(state_dir: &Path, jobs: &Jobs, id: &str, thread: &str) -> Result<()> {
    if jobs.is_busy(id) {
        bail!("the bot is answering; wait for it to finish");
    }
    set_thread_session(state_dir, id, thread, None)?;
    room::append_messages(&threads::chat_file(state_dir, id, thread), vec![note("reset", json!({}))])
}

/// The CLI session a conversation continues.
fn thread_session(state_dir: &Path, bot: &Bot, thread: &str) -> Result<Option<String>> {
    if thread == MAIN { Ok(bot.session.clone()) } else { threads::session(state_dir, &bot.id, thread) }
}

fn set_thread_session(state_dir: &Path, id: &str, thread: &str, session: Option<String>) -> Result<()> {
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

fn note(kind: &str, data: Value) -> Message {
    Message { ms: room::now_ms(), author: "bot".into(), kind: kind.into(), data }
}

fn said(kind: &str, data: Value) -> Message {
    Message { ms: room::now_ms(), author: "you".into(), kind: kind.into(), data }
}

/// The person talks to a bot. The answer arrives later, on its own thread.
/// Asks with a prompt the person did not type (an ai-memory message); `shown` is what the chat displays.
pub fn ask_with(state_dir: &Path, jobs: &Jobs, id: &str, prompt: &str, shown: Value) -> Result<()> {
    start(state_dir, jobs, id, MAIN, Turn { mode: Mode::Talk, text: prompt.to_string(), asked: Some(said("ask", shown)), home: None, command: None })
}

pub fn ask(state_dir: &Path, jobs: &Jobs, id: &str, thread: &str, text: &str) -> Result<()> {
    let text = text.trim();
    if text.is_empty() || text.chars().count() > MAX_TEXT {
        bail!("a message has 1 to {MAX_TEXT} characters");
    }
    if thread != MAIN {
        threads::name_after(state_dir, id, thread, text, DEFAULT_THREAD_TITLE)?;
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
    start(state_dir, jobs, id, thread, Turn { mode: Mode::Talk, text: for_cli, asked: Some(said("ask", json!({ "text": text }))), home: None, command: None })
}

/// The person approves the proposal in the bot's latest answer: the AI makes it in a draft copy.
pub fn approve(state_dir: &Path, home: &Path, jobs: &Jobs, id: &str, thread: &str) -> Result<()> {
    let messages = room::read_messages(&threads::chat_file(state_dir, id, thread));
    // A failed or interrupted approval can be retried, and a message sent on stays out of the way.
    let last = messages.iter().rev().find(|m| m.kind != "approve" && !ASIDE.contains(&m.kind.as_str()));
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
    start(state_dir, jobs, id, thread, Turn { mode: Mode::Edit, text, asked, home: Some(home.to_path_buf()), command: None })
}

/// Most commands a bot may run without asking, per project.
const MAX_ALLOWED: usize = 40;

/// Runs a command the bot asked for in its latest answer, then lets it go on with the output.
/// With `always`, that exact command runs without asking from now on in this project.
pub fn run_command(state_dir: &Path, jobs: &Jobs, id: &str, thread: &str, index: usize, always: bool) -> Result<()> {
    if !threads::valid(thread) {
        bail!("invalid conversation");
    }
    if jobs.is_busy(id) {
        bail!("the bot is still answering");
    }
    let messages = room::read_messages(&threads::chat_file(state_dir, id, thread));
    let last = messages.iter().rev().find(|m| !ASIDE.contains(&m.kind.as_str()));
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
    start(state_dir, jobs, id, thread, Turn { mode: Mode::Talk, text: String::new(), asked: None, home: None, command: Some(command) })
}

/// Sets the model the bot talks on (empty: the same as for changes).
pub fn set_talk_model(state_dir: &Path, jobs: &Jobs, id: &str, model: Option<&str>) -> Result<()> {
    if jobs.is_busy(id) {
        bail!("the bot is answering; wait for it to finish");
    }
    find(state_dir, id)?;
    let talk_model = check_model(model)?;
    update(state_dir, |bots| Ok(bots.into_iter().map(|b| if b.id == id { Bot { talk_model: talk_model.clone(), ..b } } else { b }).collect()))
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
pub fn set_limit(state_dir: &Path, id: &str, tokens: u64) -> Result<()> {
    let bot = find(state_dir, id)?;
    crate::budget::set(state_dir, &bot.name, (tokens > 0).then_some(tokens))
}

/// Lets an exact command run without asking in this bot's project, or stops letting it.
pub fn allow(state_dir: &Path, id: &str, command: &str, on: bool) -> Result<()> {
    let bot = find(state_dir, id)?;
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
    if !threads::valid(thread) {
        bail!("invalid conversation");
    }
    let bot = find(state_dir, id)?;
    crate::budget::check(state_dir, &bot.name)?;
    let session = thread_session(state_dir, &bot, thread)?;
    if !jobs.start(id, thread) {
        bail!("the bot is still answering");
    }
    if let Some(asked) = &turn.asked
        && let Err(error) = room::append_messages(&threads::chat_file(state_dir, id, thread), vec![asked.clone()])
    {
        jobs.finish(id);
        return Err(error);
    }
    let (state_dir, jobs, thread) = (state_dir.to_path_buf(), jobs.clone(), thread.to_string());
    std::thread::spawn(move || {
        turn::run(&state_dir, &jobs, &bot, &thread, turn, session);
        jobs.finish(&bot.id);
        crate::team::refresh(&state_dir, vec![bot.dir.clone()]);
    });
    Ok(())
}

/// Everything the panel shows about bots.
pub fn state(store: &Store, home: &Path, state_dir: &Path, jobs: &Jobs) -> Result<Value> {
    let bots: Vec<Value> = list(state_dir)
        .into_iter()
        .map(|bot| {
            let messages = room::read_messages(&chat_file(state_dir, &bot.id));
            let side: Vec<Value> = threads::list(state_dir, &bot.id)
                .into_iter()
                .map(|t| json!({ "id": t.id, "title": t.title, "has_session": t.session.is_some(), "messages": room::read_messages(&threads::chat_file(state_dir, &bot.id, &t.id)) }))
                .collect();
            json!({
                "id": bot.id,
                "agent": bot.agent,
                "name": bot.name,
                "dir": bot.dir,
                "has_session": bot.session.is_some(),
                "persona": bot.persona,
                "model": bot.model,
                "effort": bot.effort,
                "allowed": bot.allowed,
                "week": crate::room::week_of(store, home, &bot.dir),
                "talk_model": bot.talk_model,
                "reviewer": bot.reviewer,
                "budget": { "limit": crate::budget::limit(state_dir, &bot.name), "used": crate::budget::used_today(state_dir, &bot.name), "choices": crate::budget::CHOICES },
                "busy": jobs.is_busy(&bot.id),
                "busy_thread": jobs.thread_of(&bot.id),
                "step": jobs.step_of(&bot.id),
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
    Ok(json!({
        "bots": bots,
        "candidates": candidates(store, home)?,
        "agents": agents,
        "links": crate::links::list(state_dir),
        "batches": crate::batches::list(state_dir),
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
    fn activation_needs_a_project_folder_under_the_home() {
        let home = temp_dir("home");
        fs::create_dir_all(home.join("Projects/app")).unwrap();
        let state = home.join(".state");
        assert!(activate(&state, &home, "", "claude", None).is_err());
        assert!(activate(&state, &home, "../", "claude", None).is_err());
        fs::create_dir_all(home.join(".claude/projects")).unwrap();
        assert!(activate(&state, &home, ".claude/projects", "claude", None).is_err());
        assert!(activate(&state, &home, "Projects/app", "gemini", None).is_err());
        let bot = activate(&state, &home, "Projects/app", "claude", None).unwrap();
        assert_eq!(bot.name, "app");
        activate(&state, &home, "Projects/app", "claude", None).unwrap();
        assert_eq!(list(&state).len(), 1);
    }

    #[test]
    fn approval_needs_a_proposal_in_the_latest_answer() {
        let state = temp_dir("approve");
        let jobs = Jobs::default();
        let answer = |proposal: Value| note("answer", json!({ "mode": "talk", "reply": { "text": "t", "proposal": proposal } }));
        room::append_messages(&chat_file(&state, "b1"), vec![answer(Value::Null)]).unwrap();
        assert!(approve(&state, Path::new("/home"), &jobs, "b1", MAIN).unwrap_err().to_string().contains("no proposal"));
        room::append_messages(&chat_file(&state, "b1"), vec![answer(json!("rename"))]).unwrap();
        // Keeping the answer and a review of it leave the proposal approvable.
        let aside = vec![said("remembered", json!({})), said("review_asked", json!({})), note("review", json!({ "reply": { "text": "ok" } }))];
        room::append_messages(&chat_file(&state, "b1"), aside).unwrap();
        // With a proposal it gets past the check and fails only because no bot b1 is registered.
        assert!(approve(&state, Path::new("/home"), &jobs, "b1", MAIN).unwrap_err().to_string().contains("no bot"));
    }
}
