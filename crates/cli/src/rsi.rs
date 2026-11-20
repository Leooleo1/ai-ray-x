//! Embedded self-improving (RSI) agent template.
//!
//! Creates a ready-to-use project folder under `~/Projects/<slug>` from the
//! files compiled into the binary (no network access at runtime), writes the
//! `.raiox-rsi` marker so `safe_commands` lets the engine's subcommands run
//! without a click in that folder only, initializes git when available,
//! activates a bot on the chosen CLI, and opens the niche interview.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use anyhow::{bail, Result};

use crate::bots::{self, Bot, Jobs};

/// Marker file written at the root of an RSI agent project folder.
pub const MARKER: &str = ".raiox-rsi";
const MARKER_BODY: &str = "raiox-rsi template v1\n";
const MAX_SLUG: usize = 48;

/// All template files embedded at compile time from `templates/rsi/`.
const TEMPLATE: &[(&str, &[u8])] = &[
    (".gitignore", include_bytes!("../../../templates/rsi/.gitignore")),
    ("AGENTS.md", include_bytes!("../../../templates/rsi/AGENTS.md")),
    ("CLAUDE.md", include_bytes!("../../../templates/rsi/CLAUDE.md")),
    ("README.md", include_bytes!("../../../templates/rsi/README.md")),
    ("docs/architecture.md", include_bytes!("../../../templates/rsi/docs/architecture.md")),
    ("engine/__init__.py", include_bytes!("../../../templates/rsi/engine/__init__.py")),
    ("engine/collector.py", include_bytes!("../../../templates/rsi/engine/collector.py")),
    ("engine/evaluator.py", include_bytes!("../../../templates/rsi/engine/evaluator.py")),
    ("engine/presets.py", include_bytes!("../../../templates/rsi/engine/presets.py")),
    ("engine/simulator.py", include_bytes!("../../../templates/rsi/engine/simulator.py")),
    ("engine/store.py", include_bytes!("../../../templates/rsi/engine/store.py")),
    ("rsi.py", include_bytes!("../../../templates/rsi/rsi.py")),
    ("tests/fixtures/x_analytics_sample_en.csv", include_bytes!("../../../templates/rsi/tests/fixtures/x_analytics_sample_en.csv")),
    ("tests/fixtures/x_analytics_sample_pt.csv", include_bytes!("../../../templates/rsi/tests/fixtures/x_analytics_sample_pt.csv")),
    ("tests/test_rsi.py", include_bytes!("../../../templates/rsi/tests/test_rsi.py")),
];

/// Lowercase folder slug: 1 to 48 ASCII letters, digits, dashes or underscores, starting with a letter or digit.
pub fn valid_slug(slug: &str) -> bool {
    let mut bytes = slug.bytes();
    let Some(first) = bytes.next() else { return false };
    slug.len() <= MAX_SLUG
        && (first.is_ascii_lowercase() || first.is_ascii_digit())
        && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

/// Checks whether a candidate command runs Python 3 cleanly (skips Windows' Microsoft Store stub).
fn is_python3(program: &str, args: &[&str]) -> bool {
    crate::procs::command(program)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .ok()
        .filter(|out| out.status.success())
        .is_some_and(|out| {
            let combined = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
            combined.contains("Python 3.")
        })
}

/// The Python 3 command available on this machine (`python3`, `python`, or `py -3` on Windows).
pub fn python() -> Option<&'static str> {
    if is_python3("python3", &["--version"]) {
        Some("python3")
    } else if is_python3("python", &["--version"]) {
        Some("python")
    } else if is_python3("py", &["-3", "--version"]) {
        Some("py -3")
    } else {
        None
    }
}

/// Writes the embedded RSI template into `<home>/Projects/<slug>` with `.raiox-rsi` and `git init`.
/// Refuses invalid slugs or folders that already exist.
pub fn scaffold(home: &Path, slug: &str, python_cmd: &str) -> Result<PathBuf> {
    let slug = slug.trim();
    if !valid_slug(slug) {
        bail!("folder name must be 1 to {MAX_SLUG} lowercase letters, digits, dashes or underscores");
    }
    let parent = home.join("Projects");
    fs::create_dir_all(&parent)?;
    let target = parent.join(slug);
    if target.exists() {
        bail!("Projects/{slug} already exists; pick another name");
    }
    if let Err(error) = write_files(&target, python_cmd) {
        let _ = fs::remove_dir_all(&target);
        return Err(error);
    }
    // Initialize a git repository when git is available so project discovery and diffs work smoothly.
    let _ = crate::procs::command("git")
        .arg("init")
        .current_dir(&target)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    Ok(target)
}

