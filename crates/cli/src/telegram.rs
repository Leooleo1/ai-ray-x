//! The Telegram bridge: the person talks to their AIs from Telegram, through a
//! bot of their own (made with @BotFather). raiox asks Telegram for new
//! messages (long polling: nothing reaches this computer from outside), only
//! the one chat that typed the pairing code is heard, and only the AIs that
//! may talk to a phone answer there. The token stays in a private file and
//! never goes on a command line.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::bots::{self, Bot, Jobs};
use crate::{room, threads};

const FILE: &str = "telegram.json";
const API: &str = "https://api.telegram.org";
/// How long one request for new messages waits on Telegram's side.
const POLL_S: u64 = 25;
const PAIR_MS: i64 = 10 * 60 * 1000;
const CODE_LETTERS: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
const CODE_LENGTH: usize = 8;
/// Telegram takes 4096 characters a message.
const MAX_REPLY: usize = 3900;
const THREAD_TITLE: &str = "Telegram";
const IDLE: Duration = Duration::from_secs(5);
const WHILE_WAITING: Duration = Duration::from_secs(2);
const AFTER_ERROR: Duration = Duration::from_secs(15);
/// A turn that never showed up (refused before it started) stops being waited for.
const GIVE_UP_MS: i64 = 60_000;

#[derive(Serialize, Deserialize, Clone, Default)]
struct Config {
    token: String,
    username: String,
    #[serde(default)]
    chat: Option<i64>,
    #[serde(default)]
    chat_name: String,
    /// The AI that answers now.
    #[serde(default)]
    bot: Option<String>,
    #[serde(default)]
    offset: i64,
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    code_until: i64,
    #[serde(default)]
    portuguese: bool,
    #[serde(default)]
    waiting: Vec<Waiting>,
}

#[derive(Serialize, Deserialize, Clone)]
struct Waiting {
    bot: String,
    thread: String,
    since: i64,
}

fn load(state_dir: &Path) -> Option<Config> {
    fs::read_to_string(state_dir.join(FILE)).ok().and_then(|text| serde_json::from_str(&text).ok())
}

