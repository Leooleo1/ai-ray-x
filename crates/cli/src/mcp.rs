//! `raiox mcp`: a read-only MCP server over stdio (newline-delimited JSON-RPC).
//! Tools answer with small JSON on purpose: every token they return lands in
//! the caller's context. Nothing here changes a setting.

use std::io::{self, BufRead, Write};

use anyhow::Result;
use raiox_core::model::{CLAUDE, CODEX};
use raiox_core::{advice, efficiency, Roots, Store};
use serde_json::{json, Value};

const FALLBACK_PROTOCOL: &str = "2025-06-18";
const TOP_ITEMS: usize = 6;
const INSTRUCTIONS: &str = "raiox reads the local logs of Claude Code and Codex and reports where tokens go. \
Call context_status to see how much context the current session carries, what_weighs for the heaviest \
items over time, and suggest_diet for ranked ways to carry less. Read-only: it never changes settings.";

pub fn run(mut store: Store, roots: Roots) -> Result<()> {
    let stdin = io::stdin();
    let mut out = io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let Some(reply) = handle(&mut store, &roots, &line) else { continue };
        writeln!(out, "{reply}")?;
        out.flush()?;
    }
    Ok(())
}

/// Returns the response line, or None for notifications.
fn handle(store: &mut Store, roots: &Roots, line: &str) -> Option<Value> {
    let request: Value = match serde_json::from_str(line) {
        Ok(value) => value,
        Err(_) => return Some(error(Value::Null, -32700, "parse error")),
    };
    let id = request.get("id").cloned()?;
    let method = request.get("method").and_then(Value::as_str).unwrap_or("");
    let params = request.get("params").cloned().unwrap_or(Value::Null);
    let result = match method {
        "initialize" => Ok(json!({
            "protocolVersion": params.get("protocolVersion").and_then(Value::as_str).unwrap_or(FALLBACK_PROTOCOL),
            "capabilities": { "tools": { "listChanged": false } },
            "serverInfo": { "name": "raiox", "version": env!("CARGO_PKG_VERSION") },
            "instructions": INSTRUCTIONS,
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tool_list() })),
        "tools/call" => Ok(call_tool(store, roots, &params)),
        _ => Err((-32601, format!("method not found: {method}"))),
    };
    Some(match result {
        Ok(value) => json!({ "jsonrpc": "2.0", "id": id, "result": value }),
        Err((code, message)) => error(id, code, &message),
    })
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn tool_list() -> Value {
    let source = json!({ "type": "string", "enum": [CLAUDE, CODEX], "description": "Agent to analyze; default: the one with most usage" });
    json!([
        {
            "name": "context_status",
            "description": "How much context the current session carries now, what has been loaded since the last compaction, and where compaction usually happens.",
            "inputSchema": { "type": "object", "properties": { "session": { "type": "string", "description": "Session id; default: the most recent one" } }, "additionalProperties": false },
            "annotations": { "readOnlyHint": true }
        },
        {
            "name": "what_weighs",
            "description": "Where tokens went over the indexed period: context per request, fixed startup overhead, what stays loaded longest, cache rewrites and compactions.",
            "inputSchema": { "type": "object", "properties": { "source": source }, "additionalProperties": false },
            "annotations": { "readOnlyHint": true }
        },
        {
            "name": "suggest_diet",
            "description": "Ranked suggestions to carry less context, each with evidence, an estimated saving and how to apply it. Suggestions only; a person applies them.",
            "inputSchema": { "type": "object", "properties": { "source": source }, "additionalProperties": false },
            "annotations": { "readOnlyHint": true }
        }
    ])
}

fn call_tool(store: &mut Store, roots: &Roots, params: &Value) -> Value {
    let name = params.get("name").and_then(Value::as_str).unwrap_or("");
    let args = params.get("arguments").cloned().unwrap_or(Value::Null);
    let outcome = raiox_core::refresh(store, roots).and_then(|_| match name {
        "context_status" => context_status(store, &args),
        "what_weighs" => what_weighs(store, &args),
        "suggest_diet" => suggest_diet(store, &args),
        other => Err(anyhow::anyhow!("unknown tool: {other}")),
    });
    match outcome {
        Ok(value) => {
            let value = round_floats(value);
            json!({ "content": [{ "type": "text", "text": value.to_string() }], "structuredContent": value })
        }
        Err(err) => json!({ "content": [{ "type": "text", "text": format!("{err:#}") }], "isError": true }),
    }
}

pub fn context_status(store: &Store, args: &Value) -> Result<Value> {
    let session = args.get("session").and_then(Value::as_str);
    Ok(match advice::context_status(store.conn(), session)? {
        Some(status) => serde_json::to_value(status)?,
        None => json!({ "message": "no session found in the logs" }),
    })
}

/// The agent with most context re-read, unless the caller picked one.
fn pick_source(store: &Store, args: &Value) -> Result<String> {
    if let Some(source) = args.get("source").and_then(Value::as_str) {
        return Ok(source.to_string());
    }
    let busiest: Option<String> = store
        .conn()
        .query_row(
            "SELECT source FROM calls GROUP BY source ORDER BY SUM(input + cache_read + cache_write_5m + cache_write_1h) DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .ok();
    Ok(busiest.unwrap_or_else(|| CLAUDE.to_string()))
}

pub fn what_weighs(store: &Store, args: &Value) -> Result<Value> {
    let source = pick_source(store, args)?;
    let e = efficiency::efficiency(store.conn(), &source)?;
    let loaded: Vec<Value> = e
        .carried
        .iter()
        .take(TOP_ITEMS)
        .map(|c| json!({ "name": c.name, "tokens_reread": c.carried_tokens as u64, "share": round(c.share_of_context) }))
        .collect();
    Ok(json!({
        "source": source,
        "requests": e.context.calls,
        "context_per_request": { "median": e.context.median, "p90": e.context.p90 },
        "cost_shares": e.cost_shares.iter().map(|(k, v)| (k.clone(), json!(round(*v)))).collect::<serde_json::Map<String, Value>>(),
        "startup_overhead": e.fixed.iter().map(|f| json!({ "kind": f.kind, "tokens": f.median_first_request, "share": round(f.share_of_context) })).collect::<Vec<_>>(),
        "stays_loaded": loaded,
        "cache_rewrites": e.rewrites.iter().map(|r| json!({ "reason": r.reason, "count": r.count, "tokens": r.tokens })).collect::<Vec<_>>(),
        "compactions": { "count": e.compactions, "median_before": e.compaction_pre_median, "restart_floor": e.compaction.floor, "refill_after": e.compaction.refill },
        "compact_earlier": { "plans": e.compaction.plans, "best": e.compaction.best, "calibration": e.compaction.calibration },
    }))
}

pub fn suggest_diet(store: &Store, args: &Value) -> Result<Value> {
    let source = pick_source(store, args)?;
    let e = efficiency::efficiency(store.conn(), &source)?;
    let listed = e.snapshots.get("skill_listing_items").and_then(|v| v.as_u64().or_else(|| v.as_str()?.parse().ok()));
    let suggestions = advice::suggestions(&e, listed);
    Ok(json!({ "source": source, "suggestions": suggestions, "note": "Suggestions only. Savings are estimates over the indexed period." }))
}

fn round(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

/// Three decimals are enough for shares; longer floats only cost the caller tokens.
pub fn round_floats(value: Value) -> Value {
    match value {
        Value::Number(n) if n.is_f64() => n.as_f64().map_or(Value::Number(n), |f| json!(round(f))),
        Value::Array(items) => Value::Array(items.into_iter().map(round_floats).collect()),
        Value::Object(map) => Value::Object(map.into_iter().map(|(k, v)| (k, round_floats(v))).collect()),
        other => other,
    }
}
