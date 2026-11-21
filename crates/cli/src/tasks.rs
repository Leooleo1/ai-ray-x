//! The team's board: tasks for the bots, from the person or from the bots
//! themselves (a RAIOX-TASK line). Each task runs in its own conversation.
//! When the turn ends it is
//! done, or it waits for the person when the bot asks for something (a
//! proposal, a draft, a command or a question). With "pick tasks up" on, the scheduler
//! starts each idle bot's next task by itself. Each card shows what its task
//! cost in tokens.

use std::{collections::HashMap, fs};
use std::path::Path;
use std::sync::Mutex;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::bots::{self, Jobs};
use crate::progress::{self, Milestone};
use crate::{room, threads};

const FILE: &str = "tarefas.json";
const THREAD_TITLE: &str = "Tasks";
/// The title tasks' conversations had before (still found).
const OLD_TITLE: &str = "Tarefas";
const MAX_TITLE: usize = 120;
const MAX_DETAIL: usize = 2000;
const MAX_TASKS: usize = 300;
static BOARD: Mutex<()> = Mutex::new(());
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
    /// The dedicated conversation, once the task has started.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread: Option<String>,
    /// When its turn started, and the answer it got (to open it, and to count its tokens).
    #[serde(default)]
    pub started_ms: Option<i64>,
    #[serde(default)]
    pub answer_ms: Option<i64>,
    #[serde(default)]
    pub tokens: u64,
    /// Done in a copy of the project of its own, so several run at once without touching each
    /// other or the project; what it changed comes back as a draft to apply.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub isolated: bool,
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

/// Tasks that finished after a moment, oldest first: (bot, title, when it finished).
pub fn finished_since(state_dir: &Path, after_ms: i64) -> Vec<(String, String, i64)> {
    let mut done: Vec<(String, String, i64)> = load(state_dir)
        .tasks
        .into_iter()
        .filter(|task| task.status == Status::Done && task.updated_ms > after_ms)
        .map(|task| (task.bot, task.title, task.updated_ms))
        .collect();
    done.sort_by_key(|(_, _, at)| *at);
    done
}

fn save(state_dir: &Path, board: &Board) -> Result<()> {
    fs::create_dir_all(state_dir)?;
    let file = state_dir.join(FILE);
    let temp = file.with_extension("tmp");
    fs::write(&temp, serde_json::to_string_pretty(board)?)?;
    fs::rename(temp, file)?;
    Ok(())
}

fn change(state_dir: &Path, edit: impl FnOnce(&mut Board) -> Result<()>) -> Result<()> {
    let _guard = BOARD.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut board = load(state_dir);
    edit(&mut board)?;
    save(state_dir, &board)
}

fn clean(text: &str, max: usize) -> String {
    text.trim().chars().filter(|c| !c.is_control() || *c == '\n').take(max).collect()
}

/// A task for a bot; gives its id.
pub fn add(state_dir: &Path, bot: &str, title: &str, detail: &str, from: &str) -> Result<String> {
    add_with(state_dir, bot, title, detail, from, false)
}

/// A task, done in a copy of its own when `isolated`.
pub fn add_with(state_dir: &Path, bot: &str, title: &str, detail: &str, from: &str, isolated: bool) -> Result<String> {
    bots::find(state_dir, bot)?;
    let title = clean(title, MAX_TITLE);
    if title.is_empty() {
        bail!("a task needs a title");
    }
    let id = {
        let _guard = BOARD.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut board = load(state_dir);
        if board.tasks.len() >= MAX_TASKS {
            bail!("the board holds at most {MAX_TASKS} tasks; delete done ones");
        }
        let now = room::now_ms();
        let conversations = threads::list(state_dir, bot);
        let mut suffix = board.tasks.len();
        let mut id = format!("k{now:x}{suffix}");
        while board.tasks.iter().any(|task| task.id == id) || conversations.iter().any(|thread| thread.id == id) {
            suffix += 1;
            id = format!("k{now:x}{suffix}");
        }
        board.tasks.push(Task {
            id: id.clone(),
            title,
            detail: clean(detail, MAX_DETAIL),
            bot: bot.to_string(),
            status: Status::Todo,
            from: from.to_string(),
            created_ms: now,
            updated_ms: now,
            thread: None,
            started_ms: None,
            answer_ms: None,
            tokens: 0,
            isolated,
        });
        save(state_dir, &board)?;
        id
    };
    let milestones: &[Milestone] = if from == "person" { &[Milestone::Desk, Milestone::Task] } else { &[Milestone::Desk] };
    progress::record(state_dir, milestones);
    Ok(id)
}

