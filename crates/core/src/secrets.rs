//! Detects tool calls that touch secrets files such as `.env`.
//!
//! The command is inspected in memory only to pick a category; the text is
//! never stored, returned or shown.

use memchr::memmem;
use serde::Deserialize;

use crate::model::SecretKind;

const NOT_SECRET_SUFFIXES: [&str; 6] = ["example", "sample", "template", "dist", "schema", "defaults"];
const PRINT_COMMANDS: [&str; 13] =
    ["cat", "head", "tail", "less", "more", "bat", "nl", "awk", "diff", "sort", "cut", "xxd", "strings"];
const SEARCH_COMMANDS: [&str; 4] = ["grep", "rg", "ag", "ack"];
const WRITE_COMMANDS: [&str; 5] = ["cp", "mv", "tee", "echo", "printf"];
const WRAPPERS: [&str; 5] = ["sudo", "time", "command", "exec", "nohup"];
const SEPARATORS: [&str; 5] = ["|", ";", "&&", "\n", "("];

#[derive(Deserialize)]
struct ToolInput {
    command: Option<String>,
    file_path: Option<String>,
    path: Option<String>,
}

/// Category of a tool call that touched a secrets file, if it did.
pub fn classify_tool(tool: &str, input: &[u8]) -> Option<SecretKind> {
    memmem::find(input, b".env")?;
    let parsed: ToolInput = serde_json::from_slice(input).ok()?;
    let touches = |value: &Option<String>| value.as_deref().is_some_and(mentions_secret_file);
    match tool {
        "Read" | "View" => touches(&parsed.file_path).then_some(SecretKind::Printed),
        "Grep" | "Glob" => touches(&parsed.path).then_some(SecretKind::Searched),
        "Edit" | "Write" | "MultiEdit" => touches(&parsed.file_path).then_some(SecretKind::Written),
        _ => parsed.command.as_deref().and_then(classify_command),
    }
}

/// Most exposing way a shell command touched a secrets file.
pub fn classify_command(command: &str) -> Option<SecretKind> {
    secret_mentions(command)
        .map(|pos| kind_for_segment(segment_before(command, pos)))
        .max_by_key(|kind| severity(*kind))
}

pub fn mentions_secret_file(text: &str) -> bool {
    secret_mentions(text).next().is_some()
}

/// Start positions of `.env` / `.env.local` style file names (not `.env.example`,
/// not `process.env`, not a `.env/` virtualenv folder).
fn secret_mentions(text: &str) -> impl Iterator<Item = usize> + '_ {
    let bytes = text.as_bytes();
    memmem::find_iter(bytes, b".env").filter(move |&pos| {
        let before_ok = pos == 0 || b" /\"'=(:\t<>".contains(&bytes[pos - 1]);
        let mut end = pos + 4;
        if bytes.get(end) == Some(&b'.') {
            let suffix_len =
                bytes[end + 1..].iter().take_while(|b| b.is_ascii_alphanumeric() || **b == b'_' || **b == b'-').count();
            let suffix = &text[end + 1..end + 1 + suffix_len];
            if suffix_len == 0 || NOT_SECRET_SUFFIXES.contains(&suffix.to_ascii_lowercase().as_str()) {
                return false;
            }
            end += 1 + suffix_len;
        }
        let after_ok = bytes.get(end).is_none_or(|b| b" \"';|&><),\n\t`".contains(b));
        before_ok && after_ok
    })
}

fn segment_before(command: &str, pos: usize) -> &str {
    let head = &command[..pos];
    let start = SEPARATORS.iter().filter_map(|sep| head.rfind(sep).map(|i| i + sep.len())).max().unwrap_or(0);
    head[start..].trim_start()
}

fn command_word(segment: &str) -> &str {
    segment
        .split_whitespace()
        .find(|token| !WRAPPERS.contains(token) && !is_assignment(token))
        .map(|token| token.rsplit('/').next().unwrap_or(token))
        .unwrap_or("")
}

fn is_assignment(token: &str) -> bool {
    token.find('=').is_some_and(|i| i > 0 && token.as_bytes()[0].is_ascii_alphabetic())
}

fn kind_for_segment(segment: &str) -> SecretKind {
    if segment.contains('>') {
        return SecretKind::Written;
    }
    let word = command_word(segment);
    if word == "sed" {
        return if segment.contains(" -i") { SecretKind::Written } else { SecretKind::Printed };
    }
    if PRINT_COMMANDS.contains(&word) {
        SecretKind::Printed
    } else if SEARCH_COMMANDS.contains(&word) {
        SecretKind::Searched
    } else if word == "source" || word == "." || segment.contains("env-file") || segment.contains("dotenv") {
        SecretKind::Loaded
    } else if WRITE_COMMANDS.contains(&word) {
        SecretKind::Written
    } else {
        SecretKind::Mentioned
    }
}

fn severity(kind: SecretKind) -> u8 {
    match kind {
        SecretKind::Printed => 4,
        SecretKind::Searched => 3,
        SecretKind::Written => 2,
        SecretKind::Mentioned => 1,
        SecretKind::Loaded => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use SecretKind::*;

    #[test]
    fn commands_are_classified_by_what_touches_the_file() {
        assert_eq!(classify_command("cat .env"), Some(Printed));
        assert_eq!(classify_command("grep API_KEY .env.local"), Some(Searched));
        assert_eq!(classify_command("node --env-file=.env x.mjs 2>&1 | grep error"), Some(Loaded));
        assert_eq!(classify_command("echo \"X=1\" >> .env"), Some(Written));
        assert_eq!(classify_command("cat > .env <<'EOF'"), Some(Written));
        assert_eq!(classify_command("cp .env.example .env"), Some(Written));
        assert_eq!(classify_command("git status && cat .env.production"), Some(Printed));
    }

    #[test]
    fn harmless_lookalikes_are_ignored() {
        assert_eq!(classify_command("cat .env.example"), None);
        assert_eq!(classify_command("source .env/bin/activate"), None);
        assert_eq!(classify_command("source .venv/bin/activate"), None);
        assert_eq!(classify_command("node -e 'console.log(process.env.HOME)'"), None);
        assert_eq!(classify_command("direnv allow .envrc"), None);
    }

    #[test]
    fn tools_with_paths_are_classified_by_tool() {
        assert_eq!(classify_tool("Read", br#"{"file_path":"/app/.env"}"#), Some(Printed));
        assert_eq!(classify_tool("Read", br#"{"file_path":"/app/.env.example"}"#), None);
        assert_eq!(classify_tool("Bash", br#"{"command":"cat .env","description":"x"}"#), Some(Printed));
        assert_eq!(classify_tool("Bash", br#"{"command":"ls"}"#), None);
    }
}
