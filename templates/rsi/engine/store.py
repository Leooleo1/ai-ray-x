"""Persistent state, append-only experiment & draft ledgers, and versioned playbooks."""

from __future__ import annotations

import json
import re
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

OPTION_SLUG_RE = re.compile(r"^[a-z0-9][a-z0-9_-]{0,47}$")


def utc_now_iso() -> str:
    return datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def validate_option_slug(option: str) -> str:
    slug = option.strip().lower()
    if not OPTION_SLUG_RE.match(slug):
        raise ValueError(
            f"Invalid option slug '{option}'. Use 1-48 lowercase letters, digits, '_' or '-' (e.g. 'native_poll')."
        )
    return slug


class Store:
    """Manages state/ and playbooks/ under a project root."""

    def __init__(self, root: Path) -> None:
        self.root = root
        self.state_dir = root / "state"
        self.playbooks_dir = root / "playbooks"
        self.niche_file = self.state_dir / "niche.json"
        self.versions_file = self.state_dir / "versions.json"
        self.experiments_file = self.state_dir / "experiments.jsonl"
        self.drafts_file = self.state_dir / "drafts.jsonl"
        self.lessons_file = self.state_dir / "lessons.jsonl"
        self.current_playbook = self.playbooks_dir / "current.md"

    def ensure_dirs(self) -> None:
        self.state_dir.mkdir(parents=True, exist_ok=True)
        self.playbooks_dir.mkdir(parents=True, exist_ok=True)

    def is_initialized(self) -> bool:
        return self.niche_file.exists() and self.versions_file.exists()

    def load_niche(self) -> dict[str, Any]:
        if not self.niche_file.exists():
            raise FileNotFoundError("No niche configured yet. Talk to the agent or run `python3 rsi.py init`.")
        niche = json.loads(self.niche_file.read_text(encoding="utf-8"))
        niche.setdefault("option_provenance", {})
        return niche

    def save_niche(self, niche: dict[str, Any]) -> None:
        self.ensure_dirs()
        niche.setdefault("option_provenance", {})
        self._atomic_write(self.niche_file, json.dumps(niche, indent=2, ensure_ascii=False) + "\n")

    def load_versions(self) -> dict[str, Any]:
        if not self.versions_file.exists():
            raise FileNotFoundError("No playbook versions found. Initialize a niche first.")
        return json.loads(self.versions_file.read_text(encoding="utf-8"))

    def save_versions(self, data: dict[str, Any]) -> None:
        self.ensure_dirs()
        self._atomic_write(self.versions_file, json.dumps(data, indent=2, ensure_ascii=False) + "\n")

    def load_experiments(self) -> list[dict[str, Any]]:
        return self._load_jsonl(self.experiments_file)

    def append_experiment(self, record: dict[str, Any]) -> None:
        self._append_jsonl(self.experiments_file, record)

    def load_drafts(self) -> list[dict[str, Any]]:
        return self._load_jsonl(self.drafts_file)

    def append_draft(self, record: dict[str, Any]) -> None:
        self._append_jsonl(self.drafts_file, record)

    def load_lessons(self) -> list[dict[str, Any]]:
        return self._load_jsonl(self.lessons_file)

    def append_lesson(self, lesson: dict[str, Any]) -> None:
        self._append_jsonl(self.lessons_file, lesson)

    def _load_jsonl(self, path: Path) -> list[dict[str, Any]]:
        if not path.exists():
            return []
        rows: list[dict[str, Any]] = []
        for line in path.read_text(encoding="utf-8").splitlines():
            line = line.strip()
            if line:
                rows.append(json.loads(line))
        return rows

    def _append_jsonl(self, path: Path, record: dict[str, Any]) -> None:
        self.ensure_dirs()
        with path.open("a", encoding="utf-8") as fh:
            fh.write(json.dumps(record, ensure_ascii=False) + "\n")

    def next_version_id(self, versions_data: dict[str, Any]) -> str:
        return f"v{len(versions_data['versions']) + 1:03d}"

    def next_experiment_id(self) -> str:
        count = len(self.load_experiments()) + 1
        return f"exp-{int(time.time() * 1000) % 1_000_000:06d}-{count:04d}"

    def next_draft_id(self) -> str:
        count = len(self.load_drafts()) + 1
        return f"drf-{count:04d}"

    def init_niche(self, niche: dict[str, Any], force: bool = False) -> dict[str, Any]:
        if self.is_initialized() and not force:
            raise FileExistsError("A niche is already initialized. Pass --force to reset state.")
        self.ensure_dirs()
        for path in [self.experiments_file, self.drafts_file, self.lessons_file]:
            if path.exists():
                path.unlink()
        for old in self.playbooks_dir.glob("v*.md"):
            old.unlink()

        niche.setdefault("option_provenance", {})
        self.save_niche(niche)
        v1 = {
            "id": "v001",
            "parent": None,
            "created_at": utc_now_iso(),
            "status": "champion",
            "hypothesis": "Baseline v1 playbook from initial niche setup.",
            "strategy": dict(niche["initial_strategy"]),
            "rules": list(niche.get("initial_rules", [])),
            "anti_patterns": [],
            "promoted_score": None,
            "promoted_samples": 0,
        }
        versions_data = {
            "champion": "v001",
            "challenger": None,
            "versions": [v1],
        }
        self.save_versions(versions_data)
        self.write_playbook_snapshot(niche, v1, make_current=True)
        return v1

    def get_version(self, versions_data: dict[str, Any], version_id: str) -> dict[str, Any]:
        for v in versions_data["versions"]:
            if v["id"] == version_id:
                return v
        raise KeyError(f"Unknown playbook version '{version_id}'")

    def add_option(self, dimension: str, option: str, source: str, proposed_by: str = "agent") -> dict[str, Any]:
        """Registers a new candidate option in an existing dimension with provenance."""
        niche = self.load_niche()
        if dimension not in niche["dimensions"]:
            raise ValueError(f"Unknown dimension '{dimension}'. Known: {list(niche['dimensions'])}")
        slug = validate_option_slug(option)
        source = source.strip()
        if not source:
            raise ValueError("Provenance --source is required (URL or concrete observation).")
        proposed_by = proposed_by.strip() or "agent"
        if slug in niche["dimensions"][dimension]:
            raise ValueError(f"Option '{slug}' already exists in dimension '{dimension}'.")

        niche["dimensions"][dimension].append(slug)
        prov = niche.setdefault("option_provenance", {}).setdefault(dimension, {})
        meta = {
            "source": source,
            "added_at": utc_now_iso(),
            "proposed_by": proposed_by,
            "status": "candidate",
        }
        prov[slug] = meta
        self.save_niche(niche)

        # Refresh current playbook so the newly registered candidate appears immediately.
        versions_data = self.load_versions()
        champ = self.get_version(versions_data, versions_data["champion"])
        self.write_playbook_snapshot(niche, champ, make_current=True)
        return {"dimension": dimension, "option": slug, **meta}

    def list_discovered_options(self, niche: dict[str, Any] | None = None) -> list[dict[str, Any]]:
        niche = niche or self.load_niche()
        items: list[dict[str, Any]] = []
        for dim, entries in niche.get("option_provenance", {}).items():
            for opt, meta in entries.items():
                items.append(
                    {
                        "dimension": dim,
                        "option": opt,
                        "source": meta.get("source", ""),
                        "added_at": meta.get("added_at", ""),
                        "proposed_by": meta.get("proposed_by", ""),
                        "status": meta.get("status", "candidate"),
                    }
                )
        items.sort(key=lambda x: x["added_at"])
        return items

    def update_option_status(self, changes: dict[str, str], status: str) -> None:
        """Updates provenance status ('proven' / 'rejected') for any discovered options in `changes`."""
        niche = self.load_niche()
        prov = niche.get("option_provenance", {})
        touched = False
        for dim, opt in changes.items():
            if dim in prov and opt in prov[dim]:
                prov[dim][opt]["status"] = status
                touched = True
        if touched:
            self.save_niche(niche)

    def create_challenger(
        self,
        hypothesis: str,
        strategy_changes: dict[str, str],
        added_rules: list[str] | None = None,
        source: str | None = None,
        proposed_by: str = "agent",
    ) -> dict[str, Any]:
        niche = self.load_niche()
        versions_data = self.load_versions()
        if versions_data.get("challenger"):
            raise ValueError(
                f"Challenger '{versions_data['challenger']}' is already active. Evaluate or retire it first."
            )
        champion = self.get_version(versions_data, versions_data["champion"])
        strategy = dict(champion["strategy"])
        newly_registered: list[dict[str, Any]] = []

        for dim, val in strategy_changes.items():
            if dim not in niche["dimensions"]:
                raise ValueError(f"Unknown dimension '{dim}'. Known: {list(niche['dimensions'])}")
            if val not in niche["dimensions"][dim]:
                if source and source.strip():
                    registered = self.add_option(dim, val, source=source, proposed_by=proposed_by)
                    newly_registered.append(registered)
                    niche = self.load_niche()
                    val = registered["option"]
                else:
                    raise ValueError(
                        f"Unknown option '{val}' for dimension '{dim}'. Known: {niche['dimensions'][dim]}. "
                        f"To add a new tactic, pass --source \"<url or observation>\" or run `python3 rsi.py add-option`."
                    )
            strategy[dim] = val

        rules = list(champion.get("rules", []))
        for rule in added_rules or []:
            rule = rule.strip()
            if rule and rule not in rules:
                rules.append(rule)

        vid = self.next_version_id(versions_data)
        challenger = {
            "id": vid,
            "parent": champion["id"],
            "created_at": utc_now_iso(),
            "status": "challenger",
            "hypothesis": hypothesis.strip(),
            "changes": {k: strategy[k] for k in strategy_changes},
            "strategy": strategy,
            "rules": rules,
            "anti_patterns": list(champion.get("anti_patterns", [])),
            "promoted_score": None,
            "promoted_samples": 0,
        }
        if newly_registered:
            challenger["new_options"] = newly_registered
        versions_data["versions"].append(challenger)
        versions_data["challenger"] = vid
        self.save_versions(versions_data)
        self.write_playbook_snapshot(niche, challenger, make_current=False)
        return challenger

    def register_draft(
        self,
        *,
        text: str,
        client: str,
        version: str | None = None,
        post_id: str | None = None,
        variant: dict[str, str] | None = None,
        predicted_score: float | None = None,
    ) -> dict[str, Any]:
        """Registers a post draft written by the agent so later analytics exports can match it."""
        niche = self.load_niche()
        versions_data = self.load_versions()
        vid = version or versions_data["champion"]
        ver = self.get_version(versions_data, vid)
        text = text.strip()
        if not text:
            raise ValueError("Draft --text cannot be empty.")

        final_variant = dict(ver["strategy"])
        if variant:
            for dim, val in variant.items():
                if dim not in niche["dimensions"]:
                    raise ValueError(f"Unknown dimension '{dim}'. Known: {list(niche['dimensions'])}")
                if val not in niche["dimensions"][dim]:
                    raise ValueError(f"Unknown option '{val}' for dimension '{dim}'. Known: {niche['dimensions'][dim]}")
                final_variant[dim] = val

        record = {
            "draft_id": self.next_draft_id(),
            "ts": utc_now_iso(),
            "version": vid,
            "client": client.strip() or "default",
            "text": text,
            "post_id": post_id.strip() if post_id and post_id.strip() else None,
            "variant": final_variant,
            "predicted_score": round(float(predicted_score), 2) if predicted_score is not None else None,
        }
        self.append_draft(record)
        return record

    def write_playbook_snapshot(self, niche: dict[str, Any], version: dict[str, Any], make_current: bool) -> Path:
        md = render_playbook_markdown(niche, version)
        snapshot = self.playbooks_dir / f"{version['id']}.md"
        self._atomic_write(snapshot, md)
        if make_current:
            self._atomic_write(self.current_playbook, md)
        return snapshot

    def rollback(self, target_version: str, reason: str) -> dict[str, Any]:
        niche = self.load_niche()
        versions_data = self.load_versions()
        target = self.get_version(versions_data, target_version)
        previous_id = versions_data["champion"]
        if previous_id == target_version:
            raise ValueError(f"Version '{target_version}' is already the active champion.")

        prev = self.get_version(versions_data, previous_id)
        prev["status"] = "rolled_back"
        if versions_data.get("challenger"):
            active_ch = self.get_version(versions_data, versions_data["challenger"])
            active_ch["status"] = "retired"
            versions_data["challenger"] = None

        target["status"] = "champion"
        versions_data["champion"] = target_version
        self.save_versions(versions_data)
        self.write_playbook_snapshot(niche, target, make_current=True)
        self.append_lesson(
            {
                "ts": utc_now_iso(),
                "kind": "rollback",
                "from_version": previous_id,
                "to_version": target_version,
                "reason": reason,
            }
        )
        return target

    @staticmethod
    def _atomic_write(path: Path, content: str) -> None:
        tmp = path.with_suffix(path.suffix + ".tmp")
        tmp.write_text(content, encoding="utf-8")
        tmp.replace(path)