fn write_files(target: &Path, python_cmd: &str) -> Result<()> {
    fs::create_dir_all(target)?;
    for (rel, bytes) in TEMPLATE {
        let dest = target.join(rel);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        // Adapt the Python invocation in agent instructions when running on a machine without `python3` (e.g. Windows).
        if python_cmd != "python3" && matches!(*rel, "CLAUDE.md" | "AGENTS.md") {
            let text = String::from_utf8_lossy(bytes).replace("python3 rsi.py", &format!("{python_cmd} rsi.py"));
            fs::write(&dest, text.as_bytes())?;
        } else {
            fs::write(&dest, bytes)?;
        }
    }
    fs::write(target.join(MARKER), MARKER_BODY)?;
    Ok(())
}

/// Creates a new RSI agent project under `~/Projects/<slug>`, activates a bot for it,
/// and starts the opening conversation asking which niche to focus on.
pub fn create(state_dir: &Path, home: &Path, jobs: &Jobs, name: &str, agent: &str, lang: Option<&str>) -> Result<Bot> {
    let Some(py) = python() else {
        bail!("Python 3 is required to run the self-improving agent; install Python 3 first");
    };
    let slug = name.trim();
    if !valid_slug(slug) {
        bail!("folder name must be 1 to {MAX_SLUG} lowercase letters, digits, dashes or underscores");
    }
    if !crate::connections::bot_agents().any(|a| a == agent) {
        bail!("raiox cannot run {agent} as a bot");
    }
    let dir = scaffold(home, slug, py)?;
    let bot = match bots::activate(state_dir, home, &format!("Projects/{slug}"), agent, None) {
        Ok(bot) => bot,
        Err(error) => {
            let _ = fs::remove_dir_all(&dir);
            return Err(error);
        }
    };
    crate::team::refresh(state_dir, crate::team::around(state_dir, &bot.name));
    let portuguese = lang
        .or(crate::settings::Settings::load(state_dir).language.as_deref())
        .is_some_and(|l| l.to_lowercase().starts_with("pt"));
    let opening = if portuguese {
        "Vamos configurar este agente que se melhora sozinho. Comece me perguntando qual nicho eu quero escolher."
    } else {
        "Let's set up this self-improving agent. Start by asking me which niche I want to choose."
    };
    let _ = bots::ask(state_dir, jobs, &bot.id, crate::threads::MAIN, opening, &[]);
    Ok(bot)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_home(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("raiox-rsi-tmpl-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn slug_validation_rejects_paths_spaces_and_capitals() {
        assert!(valid_slug("agente-rsi"));
        assert!(valid_slug("rsi_v2"));
        assert!(valid_slug("a"));
        assert!(!valid_slug(""));
        assert!(!valid_slug("-leading-dash"));
        assert!(!valid_slug("../escape"));
        assert!(!valid_slug("a/b"));
        assert!(!valid_slug("Agent-RSI"));
        assert!(!valid_slug("with space"));
        assert!(!valid_slug("dot.name"));
        assert!(!valid_slug(&"a".repeat(MAX_SLUG + 1)));
    }

    #[test]
    fn scaffold_writes_template_marker_and_refuses_existing_folder() {
        let home = temp_home("scaffold");
        let dir = scaffold(&home, "agente-rsi", "python3").unwrap();
        assert_eq!(dir, home.join("Projects").join("agente-rsi"));
        assert!(dir.join(MARKER).is_file(), "marker file must be created");
        assert!(dir.join("rsi.py").is_file());
        assert!(dir.join("CLAUDE.md").is_file());
        assert!(dir.join("AGENTS.md").is_file());
        assert!(dir.join("engine/evaluator.py").is_file());
        assert!(dir.join("tests/test_rsi.py").is_file());
        assert!(!dir.join("state").exists(), "template starts clean without state/");
        assert!(!dir.join("playbooks").exists(), "template starts clean without playbooks/");

        // Existing folder must be refused cleanly.
        let err = scaffold(&home, "agente-rsi", "python3").unwrap_err();
        assert!(err.to_string().contains("already exists"));

        // Invalid name must be refused.
        assert!(scaffold(&home, "../fora", "python3").is_err());

        // Windows-style python command rewrites CLAUDE.md and AGENTS.md cleanly.
        let win_dir = scaffold(&home, "agente-win", "py -3").unwrap();
        assert!(fs::read_to_string(win_dir.join("CLAUDE.md")).unwrap().contains("py -3 rsi.py status"));
        assert!(fs::read_to_string(win_dir.join("AGENTS.md")).unwrap().contains("py -3 rsi.py status"));

        let _ = fs::remove_dir_all(&home);
    }
}
