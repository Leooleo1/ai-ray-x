//! One turn of a bot, in its own thread. The CLI answers; when it asks for a
//! command the person allowed for the project, raiox runs it and the CLI goes
//! on with the output, a few times at most. A command the person approves
//! by hand starts a turn the same way.

use std::path::{Path, PathBuf};

use anyhow::Context;

use serde_json::json;

use super::{drafted, find, list, note, said, set_thread_session, Bot, Jobs, Persona};
use crate::bot_run::{self, Mode, Reply};
use crate::room::{self, Message};
use crate::{commands, drafts};

/// Commands that run without asking, per question, before the bot has to answer.
const MAX_AUTO_RUNS: usize = 3;
/// Commands one answer may run by itself when the bot runs without asking (a multi-step loop, like a CLI in auto mode).
const MAX_ALONE_RUNS: usize = 12;
/// Screenshots the bot gets per question.
const MAX_SHOTS: usize = 2;
const SHOT_SIZE: (u32, u32) = (1280, 800);

/// What a turn runs: the text for the CLI, or first a command whose output replaces it.
pub(super) struct Turn {
    pub mode: Mode,
    pub text: String,
    /// The person's message, shown before the turn runs.
    pub asked: Option<Message>,
    /// For an edit: the home folder, under which the draft copy keeps the project's path.
    pub home: Option<PathBuf>,
    /// A command the person approved, run before the CLI goes on with its output.
    pub command: Option<String>,
    /// Images the person attached to this message, stored in `<state>/uploads/<bot>`.
    pub images: Vec<PathBuf>,
    /// Whether this turn's CLI session is saved on the thread (false for cross-CLI audits).
    pub keep_session: bool,
    /// True for Dev/Infra audit turns (skips chained proposal review and marks answers).
    pub is_audit: bool,
    /// CLI id that ran the audit, recorded on audit answers for transparency.
    pub auditor_agent: Option<String>,
}

/// How this bot's CLI is called, the same for every round of the turn.
pub(super) struct Setup {
    /// The guide that stays the same between messages: how raiox works, the persona, the wiring.
    stable: String,
    /// What the other bots here and in wired projects did lately, from ai-memory.
    team: String,
    /// The project's pages that relate to this question.
    related: String,
    others: Vec<String>,
    model: Option<String>,
    effort: Option<String>,
    scratch: PathBuf,
    /// The bot's screenshots folder, which its CLI may read.
    shots: PathBuf,
    /// Where raiox keeps which rules each conversation read.
    state_dir: PathBuf,
}

impl Setup {
    pub(super) fn new(state_dir: &Path, bot: &Bot, question: &str, mode: Mode) -> Self {
        // Only the projects wired to this one: the bot names no others and can message no others.
        let others = crate::links::linked(state_dir, &bot.name);
        let mut unwired: Vec<String> = list(state_dir).into_iter().map(|b| b.name).filter(|name| *name != bot.name && !others.contains(name)).collect();
        unwired.sort();
        unwired.dedup();
        let defaults = crate::settings::Settings::load(state_dir).defaults_for(&bot.agent);
        let persona = bot.persona.as_ref().map(Persona::prompt).unwrap_or_default();
        // The team notes from ai-memory: what the other bots here and in wired projects did lately.
        Setup {
            state_dir: state_dir.to_path_buf(),
            stable: bot_run::guide(&bot.name, &persona, &others, &unwired, bot.grants.full_access),
            team: crate::team::context_for(state_dir, bot),
            // The project's own pages that relate to the question, kept decisions included.
            related: format!("{}{}", crate::memory::related(&bot.dir, question), crate::wiki::for_turn(state_dir, &bot.id, question)),
            others,
            // Talk can run on a lighter model; changes always on the bot's own.
            model: if mode == Mode::Talk { bot.talk_model.clone() } else { None }.or_else(|| bot.model.clone()).or(defaults.model),
            effort: bot.effort.clone().or(defaults.effort),
            scratch: state_dir.join("bots").join(format!("{}.run", bot.id)),
            shots: crate::preview::ai_dir(state_dir, &bot.id),
        }
    }
}

