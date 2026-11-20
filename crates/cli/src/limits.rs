//! How much of each AI's plan is used, in its two windows (five hours, and the week). Codex is
//! asked live (its app server reads the account's limits without running a turn), at most once a
//! minute, in the background; until then, or when that fails, its session logs say what it saw
//! on its last turn. Claude Code says it in each turn raiox runs (its stream's rate_limit_event).
//! The others say nothing, so they get no figure.

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

/// How far back raiox looks for Codex's newest session, in days of its dated folders.
const CODEX_DAYS: usize = 14;
/// A Claude Code reading older than this is shown, but said to be old.
const SECS_PER_WEEK: i64 = 7 * 24 * 60 * 60;

/// A live reading of Codex's limits is asked again after this long.
const LIVE_EVERY: Duration = Duration::from_secs(60);
/// After a failed live reading (no Codex, signed out), the logs alone are used for this long.
const LIVE_RETRY: Duration = Duration::from_secs(2 * 60);
/// How long the app server has to answer.
const LIVE_TIMEOUT: Duration = Duration::from_secs(20);

/// The last live reading (or failure), when it was asked, and whether one is being asked now.
struct Live {
    reading: Option<(Value, i64)>,
    asked: Option<Instant>,
    failed: bool,
    asking: bool,
}

static LIVE: Mutex<Live> = Mutex::new(Live { reading: None, asked: None, failed: false, asking: false });

/// One window: how much is used (0 to 100) and when it starts again (seconds since 1970).
fn window(used: f64, resets_at: Option<i64>, now: i64) -> Value {
    // A window that has started again since the reading is empty.
    let renewed = resets_at.is_some_and(|at| at <= now);
    json!({ "used": if renewed { 0.0 } else { used.clamp(0.0, 100.0) }, "resets_at": resets_at })
}

/// A plan's name as people say it: "plus" is Plus, "pro" is Pro.
fn plan_name(plan: &str) -> Option<String> {
    let mut letters = plan.trim().chars();
    let first = letters.next()?;
    Some(first.to_uppercase().chain(letters).collect())
}

/// Codex's app server's answer to `account/rateLimits/read`, as the panel shows it.
fn codex_live_from(result: &Value, now: i64) -> Option<Value> {
    let limits = result.get("rateLimits")?;
    let side = |key: &str| {
        let part = &limits[key];
        Some(window(part["usedPercent"].as_f64()?, part["resetsAt"].as_i64(), now))
    };
    let mut out = json!({ "five_hour": side("primary")?, "week": side("secondary")?, "live": true });
    if let Some(plan) = limits["planType"].as_str().and_then(plan_name) {
        out["plan"] = json!(plan);
    }
    Some(out)
}

/// Asks Codex's app server for the account's limits: it reads them from OpenAI without a turn.
fn ask_codex_live() -> Option<Value> {
    let mut child = crate::procs::command("codex").arg("app-server").stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()?;
    let mut input = child.stdin.take()?;
    let output = child.stdout.take()?;
    let (send, answer) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(output).lines().map_while(Result::ok) {
            let Ok(message) = serde_json::from_str::<Value>(&line) else { continue };
            if message["id"] == 2 {
                let _ = send.send(message);
                return;
            }
        }
    });
    let requests = [
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "clientInfo": { "name": "raiox", "version": env!("CARGO_PKG_VERSION") } } }),
        json!({ "jsonrpc": "2.0", "method": "initialized" }),
        json!({ "jsonrpc": "2.0", "id": 2, "method": "account/rateLimits/read" }),
    ];
    let sent = requests.iter().all(|request| writeln!(input, "{request}").is_ok());
    let reply = if sent { answer.recv_timeout(LIVE_TIMEOUT).ok() } else { None };
    let _ = child.kill();
    let _ = child.wait();
    codex_live_from(reply?.get("result")?, crate::room::now_ms() / 1000)
}

