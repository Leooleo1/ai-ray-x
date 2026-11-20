//! Built-in Dev/Infra auditor: an opt-in, per-project agent that audits
//! scripts, CI workflows, raiox routines, project skills and instruction files
//! (CLAUDE.md, AGENTS.md, SKILL.md) using raiox's own runtime rules and recent
//! failure telemetry. Off by default; runs on demand or on a schedule, never
//! auto-edits silently, and delivers verified fixes through normal drafts or
//! board tasks.

use std::collections::BTreeSet;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::time::UNIX_EPOCH;

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::bots::{self, Bot, Jobs};
use crate::{room, threads};

/// Title of the dedicated conversation holding a project's Dev/Infra audits.
pub const THREAD_TITLE: &str = "Dev/Infra Audit";
/// Allowed audit cadences.
pub const CADENCES: [&str; 3] = ["manual", "on_change", "weekly"];
/// Minimum gap between automatic `on_change` checks (one day).
const ON_CHANGE_INTERVAL_MS: i64 = 24 * 60 * 60 * 1000;
/// Minimum gap between automatic `weekly` audits (seven days).
const WEEKLY_INTERVAL_MS: i64 = 7 * 24 * 60 * 60 * 1000;
/// Maximum infra/skill files listed in the snapshot.
const MAX_INFRA_FILES: usize = 45;
/// Maximum recent failures/refusals included in the snapshot.
const MAX_FAILURES: usize = 12;
/// Maximum characters kept from a failing command's output or error.
const MAX_SNIPPET: usize = 180;
/// Instruction or skill files above this many lines are flagged as context-heavy.
const HEAVY_LINES: usize = 250;
/// Folders skipped when scanning for project infra/skills.
const SKIP_DIRS: [&str; 14] = [
    ".git", "node_modules", "target", "dist", "build", ".next", ".nuxt", ".venv", "venv",
    "__pycache__", ".cache", "coverage", ".turbo", ".terraform",
];

fn default_cadence() -> String {
    "manual".into()
}

/// Per-bot Dev/Infra auditor settings and last-run state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    /// Turned on for this project (off by default: each audit spends plan tokens).
    #[serde(default)]
    pub enabled: bool,
    /// Optional CLI id ("claude", "codex", "cursor", "antigravity", "opencode") that runs the audit.
    /// None uses the bot's own CLI in a fresh session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    /// "manual", "on_change" or "weekly".
    #[serde(default = "default_cadence")]
    pub cadence: String,
    /// Dedicated thread holding this project's audit reports.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread: Option<String>,
    /// Timestamp (ms) of the last audit run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_ms: Option<i64>,
    /// Infra/skills fingerprint at the last audit run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<u64>,
    /// Timestamp (ms) when a scheduled check skipped because nothing in infra/skills changed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skipped_ms: Option<i64>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            enabled: false,
            agent: None,
            cadence: default_cadence(),
            thread: None,
            last_ms: None,
            fingerprint: None,
            skipped_ms: None,
        }
    }
}

