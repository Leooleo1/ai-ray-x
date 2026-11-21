use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const MAX_PATCH: usize = 192 * 1024;

#[derive(Clone, Copy, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
enum Area {
    Staged,
    Unstaged,
    Untracked,
}

#[derive(Serialize)]
struct Change<'a> {
    path: &'a str,
    old_path: Option<&'a str>,
    area: Area,
    code: char,
}

fn git(root: &Path) -> Command {
    let mut command = crate::procs::command("git");
    command.args(["--no-pager", "--no-optional-locks", "-c", "core.fsmonitor=false"])
        .arg("-C").arg(root).stdin(Stdio::null()).stderr(Stdio::null());
    command
}

fn project_path<'a>(path: &'a str, prefix: &str) -> Option<&'a str> {
    let path = path.strip_prefix(prefix)?;
    (!path.contains('\u{fffd}') && path.split('/').all(crate::project_files::visible)).then_some(path)
}

fn changes<'a>(status: &'a str, prefix: &str) -> (&'a str, Vec<Change<'a>>) {
    let mut branch = "";
    let mut changes = Vec::new();
    let mut records = status.split('\0');
    while let Some(record) = records.next() {
        let (area, fields) = match record.as_bytes().first() {
            Some(b'1') => (None, 8),
            Some(b'2') => (None, 9),
            Some(b'u') => (None, 10),
            Some(b'?') => (Some(Area::Untracked), 1),
            _ => {
                if let Some(head) = record.strip_prefix("# branch.head ") {
                    branch = head;
                }
                continue;
            }
        };
        let old_path = if record.starts_with("2 ") {
            records.next().and_then(|path| project_path(path, prefix))
        } else {
            None
        };
        let Some(path) = record.splitn(fields + 1, ' ').nth(fields).and_then(|path| project_path(path, prefix)) else { continue };
        if let Some(area) = area {
            changes.push(Change { path, old_path, area, code: '?' });
            continue;
        }
        if record.starts_with("u ") {
            changes.push(Change { path, old_path, area: Area::Unstaged, code: 'U' });
            continue;
        }
        for (area, code) in [Area::Staged, Area::Unstaged].into_iter().zip(record[2..].chars().take(2)) {
            if code != '.' {
                changes.push(Change { path, old_path, area, code });
            }
        }
    }
    (branch, changes)
}

fn patch(root: &Path, change: &Change<'_>) -> Result<Value> {
    let file = root.join(change.path);
    let canonical_root = root.canonicalize()?;
    let edit = file.canonicalize().is_ok_and(|resolved| resolved == canonical_root.join(change.path) && resolved.is_file());
    let source = change.old_path.filter(|_| matches!(change.code, 'R' | 'C'));
    let mut command = git(root);
    command.args(["diff", "--no-ext-diff", "--no-textconv", "--no-color", "--no-relative",
        "--src-prefix=a/", "--dst-prefix=b/", "--output-indicator-new=+", "--output-indicator-old=-",
        "--output-indicator-context= ", "--submodule=short"]);
    match change.area {
        Area::Staged => { command.arg("--cached"); }
        Area::Unstaged if change.code == 'U' => { command.arg("--ours"); }
        Area::Unstaged => {}
        Area::Untracked => {
            ensure!(edit, "file unavailable");
            command.arg("--no-index");
        }
    }
    command.arg(if source.is_some() { "--find-renames" } else { "--no-renames" });
    command.arg("--");
    if change.area == Area::Untracked {
        command.args(["/dev/null", change.path]);
    } else {
        for path in [Some(change.path), source].into_iter().flatten() {
            let mut glob = String::new();
            for ch in path.chars() {
                if matches!(ch, '*' | '?' | '[' | ']') {
                    glob.push('\\');
                }
                glob.push(ch);
            }
            // Git treats a literal pathspec as a directory prefix too. Keep this diff to the named files.
            command.arg(format!(":(literal){path}")).arg(format!(":(exclude,glob){glob}/**"));
        }
    }
    command.stdout(Stdio::piped());

    let mut child = command.spawn().context("could not start Git")?;
    let mut bytes = Vec::new();
    let result = child.stdout.take().expect("piped Git output").take(MAX_PATCH as u64 + 1).read_to_end(&mut bytes);
    let limited = bytes.len() > MAX_PATCH;
    if limited || result.is_err() {
        let _ = child.kill();
    }
    let exit = child.wait()?;
    result?;
    ensure!(limited || exit.success() || (change.area == Area::Untracked && exit.code() == Some(1)), "could not load Git diff");
    bytes.truncate(MAX_PATCH);
    Ok(json!({ "patch": String::from_utf8_lossy(&bytes), "limited": limited, "editable": edit }))
}

