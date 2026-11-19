//! Changes asked for from a chat: an AI the person talks to (through raiox's
//! MCP server) proposes a new bot, routine, task or channel, or a message to a
//! channel. Nothing is applied when it is asked: each request waits on the
//! Desk until the person approves it on the computer, or discards it.

use std::fs;
use std::path::{Component, Path};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::bots::{self, Jobs};
use crate::room;

const FILE: &str = "pedidos.json";
/// Requests waiting at once: a chat stuck in a loop cannot flood the Desk.
const MAX_WAITING: usize = 20;
const MAX_TEXT: usize = 2000;
pub const KINDS: [&str; 5] = ["add_bot", "add_routine", "add_task", "new_channel", "channel_post"];

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Request {
    pub id: String,
    pub kind: String,
    /// What to create, already checked when it was asked.
    pub input: Value,
    /// Why, in the asking AI's words.
    #[serde(default)]
    pub why: String,
    pub created_ms: i64,
}

pub fn list(state_dir: &Path) -> Vec<Request> {
    fs::read_to_string(state_dir.join(FILE)).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
}

fn save(state_dir: &Path, requests: &[Request]) -> Result<()> {
    fs::create_dir_all(state_dir)?;
    let temp = state_dir.join(format!("{FILE}.tmp"));
    fs::write(&temp, serde_json::to_string_pretty(requests)?)?;
    fs::rename(temp, state_dir.join(FILE))?;
    Ok(())
}

fn text<'a>(input: &'a Value, field: &str) -> &'a str {
    input.get(field).and_then(Value::as_str).unwrap_or("").trim()
}

fn bot_exists(state_dir: &Path, id: &str) -> Result<()> {
    bots::find(state_dir, id).map(|_| ()).with_context(|| format!("no bot {id}; raiox_team lists them"))
}

/// A project as a folder under the home, written relative to it ("Projects/shop").
fn project_under_home(home: &Path, project: &str) -> Result<String> {
    let path = Path::new(project);
    let relative = if path.is_absolute() { path.strip_prefix(home).context("the project must be inside the home folder")? } else { path };
    if relative.as_os_str().is_empty() || relative.components().any(|c| !matches!(c, Component::Normal(_))) {
        bail!("name a project folder inside the home folder, such as Projects/shop");
    }
    if !home.join(relative).is_dir() {
        bail!("no folder {} in the home folder", relative.display());
    }
    Ok(relative.to_string_lossy().replace('\\', "/"))
}

