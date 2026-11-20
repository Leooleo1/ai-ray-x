//! The lines a bot ends an answer with, which the panel turns into cards:
//! a change to approve (RAIOX-PROPOSAL) and messages for other projects'
//! bots (RAIOX-TO <project>), sent through ai-memory only if approved.

use serde::{Deserialize, Serialize};

pub const QUESTION_MARK: &str = "RAIOX-QUESTION:";
pub const PROPOSAL_MARK: &str = "RAIOX-PROPOSAL:";
pub const TO_MARK: &str = "RAIOX-TO";
pub const OPEN_MARK: &str = "RAIOX-OPEN:";
pub const RUN_MARK: &str = "RAIOX-RUN:";
pub const VERDICT_MARK: &str = "RAIOX-VERDICT:";
pub const ASK_MARK: &str = "RAIOX-ASK";
pub const SHOT_MARK: &str = "RAIOX-SHOT:";
/// A task the bot leaves on the team's board.
pub const TASK_MARK: &str = "RAIOX-TASK:";
const MAX_TASKS: usize = 3;
/// Most screenshots one answer can ask for.
const MAX_SHOTS: usize = 2;
/// What a reviewing bot can conclude.
const VERDICTS: [&str; 3] = ["ok", "concerns", "reject"];
/// Most commands one answer can ask for.
const MAX_RUNS: usize = 2;
/// Places in the panel a bot can point to with a card.
/// "access" is the one-click card that gives a bot that only reads its full access.
pub const PLACES: [&str; 7] = ["model", "settings", "memories", "connections", "wires", "app", "access"];
/// Longest message for another bot, in characters.
const MAX_NOTE: usize = 1200;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Note {
    /// The other project's folder name, as the bot wrote it.
    pub project: String,
    pub text: String,
}

#[derive(Debug, Default, PartialEq)]
pub struct Marked {
    pub text: String,
    pub proposal: Option<String>,
    pub question: Option<String>,
    pub notes: Vec<Note>,
    /// Panel places to open, from RAIOX-OPEN lines (known places only, once each).
    pub open: Vec<String>,
    /// Commands the bot asks raiox to run, from RAIOX-RUN lines.
    pub run: Vec<String>,
    /// A reviewer's conclusion: "ok", "concerns" or "reject".
    pub verdict: Option<String>,
    /// Requests for other members of a channel, from RAIOX-ASK @handle lines (the handle goes in `project`).
    pub asks: Vec<Note>,
    /// Screenshots the bot asks for, from RAIOX-SHOT lines: a path of the running app, a local URL or "simulator".
    pub shots: Vec<String>,
    /// Tasks the bot leaves on the team's board, from RAIOX-TASK lines.
    pub tasks: Vec<String>,
}

/// Drops one pair of backticks wrapping the whole value; backticks inside stay.
fn unquote(text: &str) -> String {
    let text = text.trim();
    let inner = text.strip_prefix('`').and_then(|t| t.strip_suffix('`')).filter(|t| !t.contains('`'));
    inner.unwrap_or(text).trim().to_string()
}

fn note(line: &str) -> Option<Note> {
    let rest = line.strip_prefix(TO_MARK)?;
    let (project, text) = rest.split_once(':')?;
    let project = unquote(project);
    let text: String = unquote(text).chars().take(MAX_NOTE).collect();
    (!project.is_empty() && !project.contains(char::is_whitespace) && !text.is_empty()).then_some(Note { project, text })
}

