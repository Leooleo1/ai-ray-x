//! Edits in a draft. When the person approves a proposal, the AI edits a copy
//! of the project, the panel shows what changed file by file, and only Apply
//! writes it to the project, after a backup that Undo puts back. Apply never
//! overwrites a file that changed in the project since the copy, and never
//! writes outside the project.
//!
//! The copy follows the project's .gitignore and leaves out secrets files,
//! dependency and build folders, and nested repositories (worktrees,
//! submodules). Each bot edits in one working copy, always at the same place
//! (`rascunhos/<bot>/copia/<project path under home>`), so its CLI keeps one
//! history folder for all its drafts. What a draft changed is kept apart in
//! `rascunhos/<bot>/<draft>/`, and the working copy goes when the edit ends.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::UNIX_EPOCH;

use anyhow::{anyhow, bail, Context, Result};
use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};
use similar::{ChangeTag, TextDiff};

use crate::room;

/// Folders a copy leaves out even without a .gitignore: version control,
/// dependencies, build output and tool caches.
const SKIP_DIRS: [&str; 21] = [
    ".git", "node_modules", "target", "dist", "build", ".next", ".nuxt", ".svelte-kit", ".turbo", ".venv", "venv",
    "__pycache__", ".cache", "coverage", ".gradle", "Pods", "DerivedData", ".terraform", ".wrangler", ".vercel", ".output",
];
/// The AI's instructions for the project: copied even when the .gitignore leaves them out.
const AGENT_FILES: [&str; 4] = ["CLAUDE.md", "CLAUDE.local.md", "AGENTS.md", ".cursorrules"];
/// A project with more files than this, after those cuts, is too big to copy for a draft.
const MAX_FILES: usize = 50_000;
/// Bigger files stay out of the copy: media and data dumps, not code to edit.
const MAX_COPY_BYTES: u64 = 25 * 1024 * 1024;
/// Bigger files are applied but not diffed line by line.
const MAX_DIFF_BYTES: usize = 512 * 1024;
/// A file's diff shows at most this many lines; the panel says when it was cut.
const MAX_DIFF_LINES: usize = 400;
/// Drafts and their backups are kept this long for Undo.
const KEEP_DAYS: i64 = 14;
const DAY_MS: i64 = 86_400_000;
const WORK_DIR: &str = "copia";
const META_FILE: &str = "rascunho.json";

/// Apply, Undo and Discard run one at a time, so a double click cannot apply a draft twice.
static ACTIONS: Mutex<()> = Mutex::new(());

/// Size and modification time: how a file is known to be untouched.
type Stamp = (u64, i64);

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Modified,
    Added,
    Deleted,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
enum State {
    Open,
    Applied,
    Undone,
    Discarded,
}

