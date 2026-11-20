//! Entregas: what the bots produced, in one place, newest first. Changes they
//! made to a project (approved edits, applied drafts marked when undone, and the
//! files an AI with full access edited in a conversation),
//! screenshots, routine results and finished tasks. Read from the
//! conversations as they are; nothing new is stored.

use std::path::Path;

use serde::Serialize;
use serde_json::Value;

use crate::room::{self, Message};
use crate::{bots, threads};

/// Deliveries the page lists at most.
const MAX_ITEMS: usize = 200;
/// An answer's opening reaches at least this many characters before it names a change.
const OPENING_MIN: usize = 40;
/// What a routine's result shows of its answer.
const EXCERPT: usize = 240;
/// Routine conversations' titles, now and before.
const ROUTINE_PREFIXES: [&str; 2] = ["Routine · ", "Rotina · "];

#[derive(Serialize, Debug, PartialEq)]
pub struct Delivery {
    /// "change", "draft", "shot", "routine", "audit" or "task".
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

/// An answer's opening, to name what it changed: its first paragraph, or the first few when they are
/// short (a greeting alone says nothing), on one line.
fn opening(text: &str) -> String {
    let mut said = String::new();
    for paragraph in text.split("\n\n").map(str::trim).filter(|p| !p.is_empty()) {
        if said.chars().count() >= OPENING_MIN {
            break;
        }
        said = format!("{said} {}", paragraph.split_whitespace().collect::<Vec<_>>().join(" ")).trim().to_string();
    }
    said
}

/// The deliveries of one conversation.
fn of_conversation(bot: &str, thread: &str, title: &str, messages: &[Message]) -> Vec<Delivery> {
    let base = |kind, ms, title: String| Delivery { kind, bot: bot.to_string(), thread: thread.to_string(), ms, title, files: Vec::new(), undone: false, detail: String::new(), tokens: 0 };
    let undone: Vec<&str> = messages.iter().filter(|m| m.kind == "draft_undone").filter_map(|m| m.data["draft"].as_str()).collect();
    let routine = ROUTINE_PREFIXES.iter().find_map(|prefix| title.strip_prefix(prefix));
    let is_audit_title = title == crate::auditor::THREAD_TITLE;
    messages
        .iter()
        .filter_map(|m| {
            let reply = &m.data["reply"];
            let text = reply["text"].as_str().unwrap_or("");
            let tokens = reply["read_tokens"].as_u64().unwrap_or(0) + reply["output_tokens"].as_u64().unwrap_or(0);
            let has_run = reply["run"].as_array().is_some_and(|runs| !runs.is_empty());
            match (m.author.as_str(), m.kind.as_str()) {
                ("bot", "answer") if m.data["mode"] == "edit" => {
                    let title = reply["proposal"].as_str().filter(|p| !p.is_empty()).unwrap_or(text);
                    Some(Delivery { files: edited(reply), ..base("change", m.ms, cut(title, 140)) })
                }
                ("bot", "answer") if (is_audit_title || m.data["audit"].as_bool() == Some(true)) && !has_run => {
                    let headline = reply["proposal"]
                        .as_str()
                        .filter(|p| !p.is_empty())
                        .unwrap_or(crate::auditor::THREAD_TITLE);
                    Some(Delivery { detail: cut(text, EXCERPT), tokens, ..base("audit", m.ms, cut(headline, 140)) })
                }
                ("bot", "answer") if routine.is_some() => Some(Delivery { detail: cut(text, EXCERPT), ..base("routine", m.ms, routine.unwrap_or("").to_string()) }),
                // An AI with full access edits in the conversation itself: what it changed is a delivery,
                // named by the start of its answer.
                ("bot", "answer") if !edited(reply).is_empty() => Some(Delivery { files: edited(reply), tokens, ..base("change", m.ms, cut(&opening(text), 140)) }),
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
        let renamed = of_conversation("b", "t1", "Routine · status", &messages[..1]);
        assert_eq!((renamed[0].kind, renamed[0].title.as_str()), ("routine", "status"), "old and new titles both count");
        let audit_msgs = vec![
            message(10, "bot", "answer", json!({ "mode": "talk", "audit": true, "reply": { "text": "checking", "run": [{ "command": "npm test" }] } })),
            message(11, "bot", "answer", json!({ "mode": "talk", "audit": true, "reply": { "text": "Found 1 broken flag", "proposal": "Fix test script", "read_tokens": 300, "output_tokens": 50 } })),
        ];
        let audit = of_conversation("b", "t2", crate::auditor::THREAD_TITLE, &audit_msgs);
        assert_eq!(audit.len(), 1, "intermediate run steps stay out of audit deliveries");
        assert_eq!((audit[0].kind, audit[0].title.as_str(), audit[0].tokens), ("audit", "Fix test script", 350));
    }

    #[test]
    fn an_ai_with_full_access_delivers_the_files_it_edited_in_the_conversation() {
        let messages = vec![
            message(1, "bot", "answer", json!({ "mode": "talk", "reply": { "text": "Li o projeto.", "trail": { "items": [{ "kind": "read", "target": "a.js" }] } } })),
            message(2, "bot", "answer", json!({ "mode": "talk", "reply": { "text": "Troquei o título e fiz o commit.\n\nDetalhes...", "read_tokens": 900, "output_tokens": 100, "trail": { "items": [
                { "kind": "edit", "target": "titulo.js" }, { "kind": "run", "target": "npm test" }, { "kind": "edit", "target": "titulo.test.js" }
            ] } } })),
        ];
        let found = of_conversation("b", "main", "", &messages);
        assert_eq!(found.len(), 1, "an answer that only read is not a delivery");
        assert_eq!(found[0].kind, "change");
        assert_eq!(found[0].files, vec!["titulo.js".to_string(), "titulo.test.js".to_string()]);
        assert_eq!((found[0].title.as_str(), found[0].tokens), ("Troquei o título e fiz o commit. Detalhes...", 1000), "the start of its answer, and what it cost");
        let long = vec![message(3, "bot", "answer", json!({ "mode": "talk", "reply": { "text": "Fixed. The cart now adds prices in cents and formats only at the end.\n\n- `src/cart/total.ts`: sums cents.\n- tests pass.", "trail": { "items": [{ "kind": "edit", "target": "src/cart/total.ts" }] } } }))];
        assert_eq!(of_conversation("b", "main", "", &long)[0].title, "Fixed. The cart now adds prices in cents and formats only at the end.", "its first paragraph, not the list under it");
    }
}