/// Tasks already run on older versions keep pointing to the shared conversation until run again.
fn task_thread<'a>(task: &Task, threads: &'a [threads::Thread]) -> Option<&'a str> {
    if let Some(id) = &task.thread {
        return threads.iter().find(|thread| &thread.id == id).map(|thread| thread.id.as_str());
    }
    if task.started_ms.is_none() && task.answer_ms.is_none() {
        return None;
    }
    threads.iter().find(|t| !t.task && (t.title == THREAD_TITLE || t.title == OLD_TITLE)).map(|thread| thread.id.as_str())
}

fn prompt(task: &Task) -> String {
    let detail = if task.detail.is_empty() { String::new() } else { format!("\n\n{}", task.detail) };
    if task.isolated {
        return format!(
            "Task from the team's board: {}{detail}\n\nDo it now, here: this is a copy of the project of your own, and \
             what you change comes back to the person as a draft to apply. Make the whole change and check it; do not \
             commit, push or deploy.",
            task.title
        );
    }
    format!("Task from the team's board: {}{detail}\n\nDo it now. If it needs a change to the project, propose it as usual.", task.title)
}

/// Starts a task in its own conversation, with the same history on a retry.
pub fn start(state_dir: &Path, jobs: &Jobs, id: &str) -> Result<()> {
    start_with(state_dir, jobs, id, |task, thread| {
        if task.isolated {
            let home = dirs::home_dir().context("home folder not found")?;
            // A retry uses a new copy; its CLI session cannot point to the old copy.
            threads::set_session(state_dir, &task.bot, thread, None)?;
            bots::ask_in_copy(state_dir, &home, jobs, &task.bot, thread, &prompt(task), &task.title)
        } else {
            bots::ask(state_dir, jobs, &task.bot, thread, &prompt(task), &[])
        }
    })
}

fn start_with(state_dir: &Path, jobs: &Jobs, id: &str, launch: impl FnOnce(&Task, &str) -> Result<()>) -> Result<()> {
    let _guard = BOARD.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut board = load(state_dir);
    let index = board.tasks.iter().position(|task| task.id == id).context("no such task")?;
    let previous = board.tasks[index].clone();
    if !matches!(previous.status, Status::Todo | Status::Waiting) {
        bail!("this task is not waiting to start");
    }
    let thread = previous.thread.as_deref().unwrap_or(&previous.id);
    if jobs.is_thread_busy(&previous.bot, thread) {
        bail!("this conversation is still answering");
    }
    let created = threads::create_for_task(state_dir, &previous.bot, thread, &previous.title)?;
    let started = room::now_ms();
    let task = &mut board.tasks[index];
    task.thread = Some(thread.to_string());
    task.status = Status::Doing;
    task.started_ms = Some(started);
    task.updated_ms = started;
    task.answer_ms = None;
    task.tokens = 0;
    let remove_empty_thread = || {
        if created && !threads::chat_file(state_dir, &previous.bot, thread).exists() {
            let _ = threads::delete(state_dir, &previous.bot, thread);
        }
    };
    if let Err(error) = save(state_dir, &board) {
        remove_empty_thread();
        return Err(error);
    }
    if let Err(error) = launch(&board.tasks[index], thread) {
        remove_empty_thread();
        board.tasks[index] = previous;
        save(state_dir, &board)?;
        return Err(error);
    }
    Ok(())
}

