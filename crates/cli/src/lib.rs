//! raiox as a library: the panel server and everything behind it, used by the
//! `raiox` command line and by the desktop app (which runs the same panel in
//! its own window).

pub mod access;
pub mod actions;
pub mod bot_clis;
pub mod bot_mail;
pub mod bot_marks;
pub mod bot_run;
pub mod app_update;
pub mod agent_switch;
pub mod app_start;
pub mod auditor;
pub mod auto_edit;
pub mod safe_commands;
pub mod batches;
pub mod bot_steps;
pub mod bot_trail;
pub mod bot_usage;
pub mod bots;
pub mod carry;
pub mod channels;
pub mod budget;
pub mod connections;
pub mod commands;
pub mod data;
pub mod deliveries;
pub mod devices;
pub mod grants;
pub mod limits;
pub mod drafts;
pub mod install;
pub mod links;
pub mod live;
pub mod memory;
pub mod memo;
pub mod pins;
pub mod preview;
pub mod procs;
pub mod profile;
pub mod requests;
pub mod room;
pub mod router;
pub mod routines;
pub mod rsi;
pub mod rules;
pub mod mcp;
pub mod print;
pub mod push;
pub mod remote;
pub mod serve;
pub mod settings;
pub mod simulator;
pub mod setup;
pub mod tasks;
pub mod telegram;
pub mod team;
pub mod threads;
pub mod tunnel;
pub mod uploads;
pub mod wiki;
pub mod workflow_run;
pub mod workflows;

use std::path::PathBuf;

use anyhow::{Context, Result};
use raiox_core::{Roots, Store};

/// The panel running inside another program (the desktop app).
pub struct Started {
    /// The panel's address with its session token, for the app's window.
    pub url: String,
    pub port: u16,
    pub jobs: bots::Jobs,
    pub state_dir: PathBuf,
}

/// Opens the index and starts the panel server on its own thread; the caller shows it.
/// `db` overrides the index location (tests); `web_dir` is where the built interface is.
/// The language the person picked in the panel, if any ("en" or "pt-BR"), for the app's own words.
pub fn chosen_language(db: Option<PathBuf>) -> Option<String> {
    let db = db.or_else(raiox_core::default_index_path)?;
    settings::Settings::load(db.parent()?).language
}

pub fn start_panel(web_dir: PathBuf, db: Option<PathBuf>) -> Result<Started> {
    let roots = Roots::detect()?;
    let db = db.or_else(raiox_core::default_index_path).context("no cache folder found")?;
    let mut store = Store::open(&db)?;
    raiox_core::refresh(&mut store, &roots)?;
    let state_dir = db.parent().map_or_else(|| PathBuf::from("."), std::path::Path::to_path_buf);
    let project_dir = roots.home.clone();
    let jobs = bots::Jobs::default();
    let panel = serve::Panel { store, roots, web_dir, state_dir: state_dir.clone(), project_dir, jobs: jobs.clone(), clones: setup::Clones::default() };
    let (url, port) = serve::start(panel, 0)?;
    Ok(Started { url, port, jobs, state_dir })
}
