//! Questions a bot is asked on a schedule, set by the person in the bot's
//! settings. Each routine answers in its own conversation, so the main one
//! stays clean, and only while `raiox open` runs: nothing is installed in
//! the system. Every run spends the person's plan, so the panel shows what
//! the last one read.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use chrono::{Datelike, Local, TimeZone};
use serde::{Deserialize, Serialize};

use crate::bots::{self, Jobs};
use crate::{room, threads};

/// How often the scheduler looks at the clock.
const TICK: Duration = Duration::from_secs(30);
const MAX_TEXT: usize = 500;
const MAX_ROUTINES: usize = 20;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Routine {
    pub id: String,
    pub bot: String,
    pub text: String,
    /// Days it runs, 0 = Monday … 6 = Sunday.
    pub days: Vec<u8>,
    pub hour: u8,
    pub minute: u8,
    pub enabled: bool,
    /// Its own conversation, made on the first run.
    #[serde(default)]
    pub thread: Option<String>,
    #[serde(default)]
    pub last_ms: Option<i64>,
    /// Runs only when the project's files changed since its last run.
    #[serde(default)]
    pub only_if_changed: bool,
    /// The project's files at the last run (sizes and times, hashed).
    #[serde(default)]
    pub fingerprint: Option<u64>,
    /// When it last skipped its run because nothing changed.
    #[serde(default)]
    pub skipped_ms: Option<i64>,
}

fn file(state_dir: &Path) -> PathBuf {
    state_dir.join("routines.json")
}

