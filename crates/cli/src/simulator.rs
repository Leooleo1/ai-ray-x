//! The iOS simulator, through Xcode's `xcrun simctl`: whether a device is
//! booted, and a screenshot of it for the panel and the AI.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use serde_json::Value;

/// The panel asks every few seconds; the simulator's state is read again at most this often.
const FRESH: Duration = Duration::from_secs(20);

static BOOTED: Mutex<Option<(Instant, Option<String>)>> = Mutex::new(None);

fn read_booted() -> Option<String> {
    let output = Command::new("xcrun").args(["simctl", "list", "devices", "booted", "-j"]).stderr(Stdio::null()).output().ok()?;
    let listed: Value = serde_json::from_slice(&output.stdout).ok()?;
    listed.get("devices")?.as_object()?.values().flat_map(|devices| devices.as_array().cloned().unwrap_or_default()).find_map(|device| device.get("name")?.as_str().map(str::to_string))
}

/// The name of a booted simulator device, if any.
pub fn booted() -> Option<String> {
    let mut cached = BOOTED.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some((at, name)) = cached.as_ref()
        && at.elapsed() < FRESH
    {
        return name.clone();
    }
    let name = read_booted();
    *cached = Some((Instant::now(), name.clone()));
    name
}

/// A screenshot of the booted simulator, saved with the bot's other screenshots.
pub fn shot(state_dir: &Path, bot: &str) -> Result<PathBuf> {
    if read_booted().is_none() {
        bail!("no iOS simulator is running");
    }
    let file = crate::preview::shots_dir(state_dir, bot).join(format!("{:x}.png", crate::room::now_ms()));
    std::fs::create_dir_all(file.parent().context("no screenshot folder")?)?;
    let output = Command::new("xcrun").args(["simctl", "io", "booted", "screenshot"]).arg(&file).output().context("xcrun is not available")?;
    if !output.status.success() || !file.exists() {
        bail!("the simulator made no screenshot: {}", String::from_utf8_lossy(&output.stderr).lines().last().unwrap_or(""));
    }
    Ok(file)
}
