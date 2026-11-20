//! Moves workflows on, on the routines' clock: fires the triggers that came due, sends each
//! step when its AI is free, takes the answer when the turn ends, and stops at an approval
//! until the person answers it.

use std::path::Path;

use chrono::{Datelike, Local, TimeZone};
use serde_json::Value;

use crate::bots::{self, Jobs};
use crate::workflows::{self, Flow, RunStatus, Step, StepResult, Trigger};
use crate::{channels, room, tasks, threads};

/// A question with no answer after this long (the longest turn, with room) fails the run.
const ASK_TIMEOUT_MS: i64 = 40 * 60 * 1000;
/// The AI is free again but no answer came: how long to wait for it to be written.
const ANSWER_GRACE_MS: i64 = 2 * 60 * 1000;

/// Today's time of a schedule, in milliseconds, when today is one of its days.
fn today_at(days: &[u8], hour: u8, minute: u8, now: chrono::DateTime<Local>) -> Option<i64> {
    let weekday = now.weekday().num_days_from_monday() as u8;
    if !days.contains(&weekday) {
        return None;
    }
    Local.with_ymd_and_hms(now.year(), now.month(), now.day(), u32::from(hour), u32::from(minute), 0).single().map(|t| t.timestamp_millis())
}

/// What set a workflow off since it was last looked at, if anything.
fn fired(flow: &Flow, finished: &[(String, String, i64)], now: chrono::DateTime<Local>) -> Option<String> {
    match flow.trigger.as_ref()? {
        Trigger::Manual => None,
        Trigger::Schedule { days, hour, minute } => {
            let at = today_at(days, *hour, *minute, now)?;
            (now.timestamp_millis() >= at && flow.checked_ms < at).then(|| format!("{hour:02}:{minute:02}"))
        }
        Trigger::TaskDone { bot } => finished
            .iter()
            .find(|(by, _, at)| *at > flow.checked_ms && bot.as_ref().is_none_or(|b| b == by))
            .map(|(_, title, _)| title.clone()),
    }
}

/// `{previous}` is the last step's text, `{trigger}` what set the run off (Portuguese names too).
fn fill(text: &str, previous: &str, cause: &str) -> String {
    let filled = text.replace("{anterior}", previous).replace("{previous}", previous).replace("{gatilho}", cause).replace("{trigger}", cause);
    filled.chars().take(bots::MAX_TEXT).collect()
}

enum Outcome {
    /// Nothing to do yet (the AI is busy, or still answering).
    Wait,
    Asked(i64),
    Waiting,
    Next(StepResult),
    Fail(String),
}

/// The AI's answer in its workflow conversation since the question went out.
fn answer_since(state_dir: &Path, bot: &str, thread: &str, at: i64) -> Option<Result<String, String>> {
    let messages = room::read_messages(&threads::chat_file(state_dir, bot, thread));
    let reply = messages.iter().rev().take_while(|m| m.ms >= at).find(|m| m.author == "bot" && (m.kind == "answer" || m.kind == "failed"))?;
    let text = |pointer: &str| reply.data.pointer(pointer).and_then(Value::as_str).unwrap_or("").to_string();
    Some(if reply.kind == "answer" { Ok(text("/reply/text")) } else { Err(Some(text("/text")).filter(|t| !t.is_empty()).unwrap_or_else(|| "the AI's turn failed".into())) })
}

/// The workflow's own conversation with this AI, made the first time.
fn thread_for(state_dir: &Path, flow: &mut Flow, bot: &str) -> Result<String, String> {
    if let Some(thread) = flow.threads.get(bot).filter(|t| threads::list(state_dir, bot).iter().any(|x| &x.id == *t)) {
        return Ok(thread.clone());
    }
    let made = threads::create(state_dir, bot, &format!("Workflow · {}", flow.name)).map_err(|e| format!("{e:#}"))?;
    flow.threads.insert(bot.to_string(), made.id.clone());
    Ok(made.id)
}

fn step_outcome(state_dir: &Path, jobs: &Jobs, flow: &mut Flow, now: i64) -> Option<Outcome> {
    let run = flow.run.as_ref().filter(|run| run.status == RunStatus::Running)?;
    let Some(step) = flow.steps.get(run.step).cloned() else {
        return Some(Outcome::Next(StepResult { ok: true, text: String::new(), ms: now }));
    };
    let previous = run.results.last().map(|r| r.text.clone()).unwrap_or_default();
    let (cause, asked) = (run.cause.clone(), run.asked_ms);
    Some(match step {
        Step::Approve { .. } => Outcome::Waiting,
        Step::Post { channel, text } => match channels::post(state_dir, jobs, &channel, &fill(&text, &previous, &cause)) {
            Ok(()) => Outcome::Next(StepResult { ok: true, text: previous, ms: now }),
            Err(error) => Outcome::Fail(format!("{error:#}")),
        },
        Step::Ask { bot, text } => {
            if jobs.is_busy(&bot) {
                return Some(match asked {
                    Some(at) if now - at > ASK_TIMEOUT_MS => Outcome::Fail("the AI took too long to answer".into()),
                    _ => Outcome::Wait,
                });
            }
            let thread = match thread_for(state_dir, flow, &bot) {
                Ok(thread) => thread,
                Err(error) => return Some(Outcome::Fail(error)),
            };
            match asked {
                None => match bots::ask(state_dir, jobs, &bot, &thread, &fill(&text, &previous, &cause), &[]) {
                    Ok(()) => Outcome::Asked(now),
                    Err(error) => Outcome::Fail(format!("{error:#}")),
                },
                Some(at) => match answer_since(state_dir, &bot, &thread, at) {
                    Some(Ok(answer)) => Outcome::Next(StepResult { ok: true, text: answer, ms: now }),
                    Some(Err(error)) => Outcome::Fail(error),
                    None if now - at > ANSWER_GRACE_MS => Outcome::Fail("the AI ended its turn with no answer".into()),
                    None => Outcome::Wait,
                },
            }
        }
    })
}