/// Checks a request as it is asked, so the person only sees ones that can be applied.
fn checked(state_dir: &Path, home: &Path, kind: &str, input: &Value) -> Result<Value> {
    Ok(match kind {
        "add_bot" => {
            let agent = text(input, "agent");
            if !crate::connections::bot_agents().any(|a| a == agent) {
                bail!("agent must be one of: {}", crate::connections::bot_agents().collect::<Vec<_>>().join(", "));
            }
            json!({ "project": project_under_home(home, text(input, "project"))?, "agent": agent })
        }
        "add_routine" => {
            bot_exists(state_dir, text(input, "bot"))?;
            let days: Vec<u8> = input.get("days").and_then(Value::as_array).map(|d| d.iter().filter_map(Value::as_u64).filter_map(|d| u8::try_from(d).ok()).collect()).unwrap_or_default();
            let number = |field: &str| input.get(field).and_then(Value::as_u64).and_then(|n| u8::try_from(n).ok());
            let (hour, minute) = (number("hour").context("hour 0-23")?, number("minute").unwrap_or(0));
            if days.is_empty() || days.iter().any(|d| *d > 6) || hour > 23 || minute > 59 {
                bail!("days are 0 (Monday) to 6 (Sunday), hour 0-23, minute 0-59");
            }
            if text(input, "text").is_empty() {
                bail!("a routine needs the question it asks");
            }
            json!({ "bot": text(input, "bot"), "text": text(input, "text"), "days": days, "hour": hour, "minute": minute })
        }
        "add_task" => {
            bot_exists(state_dir, text(input, "bot"))?;
            if text(input, "title").is_empty() {
                bail!("a task needs a title");
            }
            json!({ "bot": text(input, "bot"), "title": text(input, "title"), "detail": text(input, "detail") })
        }
        "new_channel" => {
            let members: Vec<String> = input.get("members").and_then(Value::as_array).map(|m| m.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default();
            if members.is_empty() {
                bail!("a channel needs members (bot ids)");
            }
            for member in &members {
                bot_exists(state_dir, member)?;
            }
            if text(input, "name").is_empty() {
                bail!("a channel needs a name");
            }
            json!({ "name": text(input, "name"), "members": members })
        }
        "channel_post" => {
            let channel = text(input, "channel");
            crate::channels::list(state_dir).into_iter().find(|c| c.id == channel).context("no such channel; raiox_team lists them")?;
            if text(input, "text").is_empty() {
                bail!("a message needs text");
            }
            json!({ "channel": channel, "text": text(input, "text") })
        }
        other => bail!("unknown kind {other}; one of {}", KINDS.join(", ")),
    })
}

/// Queues a change for the person to approve; gives its id.
pub fn propose(state_dir: &Path, home: &Path, kind: &str, input: &Value, why: &str) -> Result<String> {
    let input = checked(state_dir, home, kind, input)?;
    if input.to_string().chars().count() > MAX_TEXT {
        bail!("too long");
    }
    let mut requests = list(state_dir);
    if requests.len() >= MAX_WAITING {
        bail!("{MAX_WAITING} requests already wait for the person; ask them to look at raiox's Desk first");
    }
    let now = room::now_ms();
    let id = format!("p{now:x}{}", requests.len());
    requests.push(Request { id: id.clone(), kind: kind.to_string(), input, why: why.trim().chars().take(MAX_TEXT).collect(), created_ms: now });
    save(state_dir, &requests)?;
    Ok(id)
}

/// Applies a request the person approved, and takes it off the Desk.
pub fn approve(state_dir: &Path, home: &Path, jobs: &Jobs, id: &str) -> Result<()> {
    let request = list(state_dir).into_iter().find(|r| r.id == id).context("no such request")?;
    let input = &request.input;
    match request.kind.as_str() {
        "add_bot" => {
            bots::activate(state_dir, home, text(input, "project"), text(input, "agent"), None)?;
        }
        "add_routine" => {
            let days: Vec<u8> = input["days"].as_array().map(|d| d.iter().filter_map(Value::as_u64).filter_map(|d| u8::try_from(d).ok()).collect()).unwrap_or_default();
            let number = |field: &str| input[field].as_u64().and_then(|n| u8::try_from(n).ok()).unwrap_or(0);
            crate::routines::create(state_dir, text(input, "bot"), text(input, "text"), &days, number("hour"), number("minute"), false)?;
        }
        "add_task" => {
            crate::tasks::add(state_dir, text(input, "bot"), text(input, "title"), text(input, "detail"), "chat")?;
        }
        "new_channel" => {
            let members: Vec<String> = input["members"].as_array().map(|m| m.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default();
            crate::channels::create(state_dir, text(input, "name"), &members)?;
        }
        "channel_post" => crate::channels::post(state_dir, jobs, text(input, "channel"), text(input, "text"))?,
        other => bail!("unknown kind {other}"),
    }
    discard(state_dir, id)
}

pub fn discard(state_dir: &Path, id: &str) -> Result<()> {
    save(state_dir, &list(state_dir).into_iter().filter(|r| r.id != id).collect::<Vec<_>>())
}

/// What a chat sees of the team, to name bots and channels in its requests.
pub fn team(state_dir: &Path) -> Value {
    let bots: Vec<Value> = bots::list(state_dir)
        .into_iter()
        .map(|b| json!({ "id": b.id, "project": b.name, "agent": b.agent, "role": b.persona.as_ref().map(|p| p.role.clone()) }))
        .collect();
    let channels: Vec<Value> = crate::channels::list(state_dir).into_iter().map(|c| json!({ "id": c.id, "name": c.name, "members": c.members })).collect();
    json!({ "bots": bots, "channels": channels, "waiting_for_approval": list(state_dir).len() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_waits_for_the_person_and_is_checked_first() {
        let home = std::env::temp_dir().join(format!("raiox-requests-{}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
        fs::create_dir_all(home.join("Projects/shop")).unwrap();
        let state = home.join(".state");
        assert!(propose(&state, &home, "add_bot", &json!({ "project": "../etc", "agent": "claude" }), "").is_err(), "only folders inside the home");
        assert!(propose(&state, &home, "add_bot", &json!({ "project": "Projects/shop", "agent": "copilot" }), "").is_err());
        let id = propose(&state, &home, "add_bot", &json!({ "project": home.join("Projects/shop").to_str().unwrap(), "agent": "claude" }), "a bot for the shop").unwrap();
        assert!(bots::list(&state).is_empty(), "nothing changes before the person approves");
        assert_eq!(list(&state)[0].input["project"], "Projects/shop");
        approve(&state, &home, &Jobs::default(), &id).unwrap();
        let bot = bots::list(&state).pop().expect("the bot exists once approved");
        assert!(list(&state).is_empty());
        assert!(propose(&state, &home, "add_routine", &json!({ "bot": bot.id, "text": "status", "days": [7], "hour": 9 }), "").is_err());
        propose(&state, &home, "add_task", &json!({ "bot": bot.id, "title": "Revisar o checkout" }), "").unwrap();
        let _ = fs::remove_dir_all(&home);
    }
}
