//! Local panel server. Listens on 127.0.0.1 only, on a random port, and the
//! API answers only with the session token carried in the URL fragment
//! (fragments never reach server logs or the Referer header).

use std::collections::hash_map::RandomState;
use std::fs;
use std::hash::{BuildHasher, Hasher};
use std::io::Cursor;
use std::path::{Component, Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use raiox_core::{Roots, Store};
use tiny_http::{Header, Method, Request, Response, Server};

use crate::progress::{self, Milestone};
use crate::{actions, bots, data, setup};
use raiox_core::apply;
use serde_json::{json, Value};
use std::io::Read;

/// Bot messages can reach 4000 characters of any script.
const MAX_BODY: u64 = 32 * 1024;
/// Chat messages may carry up to four 5 MB images encoded in base64.
const MAX_ASK_BODY: u64 = 30 * 1024 * 1024;

pub(crate) type Reply = Response<Cursor<Vec<u8>>>;

/// A request answered now, or slow work that touches nothing of the panel, answered from its own
/// thread so the rest of the panel (chats, polls) does not wait behind it.
enum Answer {
    Now(Reply),
    Later(Box<dyn FnOnce() -> Reply + Send>),
}

const SECURITY_HEADERS: [(&str, &str); 5] = [
    (
        "Content-Security-Policy",
        "default-src 'self'; img-src 'self' data: blob:; style-src 'self' 'unsafe-inline'; font-src 'self'; \
         connect-src 'self'; frame-src http://localhost:* http://127.0.0.1:* http://[::1]:*; frame-ancestors 'none'; base-uri 'none'; form-action 'none'",
    ),
    ("X-Content-Type-Options", "nosniff"),
    ("Referrer-Policy", "no-referrer"),
    ("Cross-Origin-Resource-Policy", "same-origin"),
    ("Cache-Control", "no-store"),
];

const MISSING_BUILD: &str = "<!doctype html><meta charset=utf-8><title>raiox</title>\
<body style=\"background:#060607;color:#9a9aa6;font:14px ui-monospace,monospace;padding:40px\">\
The panel is not built yet. Run <code>pnpm --dir apps/web build</code>, then reload.";

pub struct Panel {
    pub store: Store,
    pub roots: Roots,
    pub web_dir: PathBuf,
    /// Where the change ledger and backups live (next to the index).
    pub state_dir: PathBuf,
    /// The project changes apply to: the folder `raiox open` started in.
    pub project_dir: PathBuf,
    pub jobs: bots::Jobs,
    pub clones: setup::Clones,
}

/// The panel in the terminal: started, announced, and kept until Ctrl+C.
pub fn run(panel: Panel, port: u16, open_browser: bool) -> Result<()> {
    let (url, _) = start(panel, port)?;
    println!("raiox panel: {url}");
    println!("Only this machine can open it (and devices you pair, when phone access is on). Ctrl+C stops it.");
    if open_browser && let Err(error) = open::that(&url) {
        eprintln!("could not open the browser ({error}); open the link above");
    }
    loop {
        std::thread::park();
    }
}

/// Starts the panel server on its own thread; gives its address (with the session token) and port.
/// One panel per state folder: another raiox already serving the same data refuses this one.
pub fn start(mut panel: Panel, port: u16) -> Result<(String, u16)> {
    crate::procs::claim_panel(&panel.state_dir)?;
    // Conversations whose turn was cut when the panel closed get a note; nothing runs on its own.
    bots::recover_interrupted(&panel.state_dir);
    let server = Server::http(("127.0.0.1", port)).map_err(|e| anyhow!("cannot listen on 127.0.0.1:{port}: {e}"))?;
    progress::initialize(&panel.state_dir, &panel.store)?;
    raiox_core::refresh(&mut panel.store, &panel.roots)?;
    let port = server.server_addr().to_ip().map(|addr| addr.port()).ok_or_else(|| anyhow!("no TCP address"))?;
    let token = session_token();
    let url = format!("http://127.0.0.1:{port}/#t={token}");
    std::thread::Builder::new().name("raiox-panel".into()).spawn(move || serve(server, panel, &token, port))?;
    Ok((url, port))
}

fn serve(server: Server, mut panel: Panel, token: &str, port: u16) {
    let hosts = [format!("127.0.0.1:{port}"), format!("localhost:{port}")];
    crate::routines::spawn(panel.state_dir.clone(), panel.jobs.clone());
    crate::telegram::spawn(panel.state_dir.clone(), panel.jobs.clone());
    crate::auto_edit::set_home(&panel.roots.home);
    // Drafts older than two weeks go, with their copies and backups.
    crate::drafts::cleanup(&panel.state_dir);
    // Dev servers and a tunnel an earlier raiox left running stop now.
    crate::procs::cleanup(&panel.state_dir);
    // Phone access opens again if the person left it on.
    crate::tunnel::resume(&panel.state_dir, port);
    for mut request in server.incoming_requests() {
        // The browser may have gone away; nothing to do about it.
        match route(&mut panel, &mut request, token, &hosts, port) {
            Answer::Now(reply) => {
                let _ = request.respond(reply);
            }
            Answer::Later(work) => {
                std::thread::spawn(move || {
                    let _ = request.respond(work());
                });
            }
        }
    }
}

fn route(panel: &mut Panel, request: &mut Request, token: &str, hosts: &[String], port: u16) -> Answer {
    let host = header(request, "Host").unwrap_or("");
    // This machine by 127.0.0.1/localhost, or a paired device by the tunnel's public address; nothing else.
    let remote = if hosts.iter().any(|h| h == host) {
        false
    } else if crate::tunnel::public_host(&panel.state_dir).is_some_and(|public| public == host) {
        true
    } else {
        return Answer::Now(text(403, "forbidden host"));
    };
    let path = request.url().split(['?', '#']).next().unwrap_or("/").to_string();
    let path = path.as_str();
    if remote && let Some(reply) = crate::remote::gate(&panel.state_dir, request, path) {
        return Answer::Now(reply);
    }
    if path.starts_with("/api/") && !remote && !authorized(request, token) {
        return Answer::Now(text(401, "missing or wrong session token"));
    }
    let fresh = request.url().split_once('?').is_some_and(|(_, q)| q.split('&').any(|pair| pair == "fresh=1"));
    match (request.method().clone(), path) {
        (Method::Get, "/api/settings") => settings_answer(panel, fresh),
        (Method::Post, "/api/settings") => match body(request).map(|input| crate::settings::update(&panel.state_dir, &input)) {
            Ok(Ok(_)) => settings_answer(panel, false),
            Ok(Err(error)) => Answer::Now(respond(Err(error))),
            Err(reply) => Answer::Now(reply),
        },
        (Method::Get, "/api/setup") => setup_answer(panel, fresh),
        (Method::Get, "/api/setup/github") => Answer::Later(Box::new(|| respond(Ok(setup::github())))),
        // The other AIs a bot can switch to: each CLI is asked for its models, never on the main loop.
        (Method::Post, "/api/bot/other_agents") if !remote => match body(request) {
            Ok(input) => match crate::bots::find(&panel.state_dir, input.get("id").and_then(Value::as_str).unwrap_or("")) {
                Ok(bot) => {
                    let settings = crate::settings::Settings::load(&panel.state_dir);
                    let codex = crate::bot_usage::codex_models(&panel.store).unwrap_or_default();
                    Answer::Later(Box::new(move || respond(Ok(crate::agent_switch::others(&bot.agent, &|id| settings.is_enabled(id), codex)))))
                }
                Err(error) => Answer::Now(respond(Err(error))),
            },
            Err(reply) => Answer::Now(reply),
        },
        // Asks the update server: never on the main loop.
        (Method::Post, "/api/app/check") if !remote => Answer::Later(Box::new(|| respond(crate::app_update::check_now()))),
        // Sign in may wait briefly for a login URL or open a browser/terminal: never on the main loop.
        (Method::Post, "/api/setup/sign_in") if !remote => match body(request) {
            Ok(input) => {
                let id = input.get("id").and_then(Value::as_str).unwrap_or("").to_string();
                Answer::Later(Box::new(move || respond(crate::install::sign_in(&id).map(|out| json!({ "opened": true, "mode": out.mode })))))
            }
            Err(reply) => Answer::Now(reply),
        },
        // Minimal prompt test to confirm a CLI responds end-to-end: desk-only and off the main loop.
        (Method::Post, "/api/setup/test") if !remote => match body(request) {
            Ok(input) => {
                let id = input.get("id").and_then(Value::as_str).unwrap_or("").to_string();
                let home = panel.roots.home.clone();
                Answer::Later(Box::new(move || respond(crate::connections::test_cli(&id, &home).map(|res| json!(res)))))
            }
            Err(reply) => Answer::Now(reply),
        },
        _ => Answer::Now(dispatch(panel, request, path, port, remote)),
    }
}

/// The requests answered on the panel's own thread (they read or change its state).
fn dispatch(panel: &mut Panel, request: &mut Request, path: &str, port: u16, remote: bool) -> Reply {
    match (request.method(), path) {
        (_, p) if p.starts_with("/api/remote") => crate::remote::desk(&panel.state_dir, request, p, port),
        (Method::Get, "/api/wiki") => respond(Ok(crate::wiki::list(&panel.state_dir))),
        (Method::Get, "/api/app") => respond(Ok(crate::app_update::state())),
        (Method::Post, "/api/app/update") => respond(crate::app_update::install()),
        (Method::Get, "/api/telegram") => respond(Ok(crate::telegram::state(&panel.state_dir))),
        (Method::Post, "/api/telegram/connect") => match body(request) {
            Ok(input) => respond(crate::telegram::connect(&panel.state_dir, input.get("token").and_then(Value::as_str).unwrap_or(""))),
            Err(reply) => reply,
        },
        (Method::Post, "/api/telegram/pair") => respond(crate::telegram::pair_again(&panel.state_dir)),
        (Method::Post, "/api/telegram/forget") => respond(crate::telegram::forget(&panel.state_dir)),
        (Method::Get, "/api/deliveries") => {
            let all = crate::deliveries::list(&panel.state_dir);
            // A device sees the deliveries of the bots it can reach.
            let hidden = if remote { crate::grants::desk_only(&panel.state_dir) } else { Vec::new() };
            respond(Ok(json!(all.into_iter().filter(|d| !hidden.contains(&d.bot)).collect::<Vec<_>>())))
        }
        (Method::Get, "/api/tasks") => {
            let board = crate::tasks::state(&panel.state_dir);
            respond(Ok(if remote { crate::grants::tasks_for_device(&panel.state_dir, board) } else { board }))
        }
        (Method::Post, p) if p.starts_with("/api/tasks/") => match body(request) {
            Ok(input) if remote => respond(
                crate::grants::device_may(&panel.state_dir, &input)
                    .and_then(|()| task_action(panel, &p["/api/tasks/".len()..], &input))
                    .map(|board| crate::grants::tasks_for_device(&panel.state_dir, board)),
            ),
            Ok(input) => respond(task_action(panel, &p["/api/tasks/".len()..], &input)),
            Err(reply) => reply,
        },
        (Method::Get, "/api/flows") => respond(Ok(crate::workflows::state(&panel.state_dir))),
        (Method::Post, p) if p.starts_with("/api/flows/") && !remote => match body(request) {
            Ok(input) => {
                let action = p["/api/flows/".len()..].to_string();
                let result = crate::workflows::act(&panel.state_dir, &action, &input);
                // Run, approve: the step that follows goes out now, not on the next beat of the clock.
                if result.is_ok() && matches!(action.as_str(), "run" | "approve") {
                    crate::workflow_run::tick(&panel.state_dir, &panel.jobs);
                }
                respond(result.map(|value| if action == "save" { value } else { crate::workflows::state(&panel.state_dir) }))
            }
            Err(reply) => reply,
        },
        (Method::Post, p) if p.starts_with("/api/wiki/") => match body(request) {
            Ok(input) => respond(wiki_action(&panel.state_dir, &p["/api/wiki/".len()..], &input)),
            Err(reply) => reply,
        },
        (Method::Get, "/api/report") => respond(report(panel)),
        (Method::Post, "/api/apply") => match body(request) {
            Ok(input) => respond(target(panel, &input).and_then(|dir| actions::apply_suggestion(
                &panel.store,
                &panel.roots.home,
                &panel.state_dir,
                &dir,
                input.get("id").and_then(Value::as_str).unwrap_or(""),
                input.get("source").and_then(Value::as_str).unwrap_or("claude"),
                input.get("value").and_then(Value::as_str),
            ))),
            Err(reply) => reply,
        },
        (Method::Post, "/api/ask") => match body(request) {
            Ok(input) => respond(
                crate::room::ask(
                    &panel.store,
                    &panel.roots,
                    &panel.state_dir,
                    &panel.project_dir,
                    input.get("agent").and_then(Value::as_str).unwrap_or(""),
                    input.get("text").and_then(Value::as_str),
                )
                .map(|messages| json!(messages)),
            ),
            Err(reply) => reply,
        },
        (Method::Post, "/api/session") => match body(request) {
            Ok(input) => respond(target(panel, &input).and_then(|dir| session_status(panel, &dir))),
            Err(reply) => reply,
        },
        (Method::Post, "/api/undo") => match body(request) {
            Ok(input) => respond(target(panel, &input).and_then(|dir| actions::undo_change(
                &panel.store,
                &panel.roots.home,
                &panel.state_dir,
                &dir,
                input.get("change").and_then(Value::as_str).unwrap_or(""),
            ))),
            Err(reply) => reply,
        },
        (Method::Get, "/api/profile") => respond(Ok(crate::profile::get(&panel.roots.home, &panel.state_dir))),
        (Method::Get, "/api/usage") => respond(usage(panel, request.url())),
        (Method::Get, "/api/bot/turn_diff") => {
            let url = request.url().to_string();
            let query = url.split_once('?').map_or("", |(_, q)| q);
            let param = |name: &str| query.split('&').find_map(|pair| pair.strip_prefix(name)?.strip_prefix('=')).unwrap_or("").to_string();
            let thread = Some(param("thread")).filter(|t| crate::threads::valid(t)).unwrap_or_else(|| crate::threads::MAIN.to_string());
            respond(bots::turn_patch(&panel.state_dir, &param("id"), &thread, param("ms").parse().unwrap_or(0)))
        }
        (Method::Get, "/api/bot/context") => {
            let url = request.url().to_string();
            let query = url.split_once('?').map_or("", |(_, q)| q);
            let param = |name: &str| query.split('&').find_map(|pair| pair.strip_prefix(name)?.strip_prefix('=')).unwrap_or("").to_string();
            let thread = Some(param("thread")).filter(|t| crate::threads::valid(t)).unwrap_or_else(|| crate::threads::MAIN.to_string());
            respond(crate::context::of_conversation(&mut panel.store, &panel.roots, &panel.state_dir, &param("id"), &thread))
        }
        (Method::Get, "/api/limits") => respond(Ok(crate::limits::state(&panel.state_dir, &panel.roots.codex, &crate::limits::claude_config(&panel.roots.home)))),
        // The person's name and photo: from this computer only.
        (Method::Post, "/api/profile") if !remote => match body_limit(request, MAX_ASK_BODY) {
            Ok(input) => {
                let changed = if input.get("from_github").and_then(Value::as_bool) == Some(true) {
                    crate::profile::photo_from_github(&panel.roots.home, &panel.state_dir)
                } else {
                    crate::profile::save(&panel.state_dir, &input)
                };
                respond(changed.map(|()| crate::profile::get(&panel.roots.home, &panel.state_dir)))
            }
            Err(reply) => reply,
        },
        (Method::Post, "/api/setup/clone") => match body(request) {
            Ok(input) => respond(
                setup::clone(&panel.store, &panel.roots.home, &panel.clones, input.get("repo").and_then(Value::as_str).unwrap_or(""))
                    .and_then(|()| setup::state(&panel.store, &panel.roots.home, &panel.state_dir, &panel.clones)),
            ),
            Err(reply) => reply,
        },
        (Method::Get, "/api/setup/install") => respond(Ok::<_, anyhow::Error>(crate::install::state())),
        (Method::Post, "/api/setup/install") if !remote => match body(request) {
            Ok(input) => respond(crate::install::install(input.get("id").and_then(Value::as_str).unwrap_or("")).map(|()| crate::install::state())),
            Err(reply) => reply,
        },
        (Method::Post, "/api/setup/create_project") if !remote => match body(request) {
            Ok(input) => respond(setup::create_project(
                &panel.store,
                &panel.roots.home,
                &panel.state_dir,
                &panel.clones,
                input.get("name").and_then(Value::as_str).unwrap_or(""),
            )),
            Err(reply) => reply,
        },
        (Method::Post, "/api/setup/finish") => respond(setup::finish(&panel.state_dir).map(|()| json!({ "done": true }))),
        (Method::Get, "/api/bots") => respond(bots::state(&panel.store, &panel.roots.home, &panel.state_dir, &panel.jobs).map(|state| seen_by(panel, remote, state))),
        (Method::Post, p) if p.starts_with("/api/bot/") => {
            let action = p["/api/bot/".len()..].to_string();
            let limit = if action == "ask" { MAX_ASK_BODY } else { MAX_BODY };
            match body_limit(request, limit) {
                // A paired device reaches only the bots (and channels) that are not kept on this computer.
                Ok(input) if remote => respond(crate::grants::device_may(&panel.state_dir, &input).and_then(|()| bot_action(panel, &action, &input)).map(|state| seen_by(panel, remote, state))),
                Ok(input) => respond(bot_action(panel, &action, &input)),
                Err(reply) => reply,
            }
        }
        (Method::Get, _) => static_file(&panel.web_dir, path, crate::settings::Settings::load(&panel.state_dir).language.as_deref()),
        _ => text(405, "method not allowed"),
    }
}

/// The bots state for whoever asks: a paired device does not see the bots kept on this computer.
fn seen_by(panel: &Panel, remote: bool, state: Value) -> Value {
    if remote { crate::grants::for_device(&panel.state_dir, state) } else { state }
}

/// The project a change goes to: a bot's folder when the panel names the bot,
/// else the folder `raiox open` started in.
fn target(panel: &Panel, input: &Value) -> anyhow::Result<std::path::PathBuf> {
    match input.get("bot").and_then(Value::as_str).filter(|id| !id.is_empty()) {
        Some(id) => bots::dir_of(&panel.state_dir, id),
        None => Ok(panel.project_dir.clone()),
    }
}

/// The session working in a project now, as Efficiency shows it; null when the project has none yet.
fn session_status(panel: &Panel, dir: &std::path::Path) -> anyhow::Result<Value> {
    Ok(match crate::room::current_session(&panel.store, &panel.roots, dir) {
        Some(session) => json!(raiox_core::advice::context_status(panel.store.conn(), Some(&session))?),
        None => Value::Null,
    })
}

/// Bot actions; each answers with the bots' state so the panel redraws at once.
fn bot_action(panel: &Panel, action: &str, input: &Value) -> anyhow::Result<Value> {
    let field = |name: &str| input.get(name).and_then(Value::as_str).unwrap_or("");
    let (state_dir, jobs) = (&panel.state_dir, &panel.jobs);
    let thread = input.get("thread").and_then(Value::as_str).filter(|t| !t.is_empty()).unwrap_or(crate::threads::MAIN);
    match action {
        "activate" => {
            let persona = input.get("persona").filter(|p| !p.is_null()).map(|p| serde_json::from_value::<bots::Persona>(p.clone())).transpose()?;
            let bot = bots::activate(state_dir, &panel.roots.home, field("project"), field("agent"), persona)?;
            // A new bot finds the team's notes waiting, and the others learn it joined.
            crate::team::refresh(state_dir, crate::team::around(state_dir, &bot.name));
            Ok(())
        }
        "create_project" => {
            let bot = bots::create_project(state_dir, &panel.roots.home, field("name"), field("agent"))?;
            crate::team::refresh(state_dir, crate::team::around(state_dir, &bot.name));
            Ok(())
        }
        "rsi_create" => {
            crate::rsi::create(state_dir, &panel.roots.home, jobs, field("name"), field("agent"), input.get("lang").and_then(Value::as_str))?;
            Ok(())
        }
        "deactivate" => {
            let dir = bots::dir_of(state_dir, field("id"));
            bots::deactivate(state_dir, jobs, field("id"))?;
            crate::team::refresh(state_dir, dir.into_iter().collect());
            Ok(())
        }
        "ask" => {
            let images = match input.get("images") {
                None | Some(Value::Null) => Vec::new(),
                Some(Value::Array(items)) => items
                    .iter()
                    .map(|item| item.as_str().map(str::to_string).ok_or_else(|| anyhow!("images must be base64 strings")))
                    .collect::<Result<Vec<_>>>()?,
                Some(_) => return Err(anyhow!("images must be an array")),
            };
            if input.get("aside").and_then(Value::as_bool) == Some(true) {
                bots::ask_aside(state_dir, jobs, field("id"), thread, field("text"))
            } else {
                bots::ask(state_dir, jobs, field("id"), thread, field("text"), &images)
            }
        }
        "branch" => crate::fork::branch(state_dir, field("id"), thread, input.get("ms").and_then(Value::as_i64).unwrap_or(0), field("title")).map(|_| ()),
        "turn_undo" => bots::undo_turn(state_dir, field("id"), thread, input.get("ms").and_then(Value::as_i64).unwrap_or(0)),
        "approve" => {
            let answer_ms = input.get("answer_ms").map(|value| value.as_i64().context("invalid proposal timestamp")).transpose()?;
            bots::approve(state_dir, &panel.roots.home, jobs, field("id"), thread, answer_ms)
        }
        "dismiss_missions" => progress::dismiss(state_dir),
        "view_usage" => progress::viewed_usage(state_dir),
        "cancel_waiting" => {
            let ms = input["answer_ms"].as_i64().context("invalid answer timestamp")?;
            let card = serde_json::from_value(input.clone())?;
            bots::cancel_waiting(state_dir, jobs, field("id"), thread, ms, card)
        }
        "switch_agent" => crate::agent_switch::switch(state_dir, jobs, field("id"), field("agent"), input.get("model").and_then(Value::as_str).filter(|m| !m.is_empty())),
        "run" => {
            let index = input.get("index").and_then(Value::as_u64).unwrap_or(u64::MAX) as usize;
            let ms = input.get("answer_ms").map(|value| value.as_i64().context("invalid answer timestamp")).transpose()?;
            bots::run_command(state_dir, jobs, field("id"), thread, ms, index, input.get("always").and_then(Value::as_bool).unwrap_or(false))
        }
        "disallow" => bots::allow(state_dir, field("id"), field("command"), false),
        "review" => bots::request_review(state_dir, jobs, field("id"), thread, input.get("answer_ms").and_then(Value::as_i64).unwrap_or(0), field("reviewer")),
        "reviewer" => bots::set_reviewer(state_dir, field("id"), field("reviewer")),
        "channel_new" => {
            let members: Vec<String> = input.get("members").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(str::to_string).collect();
            crate::channels::create(state_dir, field("name"), &members).map(|_| ())
        }
        "channel_delete" => crate::channels::delete(state_dir, field("channel")),
        "carry" => {
            let to_bot = Some(field("to_bot")).filter(|t| !t.is_empty());
            let to_channel = Some(field("to_channel")).filter(|t| !t.is_empty());
            crate::carry::carry(state_dir, field("id"), thread, to_bot, to_channel).map(|_| ())
        }
        "channel_post" => crate::channels::post(state_dir, jobs, field("channel"), field("text")),
        "channel_pass" => {
            let index = input.get("index").and_then(Value::as_u64).unwrap_or(u64::MAX) as usize;
            crate::channels::pass(state_dir, jobs, field("channel"), field("id"), input.get("answer_ms").and_then(Value::as_i64).unwrap_or(0), index)
        }
        "memory_forget" => {
            let bot = bots::find(state_dir, field("id"))?;
            crate::room::require_opt_in(state_dir, &bot.dir)?;
            crate::room::forget(&bot.dir, field("path"))
        }
        "routine_changed" => crate::routines::set_only_if_changed(state_dir, field("routine"), input.get("on").and_then(Value::as_bool).unwrap_or(false)),
        "talk_model" => bots::set_talk_model(state_dir, field("id"), Some(field("model"))),
        "grants" => bots::set_grants(state_dir, field("id"), input),
        "auditor" => crate::auditor::configure(state_dir, field("id"), input),
        "audit_run" => crate::auditor::run_now(state_dir, jobs, field("id"), input.get("lang").and_then(Value::as_str)).map(|_| ()),
        "request_approve" => crate::requests::approve(state_dir, &panel.roots.home, jobs, field("request")),
        "request_discard" => crate::requests::discard(state_dir, field("request")),
        "limit" => bots::set_limit(state_dir, field("id"), input.get("tokens").and_then(Value::as_u64).unwrap_or(0)),
        "remember" => bots::remember(state_dir, field("id"), thread, input.get("answer_ms").and_then(Value::as_i64).unwrap_or(0)),
        "draft" => bots::draft(state_dir, field("id"), thread, field("draft"), field("action")),
        "reset" => bots::reset(state_dir, jobs, field("id"), thread),
        "link_toggle" => {
            let (a, b) = (field("a"), field("b"));
            crate::links::toggle(state_dir, a, b)?;
            if crate::links::are_linked(state_dir, a, b) {
                progress::record(state_dir, &[Milestone::Link]);
            }
            let dirs = [field("a"), field("b")].iter().flat_map(|name| crate::team::around(state_dir, name)).collect();
            crate::team::refresh(state_dir, dirs);
            Ok(())
        }
        "ask_many" => {
            let ids: Vec<String> = input.get("ids").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(str::to_string).collect();
            crate::batches::ask_many(state_dir, jobs, &ids, field("text"))
        }
        "batch_delete" => crate::batches::delete(state_dir, field("batch")),
        "pin" => crate::pins::toggle(state_dir, field("id"), thread, input.get("ms").and_then(Value::as_i64).unwrap_or(0)),
        "routine_new" => {
            let days: Vec<u8> = input.get("days").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_u64).map(|d| d.min(255) as u8).collect();
            let number = |name: &str| input.get(name).and_then(Value::as_u64).unwrap_or(99).min(255) as u8;
            crate::routines::create(state_dir, field("id"), field("text"), &days, number("hour"), number("minute"), input.get("only_if_changed").and_then(Value::as_bool).unwrap_or(false))
        }
        "routine_toggle" => crate::routines::set_enabled(state_dir, field("routine"), input.get("enabled").and_then(Value::as_bool).unwrap_or(false)),
        "routine_delete" => crate::routines::delete(state_dir, field("routine")),
        "thread_new" => bots::new_thread(state_dir, field("id"), bots::DEFAULT_THREAD_TITLE).map(|_| ()),
        "thread_delete" => {
            crate::live::end(&format!("{}:{thread}", field("id")));
            crate::threads::delete(state_dir, field("id"), thread)
        }
        "house" => {
            let dir = bots::dir_of(state_dir, field("id"))?;
            let data = crate::room::answer(&panel.store, &panel.roots, state_dir, &dir, field("agent"))?;
            bots::house(state_dir, field("id"), thread, field("agent"), data)
        }
        "stop" => jobs.stop(field("id")),
        "correct" => jobs.correct(field("id"), field("text")),
        "continue" => bots::continue_turn(state_dir, jobs, field("id"), thread),
        "retry" => bots::retry_turn(state_dir, jobs, field("id"), thread),
        "configure" => {
            let persona = input.get("persona").filter(|p| !p.is_null()).map(|p| serde_json::from_value::<bots::Persona>(p.clone())).transpose()?;
            bots::configure(state_dir, field("id"), input.get("model").and_then(Value::as_str), input.get("effort").and_then(Value::as_str), persona)
        }
        "usage" => return crate::bot_usage::usage(&panel.store, &panel.roots, state_dir, field("id")),
        "inbox" => return crate::bot_mail::inbox(state_dir, field("id")),
        "memories" => return crate::bot_mail::memories(state_dir, field("id")),
        "preview" => {
            let bot = bots::find(state_dir, field("id"))?;
            return crate::preview::status(state_dir, &bot.id, &bot.dir);
        }
        "preview_start" => {
            let bot = bots::find(state_dir, field("id"))?;
            crate::app_start::start_from_panel(state_dir, &bot.id, &bot.dir, field("command"))
        }
        "preview_stop" => crate::preview::stop(state_dir, &bots::find(state_dir, field("id"))?.id),
        "preview_shot" => {
            let bot = bots::find(state_dir, field("id"))?;
            let size = |name: &str, fallback: u64| input.get(name).and_then(Value::as_u64).unwrap_or(fallback) as u32;
            let file = if field("target") == "simulator" {
                crate::simulator::shot(state_dir, &bot.id)?
            } else {
                crate::preview::shot(state_dir, &bot.id, field("url"), size("width", 1280), size("height", 800))?
            };
            let name = file.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
            let mut image = crate::preview::image(state_dir, &bot.id, &name)?;
            image["name"] = json!(name);
            return Ok(image);
        }
        "shot_image" => return crate::preview::image(state_dir, &bots::find(state_dir, field("id"))?.id, field("name")),
        "upload_image" => return crate::uploads::image(state_dir, &bots::find(state_dir, field("id"))?.id, field("name")),
        "diet" => {
            let bot = bots::find(state_dir, field("id"))?;
            return crate::room::project_diet(&panel.store, &panel.roots, &bot.dir);
        }
        "send" => crate::bot_mail::send(state_dir, field("id"), input.get("answer_ms").and_then(Value::as_i64).unwrap_or(0), field("project")),
        "handle" => crate::bot_mail::handle(state_dir, jobs, field("id"), field("message")),
        other => Err(anyhow!("unknown bot action {other}")),
    }?;
    match action {
        "ask" | "channel_post" | "ask_many" => progress::record(state_dir, &[Milestone::Chat]),
        "approve" => progress::record(state_dir, &[Milestone::Approval, Milestone::Desk]),
        "draft" if field("action") == "apply" => progress::record(state_dir, &[Milestone::Approval, Milestone::Desk]),
        _ => (),
    }
    bots::state(&panel.store, &panel.roots.home, state_dir, jobs)
}

