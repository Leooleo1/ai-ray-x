//! Synthetic rollout lines shaped like Codex's, no real data.

use super::*;
use serde_json::{Value, json};

fn parse(lines: &[Value], state: &mut FileState) -> Batch {
    let data: String = lines.iter().map(|line| format!("{line}\n")).collect();
    parse_chunk(data.as_bytes(), state, Path::new("/Users/dev"))
}

fn line(kind: &str, payload: Value) -> Value {
    json!({"type": kind, "timestamp": "2026-11-02T10:00:00Z", "payload": payload})
}

#[test]
fn usage_records_become_calls_with_the_cache_split_out() {
    let mut state = FileState::default();
    let batch = parse(
        &[
            line("session_meta", json!({"id": "s1", "cwd": "/Users/dev/Projects/api", "source": "cli"})),
            line("turn_context", json!({"model": "gpt-6.1-sol", "effort": "high"})),
            line(
                "token_usage_record",
                json!({"response_id": "r1", "usage": {"input_tokens": 1000, "cached_input_tokens": 900, "output_tokens": 50, "reasoning_output_tokens": 20}}),
            ),
        ],
        &mut state,
    );
    let call = &batch.calls[0];
    assert_eq!((call.input, call.cache_read, call.output, call.thinking), (100, 900, 50, 20));
    assert_eq!((call.model.as_str(), call.project.as_str()), ("gpt-6.1-sol", "Projects/api"));
    assert_eq!(batch.snapshots[0].key, "base_instructions");
}

#[test]
fn subagent_sessions_and_secret_commands_are_flagged() {
    let mut state = FileState::default();
    let batch = parse(
        &[
            line("session_meta", json!({"id": "s2", "cwd": "/Users/dev", "source": {"subagent": {"other": "x"}}})),
            line(
                "event_msg",
                json!({"type": "item_completed", "item": {"type": "commandExecution", "exit_code": 1, "command": ["/bin/zsh", "-lc", "cat .env"]}}),
            ),
        ],
        &mut state,
    );
    assert!(state.is_subagent);
    assert_eq!(batch.counters.get("commands_failed"), Some(&1));
    assert_eq!(batch.counters.get("secret:printed"), Some(&1));
    assert!(batch.snapshots.is_empty());
}

#[test]
fn message_text_is_skipped_and_tool_outputs_are_measured() {
    let mut state = FileState::default();
    let batch = parse(
        &[
            line(
                "response_item",
                json!({"type": "message", "role": "user", "content": [{"type": "input_text", "text": "hi"}]}),
            ),
            line("response_item", json!({"type": "function_call", "name": "exec", "call_id": "c1", "arguments": "{}"})),
            line("response_item", json!({"type": "function_call_output", "call_id": "c1", "output": "12345"})),
            line("compacted", json!({"message": "summary"})),
        ],
        &mut state,
    );
    assert_eq!(batch.lines_parsed, 3);
    assert_eq!((batch.results[0].tool.as_str(), batch.results[0].chars), ("exec", 5));
    assert_eq!((batch.compactions.len(), state.segment), (1, 1));
}
