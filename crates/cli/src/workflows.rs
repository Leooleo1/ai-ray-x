//! Workflows: a trigger (by hand, a time, or a Desk task that finished) and steps in a
//! straight line (ask an AI, wait for the person's approval, post in a channel). Each AI
//! answers in its own "Workflow · name" conversation, and one step's answer can feed the next
//! (`{previous}`). Runs advance on the routines' clock, only while `raiox open` runs.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{bots, channels, room};

const FILE: &str = "fluxos.json";
const MAX_FLOWS: usize = 30;
const MAX_STEPS: usize = 12;
const MAX_NAME: usize = 60;
/// Finished runs kept per workflow, newest first.
pub(crate) const KEPT_RUNS: usize = 5;

/// One writer at a time: the panel's requests and the clock both change the file.
static LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Trigger {
    /// Only when the person presses Run.
    Manual,
    /// On these days (0 = Monday) at this local time.
    Schedule { days: Vec<u8>, hour: u8, minute: u8 },
    /// When a Desk task finishes; of one bot, or of any.
    TaskDone {
        #[serde(default)]
        bot: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Step {
    Ask { bot: String, text: String },
    Approve { text: String },
    Post { channel: String, text: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Running,
    Waiting,
    Done,
    Failed,
    Stopped,
}

/// What one step left: its text (an AI's answer, a note) and whether it went well.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepResult {
    pub ok: bool,
    pub text: String,
    pub ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Run {
    pub started_ms: i64,
    /// What set it off ("by hand", the time, the finished task's title).
    pub cause: String,
    pub step: usize,
    pub status: RunStatus,
    /// When the current step's question went out; none while it waits to be sent.
    #[serde(default)]
    pub asked_ms: Option<i64>,
    #[serde(default)]
    pub results: Vec<StepResult>,
    #[serde(default)]
    pub ended_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Flow {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    #[serde(default)]
    pub trigger: Option<Trigger>,
    #[serde(default)]
    pub steps: Vec<Step>,
    /// Until when its trigger was looked at: a time already run, tasks already seen.
    #[serde(default)]
    pub checked_ms: i64,
    /// Each AI's "Workflow · name" conversation, made on its first question.
    #[serde(default)]
    pub threads: BTreeMap<String, String>,
    #[serde(default)]
    pub run: Option<Run>,
    #[serde(default)]
    pub runs: Vec<Run>,
}

fn file(state_dir: &Path) -> PathBuf {
    state_dir.join(FILE)
}

pub fn list(state_dir: &Path) -> Vec<Flow> {
    fs::read_to_string(file(state_dir)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn save(state_dir: &Path, flows: &[Flow]) -> Result<()> {
    fs::create_dir_all(state_dir)?;
    let temp = file(state_dir).with_extension("tmp");
    fs::write(&temp, serde_json::to_string_pretty(flows)? + "\n")?;
    fs::rename(&temp, file(state_dir))?;
    Ok(())
}

/// Reads, changes and writes the workflows under the lock; gives what `edit` returned.
pub(crate) fn change<T>(state_dir: &Path, edit: impl FnOnce(&mut Vec<Flow>) -> Result<T>) -> Result<T> {
    let _held = LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut flows = list(state_dir);
    let out = edit(&mut flows)?;
    save(state_dir, &flows)?;
    Ok(out)
}

fn text_of(input: &Value, key: &str) -> String {
    input.get(key).and_then(Value::as_str).unwrap_or("").trim().to_string()
}

fn checked_text(input: &Value, what: &str) -> Result<String> {
    let text = text_of(input, "text");
    if text.is_empty() || text.chars().count() > bots::MAX_TEXT {
        bail!("{what} needs 1 to {} characters", bots::MAX_TEXT);
    }
    Ok(text)
}

fn trigger_from(state_dir: &Path, input: &Value) -> Result<Option<Trigger>> {
    if input.is_null() {
        return Ok(None);
    }
    Ok(Some(match input.get("kind").and_then(Value::as_str) {
        Some("manual") => Trigger::Manual,
        Some("schedule") => {
            let number = |key: &str| input.get(key).and_then(Value::as_u64).context("invalid time");
            let (hour, minute) = (number("hour")?, number("minute")?);
            let mut days: Vec<u8> = input.get("days").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_u64).filter(|d| *d <= 6).map(|d| d as u8).collect();
            days.sort_unstable();
            days.dedup();
            if days.is_empty() || hour > 23 || minute > 59 {
                bail!("pick at least one day and a valid time");
            }
            Trigger::Schedule { days, hour: hour as u8, minute: minute as u8 }
        }
        Some("task_done") => {
            let bot = input.get("bot").and_then(Value::as_str).filter(|b| !b.is_empty()).map(str::to_string);
            if let Some(id) = &bot {
                bots::find(state_dir, id)?;
            }
            Trigger::TaskDone { bot }
        }
        _ => bail!("unknown trigger"),
    }))
}

fn step_from(state_dir: &Path, input: &Value) -> Result<Step> {
    Ok(match input.get("kind").and_then(Value::as_str) {
        Some("ask") => {
            let bot = text_of(input, "bot");
            bots::find(state_dir, &bot).context("pick which AI answers")?;
            Step::Ask { bot, text: checked_text(input, "a question")? }
        }
        Some("approve") => {
            let text = text_of(input, "text");
            Step::Approve { text: text.chars().take(bots::MAX_TEXT).collect() }
        }
        Some("post") => {
            let channel = text_of(input, "channel");
            if !channels::list(state_dir).iter().any(|c| c.id == channel) {
                bail!("pick a channel");
            }
            Step::Post { channel, text: checked_text(input, "a post")? }
        }
        _ => bail!("unknown step"),
    })
}

/// Creates or updates a workflow from the panel (its run and history stay); gives its id.
pub fn save_from(state_dir: &Path, input: &Value) -> Result<String> {
    let name = text_of(input, "name");
    if name.is_empty() || name.chars().count() > MAX_NAME {
        bail!("a workflow needs a name of 1 to {MAX_NAME} characters");
    }
    let trigger = trigger_from(state_dir, input.get("trigger").unwrap_or(&Value::Null))?;
    let raw_steps = input.get("steps").and_then(Value::as_array).cloned().unwrap_or_default();
    if raw_steps.len() > MAX_STEPS {
        bail!("a workflow has at most {MAX_STEPS} steps");
    }
    let steps = raw_steps.iter().map(|step| step_from(state_dir, step)).collect::<Result<Vec<_>>>()?;
    let enabled = input.get("enabled").and_then(Value::as_bool).unwrap_or(false);
    let id = text_of(input, "id");
    let now = room::now_ms();
    let saved = change(state_dir, |flows| {
        if let Some(flow) = flows.iter_mut().find(|f| !id.is_empty() && f.id == id) {
            if flow.run.as_ref().is_some_and(|run| matches!(run.status, RunStatus::Running | RunStatus::Waiting)) && flow.steps != steps {
                bail!("stop the run before changing the steps");
            }
            // A new trigger starts from now: no burst of old tasks or of today's time already past.
            if flow.trigger != trigger || (!flow.enabled && enabled) {
                flow.checked_ms = now;
            }
            *flow = Flow { name, enabled, trigger, steps, ..flow.clone() };
            return Ok(flow.id.clone());
        }
        if flows.len() >= MAX_FLOWS {
            bail!("at most {MAX_FLOWS} workflows");
        }
        let id = format!("f{now:x}");
        flows.push(Flow { id: id.clone(), name, enabled, trigger, steps, checked_ms: now, threads: BTreeMap::new(), run: None, runs: Vec::new() });
        Ok(id)
    })?;
    crate::progress::record(state_dir, &[crate::progress::Milestone::Flow]);
    Ok(saved)
}

pub fn delete(state_dir: &Path, id: &str) -> Result<()> {
    change(state_dir, |flows| {
        let before = flows.len();
        flows.retain(|f| f.id != id);
        if flows.len() == before {
            bail!("no such workflow");
        }
        Ok(())
    })
}

/// Starts a run now (Run in the panel, or its trigger); refused while one is going.
pub(crate) fn start(flow: &mut Flow, cause: &str, now: i64) -> Result<()> {
    if flow.run.as_ref().is_some_and(|run| matches!(run.status, RunStatus::Running | RunStatus::Waiting)) {
        bail!("this workflow is already running");
    }
    if flow.steps.is_empty() {
        bail!("add a step first");
    }
    if let Some(done) = flow.run.take() {
        keep(flow, done);
    }
    flow.run = Some(Run { started_ms: now, cause: cause.to_string(), step: 0, status: RunStatus::Running, asked_ms: None, results: Vec::new(), ended_ms: None });
    Ok(())
}

/// A finished run joins the history, newest first.
pub(crate) fn keep(flow: &mut Flow, run: Run) {
    flow.runs.insert(0, run);
    flow.runs.truncate(KEPT_RUNS);
}

pub fn run_now(state_dir: &Path, id: &str) -> Result<()> {
    change(state_dir, |flows| {
        let flow = flows.iter_mut().find(|f| f.id == id).context("no such workflow")?;
        start(flow, "", room::now_ms())
    })
}

/// The person answers an approval step: go on, or stop the run there.
pub fn answer(state_dir: &Path, id: &str, approve: bool) -> Result<()> {
    change(state_dir, |flows| {
        let flow = flows.iter_mut().find(|f| f.id == id).context("no such workflow")?;
        let run = flow.run.as_mut().filter(|run| run.status == RunStatus::Waiting).context("nothing waits for you here")?;
        let now = room::now_ms();
        // An approval passes the answer before it on, so the next step's {previous} still has it.
        let previous = run.results.last().map(|r| r.text.clone()).unwrap_or_default();
        run.results.push(StepResult { ok: approve, text: previous, ms: now });
        if approve {
            run.step += 1;
            run.status = RunStatus::Running;
        } else {
            run.status = RunStatus::Stopped;
            run.ended_ms = Some(now);
        }
        Ok(())
    })
}

/// Stops the run where it is (an AI already answering finishes its turn).
pub fn stop(state_dir: &Path, id: &str) -> Result<()> {
    change(state_dir, |flows| {
        let flow = flows.iter_mut().find(|f| f.id == id).context("no such workflow")?;
        let run = flow.run.as_mut().filter(|run| matches!(run.status, RunStatus::Running | RunStatus::Waiting)).context("it is not running")?;
        run.status = RunStatus::Stopped;
        run.ended_ms = Some(room::now_ms());
        Ok(())
    })
}

pub fn state(state_dir: &Path) -> Value {
    serde_json::json!({ "flows": list(state_dir) })
}

/// The panel's workflow actions: save, delete, run, approve, decline, stop.
pub fn act(state_dir: &Path, action: &str, input: &Value) -> Result<Value> {
    let id = text_of(input, "id");
    match action {
        "save" => return Ok(serde_json::json!({ "id": save_from(state_dir, input)? })),
        "delete" => delete(state_dir, &id)?,
        "run" => run_now(state_dir, &id)?,
        "approve" => answer(state_dir, &id, true)?,
        "decline" => answer(state_dir, &id, false)?,
        "stop" => stop(state_dir, &id)?,
        _ => bail!("unknown workflow action"),
    }
    Ok(state(state_dir))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("raiox-fluxos-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_workflow_needs_a_name_and_known_steps() {
        let dir = temp("valida");
        assert!(save_from(&dir, &json!({ "name": "" })).is_err());
        assert!(save_from(&dir, &json!({ "name": "x", "steps": [{ "kind": "ask", "bot": "nobody", "text": "oi" }] })).is_err(), "an AI that does not exist");
        assert!(save_from(&dir, &json!({ "name": "x", "trigger": { "kind": "schedule", "days": [], "hour": 9, "minute": 0 } })).is_err(), "a time with no day");
        let id = save_from(&dir, &json!({ "name": "Revisão", "trigger": { "kind": "manual" }, "steps": [{ "kind": "approve", "text": "ok?" }] })).unwrap();
        assert_eq!(list(&dir)[0].id, id);
    }

    #[test]
    fn an_approval_step_waits_for_the_person_then_goes_on_or_stops() {
        let dir = temp("aprova");
        let id = save_from(&dir, &json!({ "name": "x", "steps": [{ "kind": "approve", "text": "" }, { "kind": "approve", "text": "" }] })).unwrap();
        change(&dir, |flows| {
            start(&mut flows[0], "", 1)?;
            let run = flows[0].run.as_mut().unwrap();
            run.results.push(StepResult { ok: true, text: "4".into(), ms: 1 });
            run.status = RunStatus::Waiting;
            Ok(())
        })
        .unwrap();
        answer(&dir, &id, true).unwrap();
        let run = list(&dir)[0].run.clone().unwrap();
        assert_eq!((run.step, run.status), (1, RunStatus::Running));
        assert_eq!(run.results.last().map(|r| r.text.as_str()), Some("4"), "the answer before the approval goes on to the next step");
        assert!(answer(&dir, &id, true).is_err(), "nothing waits while it runs");
        stop(&dir, &id).unwrap();
        assert_eq!(list(&dir)[0].run.as_ref().unwrap().status, RunStatus::Stopped);
        run_now(&dir, &id).unwrap();
        let flow = &list(&dir)[0];
        assert_eq!((flow.runs.len(), flow.run.as_ref().unwrap().status), (1, RunStatus::Running), "the stopped run went to the history");
    }
}
