//! What each bot may do beyond reading its project: be reached from paired
//! devices, read the web, use the person's own MCP servers, and how many
//! tokens it may spend in a week. The "personal e-mail" bot stays on the
//! computer; the researcher reads the web; the costly one gets a ceiling.

use std::path::Path;

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::room;

/// The weekly ceilings the panel offers, in tokens.
pub const WEEK_CHOICES: [u64; 4] = [2_000_000, 10_000_000, 50_000_000, 200_000_000];
const WEEK_MS: i64 = 7 * 24 * 60 * 60 * 1000;

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Grants {
    /// Paired devices (the phone) may see and talk to it; off keeps it on this computer.
    #[serde(default = "yes")]
    pub devices: bool,
    /// May read web pages while it works.
    #[serde(default)]
    pub web: bool,
    /// May use the person's own MCP servers.
    #[serde(default)]
    pub mcp: bool,
    /// Tokens it may read and write in seven days; none means no ceiling.
    #[serde(default)]
    pub week_limit: Option<u64>,
}

impl Default for Grants {
    fn default() -> Self {
        Grants { devices: true, web: false, mcp: false, week_limit: None }
    }
}

/// The CLIs raiox can let read the web (Claude Code's WebFetch and WebSearch, opencode's webfetch).
pub fn web_possible(agent: &str) -> bool {
    matches!(agent, "claude" | "opencode")
}

/// The CLIs raiox can hand the person's MCP servers to (Claude Code reads its own settings).
pub fn mcp_possible(agent: &str) -> bool {
    agent == "claude"
}

/// What a bot read and wrote in the last seven days.
pub fn week_used(state_dir: &Path, bot: &str) -> u64 {
    crate::budget::bot_used_since(state_dir, bot, room::now_ms() - WEEK_MS)
}

/// Refuses a new turn once the bot spent its weekly ceiling.
pub fn check_week(state_dir: &Path, bot: &crate::bots::Bot) -> Result<()> {
    let Some(limit) = bot.grants.week_limit else { return Ok(()) };
    let used = week_used(state_dir, &bot.id);
    if used >= limit {
        bail!("this bot reached its weekly ceiling ({used} of {limit} tokens in seven days); raise it in the bot's settings, or wait");
    }
    Ok(())
}

/// The grants with the changes the panel sent (each field optional).
pub fn changed(current: &Grants, agent: &str, input: &Value) -> Result<Grants> {
    let flag = |name: &str, now: bool| input.get(name).and_then(Value::as_bool).unwrap_or(now);
    let web = flag("web", current.web);
    let mcp = flag("mcp", current.mcp);
    if web && !web_possible(agent) {
        bail!("raiox cannot let {agent} read the web");
    }
    if mcp && !mcp_possible(agent) {
        bail!("raiox cannot hand MCP servers to {agent}");
    }
    let week_limit = match input.get("week_limit") {
        None => current.week_limit,
        Some(value) => match value.as_u64().filter(|n| *n > 0) {
            None => None,
            Some(n) if WEEK_CHOICES.contains(&n) => Some(n),
            Some(_) => bail!("pick one of the offered ceilings"),
        },
    };
    Ok(Grants { devices: flag("devices", current.devices), web, mcp, week_limit })
}

/// The bots kept on this computer: paired devices neither see nor reach them.
pub fn desk_only(state_dir: &Path) -> Vec<String> {
    crate::bots::list(state_dir).into_iter().filter(|b| !b.grants.devices).map(|b| b.id).collect()
}

/// Every string a request carries (ids, a channel's members, where to carry a conversation).
fn strings(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::String(text) => out.push(text.clone()),
        Value::Array(items) => items.iter().for_each(|item| strings(item, out)),
        Value::Object(fields) => fields.values().for_each(|field| strings(field, out)),
        _ => {}
    }
}

/// Refuses a device's request that names a bot kept on this computer, or a channel it is in.
pub fn device_may(state_dir: &Path, input: &Value) -> Result<()> {
    let hidden = desk_only(state_dir);
    if hidden.is_empty() {
        return Ok(());
    }
    let mut named = Vec::new();
    strings(input, &mut named);
    let channels = crate::channels::list(state_dir);
    let reaches_hidden = named.iter().any(|text| {
        hidden.contains(text) || channels.iter().any(|c| &c.id == text && c.members.iter().any(|m| hidden.contains(m)))
    });
    if reaches_hidden {
        bail!("only on the computer");
    }
    Ok(())
}

