//! Memo keeps each applied or undone change as an ai-memory page, through
//! ai-memory's own `write-page` command (never its files or database).

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};
use raiox_core::apply::Change;
use raiox_core::outcome::Outcome;

/// Writes (or rewrites) the lesson page for a change; returns its wiki path.
pub fn keep_lesson(project_dir: &Path, change: &Change, evidence: &str, result: Option<&Outcome>) -> Result<String> {
    let day = chrono_day(change.applied_ms);
    let path = format!("raiox/{day}-{}.md", change.suggestion);
    let status = match change.undone_ms {
        Some(_) => "Undone: the previous value was restored.",
        None => "Applied and still in place.",
    };
    let measured = result.map_or_else(String::new, |o| {
        format!(
            "- Result: average context per request {before}K -> {after}K ({change:+.0}%) over {requests} requests; compactions per 100 requests {cb:.1} -> {ca:.1}\n",
            before = o.before.avg_context / 1000,
            after = o.after.avg_context / 1000,
            change = o.context_change * 100.0,
            requests = o.after.requests,
            cb = o.before.compactions_per_100,
            ca = o.after.compactions_per_100,
        )
    });
    let body = format!(
        "# Token diet: {suggestion}\n\n- Change: `{key}={after}` in `{file}` (before: {before})\n- Evidence: {evidence}\n{measured}- Status: {status}\n- Undo from the raiox panel or with `raiox undo {id}`.\n",
        suggestion = change.suggestion,
        key = change.key,
        after = change.after,
        file = change.file,
        before = change.before.as_deref().unwrap_or("not set"),
        id = change.id,
    );
    let mut child = Command::new("ai-memory")
        .args(["write-page", "--path", &path, "--body", "-", "--kind", "decision", "--tag", "raiox", "--tag", "token-diet"])
        .current_dir(project_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .context("ai-memory is not installed or not on PATH")?;
    child.stdin.take().context("no stdin for ai-memory")?.write_all(body.as_bytes())?;
    let output = child.wait_with_output()?;
    if !output.status.success() {
        bail!("ai-memory write-page failed: {}", String::from_utf8_lossy(&output.stderr).lines().last().unwrap_or(""));
    }
    Ok(path)
}

fn chrono_day(ms: i64) -> String {
    let days = ms.div_euclid(86_400_000);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(test)]
mod tests {
    #[test]
    fn days_are_civil_dates() {
        assert_eq!(super::chrono_day(0), "1970-01-01");
        assert_eq!(super::chrono_day(1_795_024_880_234), "2026-11-18");
    }
}
