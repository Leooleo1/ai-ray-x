//! Commands a bot asks to run with a `RAIOX-RUN:` line. raiox runs one only
//! when the person approves it, or allowed that exact command for the project.
//! It runs in the project folder without a shell: the words go straight to the
//! program, so pipes, redirections, `;` and variables do not exist. Commands
//! that would show secrets are refused before anyone is asked, variables that
//! look like secrets stay out of the command's environment, and values that
//! look like secrets are hidden in its output.

use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde::Serialize;

/// Longest command raiox accepts, in characters.
const MAX_COMMAND: usize = 300;
/// A command that runs longer is stopped.
pub const TIMEOUT: Duration = Duration::from_secs(120);
/// The output kept for the AI and the panel: its end, in bytes.
const MAX_OUTPUT: usize = 12_000;
const POLL: Duration = Duration::from_millis(100);
/// What only a shell understands; without one these would not do what the AI expects.
const SHELL_CHARS: [char; 9] = [';', '|', '&', '<', '>', '`', '$', '\n', '\r'];
/// Programs that would run anything or show secrets.
const BLOCKED: [&str; 16] =
    ["sh", "bash", "zsh", "fish", "dash", "ksh", "csh", "tcsh", "nu", "env", "printenv", "sudo", "su", "doas", "security", "osascript"];
/// Interpreters and the flags that run code written inline.
const INLINE: [(&str, &[&str]); 8] = [
    ("node", &["-e", "--eval", "-p", "--print"]),
    ("bun", &["-e", "--eval", "-p", "--print"]),
    ("deno", &["eval"]),
    ("python", &["-c"]),
    ("python3", &["-c"]),
    ("ruby", &["-e"]),
    ("perl", &["-e", "-E"]),
    ("php", &["-r"]),
];
/// Name parts that mark a secret, in variables and in `name=value` lines.
const SECRET_PARTS: [&str; 6] = ["KEY", "TOKEN", "SECRET", "PASSWORD", "PASSWD", "CREDENTIALS"];
/// Endings that mark a secret in names written as one word (`apiKey`, `ACCESSTOKEN`).
const SECRET_ENDINGS: [&str; 4] = ["APIKEY", "TOKEN", "SECRET", "PASSWORD"];
const HIDDEN: &str = "[hidden by raiox]";

/// A command the bot asked for, and why raiox refuses it, if it does.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Request {
    pub command: String,
    /// "long", "shell", "empty", "blocked", "inline" or "secret".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refused: Option<&'static str>,
}

pub fn request(command: &str) -> Request {
    let command = command.trim().to_string();
    let refused = check(&command).err();
    Request { command, refused }
}

/// The program and its arguments, or why raiox will not run the command.
pub fn check(command: &str) -> Result<Vec<String>, &'static str> {
    if command.chars().count() > MAX_COMMAND {
        return Err("long");
    }
    if command.contains(SHELL_CHARS) {
        return Err("shell");
    }
    // Unbalanced quotes are shell syntax gone wrong.
    let words = shell_words::split(command).map_err(|_| "shell")?;
    let program = words.first().ok_or("empty")?;
    let name = Path::new(program).file_name().and_then(|n| n.to_str()).unwrap_or(program);
    if BLOCKED.contains(&name) {
        return Err("blocked");
    }
    if INLINE.iter().any(|(interpreter, flags)| *interpreter == name && words[1..].iter().any(|w| flags.contains(&w.as_str()))) {
        return Err("inline");
    }
    if words.iter().any(|word| names_secret(word)) {
        return Err("secret");
    }
    Ok(words)
}

/// Whether a word points at a secrets file: `.env`, `--env-file=.env`, `HEAD:config/.env`.
fn names_secret(word: &str) -> bool {
    word.split(|c: char| c.is_whitespace() || matches!(c, '=' | ':' | ',' | '"' | '\''))
        .filter_map(|piece| Path::new(piece).file_name().and_then(|n| n.to_str()))
        .any(raiox_core::secrets::is_secret_file)
}

/// Whether a variable or field name looks like it holds a secret (`OPENAI_API_KEY`, `apiKey`).
pub fn is_secret_name(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    upper.split(['_', '-', '.']).any(|part| SECRET_PARTS.contains(&part) || SECRET_ENDINGS.iter().any(|end| part.ends_with(end)))
}

/// Hides the value of lines like `SOME_API_KEY=value` or `"token": "value"`.
fn redact(output: &str) -> String {
    output.lines().map(redact_line).collect::<Vec<_>>().join("\n")
}

fn redact_line(line: &str) -> String {
    let Some(at) = line.find(['=', ':']) else { return line.to_string() };
    let before = line[..at].trim_end_matches([' ', '"', '\'']);
    let start = before.rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-')).map_or(0, |i| i + 1);
    let has_value = !line[at + 1..].trim().trim_matches(['"', '\'', ',']).is_empty();
    if has_value && is_secret_name(&before[start..]) {
        format!("{} {HIDDEN}", &line[..=at])
    } else {
        line.to_string()
    }
}

/// What a command did.
#[derive(Debug, Clone, Serialize)]
pub struct Ran {
    pub command: String,
    pub code: Option<i32>,
    pub output: String,
    /// The output was longer: only its end is kept.
    pub cut: bool,
    pub timed_out: bool,
    pub stopped: bool,
    pub duration_ms: u64,
}

fn drain(pipe: Option<impl Read + Send + 'static>) -> JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut bytes);
        }
        bytes
    })
}