/// The bots state as a device sees it: without the bots kept on this computer, nor their channels.
pub fn for_device(state_dir: &Path, mut state: Value) -> Value {
    let hidden = desk_only(state_dir);
    if hidden.is_empty() {
        return state;
    }
    let is_hidden = |value: &Value| value.as_str().is_some_and(|id| hidden.iter().any(|h| h == id));
    if let Some(bots) = state.get_mut("bots").and_then(Value::as_array_mut) {
        bots.retain(|bot| !is_hidden(&bot["id"]));
    }
    // Requests from a chat are approved on the computer only.
    if let Some(object) = state.as_object_mut() {
        object.remove("requests");
    }
    if let Some(channels) = state.get_mut("channels").and_then(Value::as_array_mut) {
        channels.retain(|channel| !channel["members"].as_array().is_some_and(|members| members.iter().any(|m| is_hidden(&m["bot"]))));
    }
    state
}

/// The board as a device sees it: without the tasks of bots kept on this computer.
pub fn tasks_for_device(state_dir: &Path, mut board: Value) -> Value {
    let hidden = desk_only(state_dir);
    if hidden.is_empty() {
        return board;
    }
    let tasks = board["tasks"].as_array().cloned().unwrap_or_default();
    let threads = board["threads"].as_array().cloned().unwrap_or_default();
    let kept: Vec<(Value, Value)> = tasks.into_iter().zip(threads).filter(|(task, _)| !task["bot"].as_str().is_some_and(|bot| hidden.iter().any(|h| h == bot))).collect();
    board["tasks"] = Value::Array(kept.iter().map(|(task, _)| task.clone()).collect());
    board["threads"] = Value::Array(kept.into_iter().map(|(_, thread)| thread).collect());
    board
}

/// What the panel shows: the grants, what this CLI can be given, and the week so far.
pub fn state(state_dir: &Path, bot: &crate::bots::Bot) -> Value {
    json!({
        "devices": bot.grants.devices,
        "web": bot.grants.web,
        "mcp": bot.grants.mcp,
        "week_limit": bot.grants.week_limit,
        "week_used": week_used(state_dir, &bot.id),
        "week_choices": WEEK_CHOICES,
        "web_possible": web_possible(&bot.agent),
        "mcp_possible": mcp_possible(&bot.agent),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grants_change_only_where_the_cli_allows() {
        let base = Grants::default();
        assert!(base.devices && !base.web && !base.mcp, "new bots reach the phone and nothing else");
        let web = changed(&base, "claude", &json!({ "web": true, "mcp": true })).unwrap();
        assert!(web.web && web.mcp && web.devices);
        assert!(changed(&base, "codex", &json!({ "web": true })).is_err(), "Codex has no web switch");
        assert!(changed(&base, "opencode", &json!({ "mcp": true })).is_err());
        assert_eq!(changed(&base, "codex", &json!({ "week_limit": 10_000_000 })).unwrap().week_limit, Some(10_000_000));
        assert!(changed(&base, "codex", &json!({ "week_limit": 123 })).is_err());
        assert_eq!(changed(&web, "claude", &json!({ "week_limit": 0, "devices": false })).unwrap(), Grants { devices: false, week_limit: None, ..web });
    }

    #[test]
    fn a_bot_kept_on_the_computer_is_out_of_the_phones_reach() {
        let home = std::env::temp_dir().join(format!("raiox-grants-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        for project in ["Projects/mail", "Projects/site"] {
            std::fs::create_dir_all(home.join(project)).unwrap();
        }
        let state = home.join(".state");
        let mail = crate::bots::activate(&state, &home, "Projects/mail", "claude", None).unwrap();
        let site = crate::bots::activate(&state, &home, "Projects/site", "claude", None).unwrap();
        let channel = crate::channels::create(&state, "time", &[mail.id.clone(), site.id.clone()]).unwrap();
        assert!(device_may(&state, &json!({ "id": mail.id })).is_ok(), "every bot reaches the phone at first");
        crate::bots::set_grants(&state, &crate::bots::Jobs::default(), &mail.id, &json!({ "devices": false })).unwrap();
        assert!(device_may(&state, &json!({ "id": mail.id, "text": "oi" })).is_err());
        assert!(device_may(&state, &json!({ "channel": channel.id, "text": "oi" })).is_err(), "nor through a channel it is in");
        assert!(device_may(&state, &json!({ "id": site.id })).is_ok());
        let seen = for_device(&state, json!({ "bots": [{ "id": mail.id }, { "id": site.id }], "channels": [{ "id": channel.id, "members": [{ "bot": mail.id }, { "bot": site.id }] }] }));
        assert_eq!((seen["bots"].as_array().unwrap().len(), seen["channels"].as_array().unwrap().len()), (1, 0));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn old_bots_keep_reaching_the_phone() {
        let read: Grants = serde_json::from_str("{}").unwrap();
        assert_eq!(read, Grants::default());
    }
}