/// Runs the turn and writes each step of it into the conversation file.
pub(super) fn run(state_dir: &Path, jobs: &Jobs, bot: &Bot, thread: &str, turn: Turn, session: Option<String>) {
    let file = crate::threads::chat_file(state_dir, &bot.id, thread);
    let say = |message: Message| {
        let _ = room::append_messages(&file, vec![message]);
    };
    // An approved edit happens in the bot's working copy of the project, never in the project itself.
    let work = match (turn.mode, &turn.home) {
        (Mode::Edit, Some(home)) => match drafts::prepare(state_dir, home, &bot.id, &bot.dir) {
            Ok(work) => Some(work),
            Err(error) => return say(note("failed", json!({ "error": format!("could not make the draft copy: {error:#}") }))),
        },
        _ => None,
    };
    let setup = Setup::new(state_dir, bot, &turn.text, turn.mode);
    crate::preview::clear_ai_dir(state_dir, &bot.id);
    let mut text = match turn.mode {
        Mode::Talk => format!("{}{}", crate::preview::for_cli(state_dir, &bot.id, &bot.dir), turn.text),
        Mode::Edit => turn.text,
    };
    let mut images: Vec<PathBuf> = Vec::new();
    if let Some(command) = &turn.command {
        let Some(output) = command_output(jobs, bot, command, false, &say) else { return };
        text = output;
    } else if !turn.images.is_empty() {
        let (prompt, files) = attach_images(state_dir, &bot.id, &turn.images, text, &say);
        text = prompt;
        images = files;
    }
    // A draft starts a fresh session in its copy; talk continues the conversation's own.
    let mut session = if work.is_some() { None } else { session };
    let mut runs = 0;
    let mut shots_taken = 0;
    let mut app_started = false;
    loop {
        let dir = work.as_ref().map_or(bot.dir.as_path(), |work| work.dir.as_path());
        let conversation = turn.keep_session.then(|| format!("{}:{thread}", bot.id));
        let asked = ask_cli(jobs, bot, &setup, dir, session.as_deref(), turn.mode, &text, conversation.as_deref(), &images);
        images.clear();
        let reply = match asked {
            Ok(reply) => reply,
            Err(error) => {
                let stopped = error.to_string() == bot_run::STOPPED;
                // Stopped to be corrected: the same session goes on with the person's words.
                if stopped && let Some((correction, reached)) = jobs.take_correction(&bot.id) {
                    say(note("stopped", json!({})));
                    say(said("ask", json!({ "text": correction, "correction": true })));
                    text = match &reached {
                        Some(_) => format!("(raiox: the person stopped you midway to correct the course.) {correction}"),
                        None => format!("{text}\n\n(raiox: the person stopped you midway to correct the course.) {correction}"),
                    };
                    if reached.is_some() {
                        session = reached;
                        if turn.keep_session && bot_run::keeps_session(turn.mode) {
                            let _ = set_thread_session(state_dir, &bot.id, thread, session.clone());
                        }
                    }
                    continue;
                }
                if work.is_some() {
                    drafts::abandon(state_dir, &bot.id);
                }
                return say(if stopped { note("stopped", json!({})) } else { note("failed", json!({ "error": format!("{error:#}") })) });
            }
        };
        // Messages for projects without a bot are dropped: nobody there would read them.
        let reply = Reply {
            to: reply.to.into_iter().filter(|n| setup.others.contains(&n.project)).collect(),
            model: setup.model.clone(),
            effort: setup.effort.clone(),
            ..reply
        };
        if turn.keep_session && bot_run::keeps_session(turn.mode) && reply.session.is_some() && reply.session != session {
            let _ = set_thread_session(state_dir, &bot.id, thread, reply.session.clone());
            session = reply.session.clone();
        } else if !turn.keep_session && bot_run::keeps_session(turn.mode) && reply.session.is_some() {
            // Keep resuming within this single multi-round audit turn without saving across CLI boundaries.
            session = reply.session.clone();
        }
        if let Some(work) = &work {
            return say(drafted(state_dir, &bot.id, work, &reply));
        }
        // Past the project's daily limit, allowed commands wait for a click like any other.
        let limit = if runs_alone(state_dir, bot) { MAX_ALONE_RUNS } else { MAX_AUTO_RUNS };
        let within = runs < limit && crate::budget::check(state_dir, &bot.name).is_ok();
        let next = if within { allowed_command(state_dir, bot, &reply) } else { None };
        let wanted: Vec<String> = if next.is_none() && shots_taken < MAX_SHOTS && crate::budget::check(state_dir, &bot.name).is_ok() {
            reply.shots.iter().take(MAX_SHOTS - shots_taken).cloned().collect()
        } else {
            Vec::new()
        };
        let proposed = reply.proposal.is_some();
        // Tasks it left for the board go there, for this bot.
        crate::tasks::from_answer(state_dir, &bot.id, &reply.tasks);
        let mut answer_data = json!({ "mode": "talk", "reply": reply });
        if turn.is_audit {
            answer_data["audit"] = json!(true);
            if let Some(agent) = &turn.auditor_agent {
                answer_data["auditor_agent"] = json!(agent);
            }
        }
        let answer = note("answer", answer_data);
        let answer_ms = answer.ms;
        say(answer);
        // A project pair: the chosen reviewer reads every proposal before the person approves it (skipped for audit turns).
        if !turn.is_audit && proposed && let Some(reviewer) = find(state_dir, &bot.id).ok().and_then(|b| b.reviewer)
            && let Err(error) = super::review::request(state_dir, jobs, &bot.id, thread, answer_ms, &reviewer)
        {
            say(note("failed", json!({ "error": format!("{error:#}"), "review": answer_ms })));
        }
        if let Some(command) = next {
            runs += 1;
            let Some(output) = command_output(jobs, bot, &command, true, &say) else { return };
            text = output;
            continue;
        }
        // Running without asking, the app it offers (or needs for a screenshot) starts by itself, once a turn.
        let wants_app = reply.open.iter().any(|p| p == "app") || (wanted.iter().any(|w| w.starts_with('/')) && !crate::preview::is_running(&bot.id));
        if wants_app && !app_started && shots_taken < MAX_SHOTS && runs_alone(state_dir, bot) {
            app_started = true;
            shots_taken += 1;
            let Some((prompt, files)) = start_app(state_dir, jobs, bot, &say) else { return };
            text = prompt;
            images = files;
            continue;
        }
        if wanted.is_empty() {
            return;
        }
        shots_taken += wanted.len();
        let Some((prompt, files)) = screenshots(state_dir, jobs, bot, &wanted, &say) else { return };
        text = prompt;
        images = files;
    }
}

