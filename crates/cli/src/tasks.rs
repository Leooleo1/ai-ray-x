//! The team's board: tasks for the bots, from the person or from the bots
//! themselves (a RAIOX-TASK line). A task runs in its bot's "Tasks"
//! conversation, from a fresh session each time. When the turn ends it is
//! done, or it waits for the person when the bot asks for something (a
//! proposal, a draft, a command). With "pick tasks up" on, the scheduler
//! starts each idle bot's next task by itself. Each card shows what its task
//! cost in tokens.

use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::bots::{self, Jobs};
use crate::{room, threads};

const FILE: &str = "tarefas.json";
const THREAD_TITLE: &str = "Tasks";
/// The title tasks' conversations had before (still found).
const OLD_TITLE: &str = "Tarefas";
const MAX_TITLE: usize = 120;
const MAX_DETAIL: usize = 2000;
const MAX_TASKS: usize = 300;
/// Tasks one answer can leave on the board.
pub const MAX_FROM_ANSWER: usize = 3;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Todo,
    Doing,
    Waiting,
    Done,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Task {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub detail: String,
    /// The bot that does it.
    pub bot: String,
    pub status: Status,
    /// "person", or the id of the bot that left it on the board.
    pub from: String,
    pub created_ms: i64,
    pub updated_ms: i64,
    /// When its turn started, and the answer it got (to open it, and to count its tokens).
    #[serde(default)]
    pub started_ms: Option<i64>,
    #[serde(default)]
    pub answer_ms: Option<i64>,
    #[serde(default)]
    pub tokens: u64,
}

#[derive(Serialize, Deserialize, Default)]
struct Board {
    #[serde(default)]
    tasks: Vec<Task>,
    /// The bots pick their next task up by themselves.
    #[serde(default)]
    auto: bool,
    /// Only in these local hours (the person's free ones); none means at any hour.
    #[serde(default)]
    window: Option<Window>,
}

/// Local hours from `start` up to (not including) `end`; it may pass midnight (22 → 6).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Window {
    pub start: u8,
    pub end: u8,
}

impl Window {
    pub fn new(start: u64, end: u64) -> Result<Window> {
        if start > 23 || end > 23 || start == end {
            bail!("a window is two different hours from 0 to 23");
        }
        Ok(Window { start: start as u8, end: end as u8 })
    }

    pub fn holds(self, hour: u32) -> bool {
        let (start, end, hour) = (u32::from(self.start), u32::from(self.end), hour % 24);
        if start < end { (start..end).contains(&hour) } else { hour >= start || hour < end }
    }
}

fn load(state_dir: &Path) -> Board {
    fs::read_to_string(state_dir.join(FILE)).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
}

fn save(state_dir: &Path, board: &Board) -> Result<()> {
    fs::create_dir_all(state_dir)?;
    fs::write(state_dir.join(FILE), serde_json::to_string_pretty(board)?)?;
    Ok(())
}

fn change(state_dir: &Path, edit: impl FnOnce(&mut Board) -> Result<()>) -> Result<()> {
    let mut board = load(state_dir);
    edit(&mut board)?;
    save(state_dir, &board)
}

fn clean(text: &str, max: usize) -> String {
    text.trim().chars().filter(|c| !c.is_control() || *c == '\n').take(max).collect()
}

/// A task for a bot; gives its id.
pub fn add(state_dir: &Path, bot: &str, title: &str, detail: &str, from: &str) -> Result<String> {
    bots::find(state_dir, bot)?;
    let title = clean(title, MAX_TITLE);
    if title.is_empty() {
        bail!("a task needs a title");
    }
    let now = room::now_ms();
    let id = format!("k{now:x}{}", load(state_dir).tasks.len());
    let task = Task { id: id.clone(), title, detail: clean(detail, MAX_DETAIL), bot: bot.to_string(), status: Status::Todo, from: from.to_string(), created_ms: now, updated_ms: now, started_ms: None, answer_ms: None, tokens: 0 };
    change(state_dir, |board| {
        if board.tasks.len() >= MAX_TASKS {
            bail!("the board holds at most {MAX_TASKS} tasks; delete done ones");
        }
        board.tasks.push(task);
        Ok(())
    })?;
    Ok(id)
}

/// The bot's conversation for tasks (made when first needed).
fn task_thread(state_dir: &Path, bot: &str) -> Result<String> {
    if let Some(thread) = threads::list(state_dir, bot).into_iter().find(|t| t.title == THREAD_TITLE || t.title == OLD_TITLE) {
        return Ok(thread.id);
    }
    Ok(threads::create(state_dir, bot, THREAD_TITLE)?.id)
}

fn prompt(task: &Task) -> String {
    let detail = if task.detail.is_empty() { String::new() } else { format!("\n\n{}", task.detail) };
    format!("Task from the team's board: {}{detail}\n\nDo it now. If it needs a change to the project, propose it as usual.", task.title)
}

