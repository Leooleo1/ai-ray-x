#!/usr/bin/env python3
"""raiox-rsi: Niche-agnostic Recursive Self-Improvement (RSI) CLI engine.

Designed to run inside a raiox project folder via `RAIOX-RUN: python3 rsi.py ...`
with zero external dependencies.
"""

from __future__ import annotations

import argparse
import json
import random
import sys
from pathlib import Path
from typing import Any

from engine.collector import export_wiki_summary, ingest_file, ingest_x_export, raiox_chart_json, record_experiment
from engine.evaluator import calibration_mae, detect_drift, evaluate_challenger, lever_attribution
from engine.presets import build_custom_niche, get_preset, list_presets
from engine.simulator import propose_next_mutation, simulate_outcome
from engine.store import Store

ROOT = Path(__file__).resolve().parent


def parse_kv(items: list[str] | None, as_float: bool = False) -> dict[str, Any]:
    out: dict[str, Any] = {}
    for item in items or []:
        if "=" not in item:
            raise ValueError(f"Expected key=value, got '{item}'")
        k, v = item.split("=", 1)
        k, v = k.strip(), v.strip()
        out[k] = float(v) if as_float else v
    return out


def cmd_presets(_args: argparse.Namespace) -> None:
    print(json.dumps({"presets": list_presets()}, indent=2, ensure_ascii=False))


def cmd_init(args: argparse.Namespace) -> None:
    store = Store(Path(args.root))
    if args.custom:
        raw = json.loads(Path(args.custom).read_text(encoding="utf-8"))
        niche = build_custom_niche(
            name=raw["name"],
            goal=raw["goal"],
            feedback_window_hours=raw.get("feedback_window_hours", 24),
            metrics=raw["metrics"],
            dimensions=raw["dimensions"],
            guardrails=raw.get("guardrails"),
            initial_rules=raw.get("initial_rules"),
            min_samples=raw.get("min_samples", 5),
            min_lift_pct=raw.get("min_lift_pct", 5.0),
        )
    else:
        niche = get_preset(args.preset)
    v1 = store.init_niche(niche, force=args.force)
    print(
        json.dumps(
            {
                "initialized": niche["id"],
                "name": niche["name"],
                "champion": v1["id"],
                "playbook": str(store.current_playbook),
                "dimensions": niche["dimensions"],
                "recommended_routines": niche.get("recommended_routines", []),
            },
            indent=2,
            ensure_ascii=False,
        )
    )


def cmd_status(args: argparse.Namespace) -> None:
    store = Store(Path(args.root))
    if not store.is_initialized():
        print(
            json.dumps(
                {
                    "initialized": False,
                    "message": "No niche configured yet. Interview the user to pick a preset or custom niche.",
                    "presets": list_presets(),
                },
                indent=2,
                ensure_ascii=False,
            )
        )
        return
    niche = store.load_niche()
    versions_data = store.load_versions()
    exps = store.load_experiments()
    drafts = store.load_drafts()
    champ = store.get_version(versions_data, versions_data["champion"])
    chal = store.get_version(versions_data, versions_data["challenger"]) if versions_data.get("challenger") else None
    clients = sorted({e.get("client", "default") for e in exps})
    print(
        json.dumps(
            {
                "initialized": True,
                "niche": {"id": niche["id"], "name": niche["name"], "goal": niche["goal"], "feedback_window_hours": niche["feedback_window_hours"]},
                "dimensions": niche["dimensions"],
                "discovered_options": store.list_discovered_options(niche),
                "champion": {
                    "id": champ["id"],
                    "strategy": champ["strategy"],
                    "promoted_score": champ.get("promoted_score"),
                    "rules_count": len(champ.get("rules", [])),
                    "anti_patterns_count": len(champ.get("anti_patterns", [])),
                },
                "challenger": (
                    {"id": chal["id"], "hypothesis": chal["hypothesis"], "changes": chal.get("changes", {})}
                    if chal
                    else None
                ),
                "experiments_total": len(exps),
                "drafts_total": len(drafts),
                "clients": clients,
                "prediction_mae": calibration_mae(exps),
                "drift": detect_drift(store),
                "top_levers": {dim: rows[:2] for dim, rows in lever_attribution(exps, niche).items() if rows},
            },
            indent=2,
            ensure_ascii=False,
        )
    )