/// Moves one run as far as it can go now: several quick steps in a row, then a question or an approval.
pub(crate) fn advance(state_dir: &Path, jobs: &Jobs, flow: &mut Flow, now: i64) {
    for _ in 0..=flow.steps.len() {
        let Some(outcome) = step_outcome(state_dir, jobs, flow, now) else { return };
        let steps = flow.steps.len();
        let Some(run) = flow.run.as_mut() else { return };
        match outcome {
            Outcome::Wait => return,
            Outcome::Asked(at) => {
                run.asked_ms = Some(at);
                return;
            }
            Outcome::Waiting => {
                run.status = RunStatus::Waiting;
                return;
            }
            Outcome::Fail(error) => {
                run.results.push(StepResult { ok: false, text: error, ms: now });
                run.status = RunStatus::Failed;
                run.ended_ms = Some(now);
                return;
            }
            Outcome::Next(result) => {
                if run.step >= steps {
                    run.status = RunStatus::Done;
                    run.ended_ms = Some(now);
                    return;
                }
                run.results.push(result);
                run.asked_ms = None;
                run.step += 1;
            }
        }
    }
}

/// One beat of the clock: triggers first, then every run that is going.
pub fn tick(state_dir: &Path, jobs: &Jobs) {
    let now = Local::now();
    let now_ms = now.timestamp_millis();
    let oldest = workflows::list(state_dir).iter().filter(|f| f.enabled).map(|f| f.checked_ms).min();
    let Some(oldest) = oldest.or_else(|| workflows::list(state_dir).iter().any(|f| f.run.is_some()).then_some(now_ms)) else { return };
    let finished = tasks::finished_since(state_dir, oldest);
    let _ = workflows::change(state_dir, |flows| {
        for flow in flows.iter_mut() {
            if flow.enabled {
                if let Some(cause) = fired(flow, &finished, now) {
                    // Already running: this one is skipped rather than queued.
                    let _ = workflows::start(flow, &cause, now_ms);
                }
                flow.checked_ms = now_ms;
            }
            advance(state_dir, jobs, flow, now_ms);
            if let Some(run) = flow.run.take_if(|run| matches!(run.status, RunStatus::Done | RunStatus::Failed | RunStatus::Stopped)) {
                workflows::keep(flow, run);
            }
        }
        Ok(())
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn flow(trigger: Trigger, checked_ms: i64) -> Flow {
        Flow { id: "f".into(), name: "x".into(), enabled: true, trigger: Some(trigger), steps: Vec::new(), checked_ms, threads: BTreeMap::new(), run: None, runs: Vec::new() }
    }

    #[test]
    fn a_time_fires_once_after_it_passes_on_its_days() {
        let monday_10 = Local.with_ymd_and_hms(2026, 11, 16, 10, 0, 0).single().unwrap();
        let schedule = Trigger::Schedule { days: vec![0], hour: 9, minute: 30 };
        let before = monday_10.timestamp_millis() - 3 * 60 * 60 * 1000;
        assert_eq!(fired(&flow(schedule.clone(), before), &[], monday_10).as_deref(), Some("09:30"));
        assert!(fired(&flow(schedule, monday_10.timestamp_millis()), &[], monday_10).is_none(), "already looked at after 9:30");
        let tuesday = Trigger::Schedule { days: vec![1], hour: 9, minute: 30 };
        assert!(fired(&flow(tuesday, before), &[], monday_10).is_none());
    }

    #[test]
    fn a_finished_task_fires_only_when_new_and_of_the_right_ai() {
        let now = Local.with_ymd_and_hms(2026, 11, 16, 10, 0, 0).single().unwrap();
        let done = vec![("b1".to_string(), "old".to_string(), 50), ("b2".to_string(), "other".to_string(), 150), ("b1".to_string(), "new".to_string(), 200)];
        assert_eq!(fired(&flow(Trigger::TaskDone { bot: Some("b1".into()) }, 100), &done, now).as_deref(), Some("new"));
        assert_eq!(fired(&flow(Trigger::TaskDone { bot: None }, 100), &done, now).as_deref(), Some("other"));
        assert!(fired(&flow(Trigger::TaskDone { bot: None }, 300), &done, now).is_none());
        assert!(fired(&flow(Trigger::Manual, 0), &done, now).is_none());
    }

    #[test]
    fn a_step_reads_the_previous_answer_and_the_trigger() {
        assert_eq!(fill("Revise: {anterior} (de {gatilho})", "o diff", "deploy"), "Revise: o diff (de deploy)");
        assert_eq!(fill("{previous}", &"a".repeat(bots::MAX_TEXT + 10), "").chars().count(), bots::MAX_TEXT);
    }

    #[test]
    fn approvals_wait_and_a_run_with_no_steps_left_is_done() {
        let dir = std::env::temp_dir().join(format!("raiox-fluxo-roda-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let jobs = Jobs::default();
        let mut f = flow(Trigger::Manual, 0);
        f.steps = vec![Step::Approve { text: String::new() }];
        workflows::start(&mut f, "", 1).unwrap();
        advance(&dir, &jobs, &mut f, 2);
        assert_eq!(f.run.as_ref().unwrap().status, RunStatus::Waiting);
        let run = f.run.as_mut().unwrap();
        run.status = RunStatus::Running;
        run.step = 1;
        advance(&dir, &jobs, &mut f, 3);
        assert_eq!(f.run.as_ref().unwrap().status, RunStatus::Done);
    }
}
