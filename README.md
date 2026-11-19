# raiox

An x-ray of your AI coding agents: where the tokens of Claude Code and Codex
go on your machine, what to cut, and the AI you already use put to work inside
each of your projects.

Everything runs locally. raiox reads the logs your CLIs already write, serves a
panel on `127.0.0.1`, and never sends your data anywhere.

## What it does

**X-ray.** Tokens your AIs read and wrote, what that would cost paying per use,
how much each request re-reads, what gets re-read most (tool output, your own
prompts, system text), tokens per project, and how often a secrets file such as
`.env` entered a conversation (counts only; the text is never kept).

**Efficiency.** Ranked cuts with the estimated saving, measured from your own
logs: compacting earlier, loading less at startup, keeping screenshots out of
the main session. The ones that are a setting apply to the project you pick,
with a backup and an undo, and raiox measures the result afterwards.

**Chats.** Turn on a project and talk to the CLI you already use there (Claude
Code, Codex or Cursor), with the project's context:

- it only reads while talking. When you approve a proposal, it edits a copy
  of the project (following your `.gitignore`, without secrets files) and you
  see what changed in each file, then apply it to the project (with undo) or
  discard it;
- when an answer needs a command's output (the git history, the tests), the AI
  asks for it and raiox runs that exact command in the project folder, without
  a shell, after you approve it, or on its own once you allow it for the
  project;
- every answer shows what it read, and the bar shows each step live
  (reading a file, searching, running a command);
- parallel conversations, pinned answers, routines on a schedule, one
  question to several projects at once with the answers side by side;
- model and effort per project.

Each message runs the real CLI with your own login, so it counts against your
plan.

**Raio, Dieta and Memo.** Three house agents inside every project chat. They
answer from the logs, without AI and at no cost: Raio reports what the session
carries, Dieta lists the cuts with Apply, Memo says what was kept. They can also
float a tip now and then; each one can be muted.

**ai-memory.** With [ai-memory](https://github.com/akitaonrails/ai-memory)
installed, raiox keeps a pinned page per project (`raiox/equipe.md`) with who
works there and what each AI did last, and every bot reads its own project's
page and those of the projects wired to it before answering, plus the
project's pages that relate to the question. Any answer can be kept as a
decision of the project (`raiox/decisoes/…`) with one click, so later
questions find it. Wired projects
can message each other through ai-memory, always with your approval.

## Requirements

- Rust 1.88 or newer, and Node.js with pnpm, to build from source.
- Any of Claude Code, Codex or Cursor's `cursor-agent`, signed in, for the
  chats. The x-ray reads Claude Code and Codex logs.
- Optional: [ai-memory](https://github.com/akitaonrails/ai-memory) for the team
  notes and the messages between projects, and the GitHub CLI (`gh`) to find
  and clone your repositories during setup.

## Run it

```sh
git clone <this repository> raiox
cd raiox/apps/web && pnpm install && pnpm build
cd ../.. && cargo build --release
./target/release/raiox open
```

The first run walks you through connecting your AIs, GitHub and projects. The
binary serves the panel from `apps/web/dist`, so run it from the checkout (or
pass `--web-dir`).

## Command line

```text
raiox open      Open the panel (the default)
raiox report    Print the report in the terminal
raiox context   What the current session of a folder carries now
raiox weighs    What weighs most over the indexed period (JSON)
raiox diet      Dieta's ranked suggestions (JSON); never applies anything
raiox apply     Apply a suggestion to this project (backup and ledger)
raiox undo      Undo an applied change by its id
raiox ask       Ask Raio, Dieta or Memo about this project
raiox mcp       Read-only MCP server over stdio
```

`skills/token-diet` teaches an agent when to call these, and `docs/agents.md`
describes the three house agents.

## Privacy

- Local only: the panel listens on `127.0.0.1` with a per-run session token.
- raiox reads your CLIs' logs and keeps an index in your cache folder; it does
  not upload them.
- Secrets are counted, never stored.
- Secrets files (`.env` and its variants, `.dev.vars`, keys, certificates)
  never enter the copies the AIs edit. Claude Code is also denied reading
  them while talking; Codex and Cursor have no such switch, so they are only
  told not to.
- Commands that would open secrets files are refused before you are asked,
  variables that look like secrets stay out of their environment, and values
  that look like secrets are hidden in their output.
- Chats run your own CLI, which talks to its own provider under your account.

## Status

Early (0.1). Developed on macOS; Linux should work; Windows is untested.

## License

MIT, see `LICENSE`. Third-party pieces are listed in `THIRD_PARTY_NOTICES.md`.