/// Where a screenshot request points: the running app's page, a local URL, or the simulator.
fn take_shot(state_dir: &Path, bot: &Bot, target: &str) -> anyhow::Result<(PathBuf, String)> {
    if target.eq_ignore_ascii_case("simulator") {
        let name = crate::simulator::booted().unwrap_or_else(|| "iOS simulator".to_string());
        return Ok((crate::simulator::shot(state_dir, &bot.id)?, name));
    }
    let url = if target.starts_with('/') {
        let base = crate::preview::url_of(state_dir, &bot.id).context("the app is not running in raiox; the person can start it in See the app")?;
        format!("{}{target}", base.trim_end_matches('/'))
    } else {
        target.to_string()
    };
    Ok((crate::preview::shot(state_dir, &bot.id, &url, SHOT_SIZE.0, SHOT_SIZE.1)?, url))
}

/// Takes the screenshots the bot asked for, shows each in the conversation, and gives the CLI's
/// next prompt with the files (none when not one could be taken).
fn screenshots(state_dir: &Path, jobs: &Jobs, bot: &Bot, wanted: &[String], say: &dyn Fn(Message)) -> Option<(String, Vec<PathBuf>)> {
    let mut taken = Vec::new();
    for target in wanted {
        jobs.set_step(&bot.id, crate::bot_steps::Step { kind: "web", target: target.clone() });
        match take_shot(state_dir, bot, target) {
            Ok((file, place)) => {
                let name = file.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
                say(said("shot", json!({ "target": target, "place": place, "name": name })));
                match crate::preview::hand_over(state_dir, &bot.id, &file) {
                    Ok(handed) => taken.push((handed, place)),
                    Err(error) => say(note("failed", json!({ "error": format!("raiox could not hand the screenshot over: {error:#}") }))),
                }
            }
            Err(error) => say(note("failed", json!({ "error": format!("raiox could not take the screenshot of {target}: {error:#}") }))),
        }
    }
    if taken.is_empty() {
        return None;
    }
    let list: Vec<String> = taken.iter().map(|(file, place)| format!("- {place}: {}", file.display())).collect();
    let prompt = format!(
        "raiox took the screenshots you asked for ({}×{}):\n{}\nLook at them (open each file with your Read tool if you have one) and go on answering the person's last question.",
        SHOT_SIZE.0,
        SHOT_SIZE.1,
        list.join("\n")
    );
    Some((prompt, taken.into_iter().map(|(file, _)| file).collect()))
}

