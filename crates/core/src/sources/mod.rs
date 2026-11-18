//! One parser per agent. Each turns a chunk of a log file into a [`Batch`].

pub mod claude;
pub mod codex;
pub mod history;
pub mod openbot;

use crate::model::{Batch, Call, FileState};
use crate::util;

/// Places a call in the file timeline: merges repeated lines of the same
/// response (the last line carries the final usage), numbers new calls and
/// flags large cache rewrites. Returns true for the first call of the file.
pub(crate) fn record_call(state: &mut FileState, batch: &mut Batch, call: Call) -> bool {
    if state.last_call_id.as_deref() == Some(call.id.as_str()) {
        match batch.calls.iter().rposition(|c| c.id == call.id) {
            Some(i) => batch.calls[i] = batch.calls[i].with_usage_of(&call),
            // Stored by an earlier run: the store only refreshes its usage.
            None => batch.calls.push(Call { seq: state.seq, segment: state.segment, ..call }),
        }
        return false;
    }
    state.seq += 1;
    let ms = call.at.as_ref().map(|m| m.ms);
    let is_first = !state.seen_first;
    let rewrite_reason = (!is_first && call.cache_write() >= util::BIG_REWRITE).then(|| {
        let gap = ms.zip(state.last_ms).map(|(now, before)| now - before);
        let model_changed = state.last_model.as_deref() != Some(call.model.as_str());
        util::rewrite_reason(state.after_compact, model_changed, gap)
    });
    state.seen_first = true;
    state.after_compact = false;
    state.last_ms = ms.or(state.last_ms);
    state.last_model = Some(call.model.clone());
    state.last_call_id = Some(call.id.clone());
    batch.calls.push(Call { seq: state.seq, segment: state.segment, is_first, rewrite_reason, ..call });
    is_first
}
