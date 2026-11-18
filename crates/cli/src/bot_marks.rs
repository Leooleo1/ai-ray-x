//! The lines a bot ends an answer with, which the panel turns into cards:
//! a change to approve (RAIOX-PROPOSAL) and messages for other projects'
//! bots (RAIOX-TO <project>), sent through ai-memory only if approved.

use serde::{Deserialize, Serialize};

pub const PROPOSAL_MARK: &str = "RAIOX-PROPOSAL:";
pub const TO_MARK: &str = "RAIOX-TO";
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
    pub notes: Vec<Note>,
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
    fn a_malformed_note_stays_in_the_text() {
        let m = split("RAIOX-TO : nothing\nRAIOX-TO two words: no");
        assert!(m.notes.is_empty());
        assert_eq!(m.text, "RAIOX-TO : nothing\nRAIOX-TO two words: no");
    }
}
