"""Built-in fast-feedback niche presets and custom niche builder.

Each preset defines:
- north-star metric weights (normalized to a 0-100 composite score)
- guardrail ceilings/floors that veto unsafe or spammy variants
- experimental dimensions (levers) the agent mutates across playbook versions
- an initial v1 playbook hypothesis and recommended raiox routines
"""

from __future__ import annotations

from copy import deepcopy
from typing import Any

PRESETS: dict[str, dict[str, Any]] = {
    "x-viral": {
        "id": "x-viral",
        "name": "X / Twitter Viral Growth",
        "goal": "Maximize high-intent reach, bookmarks, replies, and follower conversion per post across client accounts.",
        "feedback_window_hours": 12,
        "min_samples": 5,
        "min_lift_pct": 5.0,
        "drift_drop_pct": 15.0,
        "metrics": {
            "bookmark_rate": {"weight": 0.35, "scale": 4.0, "higher_is_better": True, "unit": "%"},
            "reply_rate": {"weight": 0.25, "scale": 3.0, "higher_is_better": True, "unit": "%"},
            "profile_click_rate": {"weight": 0.25, "scale": 5.0, "higher_is_better": True, "unit": "%"},
            "follow_per_1k": {"weight": 0.15, "scale": 8.0, "higher_is_better": True, "unit": "per_1k"},
        },
        "guardrails": {
            "unfollow_spike_rate": {"max": 1.5, "unit": "%"},
            "spam_report_rate": {"max": 0.2, "unit": "%"},
        },
        "dimensions": {
            "hook_style": [
                "contrarian_claim",
                "specific_number_proof",
                "visual_before_after",
                "story_in_media_res",
                "generic_tip",
            ],
            "format": [
                "short_punch_with_visual",
                "micro_thread_3_posts",
                "single_long_post",
                "annotated_screenshot",
                "link_drop",
            ],
            "cta_type": [
                "open_question",
                "bookmark_trigger",
                "reply_keyword",
                "no_cta",
                "hard_sell",
            ],
            "posting_slot": [
                "morning_peak",
                "midday_scroll",
                "evening_deep",
                "late_night",
            ],
        },
        "initial_strategy": {
            "hook_style": "generic_tip",
            "format": "single_long_post",
            "cta_type": "no_cta",
            "posting_slot": "midday_scroll",
        },
        "initial_rules": [
            "Open with one clear idea in the first line; never bury the point below the fold.",
            "Keep one primary visual or concrete number per post when claiming a result.",
            "Avoid external links in the main post; place links in a follow-up reply if needed.",
            "Never use engagement-bait spam that triggers mutes or unfollows.",
        ],
        "recommended_routines": [
            {"label": "Morning experiment batch", "hour": 9, "minute": 0, "prompt": "Run `python3 rsi.py status`, check active champion vs challenger, and draft today's experiments for each active client."},
            {"label": "Evening evaluate and evolve", "hour": 20, "minute": 0, "prompt": "Ingest today's X metrics, run `python3 rsi.py evaluate`, show the score progression chart, and propose a playbook update if the gate passed."},
        ],
    },
    "paid-ads": {
        "id": "paid-ads",
        "name": "Paid Ads Creatives (Meta / TikTok)",
        "goal": "Maximize thumbstop rate, outbound CTR, and conversion efficiency while keeping fatigue and CPA under control.",
        "feedback_window_hours": 24,
        "min_samples": 6,
        "min_lift_pct": 6.0,
        "drift_drop_pct": 15.0,
        "metrics": {
            "hook_3s_rate": {"weight": 0.30, "scale": 45.0, "higher_is_better": True, "unit": "%"},
            "outbound_ctr": {"weight": 0.35, "scale": 3.0, "higher_is_better": True, "unit": "%"},
            "lp_cvr": {"weight": 0.35, "scale": 6.0, "higher_is_better": True, "unit": "%"},
        },
        "guardrails": {
            "negative_feedback_rate": {"max": 0.8, "unit": "%"},
            "cpa_ratio_vs_target": {"max": 1.35, "unit": "x"},
        },
        "dimensions": {
            "hook_3s": [
                "pain_callout",
                "unexpected_demo",
                "ugc_confession",
                "side_by_side_proof",
                "brand_logo_intro",
            ],
            "angle": [
                "time_saved",
                "hidden_cost",
                "social_proof_case",
                "feature_list",
            ],
            "visual_style": [
                "native_ugc_handheld",
                "screen_recording_annotated",
                "bold_static_comparison",
                "polished_studio",
            ],
            "offer_frame": [
                "risk_free_trial",
                "instant_audit",
                "direct_discount",
                "generic_learn_more",
            ],
        },
        "initial_strategy": {
            "hook_3s": "brand_logo_intro",
            "angle": "feature_list",
            "visual_style": "polished_studio",
            "offer_frame": "generic_learn_more",
        },
        "initial_rules": [
            "Earn attention in the first 2 seconds before introducing the brand name.",
            "Show the product solving one concrete problem instead of listing five features.",
            "Match landing page headline directly to the ad hook to protect conversion rate.",
        ],
        "recommended_routines": [
            {"label": "Daily creative brief", "hour": 9, "minute": 30, "prompt": "Run `python3 rsi.py status` and generate challenger ad hooks/angles for today's test budget."},
            {"label": "Evening ROAS/CTR eval", "hour": 21, "minute": 0, "prompt": "Collect today's ad metrics, run `python3 rsi.py evaluate`, and propose promoting or killing the challenger creative."},
        ],
    },
    "cold-outbound": {
        "id": "cold-outbound",
        "name": "Cold Outbound (Email / DM)",
        "goal": "Maximize positive reply rate and booked meetings while keeping bounce and spam complaint rates near zero.",
        "feedback_window_hours": 24,
        "min_samples": 6,
        "min_lift_pct": 8.0,
        "drift_drop_pct": 18.0,
        "metrics": {
            "open_rate": {"weight": 0.20, "scale": 70.0, "higher_is_better": True, "unit": "%"},
            "positive_reply_rate": {"weight": 0.45, "scale": 8.0, "higher_is_better": True, "unit": "%"},
            "meeting_rate": {"weight": 0.35, "scale": 3.5, "higher_is_better": True, "unit": "%"},
        },
        "guardrails": {
            "spam_complaint_rate": {"max": 0.3, "unit": "%"},
            "unsubscribe_rate": {"max": 3.0, "unit": "%"},
        },
        "dimensions": {
            "subject_pattern": [
                "two_word_internal",
                "trigger_event",
                "peer_question",
                "salesy_pitch",
            ],
            "opener": [
                "specific_signal_observation",
                "relevant_micro_audit",
                "mutual_context",
                "generic_flattery",
            ],
            "proof_type": [
                "peer_metric_one_liner",
                "loom_teardown_offer",
                "no_proof",
                "long_case_study",
            ],
            "cta_friction": [
                "soft_interest_check",
                "send_asset_permission",
                "calendar_link_cold",
            ],
        },
        "initial_strategy": {
            "subject_pattern": "salesy_pitch",
            "opener": "generic_flattery",
            "proof_type": "long_case_study",
            "cta_friction": "calendar_link_cold",
        },
        "initial_rules": [
            "Keep cold messages under 85 words with one specific reason for reaching out now.",
            "Ask for interest before dropping a calendar link.",
            "Never send generic templates that trigger spam complaints.",
        ],
        "recommended_routines": [
            {"label": "Morning outbound batch", "hour": 8, "minute": 30, "prompt": "Run `python3 rsi.py status` and draft today's champion vs challenger outbound sequences."},
            {"label": "Evening reply eval", "hour": 19, "minute": 0, "prompt": "Ingest reply metrics, run `python3 rsi.py evaluate`, and update the outbound playbook if the challenger won."},
        ],
    },
}


