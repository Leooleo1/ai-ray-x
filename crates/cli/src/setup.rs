//! First-run setup and settings data: the CLIs on this machine (see connections), the
//! person's projects (with their GitHub remote, read from `.git/config`), and
//! GitHub through the person's own `gh` CLI. raiox stores no token: it only
//! asks `gh`, `claude` and `codex` for their status. Account e-mails and
//! organization names are never read out of those answers.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};

use anyhow::{bail, Context, Result};
use raiox_core::Store;
use serde::Serialize;
use serde_json::{json, Value};

/// Folders where people usually keep their code, relative to the home.
const PARENTS: [&str; 11] =
    ["Projects", "projects", "Developer", "dev", "code", "src", "workspace", "repos", "GitHub", "Documents/GitHub", "Sites"];
const MAX_PROJECTS: usize = 200;
const MAX_REPOS: &str = "100";
const MAX_DESCRIPTION: usize = 120;

#[derive(Debug, Serialize)]
pub struct Project {
    /// Path relative to the home, as bots take it.
    pub project: String,
    pub name: String,
    pub github: Option<String>,
    pub claude: u64,
    pub codex: u64,
    pub last_ms: Option<i64>,
}

/// Clones running or failed, by "owner/name".
#[derive(Clone, Default)]
pub struct Clones(Arc<Mutex<HashMap<String, Option<String>>>>);

impl Clones {
    fn snapshot(&self) -> Value {
        let map = self.0.lock().map(|m| m.clone()).unwrap_or_default();
        let running: Vec<&String> = map.iter().filter(|(_, e)| e.is_none()).map(|(r, _)| r).collect();
        let failed: BTreeMap<&String, &String> = map.iter().filter_map(|(r, e)| e.as_ref().map(|e| (r, e))).collect();
        json!({ "running": running, "failed": failed })
    }
}

fn quiet(program: &str, args: &[&str]) -> Option<std::process::Output> {
    crate::procs::command(program).args(args).stdin(Stdio::null()).output().ok()
}

fn first_line(bytes: &[u8]) -> Option<String> {
    String::from_utf8_lossy(bytes).lines().next().map(|l| l.trim().to_string()).filter(|l| !l.is_empty())
}

/// "owner/name" from a GitHub remote URL (https, ssh or scp-like).
pub fn github_slug(url: &str) -> Option<String> {
    let rest = ["https://github.com/", "http://github.com/", "git@github.com:", "ssh://git@github.com/"]
        .iter()
        .find_map(|prefix| url.trim().strip_prefix(prefix))?;
    let slug = rest.trim_end_matches('/').trim_end_matches(".git");
    let mut parts = slug.split('/');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(owner), Some(name), None) if !owner.is_empty() && !name.is_empty() => Some(format!("{owner}/{name}")),
        _ => None,
    }
}

/// The origin remote of a checkout, read from `.git/config` without running git.
fn origin(dir: &Path) -> Option<String> {
    let config = fs::read_to_string(dir.join(".git").join("config")).ok()?;
    let mut in_origin = false;
    for line in config.lines().map(str::trim) {
        if line.starts_with('[') {
            in_origin = line == "[remote \"origin\"]";
        } else if in_origin && let Some(url) = line.strip_prefix("url").map(|r| r.trim_start().trim_start_matches('=').trim()) {
            return github_slug(url);
        }
    }
    None
}

/// The same folder under two spellings (Projects, projects) has one identity.
#[cfg(unix)]
pub(crate) type Identity = (u64, u64);
#[cfg(not(unix))]
pub(crate) type Identity = PathBuf;

#[cfg(unix)]
pub(crate) fn identity(path: &Path) -> Option<Identity> {
    use std::os::unix::fs::MetadataExt;
    fs::metadata(path).ok().filter(|m| m.is_dir()).map(|m| (m.dev(), m.ino()))
}

#[cfg(not(unix))]
pub(crate) fn identity(path: &Path) -> Option<Identity> {
    path.canonicalize().ok().filter(|p| p.is_dir())
}

/// System, cache or hidden folder names that are never user projects.
const NON_PROJECT_SEGMENTS: [&str; 3] = ["library", "caches", "appdata"];

/// True when a home-relative path points into a hidden, Library or Caches folder.
pub(crate) fn is_non_project_path(project: &str) -> bool {
    project.split(['/', '\\']).any(|seg| {
        seg.starts_with('.') || NON_PROJECT_SEGMENTS.iter().any(|bad| seg.eq_ignore_ascii_case(bad))
    })
}

fn label(home: &Path, dir: &Path) -> Option<String> {
    let rest = dir.strip_prefix(home).ok()?.to_string_lossy().into_owned();
    (!rest.is_empty() && !is_non_project_path(&rest)).then_some(rest)
}