/// The board's actions; each answers with the board, so the panel redraws at once.
fn task_action(panel: &Panel, action: &str, input: &Value) -> anyhow::Result<Value> {
    let field = |name: &str| input.get(name).and_then(Value::as_str).unwrap_or("");
    let state_dir = &panel.state_dir;
    match action {
        "add" => {
            let isolated = input.get("isolated").and_then(Value::as_bool).unwrap_or(false);
            crate::tasks::add_with(state_dir, field("bot"), field("title"), field("detail"), "person", isolated)?;
        }
        "start" => crate::tasks::start(state_dir, &panel.jobs, field("id"))?,
        "done" | "reopen" | "delete" => crate::tasks::mark(state_dir, field("id"), action)?,
        "auto" => {
            // "window": {start, end} sets the hours, null clears them, absent keeps them.
            let window = match input.get("window") {
                None => None,
                Some(Value::Null) => Some(None),
                Some(w) => Some(Some(crate::tasks::Window::new(w["start"].as_u64().unwrap_or(99), w["end"].as_u64().unwrap_or(99))?)),
            };
            crate::tasks::set_auto(state_dir, input.get("on").and_then(Value::as_bool).unwrap_or(false), window)?
        }
        other => anyhow::bail!("unknown task action {other}"),
    }
    Ok(crate::tasks::state(state_dir))
}

