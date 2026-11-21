//! Locally initiated PTY sessions. Output is bounded and remains in memory.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::sync::mpsc::{SyncSender, sync_channel};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};
use base64::Engine;
use portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system};
use serde_json::{Value, json};

const OUTPUT_LIMIT: usize = 256 * 1024;
const CHUNK_LIMIT: usize = 48 * 1024;
const IDLE_LIMIT: Duration = Duration::from_secs(30 * 60);
const SESSION_LIMIT: usize = 8;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Kind {
    Shell,
    Test,
}

struct Output {
    bytes: VecDeque<u8>,
    end: u64,
    code: Option<u32>,
    stopping: bool,
    touched: Instant,
}

impl Output {
    fn push(&mut self, bytes: &[u8]) {
        self.end += bytes.len() as u64;
        self.bytes.extend(bytes);
        let excess = self.bytes.len().saturating_sub(OUTPUT_LIMIT);
        self.bytes.drain(..excess);
    }

    fn snapshot(&self, cursor: u64) -> (Vec<u8>, u64, bool) {
        let first = self.end - self.bytes.len() as u64;
        let reset = cursor < first || cursor > self.end;
        let from = if reset { first } else { cursor };
        let bytes: Vec<u8> = self.bytes.iter().skip((from - first) as usize).take(CHUNK_LIMIT).copied().collect();
        let next = from + bytes.len() as u64;
        (bytes, next, reset)
    }
}

struct Control {
    master: Box<dyn MasterPty + Send>,
    input: SyncSender<Vec<u8>>,
    killer: Box<dyn ChildKiller + Send + Sync>,
    pid: Option<u32>,
}

struct Session {
    id: u64,
    command: Option<String>,
    output: Mutex<Output>,
    control: Mutex<Option<Control>>,
}

type Sessions = HashMap<(PathBuf, Kind), Arc<Session>>;
static SESSIONS: OnceLock<Mutex<Sessions>> = OnceLock::new();
static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn size(input: &Value) -> PtySize {
    let bound = |key: &str, default: u64, max: u64| input[key].as_u64().unwrap_or(default).clamp(2, max) as u16;
    PtySize { rows: bound("rows", 24, 150), cols: bound("cols", 80, 300), pixel_width: 0, pixel_height: 0 }
}

fn shell(root: &Path, command: Option<&str>) -> CommandBuilder {
    let mut shell = if cfg!(windows) {
        let mut shell = CommandBuilder::new("powershell.exe");
        shell.args(["-NoLogo", "-NoProfile"]);
        if let Some(command) = command {
            let script = format!("$ProgressPreference = 'SilentlyContinue'\n$global:LASTEXITCODE = 0\n& {{\n{command}\n}}\n$success = $?\nif ($LASTEXITCODE) {{ exit $LASTEXITCODE }}\nif ($success) {{ exit 0 }}\nexit 1");
            let encoded = base64::engine::general_purpose::STANDARD.encode(script.encode_utf16().flat_map(u16::to_le_bytes).collect::<Vec<_>>());
            shell.args(["-OutputFormat", "Text", "-EncodedCommand", &encoded]);
        }
        shell
    } else {
        let program = std::env::var_os("SHELL").filter(|value| !value.is_empty()).unwrap_or_else(|| "/bin/sh".into());
        let mut shell = CommandBuilder::new(program);
        if let Some(command) = command {
            shell.args(["-c", command]);
        } else {
            shell.arg("-i");
        }
        shell
    };
    shell.cwd(root);
    shell.env("TERM", "xterm-256color");
    shell
}

fn stop(session: &Session) -> Result<()> {
    let mut output = session.output.lock().unwrap();
    if output.code.is_some() || output.stopping {
        return Ok(());
    }
    output.stopping = true;
    drop(output);
    let mut guard = session.control.lock().unwrap();
    if let Some(control) = guard.as_mut() {
        #[cfg(unix)]
        if let Some(foreground) = control.master.process_group_leader().map(|pid| pid as u32)
            && Some(foreground) != control.pid
        {
            crate::procs::kill_group(foreground, true);
        }
        if let Some(pid) = control.pid {
            crate::procs::kill_group(pid, true);
        } else if let Err(error) = control.killer.kill() {
            session.output.lock().unwrap().stopping = false;
            return Err(error.into());
        }
    }
    Ok(())
}