/// Stops the command and everything it started.
fn kill(child: &mut Child) {
    #[cfg(unix)]
    // SAFETY: kill(2) on the process group the child leads (set with process_group(0)).
    unsafe {
        libc::kill(-(child.id() as i32), libc::SIGKILL);
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// The last `MAX_OUTPUT` bytes, starting on a character.
fn tail(text: &str) -> (String, bool) {
    if text.len() <= MAX_OUTPUT {
        return (text.to_string(), false);
    }
    let mut start = text.len() - MAX_OUTPUT;
    while !text.is_char_boundary(start) {
        start += 1;
    }
    (text[start..].to_string(), true)
}

/// Runs a checked command in `dir`, stopping it after `TIMEOUT` or when `stop` says so.
pub fn run(command: &str, words: &[String], dir: &Path, stop: &dyn Fn() -> bool) -> Ran {
    let started = Instant::now();
    let mut process = Command::new(&words[0]);
    process.args(&words[1..]).current_dir(dir).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).env("NO_COLOR", "1");
    for (name, _) in std::env::vars_os() {
        if name.to_str().is_some_and(is_secret_name) {
            process.env_remove(name);
        }
    }
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut process, 0);
    let finished = |code, output: String, timed_out, stopped| {
        let (output, cut) = tail(&output);
        Ran { command: command.to_string(), code, output: redact(&output), cut, timed_out, stopped, duration_ms: started.elapsed().as_millis() as u64 }
    };
    let mut child = match process.spawn() {
        Ok(child) => child,
        Err(error) => return finished(None, format!("could not start {}: {error}", words[0]), false, false),
    };
    let (out, err) = (drain(child.stdout.take()), drain(child.stderr.take()));
    let (mut timed_out, mut stopped) = (false, false);
    let code = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.code(),
            Ok(None) => {}
            Err(_) => break None,
        }
        stopped = stop();
        timed_out = started.elapsed() > TIMEOUT;
        if stopped || timed_out {
            kill(&mut child);
            break None;
        }
        std::thread::sleep(POLL);
    };
    let mut bytes = out.join().unwrap_or_default();
    let errors = err.join().unwrap_or_default();
    if !errors.is_empty() && !bytes.is_empty() {
        bytes.push(b'\n');
    }
    bytes.extend(errors);
    finished(code, String::from_utf8_lossy(&bytes).into_owned(), timed_out, stopped)
}

/// What the AI is told after the command ran, so it goes on answering.
pub fn prompt(ran: &Ran, allowed: bool) -> String {
    let why = if allowed { "a command the person allowed for this project" } else { "approved by the person" };
    let how = match (ran.timed_out, ran.code) {
        (true, _) => format!("stopped after {} s", TIMEOUT.as_secs()),
        (false, Some(code)) => format!("exit code {code}"),
        (false, None) => "it did not start".to_string(),
    };
    let part = if ran.cut { ", last part only" } else { "" };
    let seconds = ran.duration_ms as f64 / 1000.0;
    format!(
        "raiox ran `{}` in the project folder ({why}; {how}, {seconds:.1} s). Its output{part}:\n~~~\n{}\n~~~\n\
         Go on answering the person's last question with it. Ask for another command only if you really need it.",
        ran.command, ran.output
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_that_need_a_shell_or_show_secrets_are_refused() {
        assert_eq!(check("git log -1 --format='%cd'"), Ok(vec!["git".into(), "log".into(), "-1".into(), "--format=%cd".into()]));
        assert!(check("npm test").is_ok());
        for (command, why) in [
            ("git log | head", "shell"),
            ("git log; rm -rf .", "shell"),
            ("echo $HOME", "shell"),
            ("git log 'open", "shell"),
            ("", "empty"),
            ("bash -c ls", "blocked"),
            ("/usr/bin/env", "blocked"),
            ("printenv", "blocked"),
            ("python3 -c 'print(1)'", "inline"),
            ("node -e 1", "inline"),
            ("cat .env", "secret"),
            ("git show HEAD:config/.env.local", "secret"),
            ("node --env-file=.env x.js", "secret"),
            ("cat ~/.ssh/id_rsa", "secret"),
        ] {
            assert_eq!(check(command), Err(why), "{command}");
        }
        assert!(check("node --env-file=.env.example x.js").is_ok());
    }

    #[test]
    fn secret_values_are_hidden_in_the_output() {
        let output = "OPENAI_API_KEY=sk-123\nAuthor: Ana <a@b.c>\n  \"apiKey\": \"abc\",\nkeywords: rust\nGITHUB_TOKEN=\nok";
        assert_eq!(redact(output), "OPENAI_API_KEY= [hidden by raiox]\nAuthor: Ana <a@b.c>\n  \"apiKey\": [hidden by raiox]\nkeywords: rust\nGITHUB_TOKEN=\nok");
        assert!(is_secret_name("AWS_SECRET_ACCESS_KEY") && is_secret_name("NPM_TOKEN") && !is_secret_name("SSH_AUTH_SOCK") && !is_secret_name("PATH"));
    }

    #[test]
    fn a_command_runs_in_the_project_folder() {
        let dir = std::env::temp_dir().join(format!("raiox-commands-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("soma.js"), "x").unwrap();
        let words = check("ls").unwrap();
        let ran = run("ls", &words, &dir, &|| false);
        assert_eq!((ran.code, ran.output.trim()), (Some(0), "soma.js"));
        let missing = run("nope", &["raiox-no-such-program".to_string()], &dir, &|| false);
        assert!(missing.code.is_none() && missing.output.contains("could not start"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