/// The live reading when it is recent; an older one starts a new ask in the background.
fn codex_live() -> Option<Value> {
    let mut live = LIVE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let wait = if live.failed { LIVE_RETRY } else { LIVE_EVERY };
    if !live.asking && live.asked.is_none_or(|at| at.elapsed() >= wait) {
        live.asking = true;
        live.asked = Some(Instant::now());
        std::thread::spawn(|| {
            let reading = ask_codex_live();
            let mut live = LIVE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            live.failed = reading.is_none();
            if let Some(reading) = reading {
                live.reading = Some((reading, crate::room::now_ms() / 1000));
            }
            live.asking = false;
        });
    }
    // A live reading older than a failed retry's wait says less than the logs might.
    live.reading.as_ref().filter(|_| !live.failed).map(|(reading, at)| {
        let mut reading = reading.clone();
        reading["read_at"] = json!(at);
        reading
    })
}

/// The newest `rate_limits` a Codex session log holds.
fn codex_from(text: &str, now: i64) -> Option<Value> {
    let limits = text.lines().rev().filter(|line| line.contains("\"rate_limits\"")).find_map(|line| {
        let event: Value = serde_json::from_str(line).ok()?;
        event.pointer("/payload/rate_limits").or_else(|| event.pointer("/payload/info/rate_limits")).or_else(|| event.get("rate_limits")).cloned()
    })?;
    let side = |key: &str| {
        let part = &limits[key];
        Some(window(part["used_percent"].as_f64()?, part["resets_at"].as_i64(), now))
    };
    let mut out = json!({ "five_hour": side("primary")?, "week": side("secondary")? });
    if let Some(plan) = limits["plan_type"].as_str().and_then(plan_name) {
        out["plan"] = json!(plan);
    }
    Some(out)
}

/// The newest `rate_limit_event` in a Claude Code stream.
fn claude_from(text: &str, now: i64) -> Option<Value> {
    let info = text.lines().rev().filter(|line| line.contains("\"rate_limit_event\"")).find_map(|line| {
        let event: Value = serde_json::from_str(line).ok()?;
        event.get("rate_limit_info").cloned()
    })?;
    let side = |key: &str| {
        let part = &info["unifiedWindows"][key];
        Some(window(part["utilization"].as_f64()? * 100.0, part["resetsAt"].as_i64(), now))
    };
    Some(json!({ "five_hour": side("five_hour")?, "week": side("seven_day")? }))
}

fn modified(path: &Path) -> i64 {
    fs::metadata(path).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs() as i64)
}

/// Files under a folder (one level of subfolders), newest first.
fn newest_files(folder: &Path, matches: &dyn Fn(&Path) -> bool) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = Vec::new();
    let Ok(entries) = fs::read_dir(folder) else { return found };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(fs::read_dir(&path).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| matches(p)));
        } else if matches(&path) {
            found.push(path);
        }
    }
    found.sort_by_key(|p| std::cmp::Reverse(modified(p)));
    found
}

/// Codex's newest session folders (sessions/YYYY/MM/DD), newest day first.
fn codex_days(codex: &Path) -> Vec<PathBuf> {
    let mut days: Vec<PathBuf> = Vec::new();
    for year in fs::read_dir(codex.join("sessions")).into_iter().flatten().flatten() {
        for month in fs::read_dir(year.path()).into_iter().flatten().flatten() {
            days.extend(fs::read_dir(month.path()).into_iter().flatten().flatten().map(|d| d.path()));
        }
    }
    days.sort();
    days.into_iter().rev().take(CODEX_DAYS).collect()
}

