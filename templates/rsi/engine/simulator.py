"""Realistic non-stationary niche simulator (supports algorithm shifts & multi-client noise).

Used to validate, benchmark, and demonstrate the RSI loop end-to-end before
connecting live external collectors.
"""

from __future__ import annotations

import hashlib
import random
from typing import Any

from engine.evaluator import check_guardrails, compute_score

# True latent effectiveness profiles (phase 0 = initial meta, phase 1 = shifted meta).
SIM_PROFILES: dict[str, list[dict[str, dict[str, float]]]] = {
    "x-viral": [
        {
            "hook_style": {
                "specific_number_proof": 0.92,
                "contrarian_claim": 0.86,
                "visual_before_after": 0.78,
                "story_in_media_res": 0.72,
                "generic_tip": 0.35,
            },
            "format": {
                "annotated_screenshot": 0.90,
                "short_punch_with_visual": 0.85,
                "micro_thread_3_posts": 0.76,
                "single_long_post": 0.45,
                "link_drop": 0.22,
            },
            "cta_type": {
                "bookmark_trigger": 0.88,
                "open_question": 0.82,
                "no_cta": 0.55,
                "reply_keyword": 0.62,
                "hard_sell": 0.25,
            },
            "posting_slot": {
                "morning_peak": 0.88,
                "evening_deep": 0.80,
                "midday_scroll": 0.58,
                "late_night": 0.38,
            },
        },
        # Phase 1 (meta shift): algorithm rewards visual before/after + open questions, penalizes reply_keyword bait.
        {
            "hook_style": {
                "visual_before_after": 0.95,
                "story_in_media_res": 0.88,
                "specific_number_proof": 0.62,
                "contrarian_claim": 0.50,
                "generic_tip": 0.30,
            },
            "format": {
                "short_punch_with_visual": 0.94,
                "micro_thread_3_posts": 0.85,
                "annotated_screenshot": 0.65,
                "single_long_post": 0.52,
                "link_drop": 0.18,
            },
            "cta_type": {
                "open_question": 0.92,
                "no_cta": 0.74,
                "bookmark_trigger": 0.58,
                "reply_keyword": 0.22,
                "hard_sell": 0.20,
            },
            "posting_slot": {
                "evening_deep": 0.90,
                "morning_peak": 0.76,
                "midday_scroll": 0.55,
                "late_night": 0.44,
            },
        },
    ],
    "paid-ads": [
        {
            "hook_3s": {"unexpected_demo": 0.90, "ugc_confession": 0.84, "side_by_side_proof": 0.80, "pain_callout": 0.70, "brand_logo_intro": 0.25},
            "angle": {"hidden_cost": 0.88, "social_proof_case": 0.82, "time_saved": 0.74, "feature_list": 0.35},
            "visual_style": {"native_ugc_handheld": 0.90, "screen_recording_annotated": 0.80, "bold_static_comparison": 0.72, "polished_studio": 0.40},
            "offer_frame": {"instant_audit": 0.88, "risk_free_trial": 0.84, "direct_discount": 0.66, "generic_learn_more": 0.32},
        },
        {
            "hook_3s": {"side_by_side_proof": 0.94, "pain_callout": 0.86, "unexpected_demo": 0.68, "ugc_confession": 0.54, "brand_logo_intro": 0.20},
            "angle": {"social_proof_case": 0.92, "time_saved": 0.84, "hidden_cost": 0.60, "feature_list": 0.32},
            "visual_style": {"bold_static_comparison": 0.90, "screen_recording_annotated": 0.86, "native_ugc_handheld": 0.62, "polished_studio": 0.38},
            "offer_frame": {"risk_free_trial": 0.90, "instant_audit": 0.76, "direct_discount": 0.62, "generic_learn_more": 0.28},
        },
    ],
    "cold-outbound": [
        {
            "subject_pattern": {"two_word_internal": 0.90, "trigger_event": 0.86, "peer_question": 0.75, "salesy_pitch": 0.22},
            "opener": {"specific_signal_observation": 0.92, "relevant_micro_audit": 0.86, "mutual_context": 0.76, "generic_flattery": 0.25},
            "proof_type": {"peer_metric_one_liner": 0.88, "loom_teardown_offer": 0.82, "no_proof": 0.50, "long_case_study": 0.30},
            "cta_friction": {"soft_interest_check": 0.90, "send_asset_permission": 0.84, "calendar_link_cold": 0.28},
        },
        {
            "subject_pattern": {"trigger_event": 0.94, "peer_question": 0.84, "two_word_internal": 0.62, "salesy_pitch": 0.18},
            "opener": {"relevant_micro_audit": 0.94, "specific_signal_observation": 0.80, "mutual_context": 0.72, "generic_flattery": 0.20},
            "proof_type": {"loom_teardown_offer": 0.92, "peer_metric_one_liner": 0.78, "no_proof": 0.45, "long_case_study": 0.25},
            "cta_friction": {"send_asset_permission": 0.92, "soft_interest_check": 0.80, "calendar_link_cold": 0.24},
        },
    ],
}


