//! What a bot that runs without asking may run with no click: commands that read
//! the project, check it (tests, lint, types, a build) or install its locked
//! dependencies, the way a coding CLI in auto mode would. Anything else (deleting,
//! moving, git that rewrites, adding packages, scripts with other names) still
//! waits for the person, even with the grant on.

/// Programs that only read.
const READERS: [&str; 15] = ["ls", "pwd", "cat", "head", "tail", "wc", "grep", "rg", "tree", "du", "which", "file", "stat", "diff", "find"];
/// `find` actions that change files or run other programs.
const FIND_ACTIONS: [&str; 8] = ["-delete", "-exec", "-execdir", "-ok", "-okdir", "-fprint", "-fprint0", "-fprintf"];
/// Git subcommands that only read.
const GIT_READS: [&str; 11] = ["status", "log", "diff", "show", "rev-parse", "ls-files", "blame", "describe", "shortlog", "grep", "remote"];
/// Package managers, and the scripts of a project they may run without a click.
const MANAGERS: [&str; 4] = ["npm", "pnpm", "yarn", "bun"];
const CHECK_SCRIPTS: [&str; 7] = ["test", "lint", "typecheck", "type-check", "check", "build", "tsc"];
/// Programs whose `--version` is asked often.
const VERSIONED: [&str; 11] = ["node", "npm", "pnpm", "yarn", "bun", "deno", "python3", "python", "cargo", "rustc", "git"];

/// Whether a checked command (its words) runs without a click for a bot that runs without asking.
pub fn runs_alone(words: &[String]) -> bool {
    let Some(program) = words.first() else { return false };
    let rest: Vec<&str> = words[1..].iter().map(String::as_str).collect();
    let name = program.rsplit(['/', '\\']).next().unwrap_or(program).to_lowercase();
    let name = name.trim_end_matches(".cmd").trim_end_matches(".exe");
    if VERSIONED.contains(&name) && matches!(rest.as_slice(), ["--version" | "-v" | "-V"]) {
        return true;
    }
    match name {
        "find" => !rest.iter().any(|w| FIND_ACTIONS.contains(w)),
        reader if READERS.contains(&reader) => true,
        "git" => git_reads(&rest),
        manager if MANAGERS.contains(&manager) => manager_checks(&rest),
        "npx" | "tsc" | "eslint" | "vitest" | "jest" | "pytest" | "cargo" => tool_checks(name, &rest),
        _ => false,
    }
}

/// `git status`, `git log`, `git remote -v`: reading, never a rewrite.
fn git_reads(rest: &[&str]) -> bool {
    let Some(sub) = rest.iter().find(|w| !w.starts_with('-')) else { return false };
    match *sub {
        "remote" => rest.iter().all(|w| matches!(*w, "remote" | "-v" | "--verbose")),
        "branch" => rest.iter().all(|w| matches!(*w, "branch" | "-a" | "-r" | "-v" | "-vv" | "--list" | "--show-current")),
        sub => GIT_READS.contains(&sub),
    }
}

/// Installing the locked dependencies (no package named), or a project's test, lint, type or build script.
fn manager_checks(rest: &[&str]) -> bool {
    let words: Vec<&str> = rest.iter().copied().filter(|w| !w.starts_with('-')).collect();
    match words.as_slice() {
        [] => rest.is_empty(),
        ["install"] | ["i"] | ["ci"] => true,
        ["run", script] | [script] => is_check_script(script),
        _ => false,
    }
}

fn is_check_script(script: &str) -> bool {
    let base = script.split(':').next().unwrap_or(script);
    CHECK_SCRIPTS.contains(&base)
}

/// Checkers called directly: `tsc --noEmit`, `npx vitest run`, `cargo test`.
fn tool_checks(name: &str, rest: &[&str]) -> bool {
    match name {
        "npx" => rest.first().is_some_and(|tool| matches!(*tool, "tsc" | "eslint" | "vitest" | "jest") && tool_checks(tool, &rest[1..])),
        "tsc" => rest.contains(&"--noEmit"),
        "eslint" => !rest.contains(&"--fix"),
        "vitest" | "jest" | "pytest" => !rest.iter().any(|w| w.starts_with("-u") || *w == "--update"),
        "cargo" => match rest.first() {
            Some(&"fmt") => rest.contains(&"--check"),
            Some(sub) => matches!(*sub, "test" | "check" | "clippy" | "build"),
            None => false,
        },
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alone(command: &str) -> bool {
        runs_alone(&shell_words::split(command).unwrap())
    }

    #[test]
    fn reading_checking_and_installing_run_alone() {
        for command in ["git status", "git log -5 --oneline", "git remote -v", "git branch --show-current", "ls -la src", "grep -rn TODO src", "find . -name '*.ts'", "npm install", "npm ci", "pnpm install", "yarn", "npm test", "npm run lint", "pnpm run test:unit", "npm run typecheck", "npm run build", "npx tsc --noEmit", "npx vitest run", "cargo test", "node --version"] {
            assert!(alone(command), "{command} should run alone");
        }
    }

    #[test]
    fn changing_or_unknown_commands_still_wait_for_a_click() {
        for command in ["rm -rf src", "rm -v", "mv a b", "git reset --hard", "git push", "git checkout main", "git branch -D x", "git remote add o url", "npm install left-pad", "npm run deploy", "npm run db:reset", "pnpm add zod", "find . -delete", "find . -exec rm {} ;", "eslint . --fix", "vitest -u", "cargo fmt", "curl https://x.y", "node server.js", "npx prisma migrate reset"] {
            assert!(!alone(command), "{command} should wait for a click");
        }
    }
}