def list_presets() -> list[dict[str, Any]]:
    return [
        {
            "id": p["id"],
            "name": p["name"],
            "goal": p["goal"],
            "feedback_window_hours": p["feedback_window_hours"],
            "metrics": list(p["metrics"].keys()),
            "dimensions": list(p["dimensions"].keys()),
        }
        for p in PRESETS.values()
    ]


def get_preset(preset_id: str) -> dict[str, Any]:
    if preset_id not in PRESETS:
        known = ", ".join(sorted(PRESETS))
        raise ValueError(f"Unknown preset '{preset_id}'. Available presets: {known}")
    return deepcopy(PRESETS[preset_id])


def build_custom_niche(
    name: str,
    goal: str,
    feedback_window_hours: int,
    metrics: dict[str, dict[str, Any]],
    dimensions: dict[str, list[str]],
    guardrails: dict[str, dict[str, Any]] | None = None,
    initial_rules: list[str] | None = None,
    min_samples: int = 5,
    min_lift_pct: float = 5.0,
) -> dict[str, Any]:
    if not name.strip() or not goal.strip():
        raise ValueError("Niche name and goal are required.")
    if not metrics:
        raise ValueError("At least one metric is required.")
    if not dimensions:
        raise ValueError("At least one experimental dimension is required.")

    total_weight = sum(float(cfg.get("weight", 0.0)) for cfg in metrics.values())
    if total_weight <= 0:
        raise ValueError("Metric weights must sum to a positive number.")

    normalized_metrics: dict[str, dict[str, Any]] = {}
    for key, cfg in metrics.items():
        normalized_metrics[key] = {
            "weight": round(float(cfg["weight"]) / total_weight, 4),
            "scale": float(cfg.get("scale", 10.0)),
            "higher_is_better": bool(cfg.get("higher_is_better", True)),
            "unit": str(cfg.get("unit", "")),
        }

    initial_strategy = {}
    for dim, choices in dimensions.items():
        if not choices:
            raise ValueError(f"Dimension '{dim}' must have at least one option.")
        initial_strategy[dim] = choices[0]

    slug = "".join(ch if ch.isalnum() else "-" for ch in name.lower()).strip("-") or "custom"
    return {
        "id": slug,
        "name": name.strip(),
        "goal": goal.strip(),
        "feedback_window_hours": max(1, int(feedback_window_hours)),
        "min_samples": max(2, int(min_samples)),
        "min_lift_pct": float(min_lift_pct),
        "drift_drop_pct": 15.0,
        "metrics": normalized_metrics,
        "guardrails": guardrails or {},
        "dimensions": dimensions,
        "initial_strategy": initial_strategy,
        "initial_rules": initial_rules
        or [
            "Test one concrete hypothesis at a time against the current champion.",
            "Only promote a playbook change when it beats the champion over the minimum sample size.",
        ],
        "recommended_routines": [
            {"label": "Daily experiment plan", "hour": 9, "minute": 0, "prompt": "Run `python3 rsi.py status` and plan today's champion vs challenger experiments."},
            {"label": "Daily evaluate and evolve", "hour": 20, "minute": 0, "prompt": "Collect today's metrics, run `python3 rsi.py evaluate`, and propose a playbook update if the challenger won."},
        ],
    }