/// One answer from the CLI; when it lost the session (its history was cleared), talk goes on in a fresh one.
/// One answer from the CLI. A Claude Code talk turn in a conversation runs on its live session
/// (`conversation` names it): the related pages then go with the message, as the process keeps the
/// guide it started with. When the CLI lost the session (its history was cleared), talk goes on in
/// a fresh one.
#[allow(clippy::too_many_arguments)]
pub(super) fn ask_cli(jobs: &Jobs, bot: &Bot, setup: &Setup, dir: &Path, session: Option<&str>, mode: Mode, text: &str, conversation: Option<&str>, images: &[PathBuf]) -> anyhow::Result<Reply> {
    let live = conversation.filter(|_| bot.agent == "claude" && mode == Mode::Talk);
    let guide = format!("{}{}{}", setup.stable, setup.team, if live.is_some() { "" } else { &setup.related });
    // Talk can read the bot's screenshots; an edit works in its draft only.
    let shots = (mode == Mode::Talk).then_some(setup.shots.as_path());
    let target = bot_run::Target {
        agent: &bot.agent,
        dir,
        guide,
        session,
        model: setup.model.as_deref(),
        effort: setup.effort.as_deref(),
        shots,
        images,
        web: bot.grants.web,
        mcp: bot.grants.mcp,
        full: bot.grants.full_access,
    };
    let stop = || jobs.should_stop(&bot.id);
    let on_trail = |trail: &crate::bot_trail::Trail| jobs.set_trail(&bot.id, trail);
    if let Some(conversation) = live {
        let text = if setup.related.is_empty() { text.to_string() } else { format!("{}\n\n{text}", setup.related.trim()) };
        return match crate::live::ask(conversation, &setup.stable, &target, &text, &setup.scratch, &stop, &on_trail) {
            Err(error) if session.is_some() && bot_run::session_lost(&error) => {
                crate::live::ask(conversation, &setup.stable, &bot_run::Target { session: None, ..target }, &text, &setup.scratch, &stop, &on_trail)
            }
            other => other,
        };
    }
    // Codex, Cursor, Antigravity and opencode read the guide only when a conversation starts:
    // in one already going, the question carries its own pages (project memory, the team's wiki).
    let carried = (bot.agent != "claude" && session.is_some() && !setup.related.trim().is_empty()).then(|| format!("{}\n\n{text}", setup.related.trim()));
    // When raiox's rules changed since this conversation read them, it reads the whole guide again, once.
    let rules = crate::rules::version(bot.grants.full_access);
    let key = conversation.unwrap_or("");
    let stale = bot.agent != "claude" && mode == Mode::Talk && session.is_some() && !key.is_empty() && crate::rules::read_by(&setup.state_dir, key) != Some(rules);
    let again = stale.then(|| format!("{}\n\n---\n\n{text}", target.guide));
    let asked = again.as_deref().or(carried.as_deref()).unwrap_or(text);
    let answered = bot_run::run(&target, mode, asked, &setup.scratch, &stop, &on_trail);
    // A conversation that started now, or read the guide again, has today's rules.
    if answered.is_ok() && bot.agent != "claude" && mode == Mode::Talk && !key.is_empty() && (session.is_none() || stale) {
        crate::rules::remember(&setup.state_dir, key, rules);
    }
    match answered {
        // A fresh conversation gets the pages in its guide again.
        Err(error) if session.is_some() && mode == Mode::Talk && bot_run::session_lost(&error) => {
            bot_run::run(&bot_run::Target { session: None, ..target }, mode, text, &setup.scratch, &stop, &on_trail)
        }
        other => other,
    }
}

