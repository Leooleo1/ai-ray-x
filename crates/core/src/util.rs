//! Small helpers shared by the parsers.

use std::path::{Component, Path};

use chrono::{DateTime, Datelike, Local, Timelike};
use memchr::memmem;
use serde::Deserialize;
use serde::de::{DeserializeOwned, Deserializer};
use serde_json::value::RawValue;

use crate::model::Moment;

/// For small fields that may change type between versions: a mismatch becomes
/// `None` instead of dropping the whole line. Use with `#[serde(default, deserialize_with = "lenient")]`.
pub fn lenient<'de, D: Deserializer<'de>, T: DeserializeOwned>(d: D) -> Result<Option<T>, D::Error> {
    let value = serde_json::Value::deserialize(d)?;
    Ok(T::deserialize(value).ok())
}

/// Requests that write at least this many tokens to the cache are flagged as rewrites.
pub const BIG_REWRITE: u64 = 100_000;
const ONE_HOUR_MS: i64 = 3_600_000;
const FIVE_MIN_MS: i64 = 300_000;

pub fn moment(ts: &str) -> Option<Moment> {
    let local = DateTime::parse_from_rfc3339(ts).ok()?.with_timezone(&Local);
    Some(Moment {
        ms: local.timestamp_millis(),
        date: local.format("%Y-%m-%d").to_string(),
        hour: local.hour() as u8,
        weekday: local.weekday().num_days_from_monday() as u8,
    })
}

/// Why a request had to write a large part of the cache again.
pub fn rewrite_reason(after_compact: bool, model_changed: bool, gap_ms: Option<i64>) -> &'static str {
    if after_compact {
        return "after_compaction";
    }
    if model_changed {
        return "model_switch";
    }
    match gap_ms {
        Some(gap) if gap > ONE_HOUR_MS => "idle_over_1h",
        Some(gap) if gap > FIVE_MIN_MS => "idle_5m_to_1h",
        _ => "prefix_changed",
    }
}

/// Project label: the first two folders under home (`Projects/app`), `~` for home itself.
/// Where raiox keeps the copies its bots edit: `<…>/raiox/rascunhos/<bot>/copia/<path under home>`.
const DRAFT_COPY: &str = "/raiox/rascunhos/";

pub fn project_of(cwd: &str, home: &Path) -> String {
    // A bot editing a draft copy works for the project the copy came from.
    if let Some(at) = cwd.find(DRAFT_COPY)
        && let Some(inside) = cwd[at + DRAFT_COPY.len()..].split_once("/copia/").map(|(_, rest)| rest)
    {
        return project_of(&home.join(inside).to_string_lossy(), home);
    }
    let path = Path::new(cwd);
    let Ok(rel) = path.strip_prefix(home) else {
        return path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_else(|| cwd.to_string());
    };
    let parts: Vec<String> = rel
        .components()
        .filter_map(|c| match c {
            Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
            _ => None,
        })
        .take(2)
        .collect();
    if parts.is_empty() { "~".to_string() } else { parts.join("/") }
}

/// FNV-1a: stable across runs and Rust versions.
pub fn stable_hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3))
}

/// Length of a JSON string literal without its quotes (escapes counted as written).
pub fn raw_len(raw: &RawValue) -> u64 {
    let text = raw.get();
    if text.starts_with('"') { text.len().saturating_sub(2) as u64 } else { 0 }
}

/// First character of a JSON string literal.
pub fn raw_first_char(raw: &RawValue) -> Option<u8> {
    let bytes = raw.get().as_bytes();
    if bytes.first() == Some(&b'"') { bytes.get(1).copied() } else { None }
}

pub fn contains(hay: &[u8], needle: &[u8]) -> bool {
    memmem::find(hay, needle).is_some()
}

pub fn occurrences(hay: &[u8], needle: &[u8]) -> u64 {
    memmem::find_iter(hay, needle).count() as u64
}

pub fn extension(path: &str) -> String {
    Path::new(path).extension().map(|ext| format!(".{}", ext.to_string_lossy())).unwrap_or_else(|| "(none)".to_string())
}

/// Groups MCP tools by server: `mcp__github__search` -> `mcp:github`.
pub fn tool_group(name: &str) -> String {
    match name.strip_prefix("mcp__") {
        Some(rest) => format!("mcp:{}", rest.split("__").next().unwrap_or(rest)),
        None => name.to_string(),
    }
}

