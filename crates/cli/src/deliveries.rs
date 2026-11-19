//! Entregas: what the bots produced, in one place, newest first. Changes they
//! made to a project (approved edits and applied drafts, marked when undone),
//! screenshots, routine results and finished tasks. Read from the
//! conversations as they are; nothing new is stored.

use std::path::Path;

use serde::Serialize;
use serde_json::Value;

use crate::room::{self, Message};
use crate::{bots, threads};

/// Deliveries the page lists at most.
const MAX_ITEMS: usize = 200;
/// What a routine's result shows of its answer.
const EXCERPT: usize = 240;
const ROUTINE_PREFIX: &str = "Rotina · ";

#[derive(Serialize, Debug, PartialEq)]
pub struct Delivery {
    /// "change", "draft", "shot", "routine" or "task".
    pub kind: &'static str,
    pub bot: String,
    pub thread: String,
    /// The message to open.
    pub ms: i64,
    pub title: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,
    /// A draft applied and later undone.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub undone: bool,
    /// A screenshot's file name, or a routine's answer.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub detail: String,
    #[serde(skip_serializing_if = "is_zero")]
    pub tokens: u64,
}

fn is_zero(n: &u64) -> bool {
    *n == 0
}

fn cut(text: &str, max: usize) -> String {
    let line = text.trim().replace('`', "");
    if line.chars().count() <= max {
        return line;
    }
    format!("{}…", line.chars().take(max).collect::<String>().trim_end())
}

fn strings(value: &Value) -> Vec<String> {
    value.as_array().map(|items| items.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default()
}

/// The files an approved turn edited, from its path.
fn edited(reply: &Value) -> Vec<String> {
    let mut files: Vec<String> = Vec::new();
    for item in reply.pointer("/trail/items").and_then(Value::as_array).into_iter().flatten() {
        let target = item["target"].as_str().unwrap_or("");
        if item["kind"] == "edit" && !target.is_empty() && !files.iter().any(|f| f == target) {
            files.push(target.to_string());
        }
    }
    files
}

/// The deliveries of one conversation.
fn of_conversation(bot: &str, thread: &str, title: &str, messages: &[Message]) -> Vec<Delivery> {
    let base = |kind, ms, title: String| Delivery { kind, bot: bot.to_string(), thread: thread.to_string(), ms, title, files: Vec::new(), undone: false, detail: String::new(), tokens: 0 };
    let undone: Vec<&str> = messages.iter().filter(|m| m.kind == "draft_undone").filter_map(|m| m.data["draft"].as_str()).collect();
    let routine = title.strip_prefix(ROUTINE_PREFIX);
    messages
        .iter()
        .filter_map(|m| {
            let reply = &m.data["reply"];
            let text = reply["text"].as_str().unwrap_or("");
            match (m.author.as_str(), m.kind.as_str()) {
                ("bot", "answer") if m.data["mode"] == "edit" => {
                    let title = reply["proposal"].as_str().filter(|p| !p.is_empty()).unwrap_or(text);
                    Some(Delivery { files: edited(reply), ..base("change", m.ms, cut(title, 140)) })
                }
                ("bot", "answer") if routine.is_some() => Some(Delivery { detail: cut(text, EXCERPT), ..base("routine", m.ms, routine.unwrap_or("").to_string()) }),
                ("you", "draft_applied") => {
                    let draft = m.data["draft"].as_str().unwrap_or("");
                    // Older applies wrote "files", newer ones "paths".
                    let files = [strings(&m.data["paths"]), strings(&m.data["files"])].concat();
                    Some(Delivery { undone: undone.contains(&draft), ..base("draft", m.ms, files.join(", ")) }).map(|d| Delivery { files, ..d })
                }
                ("you", "shot") => Some(Delivery { detail: m.data["name"].as_str().unwrap_or("").to_string(), ..base("shot", m.ms, m.data["target"].as_str().unwrap_or("").to_string()) }),
                _ => None,
            }
        })
        .collect()
}

/// Every delivery, newest first.
pub fn list(state_dir: &Path) -> Vec<Delivery> {
    let mut all: Vec<Delivery> = Vec::new();
    for bot in bots::list(state_dir) {
        let main = room::read_messages(&threads::chat_file(state_dir, &bot.id, threads::MAIN));
        all.extend(of_conversation(&bot.id, threads::MAIN, "", &main));
        for thread in threads::list(state_dir, &bot.id) {
            let messages = room::read_messages(&threads::chat_file(state_dir, &bot.id, &thread.id));
            all.extend(of_conversation(&bot.id, &thread.id, &thread.title, &messages));
        }
    }
    let board = crate::tasks::state(state_dir);
    let tasks = board["tasks"].as_array().cloned().unwrap_or_default();
    let task_threads = board["threads"].as_array().cloned().unwrap_or_default();
    for (task, thread) in tasks.iter().zip(task_threads) {
        let (Some(ms), Some(thread)) = (task["answer_ms"].as_i64(), thread.as_str()) else { continue };
        if task["status"] != "done" {
            continue;
        }
        all.push(Delivery {
            kind: "task",
            bot: task["bot"].as_str().unwrap_or("").to_string(),
            thread: thread.to_string(),
            ms,
            title: cut(task["title"].as_str().unwrap_or(""), 140),
            files: Vec::new(),
            undone: false,
            detail: String::new(),
            tokens: task["tokens"].as_u64().unwrap_or(0),
        });
    }
    all.sort_by_key(|d| std::cmp::Reverse(d.ms));
    all.truncate(MAX_ITEMS);
    all
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn message(ms: i64, author: &str, kind: &str, data: Value) -> Message {
        Message { ms, author: author.into(), kind: kind.into(), data }
    }

    #[test]
    fn a_conversation_delivers_its_changes_drafts_and_screenshots() {
        let messages = vec![
            message(1, "bot", "answer", json!({ "mode": "talk", "reply": { "text": "só conversa" } })),
            message(2, "bot", "answer", json!({ "mode": "edit", "reply": { "text": "feito", "proposal": "Renomear `soma`", "trail": { "items": [
                { "kind": "read", "target": "a.js" }, { "kind": "edit", "target": "a.js" }, { "kind": "edit", "target": "a.js" }, { "kind": "edit", "target": "b.md" }
            ] } } })),
            message(3, "you", "draft_applied", json!({ "draft": "d1", "files": ["x.js"] })),
            message(4, "you", "draft_undone", json!({ "draft": "d1" })),
            message(5, "you", "shot", json!({ "target": "/login", "name": "1.png" })),
        ];
        let found = of_conversation("b", "main", "", &messages);
        assert_eq!(found.iter().map(|d| d.kind).collect::<Vec<_>>(), ["change", "draft", "shot"], "talk is not a delivery");
        assert_eq!((found[0].title.as_str(), found[0].files.clone()), ("Renomear soma", vec!["a.js".to_string(), "b.md".to_string()]));
        assert!(found[1].undone, "an undone draft says so");
        let routine = of_conversation("b", "t1", "Rotina · status", &messages[..1]);
        assert_eq!((routine[0].kind, routine[0].title.as_str()), ("routine", "status"));
    }
}
