"""Composite scoring, guardrail enforcement, statistical promotion gate, lever attribution, and drift detection."""

from __future__ import annotations

import math
from collections import defaultdict
from typing import Any

from engine.store import Store, utc_now_iso


def compute_score(niche: dict[str, Any], metrics: dict[str, float]) -> float:
    """Computes a 0-100 weighted composite score from raw observed metrics."""
    total = 0.0
    weight_sum = 0.0
    for name, cfg in niche["metrics"].items():
        weight = float(cfg["weight"])
        scale = max(float(cfg.get("scale", 1.0)), 1e-9)
        raw = float(metrics.get(name, 0.0))
        if cfg.get("higher_is_better", True):
            norm = max(0.0, min(1.0, raw / scale))
        else:
            norm = max(0.0, min(1.0, 1.0 - (raw / scale)))
        total += norm * weight
        weight_sum += weight
    if weight_sum <= 0:
        return 0.0
    return round((total / weight_sum) * 100.0, 2)


def check_guardrails(niche: dict[str, Any], metrics: dict[str, float]) -> list[str]:
    """Returns a list of violated guardrail descriptions (empty when safe)."""
    violations: list[str] = []
    for name, cfg in niche.get("guardrails", {}).items():
        if name not in metrics:
            continue
        val = float(metrics[name])
        if "max" in cfg and val > float(cfg["max"]):
            violations.append(f"{name}={val:.2f} > max {cfg['max']}")
        if "min" in cfg and val < float(cfg["min"]):
            violations.append(f"{name}={val:.2f} < min {cfg['min']}")
    return violations


def _mean_std(values: list[float]) -> tuple[float, float]:
    n = len(values)
    if n == 0:
        return 0.0, 0.0
    mean = sum(values) / n
    if n == 1:
        return mean, 0.0
    var = sum((x - mean) ** 2 for x in values) / (n - 1)
    return mean, math.sqrt(var)


def welch_t(a: list[float], b: list[float]) -> float:
    """Welch's t-statistic comparing mean(a) - mean(b)."""
    if len(a) < 2 or len(b) < 2:
        return 0.0
    ma, sa = _mean_std(a)
    mb, sb = _mean_std(b)
    se = math.sqrt((sa * sa) / len(a) + (sb * sb) / len(b))
    if se <= 1e-9:
        return 0.0 if abs(ma - mb) <= 1e-9 else (10.0 if ma > mb else -10.0)
    return (ma - mb) / se


def lever_attribution(experiments: list[dict[str, Any]], niche: dict[str, Any], recent_window: int = 40) -> dict[str, list[dict[str, Any]]]:
    """Ranks each dimension option by observed average score in recent experiments."""
    recent = [e for e in experiments if e.get("actual_score") is not None][-recent_window:]
    buckets: dict[str, dict[str, list[float]]] = defaultdict(lambda: defaultdict(list))
    for exp in recent:
        score = float(exp["actual_score"])
        for dim, opt in exp.get("variant", {}).items():
            if dim in niche["dimensions"]:
                buckets[dim][opt].append(score)

    ranked: dict[str, list[dict[str, Any]]] = {}
    for dim in niche["dimensions"]:
        rows = []
        for opt, scores in buckets.get(dim, {}).items():
            mean, std = _mean_std(scores)
            rows.append({"option": opt, "mean_score": round(mean, 2), "std": round(std, 2), "n": len(scores)})
        rows.sort(key=lambda r: (r["mean_score"], r["n"]), reverse=True)
        ranked[dim] = rows
    return ranked


def calibration_mae(experiments: list[dict[str, Any]], window: int = 20) -> float | None:
    """Mean absolute error between predicted_score and actual_score (measures world-model accuracy)."""
    pairs = [
        abs(float(e["predicted_score"]) - float(e["actual_score"]))
        for e in experiments
        if e.get("predicted_score") is not None and e.get("actual_score") is not None
    ][-window:]
    if not pairs:
        return None
    return round(sum(pairs) / len(pairs), 2)


def detect_drift(store: Store, recent_n: int = 6) -> dict[str, Any]:
    """Detects whether the active champion has degraded significantly vs its promotion baseline."""
    niche = store.load_niche()
    versions_data = store.load_versions()
    champ = store.get_version(versions_data, versions_data["champion"])
    baseline = champ.get("promoted_score")
    exps = [
        float(e["actual_score"])
        for e in store.load_experiments()
        if e.get("version") == champ["id"] and e.get("actual_score") is not None
    ]
    if baseline is None or len(exps) < recent_n:
        return {"drifting": False, "baseline": baseline, "recent_mean": None, "drop_pct": 0.0}
    recent_mean, _ = _mean_std(exps[-recent_n:])
    drop_pct = ((baseline - recent_mean) / max(baseline, 1e-6)) * 100.0
    threshold = float(niche.get("drift_drop_pct", 15.0))
    return {
        "drifting": drop_pct >= threshold,
        "baseline": round(baseline, 2),
        "recent_mean": round(recent_mean, 2),
        "drop_pct": round(drop_pct, 2),
        "threshold_pct": threshold,
    }