/// Splits the marker lines off an answer.
pub fn split(answer: &str) -> Marked {
    let mut marked = Marked::default();
    let mut kept = Vec::new();
    for line in answer.lines() {
        let trimmed = line.trim();
        if let Some(p) = trimmed.strip_prefix(PROPOSAL_MARK) {
            marked.proposal = Some(unquote(p)).filter(|p| !p.is_empty());
        } else if let Some(question) = trimmed.strip_prefix(QUESTION_MARK) {
            let question: String = unquote(question).chars().take(MAX_NOTE).collect();
            if !question.is_empty() && marked.question.is_none() {
                marked.question = Some(question);
            }
        } else if let Some(place) = trimmed.strip_prefix(OPEN_MARK) {
            let place = unquote(place).to_lowercase();
            if PLACES.contains(&place.as_str()) && !marked.open.contains(&place) {
                marked.open.push(place);
            }
        } else if let Some(task) = trimmed.strip_prefix(TASK_MARK) {
            let task = unquote(task).trim_matches(['"', '\u{201c}', '\u{201d}']).trim().to_string();
            if !task.is_empty() && marked.tasks.len() < MAX_TASKS && !marked.tasks.contains(&task) {
                marked.tasks.push(task);
            }
        } else if let Some(shot) = trimmed.strip_prefix(SHOT_MARK) {
            let shot = unquote(shot);
            if !shot.is_empty() && !shot.contains(char::is_whitespace) && marked.shots.len() < MAX_SHOTS && !marked.shots.contains(&shot) {
                marked.shots.push(shot);
            }
        } else if let Some(ask) = trimmed.strip_prefix(ASK_MARK).and_then(|rest| rest.trim().strip_prefix('@')).and_then(|rest| rest.split_once(':')) {
            let (handle, text) = (ask.0.trim().to_lowercase(), unquote(ask.1));
            if !handle.is_empty() && !handle.contains(char::is_whitespace) && !text.is_empty() {
                marked.asks.push(Note { project: handle, text: text.chars().take(MAX_NOTE).collect() });
            }
        } else if let Some(verdict) = trimmed.strip_prefix(VERDICT_MARK) {
            let verdict = unquote(verdict).to_lowercase();
            if VERDICTS.contains(&verdict.as_str()) {
                marked.verdict = Some(verdict);
            }
        } else if let Some(command) = trimmed.strip_prefix(RUN_MARK) {
            let command = unquote(command);
            if !command.is_empty() && marked.run.len() < MAX_RUNS && !marked.run.contains(&command) {
                marked.run.push(command);
            }
        } else if let Some(n) = note(trimmed) {
            marked.notes.push(n);
        } else {
            kept.push(line);
        }
    }
    marked.text = kept.join("\n").trim().to_string();
    marked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proposals_and_notes_become_cards() {
        let m = split("I would rename it.\n\nRAIOX-PROPOSAL: rename soma to add\nRAIOX-TO shop: the API now returns `total`\n");
        assert_eq!(m.text, "I would rename it.");
        assert_eq!(m.proposal.as_deref(), Some("rename soma to add"));
        assert_eq!(m.notes, vec![Note { project: "shop".into(), text: "the API now returns `total`".into() }]);
    }

    #[test]
    fn a_question_waits_for_the_person() {
        let marked = split("I found two options.\nRAIOX-QUESTION: `Which option should I use?`\nRAIOX-QUESTION: When?");
        assert_eq!(marked.text, "I found two options.");
        assert_eq!(marked.question.as_deref(), Some("Which option should I use?"));
    }

    #[test]
    fn starting_the_app_opens_see_the_app() {
        let m = split("I cannot start servers from a chat.\nRAIOX-OPEN: app\nRAIOX-OPEN: terminal\n");
        assert_eq!(m.open, vec!["app".to_string()], "only the places raiox has");
        let asked = split("Para editar preciso de acesso total.\nRAIOX-OPEN: access\n");
        assert_eq!(asked.open, vec!["access".to_string()], "the one-click full access card");
    }

    #[test]
    fn tasks_left_for_the_board_are_read_and_capped() {
        let m = split("Feito.\nRAIOX-TASK: Revisar o checkout\nRAIOX-TASK: \"Testar no celular\"\nRAIOX-TASK: c\nRAIOX-TASK: d\n");
        assert_eq!((m.text.as_str(), m.tasks.clone()), ("Feito.", vec!["Revisar o checkout".to_string(), "Testar no celular".to_string(), "c".to_string()]));
    }

    #[test]
    fn screenshots_asked_for_become_requests() {
        let m = split("Vou conferir a tela.\nRAIOX-SHOT: `/login`\nRAIOX-SHOT: simulator\nRAIOX-SHOT: /a\n");
        assert_eq!((m.text.as_str(), m.shots.clone()), ("Vou conferir a tela.", vec!["/login".to_string(), "simulator".to_string()]));
    }

    #[test]
    fn a_member_asks_another_through_the_person() {
        let m = split("Feito o front.\nRAIOX-ASK @Docs: atualize o README com a rota nova\nRAIOX-ASK nobody: sem arroba\n");
        assert_eq!(m.text, "Feito o front.\nRAIOX-ASK nobody: sem arroba");
        assert_eq!(m.asks, vec![Note { project: "docs".into(), text: "atualize o README com a rota nova".into() }]);
    }

    #[test]
    fn a_review_ends_with_its_verdict() {
        let m = split("Looks right.\nRAIOX-VERDICT: `Concerns`\n");
        assert_eq!((m.text.as_str(), m.verdict.as_deref()), ("Looks right.", Some("concerns")));
        assert_eq!(split("RAIOX-VERDICT: maybe").verdict, None);
    }

    #[test]
    fn commands_to_run_become_cards() {
        let m = split("I need the history.\nRAIOX-RUN: `git log -1`\nRAIOX-RUN: git log -1\nRAIOX-RUN: npm test\nRAIOX-RUN: ls\n");
        assert_eq!(m.text, "I need the history.");
        assert_eq!(m.run, vec!["git log -1".to_string(), "npm test".to_string()]);
    }

    #[test]
    fn open_lines_become_cards_for_known_places_only() {
        let m = split("Troque o modelo.\nRAIOX-OPEN: model\nRAIOX-OPEN: shell\nRAIOX-OPEN: model");
        assert_eq!((m.text.as_str(), m.open), ("Troque o modelo.", vec!["model".to_string()]));
    }

    #[test]
    fn a_malformed_note_stays_in_the_text() {
        let m = split("RAIOX-TO : nothing\nRAIOX-TO two words: no");
        assert!(m.notes.is_empty());
        assert_eq!(m.text, "RAIOX-TO : nothing\nRAIOX-TO two words: no");
    }
}
