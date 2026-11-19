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
            stable: bot_run::guide(&bot.name, &persona, &others, &unwired),
            team: crate::team::context_for(state_dir, bot),
            // The project's own pages that relate to the question, kept decisions included.
            related: crate::memory::related(&bot.dir, question),
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
    if let Some(command) = &turn.command {
        let Some(output) = command_output(jobs, bot, command, false, &say) else { return };
        text = output;
    }
    // A draft starts a fresh session in its copy; talk continues the conversation's own.
    let mut session = if work.is_some() { None } else { session };
    let mut runs = 0;
    let mut shots_taken = 0;
    let mut images: Vec<PathBuf> = Vec::new();
    loop {
        let dir = work.as_ref().map_or(bot.dir.as_path(), |work| work.dir.as_path());
        let asked = ask_cli(jobs, bot, &setup, dir, session.as_deref(), turn.mode, &text, Some(&format!("{}:{thread}", bot.id)), &images);
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
                        if bot_run::keeps_session(turn.mode) {
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
        let reply = Reply { to: reply.to.into_iter().filter(|n| setup.others.contains(&n.project)).collect(), ..reply };
        if bot_run::keeps_session(turn.mode) && reply.session.is_some() && reply.session != session {
            let _ = set_thread_session(state_dir, &bot.id, thread, reply.session.clone());
            session = reply.session.clone();
        }
        if let Some(work) = &work {
            return say(drafted(state_dir, &bot.id, work, &reply));
        }
        // Past the project's daily limit, allowed commands wait for a click like any other.
        let within = runs < MAX_AUTO_RUNS && crate::budget::check(state_dir, &bot.name).is_ok();
        let next = if within { allowed_command(state_dir, bot, &reply) } else { None };
        let wanted: Vec<String> = if next.is_none() && shots_taken < MAX_SHOTS && crate::budget::check(state_dir, &bot.name).is_ok() {
            reply.shots.iter().take(MAX_SHOTS - shots_taken).cloned().collect()
        } else {
            Vec::new()
        };
        let proposed = reply.proposal.is_some();
        let answer = note("answer", json!({ "mode": "talk", "reply": reply }));
        let answer_ms = answer.ms;
        say(answer);
        // A project pair: the chosen reviewer reads every proposal before the person approves it.
        if proposed && let Some(reviewer) = find(state_dir, &bot.id).ok().and_then(|b| b.reviewer)
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
    let target = bot_run::Target { agent: &bot.agent, dir, guide, session, model: setup.model.as_deref(), effort: setup.effort.as_deref(), shots, images };
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
    match bot_run::run(&target, mode, text, &setup.scratch, &stop, &on_trail) {
        Err(error) if session.is_some() && mode == Mode::Talk && bot_run::session_lost(&error) => {
            bot_run::run(&bot_run::Target { session: None, ..target }, mode, text, &setup.scratch, &stop, &on_trail)
        }
        other => other,
    }
}

/// The first command in the answer that the person allowed for this project.
fn allowed_command(state_dir: &Path, bot: &Bot, reply: &Reply) -> Option<String> {
    // Read again: the person may have changed the list while the bot was answering.
    let allowed = find(state_dir, &bot.id).ok()?.allowed;
    reply.run.iter().find(|r| r.refused.is_none() && allowed.contains(&r.command)).map(|r| r.command.clone())
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