/// Starts a task: its bot gets it in its "Tasks" conversation, from a fresh session.
pub fn start(state_dir: &Path, jobs: &Jobs, id: &str) -> Result<()> {
    let task = load(state_dir).tasks.into_iter().find(|t| t.id == id).context("no such task")?;
    if !matches!(task.status, Status::Todo | Status::Waiting) {
        bail!("this task is not waiting to start");
    }
    let thread = task_thread(state_dir, &task.bot)?;
    // Each task starts clean: the last one's context would only cost tokens here.
    threads::set_session(state_dir, &task.bot, &thread, None)?;
    let started = room::now_ms();
    bots::ask(state_dir, jobs, &task.bot, &thread, &prompt(&task), &[])?;
    change(state_dir, |board| {
        if let Some(t) = board.tasks.iter_mut().find(|t| t.id == id) {
            t.status = Status::Doing;
            t.started_ms = Some(started);
            t.updated_ms = started;
        }
        Ok(())
    })
}

/// Where a finished turn leaves its task: done, or waiting when the bot asks for something.
fn outcome(answer: &room::Message) -> Status {
    let reply = answer.data.get("reply");
    let asks = |key: &str| reply.and_then(|r| r.get(key)).is_some_and(|v| !v.is_null() && v.as_array().is_none_or(|a| !a.is_empty()));
    match answer.kind.as_str() {
        "answer" if !asks("proposal") && !asks("run") => Status::Done,
        _ => Status::Waiting,
    }
}

/// Tokens of a turn: what its answers read and wrote since it started.
fn spent(messages: &[room::Message], since: i64) -> u64 {
    messages
        .iter()
        .filter(|m| m.author == "bot" && m.ms >= since)
        .map(|m| {
            let count = |key: &str| m.data.pointer(&format!("/reply/{key}")).and_then(Value::as_u64).unwrap_or(0);
            count("read_tokens") + count("output_tokens")
        })
        .sum()
}

/// From the scheduler: tasks whose turn ended move on, and with "pick up" on, idle bots start their next one.
pub fn tick(state_dir: &Path, jobs: &Jobs) {
    let board = load(state_dir);
    for task in board.tasks.iter().filter(|t| t.status == Status::Doing && !jobs.is_busy(&t.bot)) {
        let Ok(thread) = task_thread(state_dir, &task.bot) else { continue };
        let messages = room::read_messages(&threads::chat_file(state_dir, &task.bot, &thread));
        let since = task.started_ms.unwrap_or(0);
        let ended = messages.iter().rev().find(|m| m.author == "bot" && m.ms >= since && matches!(m.kind.as_str(), "answer" | "draft" | "failed" | "stopped"));
        let Some(ended) = ended else { continue };
        let (status, at, tokens) = (outcome(ended), ended.ms, spent(&messages, since));
        let _ = change(state_dir, |b| {
            if let Some(t) = b.tasks.iter_mut().find(|t| t.id == task.id) {
                t.status = status;
                t.answer_ms = Some(at);
                t.tokens = tokens;
                t.updated_ms = room::now_ms();
            }
            Ok(())
        });
    }
    if !board.auto || board.window.is_some_and(|w| !w.holds(chrono::Timelike::hour(&chrono::Local::now()))) {
        return;
    }
    let board = load(state_dir);
    let mut started: Vec<String> = Vec::new();
    for task in board.tasks.iter().filter(|t| t.status == Status::Todo) {
        let busy = jobs.is_busy(&task.bot) || board.tasks.iter().any(|t| t.bot == task.bot && t.status == Status::Doing);
        if busy || started.contains(&task.bot) {
            continue;
        }
        if start(state_dir, jobs, &task.id).is_ok() {
            started.push(task.bot.clone());
        }
    }
}

/// The person moves a task by hand: done, back to do, or gone.
pub fn mark(state_dir: &Path, id: &str, action: &str) -> Result<()> {
    change(state_dir, |board| {
        let index = board.tasks.iter().position(|t| t.id == id).context("no such task")?;
        match action {
            "done" => board.tasks[index].status = Status::Done,
            "reopen" => board.tasks[index].status = Status::Todo,
            "delete" => {
                board.tasks.remove(index);
                return Ok(());
            }
            other => bail!("unknown task action {other}"),
        }
        board.tasks[index].updated_ms = room::now_ms();
        Ok(())
    })
}

/// Turns the pickup on or off; with `window`, it also sets (or, with None, clears) the hours it keeps to.
pub fn set_auto(state_dir: &Path, on: bool, window: Option<Option<Window>>) -> Result<()> {
    change(state_dir, |board| {
        board.auto = on;
        if let Some(window) = window {
            board.window = window;
        }
        Ok(())
    })
}