def render_playbook_markdown(niche: dict[str, Any], version: dict[str, Any]) -> str:
    lines = [
        f"# Playbook {version['id']} — {niche['name']}",
        "",
        f"- **Status**: `{version['status']}`",
        f"- **Parent**: `{version.get('parent') or 'none'}`",
        f"- **Updated**: `{version['created_at']}`",
        f"- **Goal**: {niche['goal']}",
        f"- **Hypothesis**: {version['hypothesis']}",
    ]
    if version.get("promoted_score") is not None:
        lines.append(
            f"- **Verified Score**: `{version['promoted_score']:.2f}/100` (n={version.get('promoted_samples', 0)})"
        )
    lines.extend(["", "## Active Strategy Levers", ""])
    prov = niche.get("option_provenance", {})
    for dim, val in version.get("strategy", {}).items():
        meta = prov.get(dim, {}).get(val)
        extra = f" _(discovered by {meta['proposed_by']} on {meta['added_at'][:10]}: {meta['source']})_" if meta else ""
        lines.append(f"- **{dim}**: `{val}`{extra}")

    lines.extend(["", "## Proven Rules", ""])
    for rule in version.get("rules", []):
        lines.append(f"- {rule}")

    discovered = []
    for dim, entries in prov.items():
        for opt, meta in entries.items():
            discovered.append((meta.get("added_at", ""), dim, opt, meta))
    if discovered:
        discovered.sort()
        lines.extend(["", "## Discovered Tactics (With Provenance)", ""])
        for _, dim, opt, meta in discovered:
            lines.append(
                f"- `{dim}={opt}` [{meta.get('status', 'candidate')}] — "
                f"by `{meta.get('proposed_by', 'agent')}` on `{meta.get('added_at', '')[:10]}` · source: {meta.get('source', '')}"
            )

    anti = version.get("anti_patterns", [])
    if anti:
        lines.extend(["", "## Anti-Patterns (Rejected by Evidence)", ""])
        for item in anti[-10:]:
            lines.append(f"- {item}")

    lines.extend(["", "## North-Star Metrics", ""])
    for metric, cfg in niche.get("metrics", {}).items():
        lines.append(f"- `{metric}` (weight {cfg['weight']:.2f}, scale {cfg['scale']} {cfg.get('unit', '')})")

    guardrails = niche.get("guardrails", {})
    if guardrails:
        lines.extend(["", "## Guardrails (Auto-Reject if Violated)", ""])
        for metric, cfg in guardrails.items():
            bound = f"<= {cfg['max']}" if "max" in cfg else f">= {cfg.get('min')}"
            lines.append(f"- `{metric}` {bound} {cfg.get('unit', '')}")

    lines.append("")
    return "\n".join(lines)
