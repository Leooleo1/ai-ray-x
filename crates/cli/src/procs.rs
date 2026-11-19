//! Child processes raiox starts and keeps (dev servers, the tunnel): each leads
//! its own process group, is recorded with its start time, and stops at the
//! next startup if raiox closed without stopping it.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::Command;

const FILE: &str = "children.json";

/// When a process started, as `ps` shows it: with the pid, it tells a child from a later process given the same pid.
fn started_at(pid: u32) -> String {
    Command::new("ps").args(["-o", "lstart=", "-p", &pid.to_string()]).output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default()
}

fn load(state_dir: &Path) -> HashMap<String, (u32, String)> {
    fs::read_to_string(state_dir.join(FILE)).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
}

fn save(state_dir: &Path, map: &HashMap<String, (u32, String)>) {
    let _ = fs::write(state_dir.join(FILE), serde_json::to_string(map).unwrap_or_default());
}

pub fn remember(state_dir: &Path, key: &str, pid: u32) {
    let mut map = load(state_dir);
    map.insert(key.to_string(), (pid, started_at(pid)));
    save(state_dir, &map);
}

pub fn forget(state_dir: &Path, key: &str) {
    let mut map = load(state_dir);
    map.remove(key);
    save(state_dir, &map);
}

/// Stops a child's whole process group (a dev server's own children included).
pub fn kill_group(pid: u32) {
    #[cfg(unix)]
    // SAFETY: kill(2) on the process group the child leads (set with process_group(0)).
    unsafe {
        libc::kill(-(pid as i32), libc::SIGTERM);
    }
}

/// At startup: children an earlier raiox left running stop, when the process is still theirs.
pub fn cleanup(state_dir: &Path) {
    for (pid, started) in load(state_dir).into_values() {
        // Only the same process (same pid, same start time): a pid can be reused by another program.
        if !started.is_empty() && started_at(pid) == started {
            kill_group(pid);
        }
    }
    save(state_dir, &HashMap::new());
}
