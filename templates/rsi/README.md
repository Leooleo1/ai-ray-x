# raiox-rsi — Niche-Agnostic RSI Agent Engine for raiox

A self-improving agent workspace built on top of **raiox** (bots, routines, task board, reviewer pairs, Wiki, and ai-memory).

The person chooses the niche in conversation with the bot — e.g. **X / Twitter viral growth**, **paid ad creatives**, **cold outbound**, or any **custom fast-feedback niche** — and the bot runs an empirical closed loop that measures results, rejects hype/spam, promotes only verified improvements, detects algorithm shifts, and compounds learnings across clients.

## Quick start in raiox

1. In **raiox**, open the bot created in this folder (Claude Code, Codex, Cursor, Antigravity, or opencode).
2. Tell the bot what niche you want to dominate (e.g. *"Quero um agente de crescimento no X para meus clientes"* or *"Help me pick the fastest-feedback niche"*).
3. With **Runs without asking** enabled in raiox, the RSI engine commands (`python3 rsi.py ...`, or `python` / `py -3` on Windows) run automatically inside this folder.
4. Add the two recommended **Routines** in the bot's settings (morning experiment batch + evening evaluate & evolve), and optionally assign a second bot on this folder as **Reviewer** to critique every playbook mutation before it ships.

## Run an immediate end-to-end RSI demo (with algorithm meta-shift)

```sh
python3 rsi.py demo --preset x-viral --cycles 8 --shift-at 5 --seed 7
python3 rsi.py report
```

## Run tests

```sh
python3 -m unittest discover -s tests -v
```

See [docs/architecture.md](docs/architecture.md) for the full RSI loop architecture and raiox integration design.
