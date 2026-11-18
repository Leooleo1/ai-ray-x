//! raiox: where your AI coding agents' tokens go. Reads local logs only.

mod print;

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use raiox_core::model::{CLAUDE, CODEX};
use raiox_core::{Roots, Store, efficiency, report};
use serde_json::json;

#[derive(Parser)]
#[command(name = "raiox", version, about = "Where your AI coding agents' tokens go. Local logs only.")]
struct Args {
    /// Index location (default: the platform cache folder).
    #[arg(long)]
    db: Option<PathBuf>,
    /// Claude Code folder (default: $CLAUDE_CONFIG_DIR or ~/.claude).
    #[arg(long)]
    claude_dir: Option<PathBuf>,
    /// Codex folder (default: $CODEX_HOME or ~/.codex).
    #[arg(long)]
    codex_dir: Option<PathBuf>,
    /// Drop the index and read every log again.
    #[arg(long)]
    rebuild: bool,
    /// Print the report as JSON.
    #[arg(long)]
    json: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let detected = Roots::detect()?;
    let roots = Roots {
        claude: args.claude_dir.unwrap_or(detected.claude),
        codex: args.codex_dir.unwrap_or(detected.codex),
        home: detected.home,
    };
    let db = args.db.or_else(raiox_core::default_index_path).context("no cache folder found; pass --db")?;
    let mut store = Store::open(&db)?;
    if args.rebuild {
        store.reset()?;
    }
    let stats = raiox_core::refresh(&mut store, &roots)?;
    let conn = store.conn();
    let mut sources = Vec::new();
    for source in [CLAUDE, CODEX] {
        let overview = report::overview(conn, source)?;
        if overview.models.is_empty() {
            continue;
        }
        let efficiency = efficiency::efficiency(conn, source)?;
        let secrets = report::secret_touches(conn, source)?;
        sources.push((overview, efficiency, secrets));
    }
    if args.json {
        let body: Vec<_> = sources
            .iter()
            .map(|(overview, efficiency, secrets)| json!({ "overview": overview, "efficiency": efficiency, "secrets": secrets }))
            .collect();
        println!("{}", serde_json::to_string_pretty(&json!({ "sources": body }))?);
        return Ok(());
    }
    print::scan(&stats, &db);
    for (overview, efficiency, secrets) in &sources {
        print::overview(overview);
        print::efficiency(efficiency);
        print::secrets(secrets);
    }
    Ok(())
}