pub fn list(state_dir: &Path) -> Vec<Routine> {
    fs::read_to_string(file(state_dir)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn save(state_dir: &Path, routines: &[Routine]) -> Result<()> {
    fs::create_dir_all(state_dir)?;
    let temp = file(state_dir).with_extension("tmp");
    fs::write(&temp, serde_json::to_string_pretty(routines)? + "\n")?;
    fs::rename(temp, file(state_dir))?;
    Ok(())
}

pub fn create(state_dir: &Path, bot: &str, text: &str, days: &[u8], hour: u8, minute: u8, only_if_changed: bool) -> Result<()> {
    let text = text.trim();
    if text.is_empty() || text.chars().count() > MAX_TEXT {
        bail!("a routine asks 1 to {MAX_TEXT} characters");
    }
    if days.is_empty() || days.iter().any(|d| *d > 6) || hour > 23 || minute > 59 {
        bail!("invalid days or time");
    }
    bots::list(state_dir).into_iter().find(|b| b.id == bot).context("no such bot")?;
    let routines = list(state_dir);
    if routines.len() >= MAX_ROUTINES {
        bail!("at most {MAX_ROUTINES} routines");
    }
    let mut days = days.to_vec();
    days.sort_unstable();
    days.dedup();
    let routine = Routine {
        id: format!("r{:x}", room::now_ms()),
        bot: bot.to_string(),
        text: text.to_string(),
        days,
        hour,
        minute,
        enabled: true,
        thread: None,
        last_ms: None,
        only_if_changed,
        fingerprint: None,
        skipped_ms: None,
    };
    save(state_dir, &routines.into_iter().chain([routine]).collect::<Vec<_>>())
}

fn change(state_dir: &Path, id: &str, edit: impl Fn(Routine) -> Option<Routine>) -> Result<()> {
    let routines = list(state_dir);
    if !routines.iter().any(|r| r.id == id) {
        bail!("no routine {id}");
    }
    save(state_dir, &routines.into_iter().filter_map(|r| if r.id == id { edit(r) } else { Some(r) }).collect::<Vec<_>>())
}

pub fn set_enabled(state_dir: &Path, id: &str, enabled: bool) -> Result<()> {
    change(state_dir, id, |r| Some(Routine { enabled, ..r }))
}

pub fn set_only_if_changed(state_dir: &Path, id: &str, on: bool) -> Result<()> {
    change(state_dir, id, |r| Some(Routine { only_if_changed: on, ..r }))
}

pub fn delete(state_dir: &Path, id: &str) -> Result<()> {
    change(state_dir, id, |_| None)
}

/// The scheduled time today, in milliseconds, when today is one of its days.
fn today_at(r: &Routine, now: chrono::DateTime<Local>) -> Option<i64> {
    let weekday = now.weekday().num_days_from_monday() as u8;
    if !r.days.contains(&weekday) {
        return None;
    }
    Local
        .with_ymd_and_hms(now.year(), now.month(), now.day(), u32::from(r.hour), u32::from(r.minute), 0)
        .single()
        .map(|t| t.timestamp_millis())
}

/// Due when today's time has passed and it has not run since.
fn is_due(r: &Routine, now: chrono::DateTime<Local>) -> bool {
    r.enabled && today_at(r, now).is_some_and(|at| now.timestamp_millis() >= at && r.last_ms.is_none_or(|last| last < at))
}

/// Runs what is due; a busy bot is tried again on the next tick.
fn tick(state_dir: &Path, jobs: &Jobs) {
    // Live sessions unused for a while close here too, not only when another turn starts.
    crate::live::reap();
    let now = Local::now();
    for routine in list(state_dir).into_iter().filter(|r| is_due(r, now)) {
        if routine.thread.as_deref().is_some_and(|t| jobs.is_thread_busy(&routine.bot, t)) {
            continue;
        }
        // Nothing changed in the project since the last run: skip today, quietly.
        let fingerprint = bots::find(state_dir, &routine.bot).ok().and_then(|bot| crate::drafts::fingerprint(&bot.dir));
        if routine.only_if_changed && fingerprint.is_some() && fingerprint == routine.fingerprint {
            let at = Some(now.timestamp_millis());
            let _ = change(state_dir, &routine.id, |r| Some(Routine { last_ms: at, skipped_ms: at, ..r }));
            continue;
        }
        let thread = match routine.thread.clone().filter(|t| threads::list(state_dir, &routine.bot).iter().any(|x| &x.id == t)) {
            Some(thread) => thread,
            None => match threads::create(state_dir, &routine.bot, &format!("Routine · {}", routine.text)) {
                Ok(made) => made.id,
                Err(_) => continue,
            },
        };
        let _ = bots::ask(state_dir, jobs, &routine.bot, &thread, &routine.text, &[]);
        let last = Some(now.timestamp_millis());
        let thread_kept = Some(thread);
        // Marked as run even when the ask was refused, so a broken routine does not retry every 30 seconds.
        let _ = change(state_dir, &routine.id, |r| Some(Routine { last_ms: last, thread: thread_kept.clone(), fingerprint, ..r }));
    }
}

/// Starts the scheduler next to the panel; it lives as long as the panel does.
pub fn spawn(state_dir: PathBuf, jobs: Jobs) {
    crate::app_update::set_auto(crate::settings::Settings::load(&state_dir).auto_update);
    std::thread::spawn(move || loop {
        tick(&state_dir, &jobs);
        // The board moves on with the same clock: finished tasks, and the next ones when bots pick them up.
        crate::tasks::tick(&state_dir, &jobs);
        // Scheduled Dev/Infra audits run on the same clock (only where enabled, respecting free hours).
        crate::auditor::tick(&state_dir, &jobs);
        // Workflows fire and move on with the same clock.
        crate::workflow_run::tick(&state_dir, &jobs);
        // A new version of the app installs itself here when the person chose so and no AI is answering.
        crate::app_update::tick_auto(&jobs);
        std::thread::sleep(TICK);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn routine(days: Vec<u8>, last_ms: Option<i64>) -> Routine {
        Routine { id: "r".into(), bot: "b".into(), text: "t".into(), days, hour: 9, minute: 0, enabled: true, thread: None, last_ms, only_if_changed: false, fingerprint: None, skipped_ms: None }
    }

    #[test]
    fn due_once_a_day_after_its_time() {
        let monday_10 = Local.with_ymd_and_hms(2026, 11, 16, 10, 0, 0).single().unwrap();
        let monday_8 = Local.with_ymd_and_hms(2026, 11, 16, 8, 0, 0).single().unwrap();
        assert!(is_due(&routine(vec![0], None), monday_10));
        assert!(!is_due(&routine(vec![0], None), monday_8));
        assert!(!is_due(&routine(vec![1], None), monday_10));
        let ran = monday_10.timestamp_millis() - 1000;
        assert!(!is_due(&routine(vec![0], Some(ran)), monday_10));
    }
}