impl Config {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// Concise, accurate runtime contract embedded directly from raiox's own behavior.
/// Keeps the auditor grounded in how raiox really executes commands, drafts, routines and skills.
const RAIOX_RUNTIME_SPEC: &str = "\
raiox runtime contract (ground truth):
1. RAIOX-RUN: commands execute directly via argv (shell_words), with NO shell.
   - Refused before asking the user if >300 chars, or containing pipes `|`, redirections `<`/`>`, `;`, `&&`, `$`, backticks, newlines, unbalanced quotes.
   - Blocked programs: sh, bash, zsh, fish, dash, env, printenv, sudo, su, osascript, cmd, powershell, pwsh, wscript, cscript, etc.
   - Blocked inline eval flags: node -e/--eval/-p, python/python3/py -c, deno eval, ruby -e, perl -e, php -r.
   - Refused if any argument names a secret file (.env*, *.pem, *.key, credentials.json, id_rsa*, .npmrc).
   - Timeout: 120 s; at most 2 RAIOX-RUN: lines per answer; output capped at last 12 KB with secret values redacted.
2. Auto-run ('Runs without asking'):
   - Runs without a click ONLY: readers (ls, pwd, cat, head, tail, wc, grep, rg, tree, du, which, file, stat, diff, find without -delete/-exec), read-only git (status, log, diff, show, rev-parse, ls-files, blame, describe, shortlog, grep, remote -v, branch list), version checks, locked installs (npm/pnpm/yarn/bun install|ci with no package args), check scripts (test, lint, typecheck, type-check, check, build, tsc, eslint without --fix, vitest/jest/pytest without -u, cargo test/check/clippy/build/fmt --check), and `python3 rsi.py <subcommand>` ONLY inside folders with a `.raiox-rsi` marker and without `--root`.
   - Any skill, routine, or CLAUDE.md/AGENTS.md instructing bots to use pipes, `&&`, `bash -c`, `python3 -c`, or non-safe scripts will stall automatic loops!
3. Protocol markers (bot_marks.rs):
   - RAIOX-PROPOSAL: <one-line change> -> shows Approve button; edits happen in a draft copy (no shell, no .git, no node_modules/target, no secret files). Only CLAUDE.md, CLAUDE.local.md, AGENTS.md and .cursorrules are copied even if gitignored.
   - RAIOX-TASK: <short title> -> adds up to 3 deduplicated tasks to the Desk board.
   - RAIOX-SHOT: </path | local URL | simulator> -> up to 2 screenshots.
   - RAIOX-TO <project>: <note> -> only delivered to wired projects.
   - Announcing 'I will run X next' without emitting `RAIOX-RUN: X` in the same answer leaves the loop stalled!
4. Routines, Board & Wiki:
   - Routines run on a local schedule while raiox is open; `only_if_changed` skips runs when the project fingerprint is unchanged.
   - Always-on Wiki pages go into every turn (4,000 char budget total); matching pages need >=2 shared words (max 2 pages, 2,500 chars each).
   - Oversized CLAUDE.md, AGENTS.md or SKILL.md files inflate context re-reading on every turn.";

/// One discovered infra, script, workflow, instruction or skill file in the project.
#[derive(Debug, Clone, PartialEq, Eq)]
struct InfraFile {
    rel: String,
    kind: &'static str,
    bytes: u64,
    mtime_ms: i64,
    lines: usize,
}

fn is_secret_rel(rel: &str) -> bool {
    Path::new(rel)
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(raiox_core::secrets::is_secret_file)
}

fn stat_file(root: &Path, rel: &str, kind: &'static str) -> Option<InfraFile> {
    if is_secret_rel(rel) {
        return None;
    }
    let path = root.join(rel);
    let meta = fs::symlink_metadata(&path).ok()?;
    if !meta.is_file() {
        return None;
    }
    let mtime_ms = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_millis() as i64);
    let lines = if meta.len() <= 512 * 1024 {
        fs::read_to_string(&path).map_or(0, |text| text.lines().count())
    } else {
        0
    };
    Some(InfraFile { rel: rel.to_string(), kind, bytes: meta.len(), mtime_ms, lines })
}

fn scan_subtree(root: &Path, sub: &str, kind: &'static str, max_depth: usize, only_ext: Option<&[&str]>, out: &mut Vec<InfraFile>) {
    let start = root.join(sub);
    if !start.is_dir() {
        return;
    }
    let mut stack = vec![(start, 0usize)];
    while let Some((dir, depth)) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = fs::symlink_metadata(&path) else { continue };
            let name = entry.file_name().to_string_lossy().into_owned();
            if meta.is_dir() {
                if depth < max_depth && !SKIP_DIRS.contains(&name.as_str()) {
                    stack.push((path, depth + 1));
                }
                continue;
            }
            if !meta.is_file() {
                continue;
            }
            if let Some(exts) = only_ext {
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
                if !exts.contains(&ext.as_str()) {
                    continue;
                }
            }
            let Ok(rel_path) = path.strip_prefix(root) else { continue };
            let rel = rel_path.to_string_lossy().replace(std::path::MAIN_SEPARATOR, "/");
            if let Some(file) = stat_file(root, &rel, kind) {
                out.push(file);
            }
        }
    }
}

/// Discovers the project's agent instructions, skills, workflows, scripts and manifests (never secrets).
fn discover_infra_files(dir: &Path) -> Vec<InfraFile> {
    let mut files = Vec::new();
    for rel in ["CLAUDE.md", "CLAUDE.local.md", "AGENTS.md", ".cursorrules", ".github/copilot-instructions.md"] {
        if let Some(f) = stat_file(dir, rel, "instructions") {
            files.push(f);
        }
    }
    for sub in [".claude/skills", ".agents/skills", "skills"] {
        scan_subtree(dir, sub, "skill", 3, Some(&["md", "py", "sh", "ts", "js"]), &mut files);
    }
    scan_subtree(dir, ".github/workflows", "workflow", 1, Some(&["yml", "yaml"]), &mut files);
    scan_subtree(dir, "scripts", "script", 2, None, &mut files);
    for (rel, kind) in [
        ("Makefile", "script"),
        ("Justfile", "script"),
        ("Procfile", "script"),
        ("rsi.py", "script"),
        (".raiox-rsi", "marker"),
        ("package.json", "manifest"),
        ("Cargo.toml", "manifest"),
        ("pyproject.toml", "manifest"),
    ] {
        if let Some(f) = stat_file(dir, rel, kind) {
            files.push(f);
        }
    }
    files.sort_by(|a, b| a.rel.cmp(&b.rel));
    files.dedup_by(|a, b| a.rel == b.rel);
    files
}

