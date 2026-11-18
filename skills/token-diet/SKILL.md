---
name: token-diet
description: Keep your own context small while you work, using the raiox MCP tools (context_status, what_weighs, suggest_diet). Use at the start of long tasks, before reading big files or taking screenshots, after a long break, and when the person asks why usage or limits run out.
---

# Token diet

Most of an agent's cost is re-reading its own context on every request, not
thinking. Carry less and every later request gets cheaper. Three roles share
this skill; act as the one the moment needs.

## Raio: look before you load

- At the start of a long task, call `context_status`. It reports the context
  you carry now, what was loaded since the last compaction and where
  compaction usually happens.
- Above about 400K tokens, finish the current step, then compact (or ask the
  person to run `/compact`) before starting the next one.
- Before reading a large file, search for the lines you need and read only
  that range. Pipe long command output through `head` or `tail`.
- Screenshots stay in context until the next compaction: take them in a
  subagent and keep only its short verdict.

## Dieta: suggest, never apply

- When the person asks why limits run out, or once per long session, call
  `suggest_diet`. Present the top suggestions in one line each: what, the
  evidence, the estimated saving and the confidence.
- Do not edit settings, environment variables, plugins or skills yourself.
  The person applies changes in the raiox panel, which keeps a backup and an
  undo.

## Memo: keep the lesson, pass the baton

Only if ai-memory tools are available in this session:
- When the person accepts a suggestion, save one short page with
  `memory_write_page`: the change, the evidence and the date.
- When the work should continue with another agent or project, open a handoff
  with `memory_handoff_begin` (or `memory_message_send` for another project)
  instead of pasting long context.
- Never write to ai-memory's files or database directly.

If the raiox tools are not available, follow the habits above without them.