/// The wiki's actions; each but `page` answers with the list, so the panel redraws at once.
fn wiki_action(state_dir: &Path, action: &str, input: &Value) -> anyhow::Result<Value> {
    let field = |name: &str| input.get(name).and_then(Value::as_str).unwrap_or("");
    match action {
        "page" => return crate::wiki::read(state_dir, field("slug")),
        "save" => {
            let slug = crate::wiki::put(state_dir, field("slug"), field("title"), field("body"), input.get("always").and_then(Value::as_bool).unwrap_or(false))?;
            let mut listed = crate::wiki::list(state_dir);
            listed["saved"] = json!(slug);
            return Ok(listed);
        }
        "delete" => crate::wiki::delete(state_dir, field("slug"))?,
        "from_answer" => {
            let thread = Some(field("thread")).filter(|t| !t.is_empty()).unwrap_or(crate::threads::MAIN);
            crate::wiki::from_answer(state_dir, field("id"), thread, input.get("answer_ms").and_then(Value::as_i64).unwrap_or(0))?;
        }
        other => anyhow::bail!("unknown wiki action {other}"),
    }
    Ok(crate::wiki::list(state_dir))
}

/// The settings page: serves cached CLI probes immediately while refreshing in the background,
/// or probes live off the main thread when `fresh` is requested (or no cache exists yet).
fn settings_answer(panel: &Panel, fresh: bool) -> Answer {
    let usage = crate::connections::usage_all(&panel.store);
    let settings = crate::settings::Settings::load(&panel.state_dir);
    let home = panel.roots.home.clone();
    let state_dir = panel.state_dir.clone();
    if !fresh
        && let Some(cached) = crate::connections::load_cached(&state_dir)
    {
        crate::connections::refresh_in_background(state_dir, home);
        return Answer::Now(respond(Ok(json!({
            "connections": crate::connections::statuses(cached, &usage, &settings),
            "defaults": settings.defaults,
            "show_all_navigation": settings.show_all_navigation,
            "cached": true,
        }))));
    }
    Answer::Later(Box::new(move || {
        let probes = crate::connections::probe_and_cache(&state_dir, &home);
        respond(Ok(json!({
            "connections": crate::connections::statuses(probes, &usage, &settings),
            "defaults": settings.defaults,
            "show_all_navigation": settings.show_all_navigation,
            "cached": false,
        })))
    }))
}

