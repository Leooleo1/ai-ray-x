//! What one turn of an AI with full access changed in its project: the folder is photographed
//! before and after (a git tree written through a temporary index, so the person's index, HEAD
//! and commits are never touched), and the two trees give the files, the diff and an undo that
//! applies the diff backwards to the files only. Secrets files stay out of all three.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::io::Write;

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

/// Files a turn's change lists at most (a generated folder could hold thousands).
const MAX_FILES: usize = 200;
/// The diff shown is cut past this many characters.
const MAX_PATCH: usize = 400_000;

/// The folder's content at one moment, and the commit it sat on.
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub tree: String,
    pub head: Option<String>,
}

fn git(dir: &Path, args: &[&str], index: Option<&Path>) -> Option<String> {
    let mut command = Command::new("git");
    command.arg("-C").arg(dir).args(args).stdin(Stdio::null()).stderr(Stdio::null());
    if let Some(index) = index {
        command.env("GIT_INDEX_FILE", index);
    }
    let output = command.output().ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn temp_index() -> PathBuf {
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    std::env::temp_dir().join(format!("raiox-index-{}-{nonce}", std::process::id()))
}

/// The folder as a git tree (what .gitignore hides stays out), or none outside a git repository.
pub fn snapshot(dir: &Path) -> Option<Snapshot> {
    let git_dir = git(dir, &["rev-parse", "--absolute-git-dir"], None)?;
    let index = temp_index();
    // Starting from a copy of the real index makes the add incremental; the real one is never written.
    let _ = std::fs::copy(Path::new(&git_dir).join("index"), &index);
    let tree = git(dir, &["add", "-A", "."], Some(&index)).and_then(|_| git(dir, &["write-tree"], Some(&index)));
    let _ = std::fs::remove_file(&index);
    Some(Snapshot { tree: tree?, head: git(dir, &["rev-parse", "HEAD"], None) })
}

fn is_secret(path: &str) -> bool {
    raiox_core::secrets::is_secret_file(path.rsplit('/').next().unwrap_or(path))
}

/// What changed between two photos: the files with their added and removed lines, or none.
pub fn compare(dir: &Path, before: &Snapshot, after: &Snapshot) -> Option<Value> {
    if before.tree == after.tree {
        return None;
    }
    let numstat = git(dir, &["diff-tree", "-r", "--numstat", "--no-renames", &before.tree, &after.tree], None)?;
    let files: Vec<Value> = numstat
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, '\t');
            let (added, removed, path) = (parts.next()?, parts.next()?, parts.next()?);
            (!is_secret(path)).then(|| json!({ "path": path, "added": added.parse::<u64>().ok(), "removed": removed.parse::<u64>().ok() }))
        })
        .take(MAX_FILES)
        .collect();
    if files.is_empty() {
        return None;
    }
    Some(json!({ "from": before.tree, "to": after.tree, "head_moved": before.head != after.head, "files": files }))
}

/// The paths of a change, as `git` pathspecs that leave the secrets files out.
fn pathspecs(changes: &Value) -> Vec<String> {
    changes["files"].as_array().into_iter().flatten().filter_map(|f| f["path"].as_str()).map(|p| format!(":(literal){p}")).collect()
}

fn trees(changes: &Value) -> Result<(String, String)> {
    let tree = |key: &str| changes[key].as_str().filter(|t| t.len() >= 40 && t.bytes().all(|b| b.is_ascii_hexdigit())).map(str::to_string);
    Ok((tree("from").context("this answer has no change to show")?, tree("to").context("this answer has no change to show")?))
}

/// The diff of a turn's change, cut when very long.
pub fn patch(dir: &Path, changes: &Value) -> Result<String> {
    let (from, to) = trees(changes)?;
    let paths = pathspecs(changes);
    let mut args = vec!["diff-tree", "-r", "-p", "--no-color", "--no-renames", from.as_str(), to.as_str(), "--"];
    args.extend(paths.iter().map(String::as_str));
    let mut text = git(dir, &args, None).context("the change's photos are gone from the repository (git cleaned them up)")?;
    if text.len() > MAX_PATCH {
        let cut = (0..=MAX_PATCH).rev().find(|i| text.is_char_boundary(*i)).unwrap_or(0);
        text.truncate(cut);
        text.push_str("\n…");
    }
    Ok(text)
}

