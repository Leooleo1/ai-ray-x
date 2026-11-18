//! The report as JSON, shared by `raiox report --json` and the panel API.

use anyhow::Result;
use raiox_core::model::{CLAUDE, CODEX};
use raiox_core::{advice, efficiency, report, Roots, ScanStats, Store};
use serde_json::{json, Value};

pub fn refresh_and_collect(store: &mut Store, roots: &Roots) -> Result<Value> {
    let stats = raiox_core::refresh(store, roots)?;
    collect(store, &stats)
}

pub fn collect(store: &Store, stats: &ScanStats) -> Result<Value> {
    let conn = store.conn();
    let mut sources = Vec::new();
    for source in [CLAUDE, CODEX] {
        let overview = report::overview(conn, source)?;
        if overview.models.is_empty() {
            continue;
        }
        let efficiency = efficiency::efficiency(conn, source)?;
        let secrets = report::secret_touches(conn, source)?;
        let listed = efficiency
            .snapshots
            .get("skill_listing_items")
            .and_then(|v| v.as_u64().or_else(|| v.as_str()?.parse().ok()));
        let suggestions = advice::suggestions(&efficiency, listed);
        sources.push(json!({ "overview": overview, "efficiency": efficiency, "secrets": secrets, "suggestions": suggestions }));
    }
    Ok(json!({
        "generated_at": chrono_now(),
        "scan": {
            "files": stats.files,
            "changed": stats.changed,
            "bytes_read": stats.bytes_read,
            "elapsed_ms": stats.elapsed_ms,
            "warnings": stats.warnings,
        },
        "sources": sources,
        "platforms": report::platforms(conn)?,
        "context_status": advice::context_status(conn, None)?,
    }))
}

fn chrono_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as i64)
}
