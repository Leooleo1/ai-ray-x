//! Which version of raiox's rules each conversation has read. Codex, Cursor,
//! Antigravity and opencode get the guide only when a conversation starts, so
//! when raiox's rules change (a new marker, a new way to do something), a
//! conversation already going would never learn them. raiox keeps the rules'
//! version each conversation read and gives it the guide again, once, when
//! the rules changed since.

use std::collections::BTreeMap;
use std::fs;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::Path;

const FILE: &str = "regras-lidas.json";

/// The rules' version: the guide's fixed text (no project, persona, team or pages in it).
pub fn version() -> u64 {
    let mut hasher = DefaultHasher::new();
    crate::bot_run::guide("project", "", &[], &[]).hash(&mut hasher);
    hasher.finish()
}

fn load(state_dir: &Path) -> BTreeMap<String, u64> {
    fs::read_to_string(state_dir.join(FILE)).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
}

/// The rules' version a conversation ("bot:thread") read last, if raiox knows it.
pub fn read_by(state_dir: &Path, conversation: &str) -> Option<u64> {
    load(state_dir).get(conversation).copied()
}

/// A conversation read the guide (at its start, or again).
pub fn remember(state_dir: &Path, conversation: &str, version: u64) {
    let mut seen = load(state_dir);
    seen.insert(conversation.to_string(), version);
    if let Ok(text) = serde_json::to_string_pretty(&seen) {
        let _ = fs::write(state_dir.join(FILE), text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_conversation_reads_the_rules_again_only_when_they_changed() {
        let state = std::env::temp_dir().join(format!("raiox-rules-{}", std::process::id()));
        fs::create_dir_all(&state).unwrap();
        assert_eq!(read_by(&state, "b:main"), None, "a conversation from before raiox kept versions reads them again");
        remember(&state, "b:main", version());
        assert_eq!(read_by(&state, "b:main"), Some(version()));
        assert_eq!(version(), version(), "the version does not depend on the moment");
        let _ = fs::remove_dir_all(&state);
    }
}
