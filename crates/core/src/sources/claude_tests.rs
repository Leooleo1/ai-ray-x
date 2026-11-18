//! Synthetic transcript lines shaped like Claude Code's, no real data.

use super::*;
use serde_json::{Value, json};

const HOME: &str = "/Users/dev";

fn chunk(lines: &[Value]) -> Vec<u8> {
    lines.iter().map(|line| format!("{line}\n")).collect::<String>().into_bytes()
}

fn parse(lines: &[Value], state: &mut FileState) -> Batch {
    parse_chunk(&chunk(lines), state, false, Path::new(HOME))
}

fn usage(read: u64, write: u64, out: u64) -> Value {
    json!({"input_tokens": 10, "cache_read_input_tokens": read, "cache_creation_input_tokens": write, "output_tokens": out})
}

fn assistant(id: &str, ts: &str, usage: Value, blocks: Value) -> Value {
    json!({
        "type": "assistant", "sessionId": "s1", "timestamp": ts, "cwd": "/Users/dev/Projects/app",
        "message": {"id": id, "model": "claude-opus-5-5", "usage": usage, "content": blocks}
    })
}

#[test]
fn repeated_lines_of_one_response_become_one_call_with_final_usage() {
    let mut state = FileState::default();
    let lines = [
        assistant("m1", "2026-11-01T10:00:00Z", usage(100, 0, 5), json!([{"type": "thinking", "thinking": "..."}])),
        assistant("m1", "2026-11-01T10:00:01Z", usage(100, 0, 42), json!([{"type": "text", "text": "done"}])),
    ];
    let batch = parse(&lines, &mut state);
    assert_eq!(batch.calls.len(), 1);
    let call = &batch.calls[0];
    assert_eq!((call.output, call.seq, call.is_first), (42, 1, true));
    assert_eq!(call.project, "Projects/app");
}

#[test]
fn request_blobs_are_skipped_without_parsing() {
    let mut state = FileState::default();
    let blob =
        json!({"type": "api-request-blob", "message": {"role": "user", "content": [{"type": "text", "text": "x"}]}});
    assert_eq!(parse(&[blob], &mut state).lines_parsed, 0);
}

#[test]
fn tool_results_are_sized_and_edits_counted_without_paths() {
    let mut state = FileState::default();
    let call = assistant(
        "m1",
        "2026-11-01T10:00:00Z",
        usage(100, 0, 5),
        json!([{"type": "tool_use", "id": "t1", "name": "Edit", "input": {"file_path": "/x/a.ts"}}]),
    );
    let result = json!({
        "type": "user", "sessionId": "s1", "timestamp": "2026-11-01T10:00:02Z", "uuid": "r1",
        "message": {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "t1", "content": "ok!"}]},
        "toolUseResult": {"filePath": "/x/a.ts", "structuredPatch": [{"lines": [" keep", "-old", "+new", "+more"]}]}
    });
    let batch = parse(&[call, result], &mut state);
    assert_eq!(batch.tool_calls.len(), 1);
    let r = &batch.results[0];
    assert_eq!((r.tool.as_str(), r.chars, r.added, r.removed), ("Edit", 3, 2, 1));
    assert_eq!(r.ext.as_deref(), Some(".ts"));
    assert!(batch.prompts.is_empty());
}

#[test]
fn compaction_opens_a_new_segment_and_explains_the_rewrite() {
    let mut state = FileState::default();
    let lines = [
        assistant("m1", "2026-11-01T10:00:00Z", usage(900_000, 0, 5), json!([])),
        json!({"type": "system", "subtype": "compact_boundary", "uuid": "c1", "timestamp": "2026-11-01T10:01:00Z",
               "compactMetadata": {"preTokens": 950_000, "postTokens": 40_000}}),
        assistant("m2", "2026-11-01T10:02:00Z", usage(0, 120_000, 5), json!([])),
        assistant("m3", "2026-11-01T12:30:00Z", usage(0, 130_000, 5), json!([])),
    ];
    let batch = parse(&lines, &mut state);
    assert_eq!(batch.compactions[0].pre, 950_000);
    assert_eq!(batch.calls[1].segment, 1);
    assert_eq!(batch.calls[1].rewrite_reason, Some("after_compaction"));
    assert_eq!(batch.calls[2].rewrite_reason, Some("idle_over_1h"));
}

#[test]
fn a_second_run_reads_only_new_lines_and_refreshes_usage() {
    let mut state = FileState::default();
    let first = parse(&[assistant("m1", "2026-11-01T10:00:00Z", usage(100, 0, 5), json!([]))], &mut state);
    assert_eq!(first.calls[0].seq, 1);
    let second = parse(
        &[
            assistant("m1", "2026-11-01T10:00:01Z", usage(100, 0, 9), json!([])),
            assistant("m2", "2026-11-01T10:00:05Z", usage(150, 0, 3), json!([])),
        ],
        &mut state,
    );
    let calls: Vec<_> = second.calls.iter().map(|c| (c.id.as_str(), c.seq, c.output)).collect();
    assert_eq!(calls, vec![("m1", 1, 9), ("m2", 2, 3)]);
}

#[test]
fn startup_attachments_are_measured_once() {
    let mut state = FileState::default();
    let lines = [
        json!({"type": "attachment", "sessionId": "s1", "timestamp": "2026-11-01T09:59:59Z",
               "attachment": {"type": "skill_listing", "content": "- a\n- b\n- c"}}),
        assistant("m1", "2026-11-01T10:00:00Z", usage(60_000, 0, 5), json!([])),
    ];
    let batch = parse(&lines, &mut state);
    let keys: Vec<_> = batch.snapshots.iter().map(|s| s.key).collect();
    assert_eq!(keys, vec!["skill_listing_items", "startup_context"]);
    assert!(state.startup_chars.is_empty());
}