/// The onboarding setup payload: instant from the probe disk cache, or live off the main thread.
fn setup_answer(panel: &Panel, fresh: bool) -> Answer {
    let base = match setup::base(&panel.store, &panel.roots.home, &panel.state_dir, &panel.clones) {
        Ok(base) => base,
        Err(error) => return Answer::Now(respond(Err(error))),
    };
    let home = panel.roots.home.clone();
    let state_dir = panel.state_dir.clone();
    if !fresh
        && let Some(cached) = crate::connections::load_cached(&state_dir)
    {
        crate::connections::refresh_in_background(state_dir, home);
        return Answer::Now(respond(Ok(base.into_state(cached, true))));
    }
    Answer::Later(Box::new(move || {
        let probes = crate::connections::probe_and_cache(&state_dir, &home);
        respond(Ok(base.into_state(probes, false)))
    }))
}

pub(crate) fn header<'r>(request: &'r Request, name: &'static str) -> Option<&'r str> {
    request.headers().iter().find(|h| h.field.equiv(name)).map(|h| h.value.as_str())
}

fn authorized(request: &Request, token: &str) -> bool {
    header(request, "Authorization")
        .and_then(|value| value.strip_prefix("Bearer "))
        .is_some_and(|given| constant_time_eq(given.as_bytes(), token.as_bytes()))
}