fn save(state_dir: &Path, config: &Config) -> Result<()> {
    let file = state_dir.join(FILE);
    fs::create_dir_all(state_dir)?;
    fs::write(&file, serde_json::to_string_pretty(config)?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&file, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

fn new_code() -> Result<String> {
    let mut buffer = [0u8; CODE_LENGTH];
    SystemRandom::new().fill(&mut buffer).map_err(|_| anyhow::anyhow!("no secure randomness available"))?;
    Ok(buffer.iter().map(|b| CODE_LETTERS[usize::from(*b) % CODE_LETTERS.len()] as char).collect())
}

/// A token as @BotFather gives it: digits, a colon, letters. Anything else could break the request.
fn valid_token(token: &str) -> bool {
    let Some((id, secret)) = token.split_once(':') else { return false };
    !id.is_empty() && id.chars().all(|c| c.is_ascii_digit()) && secret.len() >= 20 && secret.len() <= 80 && secret.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// A value for curl's config file: quoted, with its quotes and backslashes escaped.
fn quoted(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

fn api_base() -> String {
    std::env::var("RAIOX_TELEGRAM_API").ok().filter(|base| base.starts_with("http://127.0.0.1:")).unwrap_or_else(|| API.to_string())
}

/// One Bot API call. The URL (it holds the token) goes to curl on stdin, never in its arguments.
fn call(token: &str, method: &str, body: &Value, wait_s: u64) -> Result<Value> {
    let base = api_base();
    let proto = if base.starts_with("https://") { "=https" } else { "=http" };
    let config = format!(
        "url = {}\nheader = \"Content-Type: application/json\"\ndata = {}\nmax-time = {}\nproto = \"{proto}\"\nsilent\n",
        quoted(&format!("{base}/bot{token}/{method}")),
        quoted(&body.to_string()),
        wait_s + 10
    );
    let mut child = crate::procs::command("curl").args(["-K", "-"]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().context("curl is not available")?;
    child.stdin.take().context("no stdin")?.write_all(config.as_bytes())?;
    let output = child.wait_with_output()?;
    let answer: Value = serde_json::from_slice(&output.stdout).context("Telegram did not answer")?;
    if answer["ok"] != true {
        bail!("Telegram: {}", answer["description"].as_str().unwrap_or("refused"));
    }
    Ok(answer["result"].clone())
}

fn send(token: &str, chat: i64, text: &str) {
    let text: String = text.chars().take(MAX_REPLY).collect();
    let _ = call(token, "sendMessage", &json!({ "chat_id": chat, "text": text }), 0);
}

/// What the panel shows. Never the token.
pub fn state(state_dir: &Path) -> Value {
    let Some(config) = load(state_dir) else { return json!({ "connected": false }) };
    let code_live = config.chat.is_none() && config.code_until > room::now_ms();
    let bot = current(state_dir, &config).map(|b| b.id);
    json!({
        "connected": true,
        "username": config.username,
        "paired": config.chat.is_some(),
        "chat_name": config.chat_name,
        "code": if code_live { config.code.clone() } else { None },
        "code_until": config.code_until,
        "bot": bot,
    })
}

/// Connects a bot made with @BotFather; gives the pairing code to send it.
pub fn connect(state_dir: &Path, token: &str) -> Result<Value> {
    let token = token.trim();
    if !valid_token(token) {
        bail!("this is not a bot token from @BotFather (digits, a colon, letters)");
    }
    let me = call(token, "getMe", &json!({}), 0)?;
    let username = me["username"].as_str().context("Telegram did not name the bot")?.to_string();
    save(state_dir, &Config { token: token.to_string(), username, code: Some(new_code()?), code_until: room::now_ms() + PAIR_MS, ..Config::default() })?;
    Ok(state(state_dir))
}

/// A new pairing code, for another chat or after the last one expired.
pub fn pair_again(state_dir: &Path) -> Result<Value> {
    let config = load(state_dir).context("connect a Telegram bot first")?;
    save(state_dir, &Config { chat: None, chat_name: String::new(), code: Some(new_code()?), code_until: room::now_ms() + PAIR_MS, waiting: Vec::new(), ..config })?;
    Ok(state(state_dir))
}

pub fn forget(state_dir: &Path) -> Result<Value> {
    let _ = fs::remove_file(state_dir.join(FILE));
    Ok(state(state_dir))
}

/// The AIs Telegram reaches: the ones that may talk to a phone.
fn reachable(state_dir: &Path) -> Vec<Bot> {
    bots::list(state_dir).into_iter().filter(|b| b.grants.devices).collect()
}

fn current(state_dir: &Path, config: &Config) -> Option<Bot> {
    let all = reachable(state_dir);
    all.iter().find(|b| Some(&b.id) == config.bot.as_ref()).cloned().or_else(|| all.into_iter().next())
}

fn label(bot: &Bot) -> String {
    let agent = crate::connections::KNOWN.iter().find(|k| k.id == bot.agent).map_or(bot.agent.as_str(), |k| k.name);
    let role = bot.persona.as_ref().map(|p| p.role.clone()).filter(|r| !r.is_empty()).map(|r| format!(" ({r})")).unwrap_or_default();
    format!("{} · {agent}{role}", bot.name)
}

fn words(pt: bool, en: &'static str, portuguese: &'static str) -> &'static str {
    if pt { portuguese } else { en }
}

fn roster(state_dir: &Path, config: &Config) -> String {
    let all = reachable(state_dir);
    let chosen = current(state_dir, config).map(|b| b.id);
    let pt = config.portuguese;
    if all.is_empty() {
        return words(pt, "No AI may talk to a phone. Turn it on in an AI's settings in raiox.", "Nenhuma IA pode falar pelo celular. Ligue isso nos ajustes de uma IA no raiox.").to_string();
    }
    let lines: Vec<String> = all.iter().enumerate().map(|(i, b)| format!("{}{}. {}", if Some(&b.id) == chosen.as_ref() { "→ " } else { "" }, i + 1, label(b))).collect();
    format!("{}\n{}", lines.join("\n"), words(pt, "/bot 2 picks who answers.", "/bot 2 escolhe quem responde."))
}

/// A message from Telegram; gives what to answer right away, if anything.
fn on_message(state_dir: &Path, jobs: &Jobs, config: &mut Config, chat: i64, name: &str, text: &str) -> Option<String> {
    let text = text.trim();
    let Some(paired) = config.chat else {
        let typed = text.trim_start_matches("/start").trim().to_ascii_uppercase();
        let code_ok = config.code.as_deref().is_some_and(|code| code == typed) && config.code_until > room::now_ms();
        if !code_ok {
            return Some(words(config.portuguese, "This bot belongs to someone's raiox. To pair, send the code raiox shows (Settings › Phone).", "Este bot é de um raiox. Para parear, mande o código que o raiox mostra (Ajustes › Celular).").to_string());
        }
        config.chat = Some(chat);
        config.chat_name = name.chars().take(60).collect();
        config.code = None;
        let hello = words(config.portuguese, "Paired with raiox. Write and the AI below answers here.", "Pareado com o raiox. Escreva e a IA abaixo responde aqui.");
        return Some(format!("{hello}\n\n{}", roster(state_dir, config)));
    };
    if chat != paired {
        return None;
    }
    if text == "/bots" || text == "/start" {
        return Some(roster(state_dir, config));
    }
    if let Some(number) = text.strip_prefix("/bot") {
        let all = reachable(state_dir);
        let picked = number.trim().parse::<usize>().ok().and_then(|n| n.checked_sub(1)).and_then(|i| all.get(i));
        return Some(match picked {
            Some(bot) => {
                config.bot = Some(bot.id.clone());
                format!("{} {}", words(config.portuguese, "Now answering:", "Agora responde:"), label(bot))
            }
            None => roster(state_dir, config),
        });
    }
    let Some(bot) = current(state_dir, config) else { return Some(roster(state_dir, config)) };
    if jobs.is_busy(&bot.id) {
        return Some(format!("{} {}", bot.name, words(config.portuguese, "is working; send it again when it is done.", "está trabalhando; mande de novo quando ele terminar.")));
    }
    let thread = match threads::list(state_dir, &bot.id).into_iter().find(|t| t.title == THREAD_TITLE) {
        Some(thread) => thread.id,
        None => match threads::create(state_dir, &bot.id, THREAD_TITLE) {
            Ok(made) => made.id,
            Err(error) => return Some(error.to_string()),
        },
    };
    let since = room::now_ms();
    match bots::ask(state_dir, jobs, &bot.id, &thread, text) {
        Ok(()) => {
            config.waiting.push(Waiting { bot: bot.id, thread, since });
            None
        }
        Err(error) => Some(error.to_string()),
    }
}

/// The answer a turn left, as Telegram shows it.
fn answer_text(message: &room::Message, pt: bool) -> String {
    let reply = &message.data["reply"];
    let text = reply["text"].as_str().unwrap_or("").trim().to_string();
    let extra = match message.kind.as_str() {
        "failed" => format!("{} {}", words(pt, "Failed:", "Falhou:"), message.data["error"].as_str().unwrap_or("")),
        "stopped" => words(pt, "Stopped.", "Parado.").to_string(),
        "draft" => words(pt, "(a draft waits in raiox to be applied)", "(um rascunho espera no raiox para ser aplicado)").to_string(),
        _ if reply["proposal"].as_str().is_some_and(|p| !p.is_empty()) => format!("{} {}", words(pt, "(proposal to approve in raiox:", "(proposta para aprovar no raiox:"), reply["proposal"].as_str().unwrap_or("").replace('`', "") + ")"),
        _ => String::new(),
    };
    [text, extra].into_iter().filter(|part| !part.is_empty()).collect::<Vec<_>>().join("\n\n")
}

/// Answers of the turns Telegram started that ended; the rest keep waiting.
fn finished(state_dir: &Path, jobs: &Jobs, waiting: Vec<Waiting>, pt: bool) -> (Vec<String>, Vec<Waiting>) {
    let now = room::now_ms();
    waiting.into_iter().fold((Vec::new(), Vec::new()), |(mut done, mut still), w| {
        if jobs.is_busy(&w.bot) {
            still.push(w);
            return (done, still);
        }
        let messages = room::read_messages(&threads::chat_file(state_dir, &w.bot, &w.thread));
        let ended = messages.iter().rev().find(|m| m.author == "bot" && m.ms >= w.since && matches!(m.kind.as_str(), "answer" | "draft" | "failed" | "stopped"));
        match ended {
            Some(message) => done.push(answer_text(message, pt)),
            None if now - w.since < GIVE_UP_MS => still.push(w),
            None => {}
        }
        (done, still)
    })
}

/// One round: new messages from Telegram, then answers that are ready. Gives how long to rest.
fn tick(state_dir: &Path, jobs: &Jobs) -> Result<Duration> {
    let Some(mut config) = load(state_dir) else { return Ok(IDLE) };
    let poll = if config.waiting.is_empty() { POLL_S } else { 0 };
    let updates = call(&config.token, "getUpdates", &json!({ "offset": config.offset, "timeout": poll, "allowed_updates": ["message"] }), poll)?;
    let was_paired = config.chat.is_some();
    for update in updates.as_array().into_iter().flatten() {
        config.offset = config.offset.max(update["update_id"].as_i64().unwrap_or(0) + 1);
        let message = &update["message"];
        let (Some(text), Some(chat)) = (message["text"].as_str(), message["chat"]["id"].as_i64()) else { continue };
        if config.chat.is_none() || config.chat == Some(chat) {
            config.portuguese = message["from"]["language_code"].as_str().is_some_and(|l| l.starts_with("pt"));
        }
        let name = message["from"]["first_name"].as_str().unwrap_or("");
        if let Some(reply) = on_message(state_dir, jobs, &mut config, chat, name, text) {
            send(&config.token, chat, &reply);
        }
    }
    let (done, still) = finished(state_dir, jobs, std::mem::take(&mut config.waiting), config.portuguese);
    if let Some(chat) = config.chat {
        done.iter().for_each(|text| send(&config.token, chat, text));
    }
    // The panel may have changed the file while Telegram held the request: keep its code, or stop if it disconnected.
    let Some(fresh) = load(state_dir).filter(|fresh| fresh.token == config.token) else { return Ok(IDLE) };
    let (code, code_until) = if config.chat.is_some() && !was_paired { (None, 0) } else { (fresh.code, fresh.code_until) };
    let (chat, chat_name) = if fresh.chat.is_none() && was_paired { (None, String::new()) } else { (config.chat, config.chat_name.clone()) };
    save(state_dir, &Config { code, code_until, chat, chat_name, waiting: still, ..config.clone() })?;
    Ok(if config.waiting.is_empty() && done.is_empty() { Duration::ZERO } else { WHILE_WAITING })
}

/// Starts the bridge next to the panel; it does nothing until a bot is connected.
pub fn spawn(state_dir: PathBuf, jobs: Jobs) {
    std::thread::spawn(move || loop {
        let rest = tick(&state_dir, &jobs).unwrap_or(AFTER_ERROR);
        std::thread::sleep(rest);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_botfather_token_is_taken() {
        assert!(valid_token("123456789:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"));
        assert!(!valid_token("123:short"));
        assert!(!valid_token("123456789:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\"\nurl = \"http://evil"), "nothing that could add a line to curl's config");
        assert_eq!(quoted("a\"b\\c"), "\"a\\\"b\\\\c\"");
    }

    #[test]
    fn only_the_chat_with_the_code_is_heard() {
        let home = std::env::temp_dir().join(format!("raiox-telegram-{}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
        let state = home.join(".state");
        let jobs = Jobs::default();
        let mut config = Config { token: "1:x".into(), code: Some("ABCD2345".into()), code_until: room::now_ms() + PAIR_MS, ..Config::default() };
        assert!(on_message(&state, &jobs, &mut config, 7, "Eve", "/start WRONG").is_some_and(|r| r.contains("code")));
        assert_eq!(config.chat, None);
        on_message(&state, &jobs, &mut config, 9, "Leo", "/start abcd2345").unwrap();
        assert_eq!((config.chat, config.code.clone()), (Some(9), None), "the code pairs once, case aside");
        assert_eq!(on_message(&state, &jobs, &mut config, 7, "Eve", "oi"), None, "another chat gets no answer at all");
        let _ = fs::remove_dir_all(&home);
    }
}
