//! Wires between projects: a bot only knows of, and can only message, the
//! projects the person wired to its own. Fewer names in each bot's
//! instructions, and no message to a project nobody chose.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Result};

use crate::bots;

fn file(state_dir: &Path) -> PathBuf {
    state_dir.join("links.json")
}

/// Pairs of project names, each sorted, the list sorted.
pub fn list(state_dir: &Path) -> Vec<[String; 2]> {
    fs::read_to_string(file(state_dir)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn pair(a: &str, b: &str) -> [String; 2] {
    if a <= b { [a.to_string(), b.to_string()] } else { [b.to_string(), a.to_string()] }
}

pub fn are_linked(state_dir: &Path, a: &str, b: &str) -> bool {
    list(state_dir).contains(&pair(a, b))
}

/// The projects wired to this one.
pub fn linked(state_dir: &Path, project: &str) -> Vec<String> {
    list(state_dir)
        .into_iter()
        .filter_map(|[x, y]| if x == project { Some(y) } else if y == project { Some(x) } else { None })
        .collect()
}

/// Wires two projects, or unwires them when they already are.
pub fn toggle(state_dir: &Path, a: &str, b: &str) -> Result<()> {
    let projects: Vec<String> = bots::list(state_dir).into_iter().map(|bot| bot.name).collect();
    if a == b || !projects.iter().any(|p| p == a) || !projects.iter().any(|p| p == b) {
        bail!("wire two different projects that have bots");
    }
    let wire = pair(a, b);
    let current = list(state_dir);
    let mut next: Vec<[String; 2]> =
        if current.contains(&wire) { current.into_iter().filter(|w| w != &wire).collect() } else { current.into_iter().chain([wire]).collect() };
    next.sort();
    fs::create_dir_all(state_dir)?;
    let temp = file(state_dir).with_extension("tmp");
    fs::write(&temp, serde_json::to_string_pretty(&next)? + "\n")?;
    fs::rename(temp, file(state_dir))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wire_has_no_direction() {
        assert_eq!(pair("shop", "app"), pair("app", "shop"));
    }
}