/// Projects with Claude Code or Codex usage, plus git checkouts in the usual folders.
pub fn projects(store: &Store, home: &Path) -> Result<Vec<Project>> {
    let mut by_label: BTreeMap<String, Project> = BTreeMap::new();
    let mut statement = store.conn().prepare(
        "SELECT project, source, COUNT(*), MAX(ts_ms) FROM calls
         WHERE is_sub = 0 AND project IS NOT NULL AND project NOT IN ('', '~') GROUP BY project, source",
    )?;
    let rows = statement.query_map([], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, Option<i64>>(3)?))
    })?;
    let empty = |project: &str| Project {
        project: project.to_string(),
        name: project.rsplit('/').next().unwrap_or(project).to_string(),
        github: None,
        claude: 0,
        codex: 0,
        last_ms: None,
    };
    for (project, source, count, last) in rows.flatten() {
        if is_non_project_path(&project) || !home.join(&project).is_dir() {
            continue;
        }
        let entry = by_label.entry(project.clone()).or_insert_with(|| empty(&project));
        match source.as_str() {
            "claude" => entry.claude += count.max(0) as u64,
            "codex" => entry.codex += count.max(0) as u64,
            _ => {}
        }
        entry.last_ms = entry.last_ms.max(last);
    }
    // Case-insensitive disks see Projects and projects as one folder: scan each once.
    let mut scanned = HashSet::new();
    let parents: Vec<PathBuf> = PARENTS.iter().map(|p| home.join(p)).filter(|p| identity(p).is_some_and(|id| scanned.insert(id))).collect();
    let known: HashSet<Identity> = by_label.keys().filter_map(|l| identity(&home.join(l))).collect();
    for parent in parents {
        let Ok(children) = fs::read_dir(&parent) else { continue };
        // A checkout has a .git folder; worktrees and submodules have a .git file.
        for dir in children.flatten().map(|e| e.path()).filter(|p| p.join(".git").is_dir() && identity(p).is_some_and(|id| !known.contains(&id))) {
            if let Some(key) = label(home, &dir) {
                by_label.entry(key.clone()).or_insert_with(|| empty(&key));
            }
        }
    }
    let mut seen = HashSet::new();
    let mut list: Vec<Project> = by_label
        .into_values()
        .filter(|p| identity(&home.join(&p.project)).is_some_and(|id| seen.insert(id)))
        .map(|p| Project { github: origin(&home.join(&p.project)), ..p })
        .collect();
    list.sort_by(|a, b| b.last_ms.cmp(&a.last_ms).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    list.truncate(MAX_PROJECTS);
    Ok(list)
}

/// Where a cloned repository goes: the folder that already holds most projects.
fn clone_parent(home: &Path, projects: &[Project]) -> PathBuf {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for p in projects {
        if let Some((parent, _)) = p.project.rsplit_once('/') {
            *counts.entry(parent).or_default() += 1;
        }
    }
    let best = counts.into_iter().max_by_key(|(_, n)| *n).map_or("Projects", |(parent, _)| parent);
    home.join(best)
}

fn done_file(state_dir: &Path) -> PathBuf {
    state_dir.join("setup.json")
}

pub fn is_done(state_dir: &Path) -> bool {
    done_file(state_dir).exists()
}

pub fn finish(state_dir: &Path) -> Result<()> {
    fs::create_dir_all(state_dir)?;
    fs::write(done_file(state_dir), json!({ "done_ms": crate::room::now_ms() }).to_string() + "\n")?;
    Ok(())
}

/// Fast local snapshot for `/api/setup` so the server can probe CLIs off the main loop when needed.
pub struct SetupBase {
    pub projects: Vec<Project>,
    pub clone_to: String,
    pub done: bool,
    pub clones: Value,
    pub usage: Vec<(u64, u64)>,
    pub settings: crate::settings::Settings,
}

impl SetupBase {
    /// Builds the `/api/setup` payload from cached or freshly probed CLI statuses.
    pub fn into_state(self, probes: Vec<crate::connections::Probe>, ais_cached: bool) -> Value {
        json!({
            "done": self.done,
            "ais": crate::connections::statuses(probes, &self.usage, &self.settings),
            "ais_cached": ais_cached,
            "clone_to": self.clone_to,
            "clones": self.clones,
            "installs": crate::install::state(),
            "projects": self.projects,
        })
    }
}

pub fn base(store: &Store, home: &Path, state_dir: &Path, clones: &Clones) -> Result<SetupBase> {
    let projects = projects(store, home)?;
    let parent = clone_parent(home, &projects);
    Ok(SetupBase {
        clone_to: label(home, &parent).unwrap_or_else(|| "Projects".into()),
        projects,
        done: is_done(state_dir),
        clones: clones.snapshot(),
        usage: crate::connections::usage_all(store),
        settings: crate::settings::Settings::load(state_dir),
    })
}

/// Everything the setup screens need except GitHub (which goes to the network).
/// Uses the disk probe cache when available and refreshes in the background.
pub fn state(store: &Store, home: &Path, state_dir: &Path, clones: &Clones) -> Result<Value> {
    let b = base(store, home, state_dir, clones)?;
    let (probes, ais_cached) = match crate::connections::load_cached(state_dir) {
        Some(cached) => {
            crate::connections::refresh_in_background(state_dir.to_path_buf(), home.to_path_buf());
            (cached, true)
        }
        None => (crate::connections::probe_and_cache(state_dir, home), false),
    };
    Ok(b.into_state(probes, ais_cached))
}