/// Deterministic fingerprint of a project's infra surface (scripts, workflows, skills, instructions,
/// manifests, routines and allowed commands). Unrelated source changes do not alter it.
pub fn infra_fingerprint(state_dir: &Path, bot: &Bot) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for file in discover_infra_files(&bot.dir) {
        (file.rel, file.bytes, file.mtime_ms).hash(&mut hasher);
    }
    let peers: Vec<Bot> = bots::list(state_dir).into_iter().filter(|b| b.dir == bot.dir).collect();
    let peer_ids: BTreeSet<String> = peers.iter().map(|b| b.id.clone()).collect();
    for peer in &peers {
        (&peer.id, &peer.agent, &peer.allowed, peer.grants.auto_run, peer.grants.auto_edit, peer.grants.web, peer.grants.mcp).hash(&mut hasher);
    }
    for r in crate::routines::list(state_dir) {
        if peer_ids.contains(&r.bot) {
            (r.id, r.bot, r.text, r.days, r.hour, r.minute, r.enabled, r.only_if_changed).hash(&mut hasher);
        }
    }
    hasher.finish()
}

fn compact_snippet(text: &str) -> String {
    let one_line: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.chars().count() <= MAX_SNIPPET {
        one_line
    } else {
        format!("{}…", one_line.chars().take(MAX_SNIPPET).collect::<String>())
    }
}

