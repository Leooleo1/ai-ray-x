# raiox agents

Three agents carry the token diet into any harness that speaks MCP and skills
(Claude Code, Codex, OpenBot, agentes.chat). Each has an avatar made with the
open-bot-avatar engine (`assets/agents/*.json`, validated with `--strict`).
The panel renders them live with the `open-bot-avatar` npm package (MIT,
© 2026 Dominik Scholz): volume, gaze, blinks and moods.
Bots people make in the setup can wear the package's starter shapes (chat,
lightbulb, sprout) or shapes copied from its gallery into `assets/avatars/`
(book, brain, cat, ghost, magnifier, rocket, shield, star), same license.

| Agent | Avatar | Job | Tools |
| --- | --- | --- | --- |
| Ray | Magnifying Glass | Reads the logs and shows where the tokens go | `context_status`, `what_weighs` |
| Diet | Apple | Proposes cuts with the estimated saving and the exact config diff | `suggest_diet` |
| Memo | File Box | Keeps the lessons and hands work over, through ai-memory | ai-memory's own tools |

## How they work together

In the panel they live inside each project's bot chat: three chips above the
composer. They answer from the logs, without AI and without spending the
person's plan, about that bot's project; turning a bot on is that project's
opt-in, and Diet's Apply writes to that project, with a backup and an undo.

1. Ray answers "how much context am I carrying and what is in it" for the
   current session, and "what weighs most" over a period.
2. Diet turns that into ranked suggestions. It never changes a setting: a
   person applies a change in the panel, with a backup and an undo.
3. Memo saves each accepted lesson as an ai-memory page and opens a handoff
   when another agent or project should pick up the work. It uses ai-memory's
   MCP tools (`memory_write_page`, `memory_handoff_begin`,
   `memory_message_send`) and never writes to ai-memory's database.

Pieces shipped by raiox:
- `raiox mcp`: a local MCP server (stdio, or HTTP on 127.0.0.1 with a token)
  exposing the read-only tools above. It returns numbers and names only.
- `skills/token-diet/SKILL.md`: teaches an agent when to call the tools, how to
  keep its own context small, and when to hand a lesson to Memo.

## Respecting the other projects' terms

raiox is MIT and copies no code from the projects below. It talks to them only
through public, documented interfaces, and names them only to say it works
with them (no logos, no implied endorsement).

| Project | License | What raiox does | What raiox never does |
| --- | --- | --- | --- |
| ai-memory | MIT | Calls its MCP server and CLI | Write to its SQLite directly |
| agentes.chat | AGPL-3.0 (+ commercial license, trademark policy) | Is connected by its users as an external MCP server and skill | Copy its code or ship a modified copy of it |
| OpenBot (nightly-labs) | PolyForm Noncommercial (source-available) | Reads the user's own local `openbot.db` read-only, tolerating schema changes; is installable as an MCP app | Copy or redistribute its code |