pub(crate) fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// 128 bits from the OS-seeded hasher keys; no extra dependency needed.
fn session_token() -> String {
    (0..2u64)
        .map(|i| {
            let mut hasher = RandomState::new().build_hasher();
            hasher.write_u64(i);
            format!("{:016x}", hasher.finish())
        })
        .collect()
}

/// The page with the language the person picked, so the panel opens in it (no choice: the system's).
fn with_language(page: Vec<u8>, language: Option<&str>) -> Vec<u8> {
    let Some(language) = language.filter(|l| crate::settings::LANGUAGES.contains(l)) else { return page };
    let html = String::from_utf8_lossy(&page);
    html.replacen("<head>", &format!("<head><meta name=\"raiox-lang\" content=\"{language}\">"), 1).into_bytes()
}

fn static_file(root: &Path, url_path: &str, language: Option<&str>) -> Reply {
    let relative = match url_path.trim_start_matches('/') {
        "" => "index.html",
        other => other,
    };
    let candidate = Path::new(relative);
    if !candidate.components().all(|c| matches!(c, Component::Normal(_))) {
        return text(400, "bad path");
    }
    let file = root.join(candidate);
    let is_page = relative == "index.html" || !relative.contains('.');
    match fs::read(&file) {
        Ok(bytes) if relative == "index.html" => with_type(Response::from_data(with_language(bytes, language)), content_type(&file)),
        Ok(bytes) => with_type(Response::from_data(bytes), content_type(&file)),
        // Client-side routes fall back to the app shell.
        Err(_) if is_page => {
            let shell = with_language(fs::read(root.join("index.html")).unwrap_or_else(|_| MISSING_BUILD.as_bytes().to_vec()), language);
            with_type(Response::from_data(shell), "text/html; charset=utf-8")
        }
        Err(_) => text(404, "not found"),
    }
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|ext| ext.to_str()).unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "webmanifest" => "application/manifest+json",
        "webp" => "image/webp",
        "png" => "image/png",
        "woff2" => "font/woff2",
        "ico" => "image/x-icon",
        _ => "application/octet-stream",
    }
}