def _unknown_option_quality(niche_id: str, phase: int, dim: str, opt: str) -> float:
    """Deterministic latent quality in [0.35, 0.95] seeded by (niche, phase, dim, option)."""
    digest = hashlib.sha256(f"{niche_id}:{phase}:{dim}:{opt}".encode("utf-8")).digest()
    unit = int.from_bytes(digest[:8], "big") / float(1 << 64)
    return 0.35 + unit * 0.60


def _latent_quality(niche: dict[str, Any], variant: dict[str, str], phase: int, rng: random.Random) -> float:
    niche_id = niche["id"]
    profiles = SIM_PROFILES.get(niche_id)
    if profiles:
        table = profiles[phase % len(profiles)]
        scores = []
        for dim, opt in variant.items():
            known = table.get(dim, {}).get(opt)
            scores.append(known if known is not None else _unknown_option_quality(niche_id, phase, dim, opt))
        base = sum(scores) / max(len(scores), 1)
    else:
        scores = [_unknown_option_quality(niche_id, phase, dim, opt) for dim, opt in variant.items()]
        base = sum(scores) / max(len(scores), 1)
    noisy = base + rng.gauss(0.0, 0.055)
    return max(0.05, min(0.99, noisy))


def simulate_outcome(
    niche: dict[str, Any],
    variant: dict[str, str],
    phase: int = 0,
    seed: int | None = None,
) -> dict[str, Any]:
    """Generates realistic observed metrics, composite score, and guardrail checks for a variant."""
    rng = random.Random(seed)
    quality = _latent_quality(niche, variant, phase, rng)
    metrics: dict[str, float] = {}
    for name, cfg in niche["metrics"].items():
        scale = float(cfg.get("scale", 10.0))
        jitter = rng.gauss(0.0, 0.05)
        level = max(0.02, min(1.05, quality + jitter))
        if cfg.get("higher_is_better", True):
            metrics[name] = round(level * scale, 3)
        else:
            metrics[name] = round((1.0 - level) * scale, 3)

    # Guardrail metrics: spammy/hard-sell choices raise violation risk realistically.
    risky_tokens = {"hard_sell", "salesy_pitch", "link_drop", "calendar_link_cold"}
    risk_hits = sum(1 for v in variant.values() if v in risky_tokens)
    for gname, gcfg in niche.get("guardrails", {}).items():
        if "max" in gcfg:
            ceiling = float(gcfg["max"])
            base_val = ceiling * (0.25 + 0.55 * risk_hits) + rng.uniform(0.0, ceiling * 0.25)
            metrics[gname] = round(max(0.0, base_val), 3)
        elif "min" in gcfg:
            floor = float(gcfg["min"])
            metrics[gname] = round(floor * (1.2 - 0.3 * risk_hits), 3)

    score = compute_score(niche, metrics)
    violations = check_guardrails(niche, metrics)
    return {
        "metrics": metrics,
        "score": score,
        "guardrail_violations": violations,
    }


def propose_next_mutation(
    niche: dict[str, Any],
    champion_strategy: dict[str, str],
    tried_rejections: set[tuple[str, str]],
    attribution: dict[str, list[dict[str, Any]]],
    rng: random.Random,
) -> tuple[dict[str, str], str]:
    """Selects a single high-leverage dimension mutation to test as the next challenger."""
    dims = list(niche["dimensions"].keys())
    rng.shuffle(dims)
    for dim in dims:
        current = champion_strategy.get(dim)
        options = [opt for opt in niche["dimensions"][dim] if opt != current]
        # Prioritize options with strong recent attribution or untried options.
        attr_map = {row["option"]: row["mean_score"] for row in attribution.get(dim, [])}
        untried = [opt for opt in options if (dim, opt) not in tried_rejections and opt not in attr_map]
        if untried:
            chosen = rng.choice(untried)
            return {dim: chosen}, f"Explore untried lever {dim}={chosen} (was {current})."
        promising = [opt for opt in options if (dim, opt) not in tried_rejections]
        if promising:
            promising.sort(key=lambda o: attr_map.get(o, 50.0), reverse=True)
            chosen = promising[0]
            return {dim: chosen}, f"Exploit top-attributed lever {dim}={chosen} (was {current})."

    # If everything was rejected earlier (e.g., after a meta shift), re-explore fresh.
    dim = dims[0]
    current = champion_strategy.get(dim)
    choices = [opt for opt in niche["dimensions"][dim] if opt != current]
    chosen = rng.choice(choices)
    return {dim: chosen}, f"Re-explore {dim}={chosen} to test for meta shift."
