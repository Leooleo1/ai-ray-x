//! "Edits without asking": for a bot with that grant, the person's two clicks
//! happen by themselves. A proposal in its answer is approved, and the draft
//! that edit leaves is applied to the project. The draft keeps its backup, so
//! each change can still be undone from the conversation.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde_json::Value;

use crate::bots::{self, Jobs};
use crate::{room, threads};

/// The home folder, for the draft copy an approval makes (set when the panel starts).
static HOME: OnceLock<PathBuf> = OnceLock::new();

pub fn set_home(home: &Path) {
    let _ = HOME.set(home.to_path_buf());
}

/// What a finished turn leads to by itself.
#[derive(Debug, PartialEq)]
enum Next {
    Approve,
    Apply(String),
    Nothing,
}

fn next(messages: &[room::Message]) -> Next {
    let Some(last) = messages.last().filter(|m| m.author == "bot") else { return Next::Nothing };
    let has_proposal = last.data.pointer("/reply/proposal").and_then(Value::as_str).is_some_and(|p| !p.trim().is_empty());
    match last.kind.as_str() {
        "answer" if has_proposal => Next::Approve,
        "draft" => last.data["draft"].as_str().map_or(Next::Nothing, |draft| Next::Apply(draft.to_string())),
        _ => Next::Nothing,
    }
}

/// After a turn ends: approve its proposal, or apply its draft, when the bot may edit without asking.
pub fn after_turn(state_dir: &Path, jobs: &Jobs, bot: &str, thread: &str) {
    let Ok(bot) = bots::find(state_dir, bot) else { return };
    if !bot.grants.auto_edit {
        return;
    }
    let messages = room::read_messages(&threads::chat_file(state_dir, &bot.id, thread));
    let _ = match next(&messages) {
        Next::Approve => HOME.get().map(|home| bots::approve(state_dir, home, jobs, &bot.id, thread)),
        Next::Apply(draft) => Some(bots::draft(state_dir, &bot.id, thread, &draft, "apply")),
        Next::Nothing => None,
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn message(author: &str, kind: &str, data: Value) -> room::Message {
        room::Message { ms: 0, author: author.into(), kind: kind.into(), data }
    }

    #[test]
    fn a_proposal_is_approved_and_its_draft_applied_and_nothing_more() {
        assert_eq!(next(&[message("bot", "answer", json!({ "reply": { "text": "x", "proposal": "Renomear soma" } }))]), Next::Approve);
        assert_eq!(next(&[message("bot", "draft", json!({ "draft": "d1" }))]), Next::Apply("d1".into()));
        assert_eq!(next(&[message("bot", "answer", json!({ "reply": { "text": "só conversa", "proposal": null } }))]), Next::Nothing);
        assert_eq!(next(&[message("bot", "draft", json!({ "draft": "d1" })), message("you", "draft_applied", json!({}))]), Next::Nothing, "an applied draft ends the chain");
        assert_eq!(next(&[message("bot", "failed", json!({ "error": "x" }))]), Next::Nothing);
    }
}