def evaluate_challenger(store: Store, apply: bool = True) -> dict[str, Any]:
    """Evaluates active challenger vs champion and optionally promotes or rejects it."""
    niche = store.load_niche()
    versions_data = store.load_versions()
    champ_id = versions_data["champion"]
    chal_id = versions_data.get("challenger")
    champ = store.get_version(versions_data, champ_id)
    exps = store.load_experiments()

    if not chal_id:
        champ_scores = [float(e["actual_score"]) for e in exps if e["version"] == champ_id and e.get("actual_score") is not None]
        mean_c, _ = _mean_std(champ_scores)
        return {
            "decision": "no_challenger",
            "champion": champ_id,
            "champion_n": len(champ_scores),
            "champion_mean": round(mean_c, 2) if champ_scores else None,
            "drift": detect_drift(store),
        }

    chal = store.get_version(versions_data, chal_id)
    min_samples = int(niche.get("min_samples", 5))
    min_lift_pct = float(niche.get("min_lift_pct", 5.0))

    # Compare against champion experiments collected since challenger was created (or latest window if none concurrent).
    chal_exps = [e for e in exps if e["version"] == chal_id and e.get("actual_score") is not None]
    all_champ_exps = [e for e in exps if e["version"] == champ_id and e.get("actual_score") is not None]
    concurrent_champ = [e for e in all_champ_exps if e.get("ts", "") >= chal["created_at"]]
    champ_exps = concurrent_champ if len(concurrent_champ) >= min_samples else all_champ_exps[-max(min_samples * 2, len(chal_exps)) :]

    chal_scores = [float(e["actual_score"]) for e in chal_exps]
    champ_scores = [float(e["actual_score"]) for e in champ_exps]
    chal_violations = [v for e in chal_exps for v in e.get("guardrail_violations", [])]

    champ_mean, _ = _mean_std(champ_scores)
    chal_mean, _ = _mean_std(chal_scores)
    lift_pct = ((chal_mean - champ_mean) / max(champ_mean, 1.0)) * 100.0 if champ_scores and chal_scores else 0.0
    t_stat = welch_t(chal_scores, champ_scores)

    # Guardrail veto is immediate once violations exceed 20% of challenger runs (or >= 2 runs).
    if len(chal_violations) >= 2 or (chal_exps and len(chal_violations) / len(chal_exps) > 0.2):
        decision = "reject"
        reason = f"Guardrail violated in challenger ({'; '.join(chal_violations[:2])})."
    elif len(chal_scores) < min_samples or len(champ_scores) < min_samples:
        decision = "keep_testing"
        reason = f"Need at least {min_samples} samples per arm (champion={len(champ_scores)}, challenger={len(chal_scores)})."
    elif lift_pct >= min_lift_pct and t_stat >= 1.25:
        decision = "promote"
        reason = (
            f"Challenger {chal_id} beat {champ_id} by +{lift_pct:.1f}% "
            f"({chal_mean:.2f} vs {champ_mean:.2f}, t={t_stat:.2f}, n={len(chal_scores)})."
        )
    else:
        decision = "reject"
        reason = (
            f"Challenger {chal_id} did not clear +{min_lift_pct:.1f}% lift with confidence "
            f"({chal_mean:.2f} vs {champ_mean:.2f}, lift={lift_pct:+.1f}%, t={t_stat:.2f})."
        )

    result = {
        "decision": decision,
        "reason": reason,
        "champion": champ_id,
        "challenger": chal_id,
        "champion_n": len(champ_scores),
        "challenger_n": len(chal_scores),
        "champion_mean": round(champ_mean, 2),
        "challenger_mean": round(chal_mean, 2),
        "lift_pct": round(lift_pct, 2),
        "t_stat": round(t_stat, 2),
        "guardrail_violations": len(chal_violations),
    }

    if not apply or decision == "keep_testing":
        return result

    if decision == "promote":
        champ["status"] = "retired"
        chal["status"] = "champion"
        chal["promoted_score"] = round(chal_mean, 2)
        chal["promoted_samples"] = len(chal_scores)
        change_desc = ", ".join(f"{k}={v}" for k, v in chal.get("changes", {}).items())
        rule = f"[{chal_id}] Prefer {change_desc} (+{lift_pct:.1f}% over {champ_id}, n={len(chal_scores)})."
        if change_desc and rule not in chal["rules"]:
            chal["rules"].append(rule)
        versions_data["champion"] = chal_id
        versions_data["challenger"] = None
        store.save_versions(versions_data)
        store.update_option_status(chal.get("changes", {}), "proven")
        store.write_playbook_snapshot(store.load_niche(), chal, make_current=True)
        store.append_lesson(
            {
                "ts": utc_now_iso(),
                "kind": "promotion",
                "from_version": champ_id,
                "to_version": chal_id,
                "changes": chal.get("changes", {}),
                "lift_pct": round(lift_pct, 2),
                "score": round(chal_mean, 2),
                "reason": reason,
            }
        )
    elif decision == "reject":
        chal["status"] = "rejected"
        change_desc = ", ".join(f"{k}={v}" for k, v in chal.get("changes", {}).items())
        anti = f"[{chal_id}] Avoid {change_desc}: {reason}"
        if change_desc and anti not in champ["anti_patterns"]:
            champ["anti_patterns"].append(anti)
        # Update champion's baseline score if not set yet.
        if champ.get("promoted_score") is None and champ_scores:
            champ["promoted_score"] = round(champ_mean, 2)
            champ["promoted_samples"] = len(champ_scores)
        versions_data["challenger"] = None
        store.save_versions(versions_data)
        store.update_option_status(chal.get("changes", {}), "rejected")
        store.write_playbook_snapshot(store.load_niche(), champ, make_current=True)
        store.append_lesson(
            {
                "ts": utc_now_iso(),
                "kind": "rejection",
                "champion": champ_id,
                "challenger": chal_id,
                "changes": chal.get("changes", {}),
                "lift_pct": round(lift_pct, 2),
                "reason": reason,
            }
        )

    return result