pub fn request(root: &Path, input: &Value) -> Result<Value> {
    let prefix = git(root).args(["rev-parse", "--show-prefix"]).output().context("Git is unavailable")?;
    if !prefix.status.success() {
        return Ok(json!({ "repository": false }));
    }
    let prefix = std::str::from_utf8(&prefix.stdout)?.trim_end_matches(['\r', '\n']);
    let status = git(root).args(["status", "--porcelain=v2", "--branch", "-z", "--untracked-files=all", "--", "."]).output()?;
    ensure!(status.status.success(), "could not read Git status");
    let status = String::from_utf8_lossy(&status.stdout);
    let (branch, changes) = changes(&status, prefix);

    match input.get("op").and_then(Value::as_str) {
        Some("status") => Ok(json!({ "repository": true, "branch": branch, "changes": changes })),
        Some("diff") => {
            let area: Area = serde_json::from_value(input["area"].clone())?;
            let path = input["path"].as_str().context("missing Git path")?;
            let change = changes.iter().find(|change| change.path == path && change.area == area)
                .context("this change is no longer available")?;
            patch(root, change)
        }
        _ => anyhow::bail!("unknown Git operation"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn run(root: &Path, args: &[&str]) {
        assert!(git(root).args(args).status().unwrap().success(), "{args:?}");
    }

    #[test]
    fn reads_local_diffs_without_changing_the_index() {
        let root = std::env::temp_dir().join(format!("raiox-git-{}", std::process::id()));
        let scope = root.join("project");
        fs::create_dir_all(&scope).unwrap();
        assert_eq!(request(&root, &json!({"op":"status"})).unwrap()["repository"], false);
        run(&root, &["init", "-b", "fixture"]);
        run(&root, &["config", "core.autocrlf", "false"]);
        run(&root, &["config", "commit.gpgSign", "false"]);
        fs::write(scope.join("file [1].txt"), "original\n").unwrap();
        fs::write(scope.join("before rename.txt"), "rename me\n").unwrap();
        run(&root, &["add", "--all"]);
        run(&root, &["-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid", "commit", "-m", "baseline"]);
        fs::write(scope.join("file [1].txt"), "staged\n").unwrap();
        run(&root, &["--literal-pathspecs", "add", "--", "project/file [1].txt"]);
        run(&root, &["mv", "project/before rename.txt", "project/after rename.txt"]);
        fs::write(scope.join("file [1].txt"), "staged\nunstaged\n").unwrap();
        fs::write(scope.join("-new file.txt"), "untracked\n").unwrap();
        fs::write(scope.join(".env.local"), "hidden\n").unwrap();
        fs::write(root.join("outside.txt"), "outside\n").unwrap();

        let index = root.join(".git/index");
        let before = fs::read(&index).unwrap();
        let status = request(&scope, &json!({"op":"status"})).unwrap();
        assert_eq!(status["branch"], "fixture");
        assert_eq!(status["changes"].as_array().unwrap().len(), 4, "{status}");
        for (area, path, expected) in [
            ("staged", "file [1].txt", "+staged"),
            ("unstaged", "file [1].txt", "+unstaged"),
            ("staged", "after rename.txt", "rename from project/before rename.txt"),
            ("untracked", "-new file.txt", "+untracked"),
        ] {
            let result = request(&scope, &json!({"op":"diff", "area":area, "path":path})).unwrap();
            assert!(result["patch"].as_str().unwrap().contains(expected), "{result}");
            assert_eq!(result["editable"], true);
        }
        assert!(request(&scope, &json!({"op":"diff", "area":"untracked", "path":".env.local"})).is_err());
        assert!(request(&scope, &json!({"op":"diff", "area":"untracked", "path":"../outside.txt"})).is_err());
        assert_eq!(fs::read(index).unwrap(), before);
        fs::remove_dir_all(root).unwrap();
    }
}
