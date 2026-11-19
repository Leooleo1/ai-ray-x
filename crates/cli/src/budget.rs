//! A daily token limit per project for its bots. raiox adds up what the
//! project's bots read and wrote today, from their own conversations, and
//! starts no new turn past the limit until midnight. The person's own CLI
//! sessions do not count: the limit is about what raiox runs for them.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{bail, Result};
use chrono::{Local, TimeZone};
use serde_json::Value;

use crate::{bots, room, threads};

const FILE: &str = "limites.json";
/// The limits the panel offers, in tokens.
pub const CHOICES: [u64; 4] = [1_000_000, 5_000_000, 20_000_000, 50_000_000];

fn load(state_dir: &Path) -> BTreeMap<String, u64> {
    fs::read_to_string(state_dir.join(FILE)).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
}

pub fn limit(state_dir: &Path, project: &str) -> Option<u64> {
    load(state_dir).get(project).copied()
}

/// Sets or clears (`None`) a project's daily limit.
pub fn set(state_dir: &Path, project: &str, tokens: Option<u64>) -> Result<()> {
    if tokens.is_some_and(|t| !CHOICES.contains(&t)) {
        bail!("pick one of the offered limits");
    }
    let mut limits = load(state_dir);
    match tokens {
        Some(tokens) => limits.insert(project.to_string(), tokens),
        None => limits.remove(project),
    };
    fs::create_dir_all(state_dir)?;
    let temp = state_dir.join("limites.tmp");
    fs::write(&temp, serde_json::to_string_pretty(&limits)?)?;
    fs::rename(temp, state_dir.join(FILE))?;
    Ok(())
}

fn midnight_ms() -> i64 {
    let today = Local::now().date_naive().and_hms_opt(0, 0, 0).unwrap_or_default();
    Local.from_local_datetime(&today).earliest().map_or(0, |at| at.timestamp_millis())
}

/// Tokens a reply read and wrote, from the numbers the CLI reported.
fn tokens(message: &room::Message) -> u64 {
    let reply = message.data.get("reply");
    let count = |key: &str| reply.and_then(|r| r.get(key)).and_then(Value::as_u64).unwrap_or(0);
    count("read_tokens") + count("output_tokens")
}

/// What the project's bots read and wrote since midnight, across all their conversations.
pub fn used_today(state_dir: &Path, project: &str) -> u64 {
    let since = midnight_ms();
    let mut used = 0;
    for bot in bots::list(state_dir).into_iter().filter(|b| b.name == project) {
        let files = std::iter::once(bots::chat_file(state_dir, &bot.id))
            .chain(threads::list(state_dir, &bot.id).into_iter().map(|t| threads::chat_file(state_dir, &bot.id, &t.id)));
        for file in files {
            used += room::read_messages(&file).iter().filter(|m| m.author == "bot" && m.ms >= since).map(tokens).sum::<u64>();
        }
    }
    used
}

/// Refuses a new turn once the project reached its limit today.
pub fn check(state_dir: &Path, project: &str) -> Result<()> {
    let Some(limit) = limit(state_dir, project) else { return Ok(()) };
    let used = used_today(state_dir, project);
    if used >= limit {
        bail!("{project} reached its daily limit for bots ({used} of {limit} tokens today); it resets at midnight, or change it in the bot's settings");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_project_past_its_limit_starts_no_new_turn() {
        let home = std::env::temp_dir().join(format!("raiox-budget-{}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
        fs::create_dir_all(home.join("Projects/app")).unwrap();
        let state = home.join(".state");
        let bot = bots::activate(&state, &home, "Projects/app", "claude", None).unwrap();
        let answer = |ms: i64, read: u64| room::Message { ms, author: "bot".into(), kind: "answer".into(), data: json!({ "reply": { "read_tokens": read, "output_tokens": 10 } }) };
        let yesterday = midnight_ms() - 1000;
        room::append_messages(&bots::chat_file(&state, &bot.id), vec![answer(yesterday, 5_000_000), answer(room::now_ms(), 999_990)]).unwrap();

        assert_eq!(used_today(&state, "app"), 1_000_000, "only today counts");
        assert!(check(&state, "app").is_ok(), "no limit, no refusal");
        assert!(set(&state, "app", Some(123)).is_err(), "only the offered limits");
        set(&state, "app", Some(1_000_000)).unwrap();
        assert!(check(&state, "app").unwrap_err().to_string().contains("daily limit"));
        set(&state, "app", None).unwrap();
        assert!(check(&state, "app").is_ok());
        let _ = fs::remove_dir_all(&home);
    }
}