/// The first command in the answer that runs without a click: one the person allowed for this
/// project, or, for a bot that runs without asking, one that only reads, checks or installs.
fn allowed_command(state_dir: &Path, bot: &Bot, reply: &Reply) -> Option<String> {
    // Read again: the person may have changed the list or the grant while the bot was answering.
    let now = find(state_dir, &bot.id).ok()?;
    // With full access any command raiox can run goes (the CLI runs its own anyway); otherwise only the safe ones.
    let safe = |command: &str| {
        commands::check(command).is_ok_and(|words| now.grants.full_access || (now.grants.auto_run && crate::safe_commands::runs_alone(&words, &bot.dir)))
    };
    reply.run.iter().find(|r| r.refused.is_none() && (now.allowed.contains(&r.command) || safe(&r.command))).map(|r| r.command.clone())
}

/// Whether the bot runs without asking now (the grant may change while it answers); full access includes it.
fn runs_alone(state_dir: &Path, bot: &Bot) -> bool {
    find(state_dir, &bot.id).is_ok_and(|now| now.grants.auto_run || now.grants.full_access)
}

/// Starts the project's app for a bot that runs without asking, and gives the CLI's next prompt:
/// a screenshot of the page that came up, or what went wrong.
fn start_app(state_dir: &Path, jobs: &Jobs, bot: &Bot, say: &dyn Fn(Message)) -> Option<(String, Vec<PathBuf>)> {
    jobs.set_step(&bot.id, crate::bot_steps::Step { kind: "run", target: "See the app".to_string() });
    let stop = || jobs.should_stop(&bot.id);
    match crate::app_start::start_and_wait(state_dir, &bot.id, &bot.dir, &stop) {
        Ok(url) => {
            // Its output too: a second command (an API) may have stopped, for a missing .env say.
            let log = crate::preview::status(state_dir, &bot.id, &bot.dir).ok().and_then(|s| s["log"].as_str().map(str::to_string)).unwrap_or_default();
            let full = find(state_dir, &bot.id).is_ok_and(|now| now.grants.full_access);
            let secrets = if full {
                "a missing secrets file you can create from its example if there is one (you have full access); ask the person only for real keys"
            } else {
                "a missing secrets file is for the person to fill, from its example if there is one"
            };
            let refused = crate::preview::blocked_port(&url).map(|port| port_refused(port, &url, full)).unwrap_or_default();
            let started = format!(
                "(raiox started the project's app for the person: it answers at {url}. Its output so far:\n{log}\nIf a command stopped, \
                 say which and what it needs; {secrets}.{refused})"
            );
            let Some((prompt, files)) = screenshots(state_dir, jobs, bot, std::slice::from_ref(&url), say) else {
                return Some((format!("{started}\n\nraiox could not take a screenshot. Tell the person briefly."), Vec::new()));
            };
            Some((format!("{started}\n\n{prompt}"), files))
        }
        Err(_) if stop() => None,
        Err(error) => {
            say(note("failed", json!({ "error": format!("raiox could not start the app: {error:#}") })));
            let log = crate::preview::status(state_dir, &bot.id, &bot.dir).ok().and_then(|s| s["log"].as_str().map(str::to_string)).unwrap_or_default();
            Some((format!("raiox tried to start the project's app and it failed ({error:#}). Its last output:\n{log}\n\nTell the person briefly what went wrong and how to fix it."), Vec::new()))
        }
    }
}