fn start(root: &Path, command: Option<String>, dimensions: PtySize) -> Result<Arc<Session>> {
    let pty = native_pty_system().openpty(dimensions).context("could not open the terminal")?;
    let mut reader = pty.master.try_clone_reader()?;
    let mut writer = pty.master.take_writer()?;
    let mut child = pty.slave.spawn_command(shell(root, command.as_deref())).context("could not start the shell")?;
    drop(pty.slave);
    let (input, received) = sync_channel::<Vec<u8>>(32);
    #[cfg(windows)]
    let (cursor_reply, cursor_query) = std::sync::mpsc::channel();
    let session = Arc::new(Session {
        id: NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        command,
        output: Mutex::new(Output { bytes: VecDeque::new(), end: 0, code: None, stopping: false, touched: Instant::now() }),
        control: Mutex::new(Some(Control { master: pty.master, input, killer: child.clone_killer(), pid: child.process_id() })),
    });
    std::thread::spawn(move || {
        #[cfg(windows)]
        if cursor_query.recv_timeout(Duration::from_secs(3)).unwrap_or(false)
            && writer.write_all(b"\x1b[1;1R").and_then(|()| writer.flush()).is_err()
        {
            return;
        }
        while let Ok(bytes) = received.recv() {
            if writer.write_all(&bytes).and_then(|()| writer.flush()).is_err() {
                break;
            }
        }
    });
    let for_reader = session.clone();
    let read_thread = std::thread::spawn(move || {
        // ConPTY asks the terminal for its starting cursor before PowerShell can run.
        // Answer before the browser connects, including for tests launched without an interactive view.
        #[cfg(windows)]
        {
            let mut prefix = [0_u8; 4];
            let mut count = 0;
            while count < prefix.len() {
                match reader.read(&mut prefix[count..]) {
                    Ok(0) | Err(_) => break,
                    Ok(read) => count += read,
                }
                if !b"\x1b[6n".starts_with(&prefix[..count]) {
                    break;
                }
            }
            let asked = &prefix[..count] == b"\x1b[6n";
            let _ = cursor_reply.send(asked);
            if !asked {
                for_reader.output.lock().unwrap().push(&prefix[..count]);
            }
        }
        let mut buffer = [0_u8; 8192];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => for_reader.output.lock().unwrap().push(&buffer[..count]),
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => break,
            }
        }
    });
    let owner = session.clone();
    std::thread::spawn(move || {
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status.exit_code(),
                Err(_) => break 1,
                Ok(None) => {}
            }
            let idle = owner.output.lock().unwrap().touched.elapsed() >= IDLE_LIMIT;
            if idle {
                let _ = stop(&owner);
            }
            std::thread::sleep(Duration::from_millis(150));
        };
        // Closing the master after exit also releases the Windows ConPTY output pipe.
        let control = owner.control.lock().unwrap().take();
        drop(control);
        // A detached process may still own the tty on Unix. Keep status available even then.
        for _ in 0..10 {
            if read_thread.is_finished() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        owner.output.lock().unwrap().code = Some(status);
    });
    Ok(session)
}

fn test_commands(root: &Path) -> Vec<&'static str> {
    let mut commands = Vec::new();
    if std::fs::metadata(root.join("package.json")).is_ok_and(|file| file.len() <= 128 * 1024)
        && std::fs::read(root.join("package.json")).ok().and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .is_some_and(|manifest| manifest["scripts"]["test"].is_string())
    {
        commands.push(if root.join("pnpm-lock.yaml").is_file() { "pnpm test" }
            else if root.join("yarn.lock").is_file() { "yarn test" }
            else if root.join("bun.lock").is_file() || root.join("bun.lockb").is_file() { "bun test" }
            else { "npm test" });
    }
    if root.join("Cargo.toml").is_file() { commands.push("cargo test"); }
    if root.join("go.mod").is_file() { commands.push("go test ./..."); }
    if root.join("pyproject.toml").is_file() || root.join("pytest.ini").is_file() { commands.push("python -m pytest"); }
    commands
}

