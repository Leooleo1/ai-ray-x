//! Starting and stopping programs on macOS, Linux and Windows alike, and the
//! child processes raiox keeps (dev servers, the tunnel): each leads its own
//! process group, is recorded with its start time, and stops at the next
//! startup if raiox closed without stopping it.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::Command;

const FILE: &str = "children.json";

/// Windows: no console window for programs started from the app.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
#[cfg(windows)]
const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;

/// A program, found the way a terminal finds it. On Windows that means the PATH with its
/// extensions (npm installs CLIs as `.cmd` files), and no console window flashing up.
pub fn command(program: &str) -> Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // Cursor's CLI is also `agent` (its Windows installer may give only that name).
        let alias = match program {
            "cursor-agent" => Some("agent"),
            _ => None,
        };
        let found = which::which(program)
            .or_else(|error| alias.map_or(Err(error), which::which))
            .map(std::path::PathBuf::into_os_string)
            .unwrap_or_else(|_| program.into());
        let mut command = Command::new(found);
        command.creation_flags(CREATE_NO_WINDOW);
        command
    }
    #[cfg(not(windows))]
    Command::new(program)
}

/// Makes the child lead a group of its own, so stopping it stops what it started too.
pub fn own_group(command: &mut Command) {
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(command, 0);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP);
    }
}

/// When a process started: with the pid, it tells a child from a later process given the same pid.
#[cfg(not(windows))]
fn started_at(pid: u32) -> String {
    Command::new("ps").args(["-o", "lstart=", "-p", &pid.to_string()]).output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default()
}

#[cfg(windows)]
fn started_at(pid: u32) -> String {
    use windows_sys::Win32::Foundation::{CloseHandle, FILETIME};
    use windows_sys::Win32::System::Threading::{GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    let time = || FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
    let (mut created, mut exited, mut kernel, mut user) = (time(), time(), time(), time());
    // SAFETY: the handle is checked before use and closed after; the four times are ours to fill.
    let found = unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return String::new();
        }
        let found = GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user);
        CloseHandle(handle);
        found
    };
    if found == 0 {
        return String::new();
    }
    ((u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime)).to_string()
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

/// Stops a child's whole process group (a dev server's own children included); `now` skips
/// the polite request to end.
pub fn kill_group(pid: u32, now: bool) {
    #[cfg(unix)]
    // SAFETY: kill(2) on the process group the child leads (set with process_group(0)).
    unsafe {
        libc::kill(-(pid as i32), if now { libc::SIGKILL } else { libc::SIGTERM });
    }
    // Windows has no process groups to signal: taskkill ends the process and its tree
    // (console programs ignore a polite request, so it is always forced).
    #[cfg(windows)]
    {
        let _ = now;
        let _ = command("taskkill").args(["/PID", &pid.to_string(), "/T", "/F"]).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status();
    }
}

const PANEL_LOCK: &str = "panel.lock";

/// Marks this process as the panel of a state folder, unless another live raiox already is
/// (the terminal's `raiox open` and the desktop app must not serve the same data at once).
pub fn claim_panel(state_dir: &Path) -> anyhow::Result<()> {
    let lock = state_dir.join(PANEL_LOCK);
    let held: Option<(u32, String)> = fs::read_to_string(&lock).ok().and_then(|text| serde_json::from_str(&text).ok());
    if let Some((pid, started)) = held
        && pid != std::process::id()
        && !started.is_empty()
        && started_at(pid) == started
    {
        anyhow::bail!("raiox is already open (process {pid}); close it first");
    }
    fs::create_dir_all(state_dir)?;
    let me = std::process::id();
    fs::write(&lock, serde_json::to_string(&(me, started_at(me)))?)?;
    Ok(())
}

/// At startup: children an earlier raiox left running stop, when the process is still theirs.
pub fn cleanup(state_dir: &Path) {
    for (pid, started) in load(state_dir).into_values() {
        // Only the same process (same pid, same start time): a pid can be reused by another program.
        if !started.is_empty() && started_at(pid) == started {
            kill_group(pid, false);
        }
    }
    save(state_dir, &HashMap::new());
}

/// Where the official installers put the AI CLIs (and where Homebrew and npm put theirs).
#[cfg(unix)]
const CLI_DIRS: [&str; 7] = [".local/bin", ".opencode/bin", ".npm-global/bin", ".bun/bin", ".cargo/bin", "/opt/homebrew/bin", "/usr/local/bin"];
/// How long the login shell may take to say its PATH.
#[cfg(unix)]
const SHELL_WAIT: std::time::Duration = std::time::Duration::from_secs(4);

/// The PATH the person's terminal has, read from their login shell (its startup files set it).
#[cfg(unix)]
fn login_shell_path() -> Option<String> {
    use std::io::Read;
    let shell = std::env::var("SHELL").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| "/bin/zsh".into());
    let mut child = Command::new(shell)
        .args(["-ilc", "printf '\\n__RAIOX_PATH__%s__END__\\n' \"$PATH\""])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    let started = std::time::Instant::now();
    while child.try_wait().ok()?.is_none() {
        if started.elapsed() > SHELL_WAIT {
            let _ = child.kill();
            return None;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let mut out = String::new();
    child.stdout.take()?.read_to_string(&mut out).ok()?;
    let (_, rest) = out.split_once("__RAIOX_PATH__")?;
    rest.split_once("__END__").map(|(path, _)| path.to_string()).filter(|path| !path.is_empty())
}

/// A PATH with every entry once, in order.
#[cfg(unix)]
fn joined(parts: impl IntoIterator<Item = String>) -> String {
    let mut seen: Vec<String> = Vec::new();
    for part in parts.into_iter().flat_map(|p| p.split(':').map(str::to_string).collect::<Vec<_>>()) {
        if !part.is_empty() && !seen.contains(&part) {
            seen.push(part);
        }
    }
    seen.join(":")
}

/// An app opened from the Finder or the Dock gets a bare PATH (`/usr/bin:/bin`), where none of
/// the AI CLIs are. This gives raiox the terminal's PATH, plus the folders the official
/// installers use, so a CLI installed while raiox runs is found at once.
/// Call it first thing in `main`, before any thread starts.
pub fn find_clis_like_a_terminal() {
    #[cfg(unix)]
    {
        let home = std::env::var("HOME").unwrap_or_default();
        let known = CLI_DIRS.iter().map(|dir| if dir.starts_with('/') { (*dir).to_string() } else { format!("{home}/{dir}") });
        let current = std::env::var("PATH").unwrap_or_default();
        let path = joined(login_shell_path().into_iter().chain(std::iter::once(current)).chain(known));
        // SAFETY: called at the start of main, before raiox starts any thread.
        unsafe { std::env::set_var("PATH", path) };
    }
}

#[cfg(all(test, unix))]
mod path_tests {
    use super::*;

    #[test]
    fn a_path_keeps_each_folder_once_in_order() {
        assert_eq!(joined(["/a:/b".to_string(), "/b:/c".to_string(), "".to_string(), "/a".to_string()]), "/a:/b:/c");
    }
}
