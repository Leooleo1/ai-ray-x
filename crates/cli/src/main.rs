//! raiox: where your AI coding agents' tokens go. Reads local logs only.

mod data;
mod print;
mod serve;

use std::path::PathBuf;

use anyhow::{Context, Result};
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
    /// Print the report in the terminal.
    Report {
        /// Print JSON instead of text.
        #[arg(long)]
        json: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let detected = Roots::detect()?;
    let roots = Roots {
        claude: cli.common.claude_dir.unwrap_or(detected.claude),
        codex: cli.common.codex_dir.unwrap_or(detected.codex),
        home: detected.home,
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
            serve::run(serve::Panel { store, roots, web_dir }, port, !no_open)
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
    Ok(())
}
