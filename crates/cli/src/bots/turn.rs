//! One turn of a bot, in its own thread. The CLI answers; when it asks for a
//! command the person allowed for the project, raiox runs it and the CLI goes
//! on with the output, a few times at most. A command the person approves
//! by hand starts a turn the same way.

use std::path::{Path, PathBuf};

use serde_json::json;

use super::{drafted, find, list, note, said, set_thread_session, Bot, Jobs, Persona};
use crate::bot_run::{self, Mode, Reply};
use crate::room::{self, Message};
use crate::{commands, drafts};

/// Commands that run without asking, per question, before the bot has to answer.
const MAX_AUTO_RUNS: usize = 3;

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
struct Setup {
    guide: String,
    others: Vec<String>,
    model: Option<String>,
    effort: Option<String>,
    scratch: PathBuf,
}

impl Setup {
    fn new(state_dir: &Path, bot: &Bot, question: &str, mode: Mode) -> Self {
        // Only the projects wired to this one: the bot names no others and can message no others.
        let others = crate::links::linked(state_dir, &bot.name);
        let mut unwired: Vec<String> = list(state_dir).into_iter().map(|b| b.name).filter(|name| *name != bot.name && !others.contains(name)).collect();
        unwired.sort();
        unwired.dedup();
        let defaults = crate::settings::Settings::load(state_dir).defaults_for(&bot.agent);
        let persona = bot.persona.as_ref().map(Persona::prompt).unwrap_or_default();
        // The team notes from ai-memory: what the other bots here and in wired projects did lately.
        let guide = bot_run::guide(&bot.name, &persona, &others, &unwired)
            + &crate::team::context_for(state_dir, bot)
            // The project's own pages that relate to the question, kept decisions included.
            + &crate::memory::related(&bot.dir, question);
        Setup {
            guide,
            others,
            // Talk can run on a lighter model; changes always on the bot's own.
            model: if mode == Mode::Talk { bot.talk_model.clone() } else { None }.or_else(|| bot.model.clone()).or(defaults.model),
            effort: bot.effort.clone().or(defaults.effort),
            scratch: state_dir.join("bots").join(format!("{}.run", bot.id)),
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
    let mut text = turn.text;
    if let Some(command) = &turn.command {
        let Some(output) = command_output(jobs, bot, command, false, &say) else { return };
        text = output;
    }
    // A draft starts a fresh session in its copy; talk continues the conversation's own.
    let mut session = if work.is_some() { None } else { session };
    let mut runs = 0;
    loop {
        let dir = work.as_ref().map_or(bot.dir.as_path(), |work| work.dir.as_path());
        let reply = match ask_cli(jobs, bot, &setup, dir, session.as_deref(), turn.mode, &text) {
            Ok(reply) => reply,
            Err(error) => {
                if work.is_some() {
                    drafts::abandon(state_dir, &bot.id);
                }
                let stopped = error.to_string() == bot_run::STOPPED;
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
        say(note("answer", json!({ "mode": "talk", "reply": reply })));
        let Some(command) = next else { return };
        runs += 1;
        let Some(output) = command_output(jobs, bot, &command, true, &say) else { return };
        text = output;
    }
}

/// One answer from the CLI; when it lost the session (its history was cleared), talk goes on in a fresh one.
fn ask_cli(jobs: &Jobs, bot: &Bot, setup: &Setup, dir: &Path, session: Option<&str>, mode: Mode, text: &str) -> anyhow::Result<Reply> {
    let target = bot_run::Target {
        agent: &bot.agent,
        dir,
        guide: setup.guide.clone(),
        session,
        model: setup.model.as_deref(),
        effort: setup.effort.as_deref(),
    };
    let stop = || jobs.should_stop(&bot.id);
    let on_step = |step| jobs.set_step(&bot.id, step);
    match bot_run::run(&target, mode, text, &setup.scratch, &stop, &on_step) {
        Err(error) if session.is_some() && mode == Mode::Talk && bot_run::session_lost(&error) => {
            bot_run::run(&bot_run::Target { session: None, ..target }, mode, text, &setup.scratch, &stop, &on_step)
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
