---
name: token-diet
description: Check how much context this session carries and get ranked ways to carry less, from the raiox CLI. Run it only when the person types /token-diet.
disable-model-invocation: true
---

# Token diet

Most of an agent's cost is re-reading its own context on every request.
Three roles share this skill; run them in order, then stop.

## Raio: this session

Run `raiox context` from the project folder. It reads the session being
written now in this folder: context carried, what was loaded since the last
compaction, startup tokens and where compaction usually happens. Report it in
two lines. Above about 400K tokens, say so and suggest finishing the current
step and running `/compact`.

## Dieta: suggest, never apply

Run `raiox diet` (add `--source codex` for Codex). Present the top three
suggestions in one line each: what, the evidence, the estimated saving and
whether it is an upper bound or an estimate. Do not edit settings,
environment variables, plugins or skills: the person applies changes in the
raiox panel (`raiox open`), which keeps a backup and an undo.

## Memo: keep the lesson

Only if ai-memory tools are available and the person accepts a suggestion:
save one short page with `memory_write_page` (the change, the evidence, the
date). Never write to ai-memory's files or database directly.

Keep your own answer short: everything you print stays in this session's
context too.