def cmd_add_option(args: argparse.Namespace) -> None:
    store = Store(Path(args.root))
    added = store.add_option(args.dim, args.option, source=args.source, proposed_by=args.by)
    print(json.dumps(added, indent=2, ensure_ascii=False))


def cmd_propose(args: argparse.Namespace) -> None:
    store = Store(Path(args.root))
    changes = parse_kv(args.change, as_float=False)
    if not changes:
        raise ValueError("Provide at least one --change dimension=value")
    challenger = store.create_challenger(
        args.hypothesis,
        changes,
        added_rules=args.rule,
        source=args.source,
        proposed_by=args.by,
    )
    print(
        json.dumps(
            {
                "challenger": challenger["id"],
                "parent": challenger["parent"],
                "hypothesis": challenger["hypothesis"],
                "changes": challenger["changes"],
                "strategy": challenger["strategy"],
                "new_options": challenger.get("new_options", []),
            },
            indent=2,
            ensure_ascii=False,
        )
    )


def cmd_draft(args: argparse.Namespace) -> None:
    store = Store(Path(args.root))
    variant = parse_kv(args.variant, as_float=False) if args.variant else None
    draft = store.register_draft(
        text=args.text,
        client=args.client,
        version=args.version,
        post_id=args.post_id,
        variant=variant,
        predicted_score=args.predicted,
    )
    print(json.dumps(draft, indent=2, ensure_ascii=False))


def cmd_record(args: argparse.Namespace) -> None:
    store = Store(Path(args.root))
    metrics = parse_kv(args.metric, as_float=True)
    if not metrics:
        raise ValueError("Provide at least one --metric name=value")
    variant = parse_kv(args.variant, as_float=False) if args.variant else None
    rec = record_experiment(
        store,
        version=args.version,
        client=args.client,
        variant=variant,
        metrics=metrics,
        artifact=args.artifact or "",
        hypothesis=args.hypothesis or "",
        predicted_score=args.predicted,
    )
    print(json.dumps(rec, indent=2, ensure_ascii=False))


def cmd_ingest(args: argparse.Namespace) -> None:
    store = Store(Path(args.root))
    rows = ingest_file(store, Path(args.file))
    print(json.dumps({"ingested": len(rows), "ids": [r["id"] for r in rows]}, indent=2, ensure_ascii=False))


def cmd_ingest_x(args: argparse.Namespace) -> None:
    store = Store(Path(args.root))
    summary = ingest_x_export(store, Path(args.file))
    print(json.dumps(summary, indent=2, ensure_ascii=False))


def cmd_simulate(args: argparse.Namespace) -> None:
    store = Store(Path(args.root))
    niche = store.load_niche()
    versions_data = store.load_versions()
    champ_id = versions_data["champion"]
    chal_id = versions_data.get("challenger")
    arms = [champ_id, chal_id] if chal_id else [champ_id]
    clients = [c.strip() for c in args.clients.split(",") if c.strip()] or ["client-a"]
    rng = random.Random(args.seed)
    recorded = []
    for i in range(args.runs):
        for arm in arms:
            ver = store.get_version(versions_data, arm)
            client = clients[i % len(clients)]
            sim = simulate_outcome(niche, ver["strategy"], phase=args.phase, seed=rng.randint(1, 10_000_000))
            predicted = round(max(10.0, min(95.0, (ver.get("promoted_score") or 50.0) + rng.gauss(0.0, 4.0))), 2)
            rec = record_experiment(
                store,
                version=arm,
                client=client,
                variant=ver["strategy"],
                metrics=sim["metrics"],
                artifact=f"sim-phase{args.phase}-{arm}-{i + 1}",
                hypothesis=ver["hypothesis"],
                predicted_score=predicted,
            )
            recorded.append(rec)
    print(
        json.dumps(
            {
                "simulated_runs": len(recorded),
                "arms": arms,
                "phase": args.phase,
                "mean_by_arm": {
                    arm: round(
                        sum(r["actual_score"] for r in recorded if r["version"] == arm)
                        / max(1, sum(1 for r in recorded if r["version"] == arm)),
                        2,
                    )
                    for arm in arms
                },
            },
            indent=2,
            ensure_ascii=False,
        )
    )