/// Complete, non-empty lines of a chunk that ends with a newline.
pub fn lines(data: &[u8]) -> impl Iterator<Item = &[u8]> {
    let mut start = 0;
    memchr::memchr_iter(b'\n', data)
        .map(move |end| {
            let line = &data[start..end];
            start = end + 1;
            line
        })
        .filter(|line| !line.is_empty())
}

/// Splits a chunk that ends with a newline into blocks of about `target`
/// bytes, each ending at a line boundary.
pub fn blocks(data: &[u8], target: usize) -> Vec<&[u8]> {
    let mut found = Vec::new();
    let mut start = 0;
    while start < data.len() {
        let tentative = (start + target.max(1)).min(data.len());
        let end = memchr::memchr(b'\n', &data[tentative - 1..]).map_or(data.len(), |i| tentative + i);
        found.push(&data[start..end]);
        start = end;
    }
    found
}

#[derive(Deserialize)]
struct Part<'a> {
    #[serde(rename = "type")]
    kind: Option<String>,
    #[serde(borrow)]
    text: Option<&'a RawValue>,
}

/// Characters of text and number of images in a message or tool-result content.
pub fn measure_content(raw: &RawValue) -> (u64, u64) {
    match raw.get().as_bytes().first() {
        Some(b'"') => (raw_len(raw), 0),
        Some(b'[') => {
            let parts: Vec<Part> = serde_json::from_str(raw.get()).unwrap_or_default();
            parts.iter().fold((0, 0), |(chars, images), part| match part.kind.as_deref() {
                Some("image" | "input_image") => (chars, images + 1),
                _ => (chars + part.text.map(raw_len).unwrap_or(0), images),
            })
        }
        _ => (0, 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(json: &str) -> Box<RawValue> {
        RawValue::from_string(json.to_string()).unwrap()
    }

    #[test]
    fn project_is_two_folders_under_home() {
        let home = Path::new("/Users/dev");
        assert_eq!(project_of("/Users/dev/Projects/app/server", home), "Projects/app");
        assert_eq!(project_of("/Users/dev", home), "~");
        // A draft copy counts for the project it was copied from.
        assert_eq!(project_of("/Users/dev/Library/Caches/raiox/rascunhos/d1/copia/Projects/app/src", home), "Projects/app");
        assert_eq!(project_of("/tmp/worktree-x", home), "worktree-x");
    }

    #[test]
    fn rewrite_reason_prefers_compaction_then_model_then_idle() {
        assert_eq!(rewrite_reason(true, true, Some(9_999_999)), "after_compaction");
        assert_eq!(rewrite_reason(false, true, None), "model_switch");
        assert_eq!(rewrite_reason(false, false, Some(2 * ONE_HOUR_MS)), "idle_over_1h");
        assert_eq!(rewrite_reason(false, false, Some(10 * 60_000)), "idle_5m_to_1h");
        assert_eq!(rewrite_reason(false, false, Some(1_000)), "prefix_changed");
    }

    #[test]
    fn mcp_tools_group_by_server() {
        assert_eq!(tool_group("mcp__github__search_code"), "mcp:github");
        assert_eq!(tool_group("Bash"), "Bash");
    }

    #[test]
    fn lines_skip_empty_and_need_newline() {
        let chunk = b"a\n\nbb\nunterminated";
        assert_eq!(lines(chunk).collect::<Vec<_>>(), vec![&b"a"[..], &b"bb"[..]]);
    }

    #[test]
    fn blocks_end_on_line_boundaries_and_cover_everything() {
        let data = b"aa\nbb\ncc\n";
        let parts = blocks(data, 4);
        assert_eq!(parts, vec![&b"aa\nbb\n"[..], &b"cc\n"[..]]);
        assert_eq!(parts.concat(), data.to_vec());
    }

    #[test]
    fn content_counts_text_and_images() {
        assert_eq!(measure_content(&raw(r#""hello""#)), (5, 0));
        let parts = r#"[{"type":"text","text":"abc"},{"type":"image","source":{"data":"xx"}}]"#;
        assert_eq!(measure_content(&raw(parts)), (3, 1));
    }
}