/// Creates `~/Projects/<name>` from the onboarding project step and returns both the new
/// project entry and the refreshed setup state.
pub fn create_project(store: &Store, home: &Path, state_dir: &Path, clones: &Clones, name: &str) -> Result<Value> {
    let trimmed = name.trim();
    let (project, _) = crate::bots::create_project_dir(home, trimmed)?;
    let created = Project {
        project,
        name: trimmed.to_string(),
        github: None,
        claude: 0,
        codex: 0,
        last_ms: None,
    };
    Ok(json!({
        "project": created,
        "setup": state(store, home, state_dir, clones)?,
    }))
}

/// GitHub through the person's `gh`: who is logged in and their repositories.
pub fn github() -> Value {
    let installed = quiet("gh", &["--version"]).is_some_and(|o| o.status.success());
    let login = installed
        .then(|| quiet("gh", &["api", "user", "--jq", ".login"]))
        .flatten()
        .filter(|o| o.status.success())
        .and_then(|o| first_line(&o.stdout));
    let repos: Vec<Value> = login
        .as_ref()
        .and_then(|_| quiet("gh", &["repo", "list", "--limit", MAX_REPOS, "--json", "nameWithOwner,name,isPrivate,pushedAt,description"]))
        .filter(|o| o.status.success())
        .and_then(|o| serde_json::from_slice::<Vec<Value>>(&o.stdout).ok())
        .unwrap_or_default()
        .into_iter()
        .map(|r| {
            let text = |key: &str| r.get(key).and_then(Value::as_str).unwrap_or("").to_string();
            json!({
                "repo": text("nameWithOwner"),
                "name": text("name"),
                "private": r.get("isPrivate").and_then(Value::as_bool).unwrap_or(true),
                "pushed_at": text("pushedAt"),
                "description": text("description").chars().take(MAX_DESCRIPTION).collect::<String>(),
            })
        })
        .collect();
    json!({
        "installed": installed,
        "logged_in": login.is_some(),
        "login": login,
        "repos": repos,
        "install": "brew install gh",
        "connect": "gh auth login",
    })
}

fn valid_part(part: &str) -> bool {
    !part.is_empty() && !part.starts_with(['-', '.']) && part.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
}

/// Clones one of the person's repositories with `gh`, on its own thread.
pub fn clone(store: &Store, home: &Path, clones: &Clones, repo: &str) -> Result<()> {
    let (owner, name) = repo.split_once('/').context("expected owner/name")?;
    if !valid_part(owner) || !valid_part(name) {
        bail!("expected owner/name");
    }
    let target = clone_parent(home, &projects(store, home)?).join(name);
    if target.exists() {
        bail!("{} already exists", target.display());
    }
    {
        let mut running = clones.0.lock().map_err(|_| anyhow::anyhow!("clone lock poisoned"))?;
        if running.get(repo).is_some_and(Option::is_none) {
            bail!("already cloning {repo}");
        }
        running.insert(repo.to_string(), None);
    }
    let (clones, repo) = (clones.clone(), repo.to_string());
    std::thread::spawn(move || {
        let result = crate::procs::command("gh").args(["repo", "clone", &repo]).arg(&target).stdin(Stdio::null()).output();
        let failure = match result {
            Ok(out) if out.status.success() => None,
            Ok(out) => Some(first_line(&out.stderr).unwrap_or_else(|| "gh repo clone failed".into())),
            Err(error) => Some(format!("could not run gh: {error}")),
        };
        if let Ok(mut map) = clones.0.lock() {
            match failure {
                None => map.remove(&repo),
                Some(error) => map.insert(repo.clone(), Some(error)),
            };
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_remotes_in_every_form() {
        for url in ["https://github.com/a/b.git", "git@github.com:a/b.git", "ssh://git@github.com/a/b", "https://github.com/a/b/"] {
            assert_eq!(github_slug(url).as_deref(), Some("a/b"), "{url}");
        }
        assert_eq!(github_slug("https://gitlab.com/a/b"), None);
        assert_eq!(github_slug("https://github.com/a"), None);
    }

    #[test]
    fn repository_names_cannot_climb_folders() {
        assert!(valid_part("raio-x_ia.v2"));
        assert!(!valid_part(".."));
        assert!(!valid_part("-flag"));
        assert!(!valid_part("a b"));
        assert!(!valid_part(""));
    }

    #[test]
    fn create_project_makes_folder_and_returns_entry() {
        let home = std::env::temp_dir().join(format!("raiox-setup-create-{}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
        fs::create_dir_all(&home).unwrap();
        let state_dir = home.join(".state");
        let store = Store::open_in_memory().unwrap();
        let res = create_project(&store, &home, &state_dir, &Clones::default(), "first-app").unwrap();
        assert_eq!(res["project"]["project"], "Projects/first-app");
        assert_eq!(res["project"]["name"], "first-app");
        assert!(home.join("Projects/first-app").is_dir());
        assert!(create_project(&store, &home, &state_dir, &Clones::default(), "../bad").is_err());
        let _ = fs::remove_dir_all(&home);
    }
}