/// Each AI's plan, as far as raiox can tell: `{ agent: { five_hour, week, read_at } }`.
pub fn state(state_dir: &Path, codex: &Path) -> Value {
    let now = crate::room::now_ms() / 1000;
    let mut out = serde_json::Map::new();
    let rollout = |p: &Path| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("rollout-") && n.ends_with(".jsonl"));
    let codex_reading = codex_days(codex).iter().flat_map(|day| newest_files(day, &rollout)).find_map(|file| {
        let text = fs::read_to_string(&file).ok()?;
        codex_from(&text, now).map(|limits| (limits, modified(&file)))
    });
    // Live when it can be; the newer of the two otherwise (a turn may have run since the live read).
    let from_logs = codex_reading.map(|(mut limits, at)| {
        limits["read_at"] = json!(at);
        limits
    });
    let at = |reading: &Value| reading["read_at"].as_i64().unwrap_or(0);
    let codex_limits = match (codex_live(), from_logs) {
        (Some(live), Some(logs)) if at(&logs) > at(&live) => Some(logs),
        (Some(live), _) => Some(live),
        (None, logs) => logs,
    };
    if let Some(limits) = codex_limits {
        out.insert("codex".into(), limits);
    }
    let stream = |p: &Path| p.extension().and_then(|e| e.to_str()) == Some("out") || p.file_name().and_then(|n| n.to_str()) == Some("out.txt");
    let claude_reading = newest_files(&state_dir.join("bots"), &stream).into_iter().find_map(|file| {
        let text = fs::read_to_string(&file).ok()?;
        claude_from(&text, now).map(|limits| (limits, modified(&file)))
    });
    if let Some((mut limits, at)) = claude_reading {
        limits["read_at"] = json!(at);
        limits["old"] = json!(now - at > SECS_PER_WEEK);
        out.insert("claude".into(), limits);
    }
    Value::Object(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_795_160_000;

    #[test]
    fn codex_says_its_two_windows_in_its_own_log() {
        let log = r#"{"type":"event_msg","payload":{"type":"token_count","rate_limits":{"primary":{"used_percent":1.0,"window_minutes":300,"resets_at":1795182060},"secondary":{"used_percent":23.0,"window_minutes":10080,"resets_at":1795412856}}}}"#;
        let limits = codex_from(log, NOW).unwrap();
        assert_eq!((limits["five_hour"]["used"].as_f64(), limits["week"]["used"].as_f64()), (Some(1.0), Some(23.0)));
    }

    #[test]
    fn codex_answers_live_with_its_plan() {
        let result = json!({ "rateLimits": { "primary": { "usedPercent": 4, "resetsAt": 1795211046 }, "secondary": { "usedPercent": 24, "resetsAt": 1795412857 }, "planType": "plus" } });
        let limits = codex_live_from(&result, NOW).unwrap();
        assert_eq!((limits["five_hour"]["used"].as_f64(), limits["week"]["used"].as_f64()), (Some(4.0), Some(24.0)));
        assert_eq!((limits["plan"].as_str(), limits["live"].as_bool()), (Some("Plus"), Some(true)));
        assert!(codex_live_from(&json!({ "error": "signed out" }), NOW).is_none());
    }

    #[test]
    fn claude_code_says_its_two_windows_on_each_turn() {
        let stream = "{\"type\":\"system\"}\n{\"type\":\"rate_limit_event\",\"rate_limit_info\":{\"unifiedWindows\":{\"five_hour\":{\"utilization\":0.09,\"resetsAt\":1795170000},\"seven_day\":{\"utilization\":0.76,\"resetsAt\":1795345200}}}}";
        let limits = claude_from(stream, NOW).unwrap();
        assert_eq!(limits["five_hour"]["used"].as_f64().map(|u| u.round()), Some(9.0));
        assert_eq!(limits["week"]["used"].as_f64().map(|u| u.round()), Some(76.0));
    }

    #[test]
    fn a_window_that_started_again_since_the_reading_is_empty() {
        let stream = "{\"type\":\"rate_limit_event\",\"rate_limit_info\":{\"unifiedWindows\":{\"five_hour\":{\"utilization\":0.9,\"resetsAt\":1795150000},\"seven_day\":{\"utilization\":0.5,\"resetsAt\":1795345200}}}}";
        let limits = claude_from(stream, NOW).unwrap();
        assert_eq!(limits["five_hour"]["used"].as_f64(), Some(0.0), "the five hours ran out before now");
        assert_eq!(limits["week"]["used"].as_f64(), Some(50.0));
        assert!(claude_from("{\"type\":\"result\"}", NOW).is_none(), "nothing said, no figure");
    }
}
