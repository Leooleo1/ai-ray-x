//! raiox: where your AI coding agents' tokens go. Reads local logs only.


use std::path::PathBuf;

use anyhow::{Context, Result};
use raiox::{actions, bots, data, mcp, print, room, serve, setup};
use clap::{Args, Parser, Subcommand};
use raiox_core::model::{CLAUDE, CODEX};
use raiox_core::{efficiency, report, Roots, Store};

#[derive(Parser)]
#[command(name = "raiox", version, about = "Where your AI coding agents' tokens go. Local logs only.")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
    #[command(flatten)]
    common: Common,
}

#[derive(Args)]
struct Common {
    /// Index location (default: the platform cache folder).
    #[arg(long, global = true)]
    db: Option<PathBuf>,
    /// Claude Code folder (default: $CLAUDE_CONFIG_DIR or ~/.claude).
    #[arg(long, global = true)]
    claude_dir: Option<PathBuf>,
    /// Codex folder (default: $CODEX_HOME or ~/.codex).
    #[arg(long, global = true)]
    codex_dir: Option<PathBuf>,
    /// Drop the index and read every log again.
    #[arg(long, global = true)]
    rebuild: bool,
}

#[derive(Subcommand)]
enum Command {
    /// Open the panel in the browser (the default).
    Open {
        /// Port on 127.0.0.1 (0 picks a free one).
        #[arg(long, default_value_t = 0)]
        port: u16,
        /// Print the link without opening the browser.
        #[arg(long)]
        no_open: bool,
        /// Folder with the built panel.
        #[arg(long)]
        web_dir: Option<PathBuf>,
    },
    /// Run the read-only MCP server over stdio (for Claude Code, Codex, OpenBot, agentes.chat).
    Mcp,
    /// Context the session of a folder carries now (default: the current folder's newest session).
    Context {
        /// Session id; overrides the folder lookup.
        #[arg(long)]
        session: Option<String>,
        /// Project folder whose newest Claude Code session to read (default: current folder).
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// What weighs most over the indexed period (compact JSON).
    Weighs {
        #[arg(long)]
        source: Option<String>,
    },
    /// Dieta's ranked suggestions (compact JSON). Never applies anything.
    Diet {
        #[arg(long)]
        source: Option<String>,
    },
    /// Apply a Dieta suggestion to this project (backup, ledger, Memo lesson).
    Apply {
        suggestion: String,
        #[arg(long, default_value = "claude")]
        source: String,
    },
    /// Undo an applied change by its id (see the panel or the ledger).
    Undo { change: String },
    /// Ask one agent (raio, dieta or memo) in this project's room; only in opted-in projects.
    Ask {
        agent: String,
        /// What the person wrote (kept in the room; the answer comes from data).
        #[arg(long)]
        text: Option<String>,
    },
    /// Print the report in the terminal.
    Report {
        /// Print JSON instead of text.
        #[arg(long)]
        json: bool,
    },
}

fn main() -> Result<()> {
    restore_default_sigpipe();
    let cli = Cli::parse();
    let detected = Roots::detect()?;
    let roots = Roots {
        claude: cli.common.claude_dir.unwrap_or(detected.claude),
        codex: cli.common.codex_dir.unwrap_or(detected.codex),
        home: detected.home,
        openbot: detected.openbot,
    };
    let db = cli.common.db.or_else(raiox_core::default_index_path).context("no cache folder found; pass --db")?;
    let mut store = Store::open(&db)?;
    if cli.common.rebuild {
        store.reset()?;
    }
    match cli.command.unwrap_or(Command::Open { port: 0, no_open: false, web_dir: None }) {
        Command::Open { port, no_open, web_dir } => {
            let web_dir = web_dir.unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../apps/web/dist"));
            raiox_core::refresh(&mut store, &roots)?;
            let state_dir = state_dir(&db);
            let project_dir = std::env::current_dir()?;
            serve::run(serve::Panel { store, roots, web_dir, state_dir, project_dir, jobs: bots::Jobs::default(), clones: setup::Clones::default() }, port, !no_open)
        }
        Command::Mcp => mcp::run(store, roots),
        Command::Apply { suggestion, source } => {
            raiox_core::refresh(&mut store, &roots)?;
            emit(actions::apply_suggestion(&store, &roots.home, &state_dir(&db), &std::env::current_dir()?, &suggestion, &source)?)
        }
        Command::Ask { agent, text } => {
            raiox_core::refresh(&mut store, &roots)?;
            let messages = room::ask(&store, &roots, &state_dir(&db), &std::env::current_dir()?, &agent, text.as_deref())?;
            emit(serde_json::to_value(messages)?)
        }
        Command::Undo { change } => emit(actions::undo_change(&store, &roots.home, &state_dir(&db), &std::env::current_dir()?, &change)?),
        Command::Context { session, dir } => {
            raiox_core::refresh(&mut store, &roots)?;
            let dir = dir.map_or_else(std::env::current_dir, Ok)?;
            let session = session.or_else(|| room::current_session(&store, &roots, &dir));
            emit(mcp::context_status(&store, &serde_json::json!({ "session": session }))?)
        }
        Command::Weighs { source } => {
            raiox_core::refresh(&mut store, &roots)?;
            emit(mcp::what_weighs(&store, &serde_json::json!({ "source": source }))?)
        }
        Command::Diet { source } => {
            raiox_core::refresh(&mut store, &roots)?;
            emit(mcp::suggest_diet(&store, &serde_json::json!({ "source": source }))?)
        }
        Command::Report { json: true } => {
            println!("{}", serde_json::to_string_pretty(&data::refresh_and_collect(&mut store, &roots)?)?);
            Ok(())
        }
        Command::Report { json: false } => print_report(&mut store, &roots, &db),
    }
}

fn print_report(store: &mut Store, roots: &Roots, db: &std::path::Path) -> Result<()> {
    let stats = raiox_core::refresh(store, roots)?;
    print::scan(&stats, db);
    let conn = store.conn();
    for source in [CLAUDE, CODEX] {
        let overview = report::overview(conn, source)?;
        if overview.models.is_empty() {
            continue;
        }
        print::overview(&overview);
        print::efficiency(&efficiency::efficiency(conn, source)?);
        print::secrets(&report::secret_touches(conn, source)?);
    }
    print::platforms(&report::platforms(conn)?);
    Ok(())
}

/// Rust ignores SIGPIPE, so `raiox report | head` would panic on the closed pipe.
/// Restoring the default makes it exit quietly, like other command-line tools.
fn restore_default_sigpipe() {
    #[cfg(unix)]
    // SAFETY: called once at startup, before any thread is spawned.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}

fn emit(value: serde_json::Value) -> Result<()> {
    println!("{}", mcp::round_floats(value));
    Ok(())
}


/// The ledger and backups live next to the index.
fn state_dir(db: &std::path::Path) -> std::path::PathBuf {
    db.parent().map_or_else(|| std::path::PathBuf::from("."), std::path::Path::to_path_buf)
}