/// Recent real command refusals, non-zero command exits, timeouts and turn failures across all bots
/// in this project folder (excluding the audit thread itself so audits do not echo themselves).
fn recent_failures(state_dir: &Path, peers: &[Bot], audit_thread: Option<&str>) -> Vec<String> {
    let mut events: Vec<(i64, String)> = Vec::new();
    for peer in peers {
        let mut sources = vec![(threads::MAIN.to_string(), "main".to_string())];
        for th in threads::list(state_dir, &peer.id) {
            if audit_thread.is_some_and(|a| a == th.id) || th.title == THREAD_TITLE {
                continue;
            }
            sources.push((th.id, th.title));
        }
        for (thread_id, title) in sources {
            let messages = room::read_messages(&threads::chat_file(state_dir, &peer.id, &thread_id));
            for m in messages.iter().rev().take(40) {
                match m.kind.as_str() {
                    "answer" => {
                        if let Some(runs) = m.data.pointer("/reply/run").and_then(Value::as_array) {
                            for req in runs {
                                if let (Some(cmd), Some(why)) = (req.get("command").and_then(Value::as_str), req.get("refused").and_then(Value::as_str)) {
                                    events.push((m.ms, format!("[refused:{why}] ({title}) `{cmd}`")));
                                }
                            }
                        }
                    }
                    "ran" => {
                        let ran = &m.data["ran"];
                        let cmd = ran["command"].as_str().unwrap_or("");
                        let timed_out = ran["timed_out"].as_bool().unwrap_or(false);
                        let code = ran["code"].as_i64();
                        if timed_out || (code.is_some() && code != Some(0)) {
                            let status = if timed_out { "timeout".to_string() } else { format!("exit:{}", code.unwrap_or(-1)) };
                            let tail = compact_snippet(ran["output"].as_str().unwrap_or(""));
                            events.push((m.ms, format!("[{status}] ({title}) `{cmd}` -> {tail}")));
                        }
                    }
                    "failed" => {
                        let err = compact_snippet(m.data["error"].as_str().unwrap_or(""));
                        if !err.is_empty() {
                            events.push((m.ms, format!("[turn failed] ({title}) {err}")));
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    events.sort_by_key(|(ms, _)| std::cmp::Reverse(*ms));
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for (_, line) in events {
        if seen.insert(line.clone()) {
            out.push(line);
            if out.len() >= MAX_FAILURES {
                break;
            }
        }
    }
    out
}

/// Reads only the `scripts` map from `package.json` (never touching `.env` or secret values).
fn package_scripts(dir: &Path) -> Vec<(String, String)> {
    let Ok(raw) = fs::read_to_string(dir.join("package.json")) else { return Vec::new() };
    let Ok(val) = serde_json::from_str::<Value>(&raw) else { return Vec::new() };
    let Some(map) = val.get("scripts").and_then(Value::as_object) else { return Vec::new() };
    map.iter()
        .filter_map(|(k, v)| v.as_str().map(|cmd| (k.clone(), compact_snippet(cmd))))
        .take(25)
        .collect()
}

/// Builds a concise, zero-secret snapshot of the project's agent plumbing and recent failures.
pub fn snapshot(state_dir: &Path, bot: &Bot) -> String {
    let peers: Vec<Bot> = bots::list(state_dir).into_iter().filter(|b| b.dir == bot.dir).collect();
    let peer_ids: BTreeSet<String> = peers.iter().map(|b| b.id.clone()).collect();
    let files = discover_infra_files(&bot.dir);
    let mut sections: Vec<String> = Vec::new();

    // 1. Bots in this project
    let bot_lines: Vec<String> = peers
        .iter()
        .map(|b| {
            let allowed = if b.allowed.is_empty() { "none".to_string() } else { b.allowed.join(", ") };
            format!(
                "- {} (cli={}, model={}, auto_run={}, auto_edit={}, web={}, mcp={}, allowed=[{}])",
                b.persona.as_ref().map_or(b.agent.as_str(), |p| p.name.as_str()),
                b.agent,
                b.model.as_deref().unwrap_or("default"),
                b.grants.auto_run,
                b.grants.auto_edit,
                b.grants.web,
                b.grants.mcp,
                allowed,
            )
        })
        .collect();
    sections.push(format!("Bots in project \"{}\":\n{}", bot.name, bot_lines.join("\n")));

    // 2. Discovered infra, skill & instruction files
    if files.is_empty() {
        sections.push("Infra/agent files found: none.".into());
    } else {
        let mut lines: Vec<String> = files
            .iter()
            .take(MAX_INFRA_FILES)
            .map(|f| {
                let heavy = if (f.kind == "instructions" || f.kind == "skill") && f.lines >= HEAVY_LINES { " [heavy]" } else { "" };
                format!("- [{}] {} ({} lines){heavy}", f.kind, f.rel, f.lines)
            })
            .collect();
        if files.len() > MAX_INFRA_FILES {
            lines.push(format!("- … and {} more files", files.len() - MAX_INFRA_FILES));
        }
        sections.push(format!("Discovered infra, workflow, instruction and skill files:\n{}", lines.join("\n")));
    }

    // 3. package.json scripts
    let pkg = package_scripts(&bot.dir);
    if !pkg.is_empty() {
        let list: Vec<String> = pkg.into_iter().map(|(k, v)| format!("- {k}: {v}")).collect();
        sections.push(format!("package.json scripts:\n{}", list.join("\n")));
    }

    // 4. Routines configured for this project
    let routines: Vec<_> = crate::routines::list(state_dir).into_iter().filter(|r| peer_ids.contains(&r.bot)).collect();
    if !routines.is_empty() {
        let list: Vec<String> = routines
            .iter()
            .map(|r| {
                let state = if !r.enabled {
                    "disabled"
                } else if r.skipped_ms.is_some() && r.skipped_ms == r.last_ms {
                    "last skipped"
                } else if r.last_ms.is_some() {
                    "active"
                } else {
                    "never ran"
                };
                format!("- [{state}, only_if_changed={}] {:02}:{:02} days={:?}: {}", r.only_if_changed, r.hour, r.minute, r.days, compact_snippet(&r.text))
            })
            .collect();
        sections.push(format!("Configured raiox routines:\n{}", list.join("\n")));
    }

    // 5. Open board tasks for this project
    let board = crate::tasks::state(state_dir);
    let open_tasks: Vec<String> = board["tasks"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|t| t["bot"].as_str().is_some_and(|id| peer_ids.contains(id)) && t["status"].as_str() != Some("done"))
        .take(15)
        .map(|t| format!("- [{}] {}", t["status"].as_str().unwrap_or("?"), t["title"].as_str().unwrap_or("")))
        .collect();
    if !open_tasks.is_empty() {
        sections.push(format!("Open board tasks for this project:\n{}", open_tasks.join("\n")));
    }

    // 6. Always-on wiki pages (shows context weight)
    let wiki = crate::wiki::list(state_dir);
    let always_pages: Vec<String> = wiki["pages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|p| p["always"].as_bool() == Some(true))
        .take(10)
        .map(|p| format!("- \"{}\" ({} chars, {} reads this week)", p["title"].as_str().unwrap_or(""), p["chars"].as_u64().unwrap_or(0), p["reads"].as_u64().unwrap_or(0)))
        .collect();
    if !always_pages.is_empty() {
        sections.push(format!("Always-on team Wiki pages:\n{}", always_pages.join("\n")));
    }

    // 7. Recent real failures & command refusals
    let failures = recent_failures(state_dir, &peers, bot.auditor.thread.as_deref());
    if failures.is_empty() {
        sections.push("Recent failures / refused commands in project chats: none recorded.".into());
    } else {
        sections.push(format!("Recent real failures & refused commands:\n{}", failures.iter().map(|l| format!("- {l}")).collect::<Vec<_>>().join("\n")));
    }

    sections.join("\n\n")
}

/// Builds the full Dev/Infra audit prompt for one run.
pub fn build_prompt(state_dir: &Path, bot: &Bot, lang: Option<&str>) -> String {
    let settings_lang = crate::settings::Settings::load(state_dir).language;
    let is_pt = lang
        .or(settings_lang.as_deref())
        .is_some_and(|l| l.to_ascii_lowercase().starts_with("pt"));
    let language = if is_pt { "Portuguese (pt-BR)" } else { "English" };
    let snap = snapshot(state_dir, bot);
    format!(
        "You are the built-in Dev/Infra Auditor for project \"{}\" inside raiox.\n\
         Your job: audit scripts, CI workflows, raiox routines, project skills (SKILL.md), and agent instruction files \
         (CLAUDE.md, AGENTS.md, .cursorrules) to find real bugs, broken commands, raiox runtime incompatibilities, \
         stalled automation loops, security leaks, or measurable context bloat that the working agents missed.\n\n\
         Strict rules:\n\
         - Scope ONLY to infra, scripts, workflows, routines, skills, and agent instructions. Ignore unrelated product logic unless a script breaks on it.\n\
         - Evidence first: NEVER invent issues and NEVER suggest cosmetic rewrites. Every finding must be verified by reading the exact file, checking the raiox contract below, or proving it with a safe read/check `RAIOX-RUN:` command.\n\
         - Never read, print, or quote secrets files (.env*, keys, credentials).\n\
         - Reply in {language}, concisely.\n\n\
         {RAIOX_RUNTIME_SPEC}\n\n\
         === LIVE PROJECT SNAPSHOT ===\n\
         {snap}\n\
         === END SNAPSHOT ===\n\n\
         Audit checklist:\n\
         1. Inspect the listed instruction files, skills, workflows, and scripts (start with any files tied to the recent failures above or marked [heavy]).\n\
         2. Check for commands in skills/instructions/routines that violate raiox's shell-less RAIOX-RUN or safe_commands rules (pipes, `&&`, `bash -c`, `python3 -c`, wrong flags, missing files, non-portable commands).\n\
         3. Check for contradictory or stale instructions across CLAUDE.md, AGENTS.md, skills, routines, and package/build scripts.\n\
         4. Where helpful, run a safe read/check command (`RAIOX-RUN:`) to prove a script/test/lint failure instead of assuming.\n\
         5. Report verified findings grouped by severity (**Critical**, **Warning**, **Optimization**), each with exact `file:line` (or routine/skill), proof, and minimal fix.\n\
         6. If verified file fixes are needed in this project, end with one clear `RAIOX-PROPOSAL:` line so the person can review the diff in a draft and apply/undo safely. Optionally add up to 3 `RAIOX-TASK:` lines for separate follow-ups. If everything checked out clean, say so clearly and emit NO proposal.",
        bot.name,
    )
}

/// Which CLI executes the audit: the configured auditor CLI if still valid and enabled, else the bot's own CLI.
pub fn effective_agent(state_dir: &Path, bot: &Bot) -> String {
    let settings = crate::settings::Settings::load(state_dir);
    bot.auditor
        .agent
        .as_deref()
        .filter(|a| crate::connections::bot_agents().any(|k| k == *a) && settings.is_enabled(a))
        .unwrap_or(&bot.agent)
        .to_string()
}

/// Whether `thread` is the Dev/Infra Audit conversation of `bot_id` (used to block silent auto-edits).
pub fn is_audit_thread(state_dir: &Path, bot_id: &str, thread: &str) -> bool {
    if thread == threads::MAIN {
        return false;
    }
    if bots::find(state_dir, bot_id).is_ok_and(|b| b.auditor.thread.as_deref() == Some(thread)) {
        return true;
    }
    threads::list(state_dir, bot_id).into_iter().any(|t| t.id == thread && t.title == THREAD_TITLE)
}

/// Updates this bot's Dev/Infra auditor configuration (`enabled`, `agent`, `cadence`).
pub fn configure(state_dir: &Path, id: &str, input: &Value) -> Result<()> {
    let bot = bots::find(state_dir, id)?;
    let mut cfg = bot.auditor.clone();
    if let Some(enabled) = input.get("enabled").and_then(Value::as_bool) {
        cfg.enabled = enabled;
    }
    if let Some(agent_val) = input.get("agent") {
        let next = agent_val.as_str().map(str::trim).filter(|a| !a.is_empty());
        if let Some(a) = next {
            if !crate::connections::bot_agents().any(|k| k == a) {
                bail!("raiox cannot run {a} as an auditor");
            }
            let settings = crate::settings::Settings::load(state_dir);
            if !settings.is_enabled(a) {
                bail!("turn on {a} in Settings before picking it as auditor");
            }
            cfg.agent = (a != bot.agent).then(|| a.to_string());
        } else {
            cfg.agent = None;
        }
    }
    if let Some(cadence) = input.get("cadence").and_then(Value::as_str) {
        if !CADENCES.contains(&cadence) {
            bail!("unknown audit cadence {cadence}");
        }
        cfg.cadence = cadence.to_string();
    }
    bots::update(state_dir, |list| {
        Ok(list.into_iter().map(|b| if b.id == id { Bot { auditor: cfg.clone(), ..b } } else { b }).collect())
    })
}

fn ensure_thread(state_dir: &Path, bot: &Bot) -> Result<String> {
    let existing = threads::list(state_dir, &bot.id);
    if let Some(id) = bot.auditor.thread.as_deref().filter(|id| existing.iter().any(|t| t.id == *id)) {
        return Ok(id.to_string());
    }
    if let Some(found) = existing.into_iter().find(|t| t.title == THREAD_TITLE) {
        return Ok(found.id);
    }
    Ok(threads::create(state_dir, &bot.id, THREAD_TITLE)?.id)
}

/// Starts a Dev/Infra audit immediately in the bot's dedicated audit thread; returns that thread id.
pub fn run_now(state_dir: &Path, jobs: &Jobs, id: &str, lang: Option<&str>) -> Result<String> {
    let bot = bots::find(state_dir, id)?;
    if !bot.auditor.enabled {
        bail!("turn on the Dev/Infra auditor for this project first");
    }
    let thread = ensure_thread(state_dir, &bot)?;
    let audit_agent = effective_agent(state_dir, &bot);
    let fp = infra_fingerprint(state_dir, &bot);
    let prompt = build_prompt(state_dir, &bot, lang);
    let shown_text = if lang
        .or(crate::settings::Settings::load(state_dir).language.as_deref())
        .is_some_and(|l| l.to_ascii_lowercase().starts_with("pt"))
    {
        "Auditoria Dev/Infra: scripts, workflows, rotinas e skills"
    } else {
        "Dev/Infra audit: scripts, workflows, routines and skills"
    };
    let shown = json!({ "text": shown_text, "audit": true, "auditor_agent": audit_agent });
    bots::run_audit_turn(state_dir, jobs, id, &thread, &audit_agent, &prompt, shown)?;
    let now = room::now_ms();
    let thread_saved = thread.clone();
    let _ = bots::update(state_dir, |list| {
        Ok(list
            .into_iter()
            .map(|b| {
                if b.id != id {
                    return b;
                }
                Bot {
                    auditor: Config {
                        thread: Some(thread_saved.clone()),
                        last_ms: Some(now),
                        fingerprint: Some(fp),
                        skipped_ms: None,
                        ..b.auditor
                    },
                    ..b
                }
            })
            .collect())
    });
    Ok(thread)
}

/// Whether a scheduled audit should run or check at `now_ms`.
fn is_due(cfg: &Config, now_ms: i64) -> bool {
    if !cfg.enabled || cfg.cadence == "manual" {
        return false;
    }
    let interval = match cfg.cadence.as_str() {
        "on_change" => ON_CHANGE_INTERVAL_MS,
        "weekly" => WEEKLY_INTERVAL_MS,
        _ => return false,
    };
    let latest = cfg.last_ms.unwrap_or(0).max(cfg.skipped_ms.unwrap_or(0));
    now_ms - latest >= interval
}

/// Scheduled Dev/Infra audit check, called on the routines clock every 30 s.
/// Respects the Desk's free-hours window when set, skips when nothing in infra/skills changed
/// under `on_change`, and never runs while the bot is busy.
pub fn tick(state_dir: &Path, jobs: &Jobs) {
    if crate::tasks::window(state_dir).is_some_and(|w| !w.holds(chrono::Timelike::hour(&chrono::Local::now()))) {
        return;
    }
    let now = room::now_ms();
    for bot in bots::list(state_dir) {
        if !is_due(&bot.auditor, now) || jobs.is_busy(&bot.id) {
            continue;
        }
        let fp = infra_fingerprint(state_dir, &bot);
        if bot.auditor.cadence == "on_change" && bot.auditor.fingerprint == Some(fp) {
            let _ = bots::update(state_dir, |list| {
                Ok(list
                    .into_iter()
                    .map(|b| if b.id == bot.id { Bot { auditor: Config { skipped_ms: Some(now), ..b.auditor }, ..b } } else { b })
                    .collect())
            });
            continue;
        }
        if run_now(state_dir, jobs, &bot.id, None).is_err() {
            // Stamp skipped_ms so a budget-blocked or misconfigured bot does not retry every 30 s.
            let _ = bots::update(state_dir, |list| {
                Ok(list
                    .into_iter()
                    .map(|b| if b.id == bot.id { Bot { auditor: Config { skipped_ms: Some(now), ..b.auditor }, ..b } } else { b })
                    .collect())
            });
        }
    }
}

/// Auditor state sent to the panel per bot.
pub fn state(state_dir: &Path, bot: &Bot) -> Value {
    json!({
        "enabled": bot.auditor.enabled,
        "agent": bot.auditor.agent,
        "effective_agent": effective_agent(state_dir, bot),
        "cadence": bot.auditor.cadence,
        "thread": bot.auditor.thread,
        "last_ms": bot.auditor.last_ms,
        "skipped_ms": bot.auditor.skipped_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("raiox-auditor-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn configure_validates_agent_and_cadence_and_defaults_to_off() {
        let home = scratch("cfg");
        fs::create_dir_all(home.join("Projects/app")).unwrap();
        let state = home.join(".state");
        let bot = bots::activate(&state, &home, "Projects/app", "claude", None).unwrap();
        assert!(!bot.auditor.enabled);
        assert_eq!(bot.auditor.cadence, "manual");

        // Enable another CLI in settings so it can be chosen as auditor.
        crate::settings::update(&state, &json!({ "enabled": { "claude": true, "codex": true, "cursor": false } })).unwrap();
        configure(&state, &bot.id, &json!({ "enabled": true, "agent": "codex", "cadence": "on_change" })).unwrap();
        let updated = bots::find(&state, &bot.id).unwrap();
        assert!(updated.auditor.enabled);
        assert_eq!(updated.auditor.agent.as_deref(), Some("codex"));
        assert_eq!(updated.auditor.cadence, "on_change");
        assert_eq!(effective_agent(&state, &updated), "codex");

        // Picking the bot's own CLI stores None ("same AI").
        configure(&state, &bot.id, &json!({ "agent": "claude" })).unwrap();
        assert_eq!(bots::find(&state, &bot.id).unwrap().auditor.agent, None);

        // Disabled CLI or invalid cadence is rejected.
        assert!(configure(&state, &bot.id, &json!({ "agent": "cursor" })).is_err());
        assert!(configure(&state, &bot.id, &json!({ "agent": "unknown" })).is_err());
        assert!(configure(&state, &bot.id, &json!({ "cadence": "every_minute" })).is_err());

        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn infra_fingerprint_ignores_unrelated_source_and_tracks_skills_scripts_and_routines() {
        let home = scratch("fp");
        let project = home.join("Projects/app");
        fs::create_dir_all(project.join("src")).unwrap();
        fs::create_dir_all(project.join(".claude/skills/deploy")).unwrap();
        fs::create_dir_all(project.join("scripts")).unwrap();
        fs::write(project.join("src/index.ts"), "export const a = 1\n").unwrap();
        fs::write(project.join("CLAUDE.md"), "# Rules\n").unwrap();
        fs::write(project.join(".claude/skills/deploy/SKILL.md"), "# Deploy\n").unwrap();
        fs::write(project.join("scripts/check.sh"), "echo ok\n").unwrap();
        fs::write(project.join(".env"), "API_KEY=secret\n").unwrap();

        let state = home.join(".state");
        let bot = bots::activate(&state, &home, "Projects/app", "claude", None).unwrap();
        let fp1 = infra_fingerprint(&state, &bot);

        // Changing app source or .env does NOT change the infra fingerprint.
        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(project.join("src/index.ts"), "export const a = 2\n").unwrap();
        fs::write(project.join(".env"), "API_KEY=other\n").unwrap();
        assert_eq!(infra_fingerprint(&state, &bot), fp1, "unrelated code and secrets stay out of infra fingerprint");

        // Changing a skill DOES change the infra fingerprint.
        fs::write(project.join(".claude/skills/deploy/SKILL.md"), "# Deploy\nrun pnpm test\n").unwrap();
        let fp2 = infra_fingerprint(&state, &bot);
        assert_ne!(fp2, fp1, "skill edit must change infra fingerprint");

        // Adding a routine also changes the infra fingerprint.
        crate::routines::create(&state, &bot.id, "check builds", &[0, 2, 4], 9, 0, true).unwrap();
        let fp3 = infra_fingerprint(&state, &bot);
        assert_ne!(fp3, fp2, "routine change must change infra fingerprint");

        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn snapshot_collects_infra_files_and_failures_without_leaking_secrets() {
        let home = scratch("snap");
        let project = home.join("Projects/demo");
        fs::create_dir_all(project.join(".claude/skills/lint")).unwrap();
        fs::create_dir_all(project.join(".github/workflows")).unwrap();
        fs::write(project.join("AGENTS.md"), "Run `bash scripts/test.sh`\n").unwrap();
        fs::write(project.join(".claude/skills/lint/SKILL.md"), "# Lint skill\n").unwrap();
        fs::write(project.join(".github/workflows/ci.yml"), "name: CI\n").unwrap();
        fs::write(project.join("package.json"), r#"{"scripts":{"test":"vitest run","build":"vite build"}}"#).unwrap();
        fs::write(project.join(".env"), "SUPER_SECRET_TOKEN=never_show_me\n").unwrap();

        let state = home.join(".state");
        let bot = bots::activate(&state, &home, "Projects/demo", "claude", None).unwrap();
        // Record a refused RAIOX-RUN and a failing command in the main chat.
        room::append_messages(
            &threads::chat_file(&state, &bot.id, threads::MAIN),
            vec![
                room::Message {
                    ms: 100,
                    author: "bot".into(),
                    kind: "answer".into(),
                    data: json!({ "reply": { "text": "trying", "run": [{ "command": "git log | head", "refused": "shell" }] } }),
                },
                room::Message {
                    ms: 200,
                    author: "you".into(),
                    kind: "ran".into(),
                    data: json!({ "ran": { "command": "npm test", "code": 1, "output": "AssertionError in test.ts", "timed_out": false } }),
                },
            ],
        )
        .unwrap();

        let snap = snapshot(&state, &bot);
        assert!(snap.contains("AGENTS.md") && snap.contains(".claude/skills/lint/SKILL.md") && snap.contains(".github/workflows/ci.yml"));
        assert!(snap.contains("vitest run"));
        assert!(snap.contains("[refused:shell]") && snap.contains("git log | head"));
        assert!(snap.contains("[exit:1]") && snap.contains("AssertionError"));
        assert!(!snap.contains("SUPER_SECRET_TOKEN") && !snap.contains("never_show_me") && !snap.contains(".env"));

        let prompt = build_prompt(&state, &bot, Some("pt-BR"));
        assert!(prompt.contains("Portuguese (pt-BR)") && prompt.contains("RAIOX-RUN:") && prompt.contains("RAIOX-PROPOSAL:"));

        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn schedule_respects_cadence_and_intervals() {
        let now = 10_000_000_000i64;
        assert!(!is_due(&Config::default(), now), "disabled auditor never runs");
        assert!(!is_due(&Config { enabled: true, cadence: "manual".into(), ..Config::default() }, now));
        assert!(is_due(&Config { enabled: true, cadence: "on_change".into(), ..Config::default() }, now));
        assert!(!is_due(&Config { enabled: true, cadence: "on_change".into(), last_ms: Some(now - 60_000), ..Config::default() }, now));
        assert!(!is_due(&Config { enabled: true, cadence: "on_change".into(), skipped_ms: Some(now - 60_000), ..Config::default() }, now));
        assert!(is_due(&Config { enabled: true, cadence: "on_change".into(), last_ms: Some(now - ON_CHANGE_INTERVAL_MS), ..Config::default() }, now));
        assert!(!is_due(&Config { enabled: true, cadence: "weekly".into(), last_ms: Some(now - ON_CHANGE_INTERVAL_MS), ..Config::default() }, now));
        assert!(is_due(&Config { enabled: true, cadence: "weekly".into(), last_ms: Some(now - WEEKLY_INTERVAL_MS), ..Config::default() }, now));
    }

    #[test]
    fn old_bots_json_deserializes_with_auditor_off() {
        let raw = json!({ "id": "b1", "agent": "claude", "dir": "/tmp/p", "name": "p", "created_ms": 1 });
        let bot: Bot = serde_json::from_value(raw).unwrap();
        assert!(!bot.auditor.enabled && bot.auditor.agent.is_none() && bot.auditor.cadence == "manual");
    }

    #[test]
    fn run_now_refuses_when_auditor_is_off() {
        let home = scratch("off");
        fs::create_dir_all(home.join("Projects/app")).unwrap();
        let state = home.join(".state");
        let bot = bots::activate(&state, &home, "Projects/app", "claude", None).unwrap();
        assert!(run_now(&state, &Jobs::default(), &bot.id, None).is_err());
        let _ = fs::remove_dir_all(&home);
    }
}