pub(crate) fn text(status: u16, message: &str) -> Reply {
    with_type(Response::from_data(message.as_bytes().to_vec()), "text/plain; charset=utf-8").with_status_code(status)
}

fn with_type(response: Reply, content_type: &str) -> Reply {
    SECURITY_HEADERS
        .iter()
        .chain([("Content-Type", content_type)].iter())
        .filter_map(|(name, value)| Header::from_bytes(name.as_bytes(), value.as_bytes()).ok())
        .fold(response, |reply, header| reply.with_header(header))
}

fn report(panel: &mut Panel) -> anyhow::Result<Value> {
    let mut value = data::refresh_and_collect(&mut panel.store, &panel.roots)?;
    if let Some(object) = value.as_object_mut() {
        object.insert("changes".into(), json!(apply::ledger(&panel.state_dir.join("changes.json"))?));
        // Before/after for changes in the panel's project and in every bot's project.
        let dirs: std::collections::BTreeSet<std::path::PathBuf> =
            std::iter::once(panel.project_dir.clone()).chain(bots::list(&panel.state_dir).into_iter().map(|bot| bot.dir)).collect();
        let mut measured = Vec::new();
        for dir in &dirs {
            measured.extend(crate::room::outcomes(&panel.store, &panel.roots, &panel.state_dir, dir)?.into_iter().map(|(_, outcome)| outcome));
        }
        object.insert("outcomes".into(), json!(measured));
        object.insert("applicable".into(), json!({ "claude": apply::applicable("claude"), "codex": apply::applicable("codex") }));
        object.insert("project_dir".into(), json!(panel.project_dir.to_string_lossy()));
        object.insert("setup".into(), json!({ "done": setup::is_done(&panel.state_dir) }));
        object.insert("bots".into(), bots::state(&panel.store, &panel.roots.home, &panel.state_dir, &panel.jobs)?);
    }
    Ok(value)
}

