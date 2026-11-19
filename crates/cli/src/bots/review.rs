//! Two AIs in a pair: when a bot proposes a change, another bot of the same
//! project (another CLI) reads the proposal and the files involved and says
//! whether it holds, before the person approves. The review lands under the
//! proposal in the proposer's conversation, and the approved edit is told
//! what the reviewer said.

use std::path::Path;

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

use super::turn::{ask_cli, Setup};
use super::{find, list, note, said, Bot, Jobs};
use crate::bot_run::Mode;
use crate::{room, threads};

/// The conversation name a reviewing bot is busy in.
pub(super) const REVIEWING: &str = "review";

/// How the person knows a CLI: "Claude Code", "Codex", "Cursor".
fn named(agent: &str) -> &str {
    crate::connections::KNOWN.iter().find(|k| k.id == agent).map_or(agent, |k| k.name)
}

/// The other bots of a project that can review this one's proposals.
pub fn reviewers(state_dir: &Path, bot: &Bot) -> Vec<Bot> {
    list(state_dir).into_iter().filter(|b| b.dir == bot.dir && b.id != bot.id).collect()
}

fn prompt(proposer: &Bot, proposal: &str, answer: &str) -> String {
    format!(
        "Another AI working in this project ({}) proposes a change, and the person wants your review before approving it.\n\n\
         Proposal: {proposal}\n\nIts explanation:\n{answer}\n\n\
         Read the files involved. Say briefly, in the language of the explanation, whether the change is right, complete \
         and safe, and what it would break or miss. Do not make the change and do not propose another one. End with \
         exactly one line:\n{} ok | concerns | reject",
        named(&proposer.agent),
        crate::bot_marks::VERDICT_MARK
    )
}

/// Asks `reviewer` to review the proposal in the answer `answer_ms` of this conversation.
pub fn request(state_dir: &Path, jobs: &Jobs, id: &str, thread: &str, answer_ms: i64, reviewer: &str) -> Result<()> {
    if !threads::valid(thread) {
        bail!("invalid conversation");
    }
    let proposer = find(state_dir, id)?;
    let reviewer = reviewers(state_dir, &proposer).into_iter().find(|b| b.id == reviewer).context("no such reviewer in this project")?;
    let file = threads::chat_file(state_dir, id, thread);
    let messages = room::read_messages(&file);
    let answer = messages.iter().find(|m| m.ms == answer_ms && m.author == "bot" && m.kind == "answer").context("no such answer")?;
    let proposal = answer.data.pointer("/reply/proposal").and_then(Value::as_str).context("that answer proposes no change")?.to_string();
    let explanation = answer.data.pointer("/reply/text").and_then(Value::as_str).unwrap_or("").to_string();
    crate::budget::check(state_dir, &reviewer.name)?;
    if !jobs.start(&reviewer.id, REVIEWING) {
        bail!("{} is answering now; ask for the review again in a moment", named(&reviewer.agent));
    }
    let asked = said("review_asked", json!({ "answer_ms": answer_ms, "reviewer": reviewer.id }));
    if let Err(error) = room::append_messages(&file, vec![asked]) {
        jobs.finish(&reviewer.id);
        return Err(error);
    }
    let (state_dir, jobs) = (state_dir.to_path_buf(), jobs.clone());
    std::thread::spawn(move || {
        let text = prompt(&proposer, &proposal, &explanation);
        let setup = Setup::new(&state_dir, &reviewer, &text, Mode::Talk);
        // A fresh session in the reviewer's own folder: the review stays out of its conversations.
        let message = match ask_cli(&jobs, &reviewer, &setup, &reviewer.dir, None, Mode::Talk, &text, None, &[]) {
            Ok(reply) => note("review", json!({ "answer_ms": answer_ms, "reviewer": reviewer.id, "agent": reviewer.agent, "reply": reply })),
            Err(error) => note("failed", json!({ "error": format!("{} could not review: {error:#}", named(&reviewer.agent)), "review": answer_ms })),
        };
        let _ = room::append_messages(&file, vec![message]);
        jobs.finish(&reviewer.id);
    });
    Ok(())
}

/// What the reviewer said about an answer's proposal, for the approved edit.
pub fn for_answer(messages: &[room::Message], answer_ms: i64) -> Option<String> {
    let review = messages.iter().rev().find(|m| m.kind == "review" && m.data.get("answer_ms").and_then(Value::as_i64) == Some(answer_ms))?;
    let agent = review.data.get("agent").and_then(Value::as_str).map_or("another AI", named);
    let text = review.data.pointer("/reply/text").and_then(Value::as_str)?;
    Some(format!("\n\nBefore the approval, {agent} reviewed the proposal:\n{text}\nTake its points into account where they are right."))
}