/// A title as two tasks are compared: no code quotes, case or extra spaces.
fn same_title(title: &str) -> String {
    title.replace('`', "").split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

/// Tasks a bot's answer left on the board (its RAIOX-TASK lines), for itself; one it already has
/// waiting on the board is not added again (bots repeat a task in later answers).
pub fn from_answer(state_dir: &Path, bot: &str, titles: &[String]) {
    let open: Vec<String> = load(state_dir).tasks.iter().filter(|t| t.bot == bot && t.status != Status::Done).map(|t| same_title(&t.title)).collect();
    let mut seen = open;
    for title in titles.iter().take(MAX_FROM_ANSWER) {
        let key = same_title(title);
        if seen.contains(&key) {
            continue;
        }
        // A card title is plain text: no code quotes.
        if add(state_dir, bot, &title.replace('`', ""), "", bot).is_ok() {
            seen.push(key);
        }
    }
}

/// The free-hours window configured for automatic background runs, if any.
pub fn window(state_dir: &Path) -> Option<Window> {
    load(state_dir).window
}

/// The board for the panel: the tasks, newest first, and where each task's conversation is.
pub fn state(state_dir: &Path) -> Value {
    let board = load(state_dir);
    let mut tasks = board.tasks;
    tasks.sort_by_key(|t| std::cmp::Reverse(t.updated_ms));
    let threads: Vec<Value> = tasks
        .iter()
        .map(|t| threads::list(state_dir, &t.bot).into_iter().find(|th| th.title == THREAD_TITLE || th.title == OLD_TITLE).map_or(Value::Null, |th| json!(th.id)))
        .collect();
    json!({ "tasks": tasks, "threads": threads, "auto": board.auto, "window": board.window })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(kind: &str, reply: Value) -> room::Message {
        room::Message { ms: 10, author: "bot".into(), kind: kind.into(), data: json!({ "reply": reply }) }
    }

    #[test]
    fn a_window_holds_its_hours_even_past_midnight() {
        let night = Window::new(22, 6).unwrap();
        assert!(night.holds(23) && night.holds(0) && night.holds(5));
        assert!(!night.holds(6) && !night.holds(12) && !night.holds(21));
        let morning = Window::new(3, 9).unwrap();
        assert!(morning.holds(3) && morning.holds(8) && !morning.holds(9) && !morning.holds(2));
        assert!(Window::new(5, 5).is_err() && Window::new(24, 2).is_err());
    }

    #[test]
    fn a_bot_repeating_a_task_does_not_fill_the_board_twice() {
        let home = std::env::temp_dir().join(format!("raiox-tasks-dup-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join("Projects/app")).unwrap();
        let state = home.join(".state");
        let bot = bots::activate(&state, &home, "Projects/app", "claude", None).unwrap();
        let titles = |list: &[&str]| list.iter().map(|t| t.to_string()).collect::<Vec<_>>();

        from_answer(&state, &bot.id, &titles(&["Testar v003: format=`micro_thread`", "Testar  v003: FORMAT=micro_thread"]));
        from_answer(&state, &bot.id, &titles(&["testar v003: format=micro_thread"]));
        assert_eq!(load(&state).tasks.len(), 1, "the same task, in the same answer or a later one, is added once");

        let id = load(&state).tasks[0].id.clone();
        mark(&state, &id, "done").unwrap();
        from_answer(&state, &bot.id, &titles(&["Testar v003: format=micro_thread"]));
        assert_eq!(load(&state).tasks.len(), 2, "once done, the same work can be asked for again");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn a_turn_that_asks_for_something_waits_for_the_person() {
        assert_eq!(outcome(&message("answer", json!({ "text": "feito", "proposal": null, "run": [] }))), Status::Done);
        assert_eq!(outcome(&message("answer", json!({ "text": "x", "proposal": "mudar a.ts" }))), Status::Waiting);
        assert_eq!(outcome(&message("answer", json!({ "text": "x", "run": [{ "command": "npm test" }] }))), Status::Waiting);
        assert_eq!(outcome(&message("draft", json!({}))), Status::Waiting);
        assert_eq!(outcome(&message("failed", json!({}))), Status::Waiting);
    }

    #[test]
    fn a_task_costs_what_its_turn_read_and_wrote() {
        let messages = vec![
            room::Message { ms: 5, author: "bot".into(), kind: "answer".into(), data: json!({ "reply": { "read_tokens": 999, "output_tokens": 1 } }) },
            room::Message { ms: 20, author: "bot".into(), kind: "answer".into(), data: json!({ "reply": { "read_tokens": 100, "output_tokens": 20 } }) },
        ];
        assert_eq!(spent(&messages, 10), 120, "only what came after the task started");
    }
}