impl State {
    fn word(self) -> &'static str {
        match self {
            State::Open => "open",
            State::Applied => "applied",
            State::Undone => "undone",
            State::Discarded => "discarded",
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Change {
    status: Status,
    /// The file in the project when it was copied; none for a new file.
    before: Option<Stamp>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Meta {
    project: PathBuf,
    created_ms: i64,
    state: State,
    changes: BTreeMap<String, Change>,
    /// What Apply wrote, so Undo can tell nothing changed since.
    #[serde(default)]
    written: BTreeMap<String, Stamp>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FileChange {
    pub path: String,
    pub status: Status,
    pub added: usize,
    pub removed: usize,
    pub diff: String,
    pub binary: bool,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Changes {
    pub files: Vec<FileChange>,
    /// Secrets files the AI created in the copy: listed, never applied.
    pub blocked: Vec<String>,
}

/// A bot's working copy of its project, ready for its CLI.
pub struct Workspace {
    pub dir: PathBuf,
    project: PathBuf,
    /// Each copied file: its stamp in the project and in the copy.
    copied: BTreeMap<String, (Stamp, Stamp)>,
}

fn bot_dir(state_dir: &Path, bot: &str) -> Result<PathBuf> {
    if bot.is_empty() || bot.len() > 32 || !bot.bytes().all(|b| b.is_ascii_alphanumeric()) {
        bail!("invalid bot");
    }
    Ok(state_dir.join("rascunhos").join(bot))
}

fn draft_dir(state_dir: &Path, bot: &str, id: &str) -> Result<PathBuf> {
    let valid = id.len() > 1 && id.len() <= 24 && id.starts_with('d') && id[1..].bytes().all(|b| b.is_ascii_hexdigit());
    if !valid {
        bail!("no such draft");
    }
    Ok(bot_dir(state_dir, bot)?.join(id))
}

/// The project's path under the home folder (under the root, outside it), always relative.
fn relative(project: &Path, home: &Path) -> PathBuf {
    let inside = project.strip_prefix(home).unwrap_or(project);
    inside.components().filter(|part| matches!(part, Component::Normal(_))).collect()
}

fn stamp(path: &Path) -> Option<Stamp> {
    let meta = fs::symlink_metadata(path).ok()?;
    let modified = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_millis() as i64;
    Some((meta.len(), modified))
}

fn create_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn is_secret(rel: &str) -> bool {
    Path::new(rel).file_name().and_then(|name| name.to_str()).is_some_and(raiox_core::secrets::is_secret_file)
}

/// Folders a walk never enters: the fixed list, and any folder that is a repository of its own.
fn skipped(entry: &ignore::DirEntry) -> bool {
    let is_dir = entry.file_type().is_some_and(|kind| kind.is_dir());
    is_dir && (SKIP_DIRS.contains(&entry.file_name().to_string_lossy().as_ref()) || entry.path().join(".git").exists())
}

/// Every regular file under `dir`, as sorted relative paths, without the skipped folders and
/// symlinks; with `gitignore`, also without what the folder's .gitignore files leave out.
fn walk(dir: &Path, gitignore: bool) -> Result<Vec<String>> {
    let entries = WalkBuilder::new(dir)
        .standard_filters(gitignore)
        .hidden(false)
        .parents(false)
        .require_git(false)
        .filter_entry(|entry| entry.depth() == 0 || !skipped(entry))
        .build();
    let mut found = Vec::new();
    for entry in entries.flatten() {
        if !entry.file_type().is_some_and(|kind| kind.is_file()) {
            continue;
        }
        let Some(rel) = entry.path().strip_prefix(dir).ok().and_then(Path::to_str) else { continue };
        found.push(rel.to_string());
        if found.len() > MAX_FILES {
            bail!("more than {MAX_FILES} files outside .gitignore, dependency and build folders: too big for a draft copy");
        }
    }
    found.sort();
    Ok(found)
}

/// The project's files as they are now, hashed from their paths, sizes and times (the same files a
/// draft copy would take): equal fingerprints mean nothing changed.
pub fn fingerprint(project: &Path) -> Option<u64> {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for rel in walk(project, true).ok()? {
        (rel.as_str(), stamp(&project.join(&rel))).hash(&mut hasher);
    }
    Some(hasher.finish())
}

/// Copies the project into the bot's working copy, replacing what an earlier edit left there.
pub fn prepare(state_dir: &Path, home: &Path, bot: &str, project: &Path) -> Result<Workspace> {
    let rel = relative(project, home);
    if rel.as_os_str().is_empty() {
        bail!("a draft needs a project folder, not the home folder");
    }
    let root = bot_dir(state_dir, bot)?.join(WORK_DIR);
    if let Err(error) = fs::remove_dir_all(&root)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        return Err(error).context("clearing the earlier working copy");
    }
    let dir = root.join(rel);
    match copy_project(project, &dir) {
        Ok(copied) => Ok(Workspace { dir, project: project.to_path_buf(), copied }),
        Err(error) => {
            let _ = fs::remove_dir_all(&root);
            Err(error)
        }
    }
}

fn copy_project(project: &Path, dir: &Path) -> Result<BTreeMap<String, (Stamp, Stamp)>> {
    fs::create_dir_all(dir)?;
    let mut files = walk(project, true)?;
    let instructions: Vec<String> =
        AGENT_FILES.iter().filter(|name| project.join(name).is_file() && !files.iter().any(|f| f == *name)).map(|name| name.to_string()).collect();
    files.extend(instructions);
    let mut copied = BTreeMap::new();
    for rel in files {
        let from = project.join(&rel);
        let Some(original) = stamp(&from) else { continue };
        if is_secret(&rel) || original.0 > MAX_COPY_BYTES {
            continue;
        }
        let to = dir.join(&rel);
        create_parent(&to)?;
        // On APFS this is a copy-on-write clone: instant, and free until the AI changes the file.
        fs::copy(&from, &to).with_context(|| format!("copying {rel}"))?;
        if let Some(copy) = stamp(&to) {
            copied.insert(rel, (original, copy));
        }
    }
    Ok(copied)
}

/// Removes a bot's working copy: after an edit, or when it failed or was stopped.
pub fn abandon(state_dir: &Path, bot: &str) {
    if let Ok(dir) = bot_dir(state_dir, bot) {
        let _ = fs::remove_dir_all(dir.join(WORK_DIR));
    }
}

/// Keeps what the AI changed in its working copy as a draft, then removes the working copy.
/// Gives the draft's id, or none when nothing changed.
pub fn collect(state_dir: &Path, bot: &str, work: &Workspace) -> Result<(Option<String>, Changes)> {
    let kept = keep(state_dir, bot, work);
    abandon(state_dir, bot);
    kept
}

fn keep(state_dir: &Path, bot: &str, work: &Workspace) -> Result<(Option<String>, Changes)> {
    let found = compare(work)?;
    if found.files.is_empty() {
        return Ok((None, found));
    }
    let id = format!("d{:x}", room::now_ms());
    let root = draft_dir(state_dir, bot, &id)?;
    let mut changes = BTreeMap::new();
    for file in &found.files {
        if file.status != Status::Deleted {
            let saved = root.join("novos").join(&file.path);
            create_parent(&saved)?;
            fs::copy(work.dir.join(&file.path), &saved)?;
        }
        let before = work.copied.get(&file.path).map(|(original, _)| *original);
        changes.insert(file.path.clone(), Change { status: file.status, before });
    }
    let meta = Meta { project: work.project.clone(), created_ms: room::now_ms(), state: State::Open, changes, written: BTreeMap::new() };
    write_meta(&root, &meta)?;
    Ok((Some(id), found))
}

/// What the AI changed: copied files it edited or removed (a link in a file's place counts as
/// removed), and new files the .gitignore keeps. New secrets files are listed apart.
fn compare(work: &Workspace) -> Result<Changes> {
    let mut files = Vec::new();
    for (rel, (_, copy)) in &work.copied {
        let path = work.dir.join(rel);
        let is_file = fs::symlink_metadata(&path).is_ok_and(|meta| meta.is_file());
        let before = || fs::read(work.project.join(rel)).unwrap_or_default();
        if !is_file {
            files.push(change(rel, Status::Deleted, &before(), &[]));
        } else if stamp(&path) != Some(*copy) {
            let (old, new) = (before(), fs::read(&path)?);
            if old != new {
                files.push(change(rel, Status::Modified, &old, &new));
            }
        }
    }
    for rel in walk(&work.dir, true)? {
        if !work.copied.contains_key(&rel) && !is_secret(&rel) {
            files.push(change(&rel, Status::Added, &[], &fs::read(work.dir.join(&rel))?));
        }
    }
    let blocked = walk(&work.dir, false)?.into_iter().filter(|rel| is_secret(rel)).collect();
    Ok(Changes { files, blocked })
}

fn is_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8000).any(|b| *b == 0)
}

fn change(rel: &str, status: Status, before: &[u8], after: &[u8]) -> FileChange {
    let base = FileChange { path: rel.to_string(), status, added: 0, removed: 0, diff: String::new(), binary: true, truncated: false };
    if is_binary(before) || is_binary(after) || before.len().max(after.len()) > MAX_DIFF_BYTES {
        return base;
    }
    let (old, new) = (String::from_utf8_lossy(before), String::from_utf8_lossy(after));
    let diff = TextDiff::from_lines(old.as_ref(), new.as_ref());
    let added = diff.iter_all_changes().filter(|item| item.tag() == ChangeTag::Insert).count();
    let removed = diff.iter_all_changes().filter(|item| item.tag() == ChangeTag::Delete).count();
    let full = diff.unified_diff().context_radius(3).to_string();
    let lines: Vec<&str> = full.lines().collect();
    let shown = lines.iter().take(MAX_DIFF_LINES).copied().collect::<Vec<_>>().join("\n");
    FileChange { added, removed, diff: shown, binary: false, truncated: lines.len() > MAX_DIFF_LINES, ..base }
}

fn read_meta(root: &Path) -> Result<Meta> {
    let text = fs::read_to_string(root.join(META_FILE)).context("no such draft")?;
    Ok(serde_json::from_str(&text)?)
}

fn write_meta(root: &Path, meta: &Meta) -> Result<()> {
    fs::create_dir_all(root)?;
    let temp = root.join("rascunho.tmp");
    fs::write(&temp, serde_json::to_string(meta)?)?;
    fs::rename(temp, root.join(META_FILE))?;
    Ok(())
}

fn lock() -> MutexGuard<'static, ()> {
    ACTIONS.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Whether writing `rel` lands inside the project: no symlink on the way leads elsewhere.
fn stays_inside(project: &Path, rel: &str) -> bool {
    let Ok(base) = project.canonicalize() else { return false };
    let target = project.join(rel);
    if fs::symlink_metadata(&target).is_ok_and(|meta| meta.file_type().is_symlink()) {
        return false;
    }
    // The nearest folder that exists, followed through any symlink, has to be in the project.
    let nearest = target.ancestors().skip(1).find(|dir| dir.exists());
    nearest.and_then(|dir| dir.canonicalize().ok()).is_some_and(|real| real.starts_with(&base))
}

/// The project folder, checked to exist and to hold every file the draft touches.
fn usable(meta: &Meta) -> Result<PathBuf> {
    if !meta.project.is_dir() {
        bail!("the project folder is gone: {}", meta.project.display());
    }
    let outside: Vec<&str> = meta.changes.keys().filter(|rel| !stays_inside(&meta.project, rel)).map(String::as_str).collect();
    if !outside.is_empty() {
        bail!("these files would land outside the project through a symlink: {}", outside.join(", "));
    }
    Ok(meta.project.clone())
}

/// Backs up the project's file, then writes or removes it. Gives what was written, if anything.
fn write_one(root: &Path, project: &Path, rel: &str, status: Status) -> Result<Option<Stamp>> {
    let target = project.join(rel);
    if status != Status::Added {
        let saved = root.join("backup").join(rel);
        create_parent(&saved)?;
        fs::copy(&target, &saved)?;
    }
    if status == Status::Deleted {
        fs::remove_file(&target)?;
        return Ok(None);
    }
    create_parent(&target)?;
    fs::copy(root.join("novos").join(rel), &target)?;
    Ok(stamp(&target))
}

/// Puts one file back as it was before Apply: its backup, or no file when Apply created it.
fn restore(root: &Path, project: &Path, rel: &str, status: Status) -> Result<()> {
    let target = project.join(rel);
    if status == Status::Added {
        fs::remove_file(&target)?;
        return Ok(());
    }
    create_parent(&target)?;
    fs::copy(root.join("backup").join(rel), &target)?;
    Ok(())
}

/// Puts back what a failed Apply already wrote, and says so in the error.
fn halfway(root: &Path, project: &Path, done: &[(String, Status)], error: anyhow::Error) -> anyhow::Error {
    let stuck: Vec<&str> = done.iter().rev().filter(|(rel, status)| restore(root, project, rel, *status).is_err()).map(|(rel, _)| rel.as_str()).collect();
    if stuck.is_empty() {
        anyhow!("{error:#}; nothing was applied, the project is as it was")
    } else {
        anyhow!("{error:#}; the draft was half applied and these could not be put back: {}", stuck.join(", "))
    }
}

/// Writes the draft into the project, when every file stays inside it and none changed there
/// since the copy. A failure halfway puts back what was already written.
pub fn apply(state_dir: &Path, bot: &str, id: &str) -> Result<Vec<String>> {
    let _one = lock();
    let root = draft_dir(state_dir, bot, id)?;
    let mut meta = read_meta(&root)?;
    if meta.state != State::Open {
        bail!("this draft was already {}", meta.state.word());
    }
    let project = usable(&meta)?;
    let moved: Vec<&str> = meta.changes.iter().filter(|(rel, change)| stamp(&project.join(rel)) != change.before).map(|(rel, _)| rel.as_str()).collect();
    if !moved.is_empty() {
        bail!("these files changed in the project after the draft was made: {}. Discard the draft and ask again.", moved.join(", "));
    }
    let mut done = Vec::new();
    for (rel, change) in &meta.changes {
        match write_one(&root, &project, rel, change.status) {
            Ok(written) => {
                meta.written.extend(written.map(|at| (rel.clone(), at)));
                done.push((rel.clone(), change.status));
            }
            Err(error) => return Err(halfway(&root, &project, &done, error.context(format!("applying {rel}")))),
        }
    }
    meta.state = State::Applied;
    if let Err(error) = write_meta(&root, &meta) {
        return Err(halfway(&root, &project, &done, error));
    }
    Ok(done.into_iter().map(|(rel, _)| rel).collect())
}

/// Puts back what Apply replaced, when nobody touched those files since.
pub fn undo(state_dir: &Path, bot: &str, id: &str) -> Result<Vec<String>> {
    let _one = lock();
    let root = draft_dir(state_dir, bot, id)?;
    let mut meta = read_meta(&root)?;
    if meta.state != State::Applied {
        bail!("only an applied draft can be undone");
    }
    let project = usable(&meta)?;
    let touched: Vec<&str> = meta.changes.keys().filter(|rel| stamp(&project.join(rel)) != meta.written.get(*rel).copied()).map(String::as_str).collect();
    if !touched.is_empty() {
        bail!("these files changed after the draft was applied: {}. Undo them by hand.", touched.join(", "));
    }
    for (rel, change) in &meta.changes {
        restore(&root, &project, rel, change.status).with_context(|| format!("putting back {rel}"))?;
    }
    meta.state = State::Undone;
    write_meta(&root, &meta)?;
    Ok(meta.changes.into_keys().collect())
}

pub fn discard(state_dir: &Path, bot: &str, id: &str) -> Result<()> {
    let _one = lock();
    let root = draft_dir(state_dir, bot, id)?;
    let mut meta = read_meta(&root)?;
    if meta.state != State::Open {
        bail!("this draft was already {}", meta.state.word());
    }
    meta.state = State::Discarded;
    write_meta(&root, &meta)?;
    let _ = fs::remove_dir_all(root.join("novos"));
    Ok(())
}

/// At startup: removes working copies an interrupted edit left, and drafts older than two weeks.
pub fn cleanup(state_dir: &Path) {
    let limit = room::now_ms() - KEEP_DAYS * DAY_MS;
    let Ok(bots) = fs::read_dir(state_dir.join("rascunhos")) else { return };
    for bot in bots.flatten() {
        let _ = fs::remove_dir_all(bot.path().join(WORK_DIR));
        let Ok(drafts) = fs::read_dir(bot.path()) else { continue };
        for draft in drafts.flatten() {
            if !read_meta(&draft.path()).is_ok_and(|meta| meta.created_ms >= limit) {
                let _ = fs::remove_dir_all(draft.path());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("raiox-drafts-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A project under `home`: source, a file to delete, a secret, dependencies, an ignored
    /// folder, a worktree of another branch and the AI's instructions left out by .gitignore.
    fn project(home: &Path) -> PathBuf {
        let project = home.join("Projects/app");
        for (rel, text) in [
            ("src/a.ts", "export const a = 1\n"),
            ("old.txt", "bye\n"),
            (".env", "KEY=secret\n"),
            (".gitignore", "scratch/\nCLAUDE.local.md\n"),
            ("CLAUDE.local.md", "be brief\n"),
            ("scratch/notes.txt", "x\n"),
            ("node_modules/pkg/index.js", "x\n"),
            (".claude/worktrees/other/.git", "gitdir: elsewhere\n"),
            (".claude/worktrees/other/src/a.ts", "export const a = 0\n"),
        ] {
            let path = project.join(rel);
            create_parent(&path).unwrap();
            fs::write(path, text).unwrap();
        }
        project
    }

    #[test]
    fn a_draft_leaves_secrets_out_and_applies_then_undoes() {
        let home = scratch("home");
        let project = project(&home);
        let state = home.join("state");
        assert!(prepare(&state, &home, "b1", &home).is_err(), "never the whole home folder");

        let work = prepare(&state, &home, "b1", &project).unwrap();
        assert!(work.dir.ends_with("rascunhos/b1/copia/Projects/app"));
        let copied: Vec<&str> = work.copied.keys().map(String::as_str).collect();
        assert_eq!(copied, vec![".gitignore", "CLAUDE.local.md", "old.txt", "src/a.ts"]);

        fs::write(work.dir.join("src/a.ts"), "export const a = 2\n").unwrap();
        fs::write(work.dir.join("src/b.ts"), "export const b = 1\n").unwrap();
        fs::remove_file(work.dir.join("old.txt")).unwrap();
        fs::write(work.dir.join(".env"), "KEY=stolen\n").unwrap();
        let (id, found) = collect(&state, "b1", &work).unwrap();
        let id = id.unwrap();
        let summary: Vec<(&str, Status)> = found.files.iter().map(|f| (f.path.as_str(), f.status)).collect();
        assert_eq!(summary, vec![("old.txt", Status::Deleted), ("src/a.ts", Status::Modified), ("src/b.ts", Status::Added)]);
        assert_eq!(found.blocked, vec![".env".to_string()]);
        assert!(!work.dir.exists(), "the working copy goes when the edit ends");

        apply(&state, "b1", &id).unwrap();
        assert_eq!(fs::read_to_string(project.join("src/a.ts")).unwrap(), "export const a = 2\n");
        assert!(project.join("src/b.ts").exists() && !project.join("old.txt").exists());
        assert_eq!(fs::read_to_string(project.join(".env")).unwrap(), "KEY=secret\n");
        assert!(apply(&state, "b1", &id).is_err(), "a draft applies once");

        undo(&state, "b1", &id).unwrap();
        assert_eq!(fs::read_to_string(project.join("src/a.ts")).unwrap(), "export const a = 1\n");
        assert!(!project.join("src/b.ts").exists() && project.join("old.txt").exists());
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn the_fingerprint_changes_only_when_a_file_does() {
        let home = scratch("print");
        let project = project(&home);
        let first = fingerprint(&project).unwrap();
        assert_eq!(fingerprint(&project), Some(first));
        // Ignored folders do not count.
        fs::write(project.join("scratch/notes.txt"), "changed\n").unwrap();
        assert_eq!(fingerprint(&project), Some(first));
        fs::write(project.join("src/a.ts"), "export const a = 10\n").unwrap();
        assert_ne!(fingerprint(&project), Some(first));
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn a_project_outside_home_is_copied_under_the_bot_folder() {
        assert_eq!(relative(Path::new("/opt/app"), Path::new("/Users/x")), PathBuf::from("opt/app"));
        assert_eq!(relative(Path::new("/Users/x/Projects/app"), Path::new("/Users/x")), PathBuf::from("Projects/app"));
    }

    #[test]
    fn apply_refuses_when_the_project_changed_meanwhile() {
        let home = scratch("conflict");
        let project = project(&home);
        let state = home.join("state");
        let work = prepare(&state, &home, "b1", &project).unwrap();
        fs::write(work.dir.join("src/a.ts"), "2\n").unwrap();
        let id = collect(&state, "b1", &work).unwrap().0.unwrap();
        // The person edits the same file before applying.
        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(project.join("src/a.ts"), "1 and more\n").unwrap();
        let refused = apply(&state, "b1", &id).unwrap_err().to_string();
        assert!(refused.contains("src/a.ts"), "{refused}");
        assert_eq!(fs::read_to_string(project.join("src/a.ts")).unwrap(), "1 and more\n");
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn undo_refuses_when_the_person_touched_a_file_after_apply() {
        let home = scratch("undo");
        let project = project(&home);
        let state = home.join("state");
        let work = prepare(&state, &home, "b1", &project).unwrap();
        fs::remove_file(work.dir.join("old.txt")).unwrap();
        let id = collect(&state, "b1", &work).unwrap().0.unwrap();
        apply(&state, "b1", &id).unwrap();
        // The person writes the removed file again before undoing.
        fs::write(project.join("old.txt"), "mine\n").unwrap();
        let refused = undo(&state, "b1", &id).unwrap_err().to_string();
        assert!(refused.contains("old.txt"), "{refused}");
        assert_eq!(fs::read_to_string(project.join("old.txt")).unwrap(), "mine\n");
        let _ = fs::remove_dir_all(&home);
    }

    #[cfg(unix)]
    #[test]
    fn apply_never_writes_outside_the_project() {
        let home = scratch("outside");
        let project = project(&home);
        let elsewhere = home.join("elsewhere");
        fs::create_dir_all(&elsewhere).unwrap();
        let state = home.join("state");
        let work = prepare(&state, &home, "b1", &project).unwrap();
        fs::create_dir_all(work.dir.join("lib")).unwrap();
        fs::write(work.dir.join("lib/x.ts"), "1\n").unwrap();
        let id = collect(&state, "b1", &work).unwrap().0.unwrap();
        // Meanwhile `lib` in the project becomes a link to a folder outside it.
        std::os::unix::fs::symlink(&elsewhere, project.join("lib")).unwrap();
        let refused = apply(&state, "b1", &id).unwrap_err().to_string();
        assert!(refused.contains("lib/x.ts"), "{refused}");
        assert!(!elsewhere.join("x.ts").exists());
        let _ = fs::remove_dir_all(&home);
    }
}