/// What the AI is told when the app runs on a port browsers refuse: the person cannot open it there.
fn port_refused(port: u16, url: &str, full: bool) -> String {
    let next = if full {
        "Change it now (in vite.config, package.json or the server's own settings) to one such as 5173. Vite restarts by itself on the \
         new port and raiox follows it: then check the page with RAIOX-SHOT: /. A server that does not restart by itself needs the \
         person to press Stop and Run in See the app."
    } else {
        "Propose changing it (in vite.config, package.json or the server's own settings) to one such as 5173."
    };
    format!(" Browsers (Safari, Chrome, Firefox) refuse port {port}, a restricted port, so the person cannot open {url} even though it runs. {next}")
}

/// Hands the person's attached images over to the CLI through `ai_dir`, and tells the CLI where they are.
fn attach_images(state_dir: &Path, bot: &str, files: &[PathBuf], text: String, say: &dyn Fn(Message)) -> (String, Vec<PathBuf>) {
    let mut handed = Vec::new();
    for file in files {
        match crate::preview::hand_over(state_dir, bot, file) {
            Ok(path) => handed.push(path),
            Err(error) => say(note("failed", json!({ "error": format!("raiox could not hand the image over: {error:#}") }))),
        }
    }
    if handed.is_empty() {
        return (text, Vec::new());
    }
    let list: Vec<String> = handed.iter().map(|path| format!("- {}", path.display())).collect();
    let noun = if handed.len() == 1 { "image" } else { "images" };
    let pronoun = if handed.len() == 1 { "it" } else { "them" };
    let header = format!(
        "(raiox: the person attached {} {noun}:\n{}\nLook at {pronoun} — open each file with your Read tool if you do not see {pronoun} directly.)",
        handed.len(),
        list.join("\n"),
    );
    let prompt = if text.trim().is_empty() {
        format!("{header}\n\nLook at the attached {noun} and help the person.")
    } else {
        format!("{header}\n\n{text}")
    };
    (prompt, handed)
}

/// Runs a command in the project, shows what it did, and gives the CLI's next prompt
/// (none when it was refused or the person stopped it).
fn command_output(jobs: &Jobs, bot: &Bot, command: &str, allowed: bool, say: &dyn Fn(Message)) -> Option<String> {
    let words = match commands::check(command) {
        Ok(words) => words,
        Err(why) => {
            say(note("failed", json!({ "error": format!("raiox will not run `{command}` ({why})") })));
            return None;
        }
    };
    jobs.set_step(&bot.id, crate::bot_steps::Step { kind: "run", target: command.to_string() });
    let ran = commands::run(command, &words, &bot.dir, &|| jobs.should_stop(&bot.id));
    let prompt = commands::prompt(&ran, allowed);
    let stopped = ran.stopped;
    say(said("ran", json!({ "ran": ran, "allowed": allowed })));
    if stopped {
        say(note("stopped", json!({})));
        return None;
    }
    Some(prompt)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attach_images_hands_files_over_into_ai_dir_and_mentions_them() {
        let state = std::env::temp_dir().join(format!("raiox-turn-images-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&state);
        let uploads = crate::uploads::dir(&state, "b1");
        std::fs::create_dir_all(&uploads).unwrap();
        let img = uploads.join("abc1-0001.png");
        std::fs::write(&img, [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]).unwrap();

        crate::preview::clear_ai_dir(&state, "b1");
        let (prompt, handed) = attach_images(&state, "b1", std::slice::from_ref(&img), "what is this?".into(), &|_| {});
        assert_eq!(handed.len(), 1);
        assert_eq!(handed[0], crate::preview::ai_dir(&state, "b1").join("abc1-0001.png"));
        assert!(handed[0].exists());
        assert!(prompt.contains(&handed[0].display().to_string()));
        assert!(prompt.contains("what is this?"));

        // Image-only message still produces a clear instruction for the CLI.
        crate::preview::clear_ai_dir(&state, "b1");
        let (prompt_only, handed_only) = attach_images(&state, "b1", &[img], String::new(), &|_| {});
        assert_eq!(handed_only.len(), 1);
        assert!(prompt_only.contains("attached image"));

        let _ = std::fs::remove_dir_all(&state);
    }
}
