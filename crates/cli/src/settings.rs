//! The person's raiox settings: which CLIs they use, and the model and effort
//! their bots start with. Kept next to the index, never in a project.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{bots, connections};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Defaults {
    pub model: Option<String>,
    pub effort: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub enabled: BTreeMap<String, bool>,
    #[serde(default)]
    pub defaults: BTreeMap<String, Defaults>,
    /// The panel's language, when the person picked one ("en" or "pt-BR"); otherwise the system's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

/// The languages the panel speaks.
pub const LANGUAGES: [&str; 2] = ["en", "pt-BR"];

fn file(state_dir: &Path) -> PathBuf {
    state_dir.join("settings.json")
}

impl Settings {
    pub fn load(state_dir: &Path) -> Settings {
        fs::read_to_string(file(state_dir)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    fn save(&self, state_dir: &Path) -> Result<()> {
        fs::create_dir_all(state_dir)?;
        let temp = file(state_dir).with_extension("tmp");
        fs::write(&temp, serde_json::to_string_pretty(self)? + "\n")?;
        fs::rename(temp, file(state_dir))?;
        Ok(())
    }

    /// Claude Code and Codex start on; any other CLI waits for the person to turn it on.
    pub fn is_enabled(&self, id: &str) -> bool {
        self.enabled.get(id).copied().unwrap_or(matches!(id, "claude" | "codex"))
    }

    pub fn defaults_for(&self, id: &str) -> Defaults {
        self.defaults.get(id).cloned().unwrap_or_default()
    }
}

/// Applies `{ "enabled": { id: bool }, "defaults": { id: { model, effort } }, "language": "en" | "pt-BR" | null }`.
pub fn update(state_dir: &Path, input: &Value) -> Result<Settings> {
    let current = Settings::load(state_dir);
    let mut enabled = current.enabled.clone();
    for (id, on) in input.get("enabled").and_then(Value::as_object).into_iter().flatten() {
        if connections::known(id).is_none() {
            bail!("unknown CLI {id}");
        }
        enabled.insert(id.clone(), on.as_bool().unwrap_or(false));
    }
    let mut defaults = current.defaults.clone();
    for (id, value) in input.get("defaults").and_then(Value::as_object).into_iter().flatten() {
        if !connections::known(id).is_some_and(|k| k.bots) {
            bail!("{id} cannot run bots");
        }
        let field = |key: &str| value.get(key).and_then(Value::as_str);
        let chosen = Defaults { model: bots::check_model(field("model"))?, effort: bots::check_effort(id, field("effort"))? };
        defaults.insert(id.clone(), chosen);
    }
    let language = match input.get("language") {
        None => current.language.clone(),
        Some(Value::Null) => None,
        Some(value) => match value.as_str() {
            Some(language) if LANGUAGES.contains(&language) => Some(language.to_string()),
            _ => bail!("the panel speaks {}", LANGUAGES.join(" or ")),
        },
    };
    let next = Settings { enabled, defaults, language };
    next.save(state_dir)?;
    Ok(next)
}