/// The longest period the Usage page offers, in days.
const USAGE_MAX_DAYS: i64 = 365;

/// What the AIs' use would cost at API prices: `?days=30&source=claude` (no source: every AI).
fn usage(panel: &Panel, url: &str) -> anyhow::Result<Value> {
    let query = url.split_once('?').map_or("", |(_, q)| q);
    let param = |name: &str| query.split('&').find_map(|pair| pair.strip_prefix(name)?.strip_prefix('='));
    let days = param("days").and_then(|d| d.parse::<i64>().ok()).unwrap_or(30).clamp(1, USAGE_MAX_DAYS);
    let source = param("source").filter(|s| matches!(*s, "claude" | "codex"));
    let since = (chrono::Local::now().date_naive() - chrono::Duration::days(days - 1)).format("%Y-%m-%d").to_string();
    Ok(json!(raiox_core::usage::usage(panel.store.conn(), &since, source)?))
}

pub(crate) fn respond(result: anyhow::Result<Value>) -> Reply {
    match result {
        Ok(value) => with_type(Response::from_data(value.to_string().into_bytes()), "application/json"),
        Err(error) => {
            let message = json!({ "error": format!("{error:#}") }).to_string();
            with_type(Response::from_data(message.into_bytes()), "application/json").with_status_code(422)
        }
    }
}

/// Small JSON bodies only; anything else is refused.
pub(crate) fn body(request: &mut Request) -> Result<Value, Reply> {
    body_limit(request, MAX_BODY)
}

fn body_limit(request: &mut Request, max: u64) -> Result<Value, Reply> {
    let mut raw = String::new();
    request.as_reader().take(max + 1).read_to_string(&mut raw).map_err(|_| text(400, "unreadable body"))?;
    if raw.len() as u64 > max {
        return Err(text(413, "body too large"));
    }
    serde_json::from_str(&raw).map_err(|_| text(400, "body must be JSON"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_long_and_differ() {
        let (a, b) = (session_token(), session_token());
        assert_eq!(a.len(), 32);
        assert_ne!(a, b);
    }

    #[test]
    fn comparison_needs_the_exact_token() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"abcd"));
    }
}