def cmd_evaluate(args: argparse.Namespace) -> None:
    store = Store(Path(args.root))
    result = evaluate_challenger(store, apply=not args.dry_run)
    print(json.dumps(result, indent=2, ensure_ascii=False))


def cmd_rollback(args: argparse.Namespace) -> None:
    store = Store(Path(args.root))
    restored = store.rollback(args.to, args.reason)
    print(json.dumps({"rolled_back_to": restored["id"], "strategy": restored["strategy"]}, indent=2, ensure_ascii=False))


def cmd_report(args: argparse.Namespace) -> None:
    store = Store(Path(args.root))
    niche = store.load_niche()
    versions_data = store.load_versions()
    exps = store.load_experiments()
    champ = store.get_version(versions_data, versions_data["champion"])
    drift = detect_drift(store)
    mae = calibration_mae(exps)
    discovered = store.list_discovered_options(niche)

    print(f"# RSI Report — {niche['name']}")
    print(f"- Champion: {champ['id']} (score={champ.get('promoted_score')}, experiments={len(exps)})")
    print(f"- Active strategy: {champ['strategy']}")
    print(f"- Prediction MAE: {mae if mae is not None else 'n/a'}")
    print(f"- Drift alert: {'YES (' + str(drift['drop_pct']) + '% drop)' if drift['drifting'] else 'no'}")
    if discovered:
        print("- Discovered tactics:")
        for item in discovered:
            print(
                f"  - {item['dimension']}={item['option']} [{item['status']}] "
                f"(by {item['proposed_by']} on {item['added_at'][:10]}: {item['source']})"
            )
    print("\n```raiox-chart")
    print(raiox_chart_json(store))
    print("```")


def cmd_export_wiki(args: argparse.Namespace) -> None:
    store = Store(Path(args.root))
    print(export_wiki_summary(store))


