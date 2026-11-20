# AGENTS.md

Follow the full RSI protocol in `CLAUDE.md`.

Key operating rules:
- **Never stall mid-cycle**: whenever the next step is ready, end your reply with its `RAIOX-RUN:` line.
- **Exact slugs only**: read `dimensions` from `python3 rsi.py status` before calling `propose` or emitting `RAIOX-TASK:` (e.g. `micro_thread_3_posts`, never shorthand like `thread`).
- **No duplicate tasks**: check whether a task or active challenger already exists before emitting `RAIOX-TASK:`.
- **Discover new tactics**: with "Reads the web" on, search current trends and register new options with `add-option --source "<URL>" --by agent`; without web access, register tactics the person shares (`--by person`).
- **Never publish to X**: only register local drafts with `draft`; the person posts manually and exports the free X Analytics CSV for `ingest-x`.

Quick reference for raiox-safe commands (`RAIOX-RUN:` — on Windows use `python` or `py -3` if `python3` is unavailable):
- `python3 rsi.py status` — active niche, exact `dimensions`, `discovered_options`, champion/challenger, drift
- `python3 rsi.py presets` — list built-in fast-feedback presets (`x-viral`, `paid-ads`, `cold-outbound`)
- `python3 rsi.py init --preset <id>` — initialize a niche and `v001` playbook
- `python3 rsi.py add-option --dim <d> --option <slug> --source "<url/obs>" --by agent` — register a new candidate tactic with provenance
- `python3 rsi.py propose --hypothesis "..." --change <dim>=<exact_slug>` — create a challenger (pass `--source` to register a brand-new tactic inline)
- `python3 rsi.py draft --version <v> --client <c> --text "..."` — register a post draft locally before manual X posting
- `python3 rsi.py ingest-x <export.csv>` — import free X Analytics CSV (EN/PT), convert counts to rates, match to registered drafts
- `python3 rsi.py ingest <file.csv>` — import generic rate CSV/JSON
- `python3 rsi.py simulate --runs 5 --phase 0` — run balanced A/B simulation
- `python3 rsi.py evaluate` — run statistical promotion gate and update `playbooks/current.md`
- `python3 rsi.py report` — print report, discovered tactics, and `raiox-chart` block
- `python3 rsi.py export-wiki` — export compact playbook digest for raiox Wiki / ai-memory
- `python3 rsi.py rollback --to v001 --reason "..."` — revert safely
