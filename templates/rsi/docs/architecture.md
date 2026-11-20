# RSI Loop Architecture on top of raiox

## Why empirical RSI beats static prompts

A static prompt decays as soon as a platform's algorithm, audience taste, or auction dynamics shift. A naive "rewrite my own prompt after every run" agent degrades even faster because it overfits to single-sample noise.

`raiox-rsi` implements **controlled, empirical Recursive Self-Improvement** using five decoupled stages mapped directly onto **raiox** primitives:

```text
  [1. Conversational Niche Spec]  (state/niche.json)
               │
               ▼
  [2. Champion vs Challenger]     (playbooks/current.md + playbooks/vNNN.md)
               │
               ▼
  [3. Multi-Client Execution]     (raiox Routines + Board RAIOX-TASK)
               │
               ▼
  [4. Collector / Simulator]      (state/experiments.jsonl — real CSV/JSON or sim)
               │
               ▼
  [5. Statistical Promotion Gate] (Welch t-stat + min_samples + guardrails + drift)
               │
     ┌─────────┴──────────┐
     ▼                    ▼
  PROMOTE              REJECT
  update current.md    record anti-pattern
  export to raiox Wiki keep champion
```

## Core Safeguards

1. **Minimum sample size (`min_samples`)**: No mutation promotes on 1–2 lucky runs.
2. **Relative lift + confidence (`min_lift_pct` & Welch's t-statistic)**: Prevents random noise from churning the champion playbook.
3. **Hard Guardrails**: Even if a spammy or aggressive variant scores high on engagement, violating a guardrail (e.g. `unfollow_spike_rate`, `spam_report_rate`, `cpa_ratio_vs_target`) triggers an automatic rejection and logs an anti-pattern.
4. **Immutable Version Snapshots + Instant Rollback**: Every version lives in `playbooks/vNNN.md`. `python3 rsi.py rollback --to vNNN --reason "..."` restores any earlier champion cleanly.
5. **Non-Stationary Drift Detector**: Tracks the active champion's rolling score against its promotion baseline. If performance drops past `drift_drop_pct` (e.g. an X algorithm change), it flags drift so the agent re-opens exploration across previously rejected levers.
6. **Multi-Client Compounding**: Every experiment tags `client`. Running across 5–10 clients fills `min_samples` in hours instead of weeks, compounding learnings into one shared playbook.
7. **Prediction Calibration (MAE)**: The agent records `predicted_score` before seeing outcomes; tracking MAE measures whether the agent's internal world model is getting sharper over time.

## Mapping to raiox Primitives

| RSI Component | raiox Primitive |
| --- | --- |
| Niche interview & strategy discussion | Project Bot Chat (`CLAUDE.md` / `AGENTS.md`) |
| Safe CLI execution (status, ingest, evaluate) | `RAIOX-RUN:` + "Run and always allow" |
| Autonomous daily hypothesize & evolve cycles | **Routines** (morning batch + evening eval) |
| Playbook mutation review before landing | **Drafts** (`RAIOX-PROPOSAL:`) + optional **Reviewer Bot** pair |
| Queuing next A/B tests | **Board** (`RAIOX-TASK:`) |
| Visual score evolution across versions | `raiox-chart` JSON block (`python3 rsi.py report`) |
| Sharing latest expertise with other projects/bots | **Wiki** & **ai-memory** (`python3 rsi.py export-wiki`) |
