//! Plain-text report for the terminal.

use std::path::Path;

use raiox_core::ScanStats;
use raiox_core::efficiency::Efficiency;
use raiox_core::report::Overview;

const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const TOP_CARRIED: usize = 10;

fn short(value: f64) -> String {
    let abs = value.abs();
    match abs {
        a if a >= 1e9 => format!("{:.1}B", value / 1e9),
        a if a >= 1e6 => format!("{:.1}M", value / 1e6),
        a if a >= 1e3 => format!("{:.1}K", value / 1e3),
        _ => format!("{value:.0}"),
    }
}

fn n(value: u64) -> String {
    short(value as f64)
}

fn pct(share: f64) -> String {
    format!("{:.0}%", share * 100.0)
}

fn join(items: &[(String, u64)]) -> String {
    items.iter().map(|(name, count)| format!("{name} {}", n(*count))).collect::<Vec<_>>().join(" · ")
}

pub fn scan(stats: &ScanStats, db: &Path) {
    println!(
        "raiox · index updated in {:.1} s (read {:.1} s, write {:.1} s) · {} of {} files changed · {}B read · {} lines parsed",
        stats.elapsed_ms as f64 / 1000.0,
        stats.parse_ms as f64 / 1000.0,
        stats.write_ms as f64 / 1000.0,
        stats.changed,
        stats.files,
        n(stats.bytes_read),
        n(stats.lines_parsed),
    );
    println!("index: {}", db.display());
    for warning in &stats.warnings {
        println!("warning: {warning}");
    }
}

pub fn overview(o: &Overview) {
    let total: u64 = o.models.iter().map(|m| m.total()).sum();
    let cached: u64 = o.models.iter().map(|m| m.cache_read).sum();
    println!("\n━━ {} ━━", o.source.to_uppercase());
    println!(
        "{} to {} · {} active days · longest streak {} · ~{:.0} h active · {} sessions",
        o.first_day.as_deref().unwrap_or("?"),
        o.last_day.as_deref().unwrap_or("?"),
        o.active_days,
        o.longest_streak,
        o.active_hours,
        o.sessions,
    );
    let prompts = if o.prompts > 0 { format!(" · prompts {}", n(o.prompts)) } else { String::new() };
    println!(
        "tokens {} · {} read from cache{prompts} · subagent requests {}",
        n(total),
        pct(cached as f64 / total.max(1) as f64),
        n(o.subagent_calls)
    );
    for m in &o.models {
        println!(
            "  {:26} {:>7} req · in {:>7} · cache read {:>7} · cache write {:>7} · out {:>7} (thinking {})",
            m.model,
            n(m.calls),
            n(m.input),
            n(m.cache_read),
            n(m.cache_write),
            n(m.output),
            n(m.thinking)
        );
    }
    if let Some((day, calls)) = &o.busiest_day {
        let peak_hour = (0..24).max_by_key(|h| o.by_hour[*h]).unwrap_or(0);
        let peak_day = (0..7).max_by_key(|d| o.by_weekday[*d]).unwrap_or(0);
        println!(
            "busiest day {day} ({} requests) · peak hour {peak_hour}h · busiest weekday {}",
            n(*calls),
            WEEKDAYS[peak_day]
        );
    }
    println!("projects  {}", join(&o.projects));
    println!("tools     {}", join(&o.tools));
    println!(
        "work      +{} / -{} lines in {} files · {} compactions",
        n(o.lines_added),
        n(o.lines_removed),
        n(o.files_edited),
        o.compactions
    );
    if let Some(usd) = o.reported_cost_usd {
        println!("cost reported by the agent: ${usd:.0}");
    }
}

pub fn efficiency(e: &Efficiency) {
    println!("where the tokens go");
    let shares: Vec<String> = e.cost_shares.iter().map(|(k, v)| format!("{k} {}", pct(*v))).collect();
    println!("  cost weight      {}", shares.join(" · "));
    let c = &e.context;
    println!(
        "  context/request  mean {} · median {} · p90 {} · {} requests",
        short(c.mean),
        n(c.median),
        n(c.p90),
        n(c.calls)
    );
    let buckets: Vec<String> = e
        .buckets
        .iter()
        .map(|b| {
            format!("<{} {}/{}", b.up_to.map_or("∞".to_string(), n), pct(b.share_of_calls), pct(b.share_of_tokens))
        })
        .collect();
    println!("  requests/tokens  {}", buckets.join(" · "));
    for f in &e.fixed {
        println!(
            "  fixed overhead   {}: {} before any work · × {} requests = {} of context",
            f.kind,
            n(f.median_first_request),
            n(f.calls),
            pct(f.share_of_context)
        );
    }
    let rewrites: Vec<String> =
        e.rewrites.iter().map(|r| format!("{} {}× ({})", r.reason, r.count, n(r.tokens))).collect();
    println!("  cache rewrites   {}", rewrites.join(" · "));
    match (e.compactions, e.compaction_pre_median) {
        (0, _) => {}
        (count, 0) => println!("  compactions      {count}"),
        (count, pre) => println!("  compactions      {count} · median {} → {}", n(pre), n(e.compaction_post_median)),
    }
    let simulation: Vec<String> =
        e.simulation.iter().map(|(cap, saved)| format!("at {} -{}", n(*cap), pct(*saved))).collect();
    println!("  compact earlier  {} (upper bound)", simulation.join(" · "));
    println!("  stays loaded (tokens re-read):");
    for item in e.carried.iter().take(TOP_CARRIED) {
        println!(
            "    {:28} {:>7} ({:>3}) · {} items, avg {} tok{}",
            item.name,
            short(item.carried_tokens),
            pct(item.share_of_context),
            n(item.results),
            short(item.avg_tokens),
            if item.images > 0 { format!(", {} images", n(item.images)) } else { String::new() }
        );
    }
    if let Some(listed) = e.snapshots.get("skill_listing_items") {
        let calls: u64 = e.skills_used.iter().map(|(_, c)| c).sum();
        println!(
            "  skills           {listed} listed · {} used ({calls} calls): {}",
            e.skills_used.len(),
            join(&e.skills_used[..e.skills_used.len().min(6)])
        );
    }
    if !e.threads_by_model.is_empty() {
        println!("  thread index     {}", join(&e.threads_by_model));
    }
    for key in ["context_window", "rate_limits", "base_instructions", "startup_context"] {
        if let Some(value) = e.snapshots.get(key) {
            println!("  {key:16} {value}");
        }
    }
}

pub fn secrets(found: &[(String, u64)]) {
    if !found.is_empty() {
        println!("secrets files touched (counts only): {}", join(found));
    }
}

pub fn platforms(platforms: &[raiox_core::report::Platform]) {
    for p in platforms {
        println!("\n━━ {} (already counted in Claude Code / Codex) ━━", p.source.to_uppercase());
        for a in &p.agents {
            println!("  {:28} {:22} {:>7} tokens · {} turns · ${:.2}", a.name, a.model, n(a.tokens), a.records, a.cost_usd);
        }
        for s in &p.skill_loads {
            println!("  skill {:22} {} loads · {} tokens · ${:.2}", s.skill, s.loads, n(s.tokens), s.cost_usd);
        }
    }
}