/// Undoes a turn's change in the files, refusing when they changed since.
pub fn undo(dir: &Path, changes: &Value) -> Result<()> {
    let (from, to) = trees(changes)?;
    let paths = pathspecs(changes);
    let mut args = vec!["diff-tree", "-r", "-p", "--binary", "--no-renames", to.as_str(), from.as_str(), "--"];
    args.extend(paths.iter().map(String::as_str));
    let reverse = git(dir, &args, None).context("the change's photos are gone from the repository (git cleaned them up)")?;
    let mut apply = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["apply", "--whitespace=nowarn", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()?;
    apply.stdin.take().context("git apply has no input")?.write_all(format!("{reverse}\n").as_bytes())?;
    let output = apply.wait_with_output()?;
    if !output.status.success() {
        bail!("these files changed after this turn; undo it by hand ({})", String::from_utf8_lossy(&output.stderr).lines().next().unwrap_or("").trim());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn repo(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("raiox-turno-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        for args in [vec!["init", "-q"], vec!["config", "user.email", "t@t"], vec!["config", "user.name", "t"]] {
            Command::new("git").arg("-C").arg(&dir).args(&args).status().unwrap();
        }
        fs::write(dir.join("a.txt"), "one\ntwo\n").unwrap();
        Command::new("git").arg("-C").arg(&dir).args(["add", "."]).status().unwrap();
        Command::new("git").arg("-C").arg(&dir).args(["commit", "-qm", "start"]).status().unwrap();
        dir
    }

    #[test]
    fn a_turn_shows_its_files_and_diff_and_leaves_the_index_alone() {
        let dir = repo("mostra");
        let before = snapshot(&dir).unwrap();
        fs::write(dir.join("a.txt"), "one\nTWO\nthree\n").unwrap();
        fs::write(dir.join("new.txt"), "hi\n").unwrap();
        fs::write(dir.join(".env"), "KEY=secret\n").unwrap();
        let after = snapshot(&dir).unwrap();
        let changes = compare(&dir, &before, &after).unwrap();
        let paths: Vec<&str> = changes["files"].as_array().unwrap().iter().map(|f| f["path"].as_str().unwrap()).collect();
        assert_eq!(paths, ["a.txt", "new.txt"], "the .env stays out");
        assert_eq!((changes["files"][0]["added"].as_u64(), changes["files"][0]["removed"].as_u64()), (Some(2), Some(1)));
        let diff = patch(&dir, &changes).unwrap();
        assert!(diff.contains("+TWO") && !diff.contains("secret"));
        let staged = git(&dir, &["diff", "--cached", "--name-only"], None).unwrap();
        assert!(staged.is_empty(), "the person's index is untouched: {staged}");
    }

    #[test]
    fn undo_puts_the_files_back_and_refuses_after_a_later_change() {
        let dir = repo("desfaz");
        let before = snapshot(&dir).unwrap();
        fs::write(dir.join("a.txt"), "one\nchanged\n").unwrap();
        let changes = compare(&dir, &before, &snapshot(&dir).unwrap()).unwrap();
        fs::write(dir.join("a.txt"), "one\nchanged again\n").unwrap();
        assert!(undo(&dir, &changes).is_err(), "a later edit makes it refuse");
        fs::write(dir.join("a.txt"), "one\nchanged\n").unwrap();
        undo(&dir, &changes).unwrap();
        assert_eq!(fs::read_to_string(dir.join("a.txt")).unwrap(), "one\ntwo\n");
    }

    #[test]
    fn nothing_changed_is_no_change_and_outside_git_there_is_no_photo() {
        let dir = repo("nada");
        let before = snapshot(&dir).unwrap();
        assert!(compare(&dir, &before, &snapshot(&dir).unwrap()).is_none());
        let outside = std::env::temp_dir().join(format!("raiox-sem-git-{}", std::process::id()));
        fs::create_dir_all(&outside).unwrap();
        assert!(snapshot(&outside).is_none());
    }
}