/// Where a finished turn leaves its task: done, or waiting when the bot asks for something.
fn outcome(answer: &room::Message) -> Status {
    let reply = answer.data.get("reply");
    let asks = |key: &str| reply.and_then(|r| r.get(key)).is_some_and(|v| !v.is_null() && v.as_array().is_none_or(|a| !a.is_empty()));
    match answer.kind.as_str() {
        "answer" if !asks("proposal") && !asks("run") && !asks("question") => Status::Done,
        _ => Status::Waiting,
    }
}

/// A task done in its own copy: its draft waits for the person; a turn that changed nothing is done.
fn copy_outcome(ended: &room::Message) -> Status {
    match ended.kind.as_str() {
        "draft" if ended.data["draft"].is_null() => Status::Done,
        "answer" => outcome(ended),
        _ => Status::Waiting,
    }
}

/// Whether the draft an isolated task left was applied: then the task is done.
fn draft_applied(messages: &[room::Message], answer_ms: i64) -> bool {
    let Some(draft) = messages.iter().find(|m| m.ms == answer_ms && m.kind == "draft").and_then(|m| m.data["draft"].as_str()) else { return false };
    messages.iter().any(|m| m.kind == "draft_applied" && m.data["draft"].as_str() == Some(draft))
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

fn sync_turn(state_dir: &Path, task: &Task) -> Result<()> {
    let conversations = threads::list(state_dir, &task.bot);
    let messages = task_thread(task, &conversations)
        .map(|thread| room::read_messages(&threads::chat_file(state_dir, &task.bot, thread)))
        .unwrap_or_default();
    let since = task.started_ms.unwrap_or(0);
    let ended = messages.iter().rev().find(|message| {
        message.author == "bot" && message.ms >= since
            && matches!(message.kind.as_str(), "answer" | "draft" | "failed" | "stopped" | "interrupted")
    });
    let (status, answer, tokens) = if task.status == Status::Waiting && task.thread.is_none() {
        // The shared history can contain later work on a different card.
        if !task.isolated || !task.answer_ms.is_some_and(|at| draft_applied(&messages, at)) {
            return Ok(());
        }
        (Status::Done, task.answer_ms, task.tokens)
    } else if let Some(ended) = ended {
        let status = if draft_applied(&messages, ended.ms) {
            Status::Done
        } else if task.isolated {
            copy_outcome(ended)
        } else {
            outcome(ended)
        };
        (status, Some(ended.ms), spent(&messages, since))
    } else if task.thread.is_some() && task.status == Status::Doing {
        // The app may have stopped between saving the start and launching the turn.
        (Status::Waiting, None, 0)
    } else {
        return Ok(());
    };
    if (task.status, task.answer_ms, task.tokens) == (status, answer, tokens) {
        return Ok(());
    }
    change(state_dir, |board| {
        if let Some(current) = board.tasks.iter_mut().find(|item| item.id == task.id)
            && (current.status, current.started_ms, current.answer_ms) == (task.status, task.started_ms, task.answer_ms)
        {
            current.status = status;
            current.answer_ms = answer;
            current.tokens = tokens;
            current.updated_ms = room::now_ms();
        }
        Ok(())
    })
}

/// From the scheduler: tasks whose turn ended move on, and with "pick up" on, idle bots start their next one.
pub fn tick(state_dir: &Path, jobs: &Jobs) {
    // Do not read a partially started task while start_with holds this lock.
    let board = {
        let _guard = BOARD.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        load(state_dir)
    };
    for task in board.tasks.iter().filter(|t| matches!(t.status, Status::Doing | Status::Waiting)) {
        let conversations = threads::list(state_dir, &task.bot);
        let task_running = task_thread(task, &conversations).is_some_and(|thread| jobs.is_thread_busy(&task.bot, thread));
        if !task_running {
            let _ = sync_turn(state_dir, task);
        }
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

pub(crate) fn linked_threads(state_dir: &Path) -> HashMap<String, String> {
    load(state_dir).tasks.into_iter().filter_map(|task| task.thread.map(|thread| (thread, task.bot))).collect()
}

/// The board keeps its conversation until the person removes the task card.
pub(crate) fn close_thread(state_dir: &Path, bot: &str, thread: &str) -> Result<()> {
    let _guard = BOARD.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if linked_threads(state_dir).get(thread).is_some_and(|owner| owner == bot) {
        bail!("this conversation belongs to a task on the board");
    }
    threads::delete(state_dir, bot, thread)
}

/// The board for the panel: the tasks, newest first, and where each task's conversation is.
pub fn state(state_dir: &Path) -> Value {
    let board = load(state_dir);
    let mut tasks = board.tasks;
    tasks.sort_by_key(|t| std::cmp::Reverse(t.updated_ms));
    let mut conversations = HashMap::new();
    for task in &tasks {
        conversations.entry(task.bot.as_str()).or_insert_with(|| threads::list(state_dir, &task.bot));
    }
    let threads: Vec<Option<&str>> = tasks.iter().map(|task| task_thread(task, &conversations[task.bot.as_str()])).collect();
    json!({ "tasks": tasks, "threads": threads, "auto": board.auto, "window": board.window })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> (std::path::PathBuf, std::path::PathBuf, String) {
        let home = std::env::temp_dir().join(format!("raiox-task-{name}-{}", std::process::id()));
        std::fs::create_dir_all(home.join("Projects/app")).unwrap();
        let state = home.join(".state");
        let owner = bots::activate(&state, &home, "Projects/app", "claude", None).unwrap().id;
        (home, state, owner)
    }

    fn message(kind: &str, reply: Value) -> room::Message {
        room::Message { ms: 10, author: "bot".into(), kind: kind.into(), data: json!({ "reply": reply }) }
    }

    #[test]
    fn task_cards_and_deliveries_open_their_own_conversations_for_one_bot() {
        let (home, state_dir, bot) = fixture("conversations");
        let jobs = Jobs::default();
        let first = add(&state_dir, &bot, "Fix checkout", "", "person").unwrap();
        let second = add(&state_dir, &bot, "Review tests", "", "person").unwrap();
        for (id, text, tokens) in [(&first, "Checkout changed", 5), (&second, "Tests reviewed", 9)] {
            start_with(&state_dir, &jobs, id, |task, thread| {
                let mut answer = message("answer", json!({ "text": text, "read_tokens": tokens }));
                answer.ms = task.started_ms.unwrap() + 1;
                room::append_messages(&threads::chat_file(&state_dir, &bot, thread), vec![answer])
            }).unwrap();
            tick(&state_dir, &jobs);
        }
        let panel = state(&state_dir);
        for (index, task) in panel["tasks"].as_array().unwrap().iter().enumerate() {
            assert_eq!(panel["threads"][index], task["id"]);
            assert_eq!(task["status"], "done");
        }
        assert_ne!(first, second);
        let deliveries = crate::deliveries::list(&state_dir);
        assert!(deliveries.iter().any(|item| item.kind == "task" && item.thread == first && item.tokens == 5));
        assert!(deliveries.iter().any(|item| item.kind == "task" && item.thread == second && item.tokens == 9));
        assert!(close_thread(&state_dir, &bot, &first).is_err());
        mark(&state_dir, &first, "reopen").unwrap();
        start_with(&state_dir, &jobs, &first, |task, thread| {
            assert_eq!(thread, first);
            assert_eq!(task.answer_ms, None);
            assert_eq!(room::read_messages(&threads::chat_file(&state_dir, &bot, thread))[0].data["reply"]["text"], "Checkout changed");
            Ok(())
        }).unwrap();
        mark(&state_dir, &first, "done").unwrap();
        mark(&state_dir, &first, "delete").unwrap();
        assert!(threads::list(&state_dir, &bot).iter().any(|thread| thread.id == first));
        close_thread(&state_dir, &bot, &first).unwrap();
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn a_legacy_card_keeps_its_shared_history_if_starting_a_new_turn_fails() {
        let (home, state_dir, bot) = fixture("legacy");
        let legacy = threads::create(&state_dir, &bot, OLD_TITLE).unwrap();
        let task = add(&state_dir, &bot, "Review totals", "", "person").unwrap();
        change(&state_dir, |board| {
            board.tasks[0].status = Status::Waiting;
            board.tasks[0].started_ms = Some(5);
            board.tasks[0].answer_ms = Some(10);
            Ok(())
        }).unwrap();
        let jobs = Jobs::default();
        assert_eq!(state(&state_dir)["threads"][0], legacy.id);
        assert!(start_with(&state_dir, &jobs, &task, |_, _| anyhow::bail!("could not launch")).is_err());
        assert_eq!(state(&state_dir)["threads"][0], legacy.id);
        assert_eq!(threads::list(&state_dir, &bot).len(), 1);
        start_with(&state_dir, &jobs, &task, |_, _| Ok(())).unwrap();
        assert_eq!(state(&state_dir)["threads"][0], task);
        assert_eq!(threads::list(&state_dir, &bot).len(), 2);
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn a_draft_applied_after_a_task_question_finishes_the_same_card() {
        let (home, state_dir, bot) = fixture("follow-up");
        let task_id = add(&state_dir, &bot, "Fix totals", "", "person").unwrap();
        let jobs = Jobs::default();
        start_with(&state_dir, &jobs, &task_id, |task, thread| {
            let mut question = message("answer", json!({ "question": "Which totals?" }));
            question.ms = task.started_ms.unwrap() + 1;
            room::append_messages(&threads::chat_file(&state_dir, &bot, thread), vec![question])
        }).unwrap();
        tick(&state_dir, &jobs);
        let waiting = load(&state_dir).tasks.remove(0);
        assert_eq!(waiting.status, Status::Waiting);
        let file = threads::chat_file(&state_dir, &bot, &task_id);
        let draft_ms = waiting.answer_ms.unwrap() + 1;
        let draft = room::Message { ms: draft_ms, author: "bot".into(), kind: "draft".into(), data: json!({ "draft": "d1" }) };
        room::append_messages(&file, vec![draft]).unwrap();
        tick(&state_dir, &jobs);
        assert_eq!(load(&state_dir).tasks[0].answer_ms, Some(draft_ms));
        let applied = room::Message { ms: draft_ms + 1, author: "you".into(), kind: "draft_applied".into(), data: json!({ "draft": "d1" }) };
        room::append_messages(&file, vec![applied]).unwrap();
        tick(&state_dir, &jobs);
        assert_eq!(load(&state_dir).tasks[0].status, Status::Done);
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn a_task_in_its_own_copy_waits_on_its_draft_and_ends_when_applied() {
        let draft = |id: Value| room::Message { ms: 10, author: "bot".into(), kind: "draft".into(), data: json!({ "draft": id }) };
        assert_eq!(copy_outcome(&draft(json!("d1"))), Status::Waiting, "a draft waits for the person");
        assert_eq!(copy_outcome(&draft(Value::Null)), Status::Done, "nothing changed");
        let applied = room::Message { ms: 20, author: "you".into(), kind: "draft_applied".into(), data: json!({ "draft": "d1" }) };
        assert!(!draft_applied(&[draft(json!("d1"))], 10));
        assert!(draft_applied(&[draft(json!("d1")), applied], 10));
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
        assert_eq!(outcome(&message("answer", json!({ "question": "Which version?" }))), Status::Waiting);
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