def cmd_demo(args: argparse.Namespace) -> None:
    """Runs an end-to-end multi-cycle RSI simulation, including a mid-run meta shift."""
    store = Store(Path(args.root))
    niche = get_preset(args.preset)
    store.init_niche(niche, force=True)
    rng = random.Random(args.seed)
    clients = ["client-alpha", "client-beta", "client-gamma"]
    tried_rejections: set[tuple[str, str]] = set()
    timeline: list[dict[str, Any]] = []

    for i in range(niche["min_samples"]):
        sim = simulate_outcome(niche, niche["initial_strategy"], phase=0, seed=rng.randint(1, 10_000_000))
        record_experiment(
            store,
            version="v001",
            client=clients[i % len(clients)],
            variant=niche["initial_strategy"],
            metrics=sim["metrics"],
            artifact=f"baseline-{i + 1}",
            predicted_score=50.0,
        )

    for cycle in range(1, args.cycles + 1):
        phase = 1 if args.shift_at and cycle >= args.shift_at else 0
        if args.shift_at and cycle == args.shift_at:
            tried_rejections.clear()

        versions_data = store.load_versions()
        champ = store.get_version(versions_data, versions_data["champion"])
        exps = store.load_experiments()
        attr = lever_attribution(exps, niche)
        changes, hypothesis = propose_next_mutation(niche, champ["strategy"], tried_rejections, attr, rng)
        chal = store.create_challenger(hypothesis, changes)

        runs = niche["min_samples"]
        for i in range(runs):
            for arm in (champ["id"], chal["id"]):
                ver = store.get_version(store.load_versions(), arm)
                sim = simulate_outcome(niche, ver["strategy"], phase=phase, seed=rng.randint(1, 10_000_000))
                record_experiment(
                    store,
                    version=arm,
                    client=clients[i % len(clients)],
                    variant=ver["strategy"],
                    metrics=sim["metrics"],
                    artifact=f"c{cycle}-{arm}-{i + 1}",
                    predicted_score=ver.get("promoted_score") or 55.0,
                )

        ev = evaluate_challenger(store, apply=True)
        if ev["decision"] == "reject":
            for dim, val in changes.items():
                tried_rejections.add((dim, val))
        timeline.append(
            {
                "cycle": cycle,
                "phase": phase,
                "tested": changes,
                "challenger": chal["id"],
                "decision": ev["decision"],
                "champion_mean": ev["champion_mean"],
                "challenger_mean": ev["challenger_mean"],
                "lift_pct": ev["lift_pct"],
            }
        )

    final_versions = store.load_versions()
    final_champ = store.get_version(final_versions, final_versions["champion"])
    print(
        json.dumps(
            {
                "preset": args.preset,
                "cycles": args.cycles,
                "shift_at": args.shift_at,
                "final_champion": final_champ["id"],
                "final_strategy": final_champ["strategy"],
                "final_score": final_champ.get("promoted_score"),
                "timeline": timeline,
                "chart": json.loads(raiox_chart_json(store)),
            },
            indent=2,
            ensure_ascii=False,
        )
    )


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="rsi.py", description="Niche-agnostic RSI loop engine for raiox.")
    parser.add_argument("--root", default=str(ROOT), help="Project root folder (default: script directory)")
    sub = parser.add_subparsers(dest="command", required=True)

    sub.add_parser("presets", help="List built-in fast-feedback niche presets").set_defaults(func=cmd_presets)

    p_init = sub.add_parser("init", help="Initialize a niche from a preset or custom JSON file")
    group = p_init.add_mutually_exclusive_group(required=True)
    group.add_argument("--preset", help="Preset id (e.g. x-viral, paid-ads, cold-outbound)")
    group.add_argument("--custom", help="Path to custom niche JSON definition")
    p_init.add_argument("--force", action="store_true", help="Reset existing state and playbooks")
    p_init.set_defaults(func=cmd_init)

    sub.add_parser("status", help="Show active niche, dimensions, discovered options, champion/challenger, and drift").set_defaults(func=cmd_status)

    p_add = sub.add_parser("add-option", help="Register a new candidate tactic in a dimension with provenance")
    p_add.add_argument("--dim", required=True, help="Existing dimension name (from status)")
    p_add.add_argument("--option", required=True, help="New option slug (lowercase, e.g. native_poll)")
    p_add.add_argument("--source", required=True, help="Where it came from: URL or concrete observation")
    p_add.add_argument("--by", default="agent", help="Who proposed it (default: agent)")
    p_add.set_defaults(func=cmd_add_option)

    p_prop = sub.add_parser("propose", help="Create a challenger playbook version to A/B test")
    p_prop.add_argument("--hypothesis", required=True, help="Why this mutation should beat the champion")
    p_prop.add_argument("--change", action="append", required=True, help="Lever change as dimension=option (repeatable)")
    p_prop.add_argument("--rule", action="append", help="Optional rule to add to the challenger playbook")
    p_prop.add_argument("--source", help="Provenance URL or observation when introducing a brand-new option")
    p_prop.add_argument("--by", default="agent", help="Who proposed the new option (default: agent)")
    p_prop.set_defaults(func=cmd_propose)

    p_draft = sub.add_parser("draft", help="Register a post draft locally (never publishes) for later X export matching")
    p_draft.add_argument("--client", default="default", help="Client or account identifier")
    p_draft.add_argument("--text", required=True, help="Post text written by the agent")
    p_draft.add_argument("--version", help="Playbook version id (default: current champion)")
    p_draft.add_argument("--post-id", help="Optional X post id or status URL once published by the person")
    p_draft.add_argument("--variant", action="append", help="Optional lever override as dim=val")
    p_draft.add_argument("--predicted", type=float, help="Predicted score 0-100 before publishing")
    p_draft.set_defaults(func=cmd_draft)

    p_rec = sub.add_parser("record", help="Record one real-world experiment outcome")
    p_rec.add_argument("--metric", action="append", required=True, help="Observed metric as name=value (repeatable)")
    p_rec.add_argument("--variant", action="append", help="Override lever as dim=val (repeatable)")
    p_rec.add_argument("--version", help="Playbook version id (default: current champion)")
    p_rec.add_argument("--client", default="default", help="Client or account identifier")
    p_rec.add_argument("--artifact", help="Short reference to the post/ad/email tested")
    p_rec.add_argument("--hypothesis", help="Optional hypothesis override")
    p_rec.add_argument("--predicted", type=float, help="Predicted score 0-100 before publishing")
    p_rec.set_defaults(func=cmd_record)

    p_ing = sub.add_parser("ingest", help="Batch-import real outcomes (rates) from a CSV or JSON file")
    p_ing.add_argument("file", help="Path to .csv or .json outcomes file")
    p_ing.set_defaults(func=cmd_ingest)

    p_ingx = sub.add_parser("ingest-x", help="Import a free X Analytics CSV export, matching posts to registered drafts")
    p_ingx.add_argument("file", help="Path to the X Analytics CSV export")
    p_ingx.set_defaults(func=cmd_ingest_x)

    p_sim = sub.add_parser("simulate", help="Simulate balanced A/B runs across clients")
    p_sim.add_argument("--runs", type=int, default=5, help="Runs per arm")
    p_sim.add_argument("--phase", type=int, default=0, help="Environment meta phase (0=initial, 1=shifted)")
    p_sim.add_argument("--clients", default="client-a,client-b", help="Comma-separated client names")
    p_sim.add_argument("--seed", type=int, default=None, help="Optional RNG seed")
    p_sim.set_defaults(func=cmd_simulate)

    p_eval = sub.add_parser("evaluate", help="Evaluate active challenger vs champion and apply promotion gate")
    p_eval.add_argument("--dry-run", action="store_true", help="Compute decision without mutating state")
    p_eval.set_defaults(func=cmd_evaluate)

    p_rb = sub.add_parser("rollback", help="Roll back champion playbook to an earlier version")
    p_rb.add_argument("--to", required=True, help="Target version id (e.g. v001)")
    p_rb.add_argument("--reason", required=True, help="Why we are rolling back")
    p_rb.set_defaults(func=cmd_rollback)

    sub.add_parser("report", help="Print human report, discovered tactics, and raiox-chart block").set_defaults(func=cmd_report)
    sub.add_parser("export-wiki", help="Print compact playbook summary for raiox Wiki / ai-memory").set_defaults(func=cmd_export_wiki)

    p_demo = sub.add_parser("demo", help="Run a multi-cycle end-to-end RSI demo with a meta shift")
    p_demo.add_argument("--preset", default="x-viral", help="Preset id")
    p_demo.add_argument("--cycles", type=int, default=6, help="Number of RSI evolution cycles")
    p_demo.add_argument("--shift-at", type=int, default=4, help="Cycle number where algorithm meta shifts (0 to disable)")
    p_demo.add_argument("--seed", type=int, default=7, help="Deterministic seed")
    p_demo.set_defaults(func=cmd_demo)

    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    try:
        args.func(args)
        return 0
    except Exception as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