pub(crate) fn request(root: &Path, input: &Value) -> Result<Value> {
    let kind = match input["kind"].as_str() {
        Some("shell") => Kind::Shell,
        Some("test") => Kind::Test,
        _ => bail!("unknown terminal type"),
    };
    let root = root.canonicalize()?;
    let action = input["action"].as_str().unwrap_or("");
    if action == "commands" {
        return Ok(json!({ "commands": test_commands(&root) }));
    }
    let sessions = SESSIONS.get_or_init(Default::default);
    let key = (root.clone(), kind);
    let session = {
        let mut map = sessions.lock().unwrap();
        let existing = map.get(&key).cloned();
        if action != "start" {
            existing
        } else if existing.as_ref().is_some_and(|run| run.output.lock().unwrap().code.is_none()) {
            ensure!(kind == Kind::Shell, "a test command is already running");
            existing
        } else {
            let command = if kind == Kind::Test {
                let command = input["command"].as_str().unwrap_or("").trim();
                ensure!(!command.is_empty() && command.len() <= 4096 && !command.contains('\0'), "invalid test command");
                Some(command.to_string())
            } else {
                None
            };
            if map.len() >= SESSION_LIMIT && !map.contains_key(&key) {
                let finished = map.iter().filter_map(|(key, run)| {
                    let output = run.output.lock().unwrap();
                    output.code.is_some().then(|| (key.clone(), output.touched))
                }).min_by_key(|(_, touched)| *touched);
                let Some((oldest, _)) = finished else { bail!("too many running terminals") };
                map.remove(&oldest);
            }
            let session = start(&root, command, size(input))?;
            map.insert(key, session.clone());
            Some(session)
        }
    };
    let Some(session) = session else { return Ok(Value::Null) };
    let expected = input["session"].as_u64();
    match action {
        "poll" | "start" => {}
        "stop" | "input" | "resize" if expected != Some(session.id) => bail!("the terminal session changed"),
        "stop" => stop(&session)?,
        "input" if kind == Kind::Shell => {
            let bytes = input["data"].as_str().unwrap_or("").as_bytes();
            ensure!(bytes.len() <= 32 * 1024, "terminal input is too large");
            let guard = session.control.lock().unwrap();
            let control = guard.as_ref().context("terminal closed")?;
            control.input.try_send(bytes.to_vec()).context("terminal input is unavailable")?;
        }
        "resize" => {
            if let Some(control) = session.control.lock().unwrap().as_ref() {
                control.master.resize(size(input))?;
            }
        }
        _ => bail!("unknown terminal action"),
    }
    let mut output = session.output.lock().unwrap();
    output.touched = Instant::now();
    if !matches!(action, "poll" | "start") {
        return Ok(Value::Null);
    }
    let changed = expected.is_some_and(|id| id != session.id);
    let cursor = if changed { 0 } else { input["cursor"].as_u64().unwrap_or(0) };
    let (bytes, cursor, reset) = output.snapshot(cursor);
    Ok(json!({
        "session": session.id, "cursor": cursor, "reset": changed || reset,
        "more": cursor < output.end, "code": output.code, "stopped": output.stopping,
        "command": session.command, "bytes": base64::engine::general_purpose::STANDARD.encode(bytes),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_cursors_reset_and_output_chunks_stay_bounded() {
        let mut output = Output { bytes: VecDeque::new(), end: 0, code: None, stopping: false, touched: Instant::now() };
        output.push(&vec![b'x'; OUTPUT_LIMIT + 7]);
        let (bytes, next, reset) = output.snapshot(0);
        assert!(reset);
        assert_eq!(next, 7 + CHUNK_LIMIT as u64);
        assert_eq!(bytes.len(), CHUNK_LIMIT);
        assert!(!output.snapshot(next).2);
        assert!(output.snapshot(output.end + 1).2);
    }

    #[test]
    fn finite_test_command_preserves_output_and_exit_code() {
        let root = std::env::temp_dir().canonicalize().unwrap();
        let command = if cfg!(windows) { "Write-Output raiox-pty-fixture; cmd.exe /D /C 'exit /B 7'" }
            else { "printf 'raiox-pty-fixture\\n'; exit 7" };
        let session = start(&root, Some(command.to_string()), size(&json!({}))).unwrap();
        let deadline = Instant::now() + Duration::from_secs(12);
        loop {
            {
                let output = session.output.lock().unwrap();
                if let Some(code) = output.code {
                    let (bytes, _, _) = output.snapshot(0);
                    let text = String::from_utf8_lossy(&bytes);
                    assert_eq!(code, 7, "{text}");
                    assert!(text.contains("raiox-pty-fixture"), "{text}");
                    break;
                }
            }
            if Instant::now() >= deadline {
                let _ = stop(&session);
                panic!("the finite terminal fixture did not exit");
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }
}
