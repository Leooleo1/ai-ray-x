"""Real-world metrics collectors (generic CSV/JSON, X Analytics export) and raiox Wiki / chart exporters."""

from __future__ import annotations

import csv
import json
import re
import unicodedata
from pathlib import Path
from typing import Any

from engine.evaluator import check_guardrails, compute_score
from engine.store import Store, utc_now_iso

# Tolerant English & Portuguese column aliases for X (Twitter) Analytics CSV exports.
X_COLUMN_ALIASES: dict[str, set[str]] = {
    "post_id": {
        "post id",
        "tweet id",
        "id",
        "id do post",
        "id do tweet",
        "post link",
        "tweet permalink",
        "permalink",
        "link do post",
        "link",
    },
    "text": {
        "post text",
        "tweet text",
        "text",
        "texto do post",
        "texto do tweet",
        "texto",
        "conteudo",
    },
    "impressions": {
        "impressions",
        "impression count",
        "views",
        "impressoes",
        "visualizacoes",
    },
    "bookmarks": {
        "bookmarks",
        "bookmark count",
        "itens salvos",
        "salvos",
        "marcadores",
    },
    "replies": {
        "replies",
        "reply count",
        "respostas",
    },
    "profile_clicks": {
        "profile visits",
        "user profile clicks",
        "profile clicks",
        "visitas ao perfil",
        "cliques no perfil",
        "cliques no perfil do usuario",
    },
    "follows": {
        "new follows",
        "follows",
        "novos seguidores",
        "seguidores",
    },
    "engagements": {
        "engagements",
        "post engagements",
        "engajamentos",
    },
}

TCO_URL_RE = re.compile(r"https?://t\.co/\S+", re.IGNORECASE)
STATUS_ID_RE = re.compile(r"/status/(\d+)")


def _norm_header(name: str) -> str:
    folded = unicodedata.normalize("NFKD", name).encode("ascii", "ignore").decode("ascii")
    cleaned = re.sub(r"[_\-]+", " ", folded.lower())
    return re.sub(r"\s+", " ", cleaned).strip()


def _norm_post_text(text: str) -> str:
    no_tco = TCO_URL_RE.sub("", text)
    return re.sub(r"\s+", " ", no_tco).strip().lower()


def _extract_post_id(raw: str | None) -> str | None:
    if not raw:
        return None
    s = raw.strip()
    if not s:
        return None
    m = STATUS_ID_RE.search(s)
    if m:
        return m.group(1)
    if s.isdigit():
        return s
    return s


def _parse_number(raw: str | None) -> float:
    if raw is None:
        return 0.0
    s = raw.strip().replace("\u00a0", "").replace(" ", "")
    if not s or s == "-":
        return 0.0
    # Handle thousands separators like "1,250" or "1.250" (when followed by 3 digits and no decimals).
    if re.match(r"^\d{1,3}(,\d{3})+$", s):
        s = s.replace(",", "")
    elif re.match(r"^\d{1,3}(\.\d{3})+$", s):
        s = s.replace(".", "")
    else:
        s = s.replace(",", ".")
    return float(s)


def _resolve_x_columns(fieldnames: list[str]) -> dict[str, str]:
    mapping: dict[str, str] = {}
    for raw in fieldnames:
        key = _norm_header(raw)
        for canonical, aliases in X_COLUMN_ALIASES.items():
            if canonical not in mapping and key in aliases:
                mapping[canonical] = raw
    if "impressions" not in mapping:
        raise ValueError(
            "Missing required impressions column (expected e.g. 'Impressions' or 'Impressões'). "
            f"Found columns: {fieldnames}"
        )
    if "post_id" not in mapping and "text" not in mapping:
        raise ValueError(
            "Missing post identifier column (expected 'Post id'/'Tweet id' or 'Post text'/'Texto do post'). "
            f"Found columns: {fieldnames}"
        )
    if not any(k in mapping for k in ("bookmarks", "replies", "profile_clicks", "follows")):
        raise ValueError(
            "Missing engagement count columns (expected at least one of Bookmarks/Itens salvos, "
            f"Replies/Respostas, Profile visits/Visitas ao perfil, New follows/Novos seguidores). Found columns: {fieldnames}"
        )
    return mapping


def _match_draft(drafts: list[dict[str, Any]], post_id: str | None, text: str) -> dict[str, Any] | None:
    if post_id:
        for d in reversed(drafts):
            if d.get("post_id") and _extract_post_id(d["post_id"]) == post_id:
                return d
    norm = _norm_post_text(text)
    if not norm:
        return None
    for d in reversed(drafts):
        d_norm = _norm_post_text(d.get("text", ""))
        if d_norm and d_norm == norm:
            return d
    # Fallback prefix/containment match when X truncates long posts or adds media links.
    if len(norm) >= 24:
        for d in reversed(drafts):
            d_norm = _norm_post_text(d.get("text", ""))
            if len(d_norm) >= 24 and (norm.startswith(d_norm) or d_norm.startswith(norm)):
                return d
    return None


def record_experiment(
    store: Store,
    *,
    version: str | None,
    client: str,
    variant: dict[str, str] | None,
    metrics: dict[str, float],
    artifact: str = "",
    hypothesis: str = "",
    predicted_score: float | None = None,
    extra: dict[str, Any] | None = None,
) -> dict[str, Any]:
    """Validates, scores, and appends one observed experiment outcome to the ledger."""
    niche = store.load_niche()
    versions_data = store.load_versions()
    vid = version or versions_data["champion"]
    ver = store.get_version(versions_data, vid)
    final_variant = dict(ver["strategy"])
    if variant:
        for dim, val in variant.items():
            if dim in niche["dimensions"]:
                final_variant[dim] = val

    score = compute_score(niche, metrics)
    violations = check_guardrails(niche, metrics)
    record: dict[str, Any] = {
        "id": store.next_experiment_id(),
        "ts": utc_now_iso(),
        "client": client.strip() or "default",
        "version": vid,
        "hypothesis": hypothesis or ver.get("hypothesis", ""),
        "variant": final_variant,
        "artifact": artifact.strip(),
        "predicted_score": round(float(predicted_score), 2) if predicted_score is not None else None,
        "actual_metrics": {k: float(v) for k, v in metrics.items()},
        "actual_score": score,
        "guardrail_violations": violations,
    }
    if extra:
        record.update(extra)
    store.append_experiment(record)
    return record


def ingest_x_export(store: Store, path: Path) -> dict[str, Any]:
    """Ingests a free X Analytics CSV export, matching posts to registered drafts and converting counts to rates."""
    if not path.exists():
        raise FileNotFoundError(f"X export file not found: {path}")
    drafts = store.load_drafts()
    existing_keys = {
        e.get("x_match_key")
        for e in store.load_experiments()
        if e.get("x_match_key")
    }

    with path.open("r", encoding="utf-8-sig", newline="") as fh:
        reader = csv.DictReader(fh)
        fieldnames = list(reader.fieldnames or [])
        if not fieldnames:
            raise ValueError("CSV file has no header row.")
        cols = _resolve_x_columns(fieldnames)

        rows_total = 0
        skipped_unmatched = 0
        skipped_duplicates = 0
        skipped_zero_impressions = 0
        ingested: list[dict[str, Any]] = []

        for row in reader:
            rows_total += 1
            post_id = _extract_post_id(row.get(cols["post_id"])) if "post_id" in cols else None
            post_text = (row.get(cols["text"]) or "").strip() if "text" in cols else ""

            draft = _match_draft(drafts, post_id, post_text)
            if draft is None:
                skipped_unmatched += 1
                continue

            match_key = f"id:{post_id}" if post_id else f"draft:{draft['draft_id']}:{_norm_post_text(post_text)[:64]}"
            if match_key in existing_keys:
                skipped_duplicates += 1
                continue

            impressions = _parse_number(row.get(cols["impressions"]))
            if impressions <= 0:
                skipped_zero_impressions += 1
                continue

            bookmarks = _parse_number(row.get(cols["bookmarks"])) if "bookmarks" in cols else 0.0
            replies = _parse_number(row.get(cols["replies"])) if "replies" in cols else 0.0
            profile_clicks = _parse_number(row.get(cols["profile_clicks"])) if "profile_clicks" in cols else 0.0
            follows = _parse_number(row.get(cols["follows"])) if "follows" in cols else 0.0
            engagements = _parse_number(row.get(cols["engagements"])) if "engagements" in cols else None

            metrics = {
                "bookmark_rate": round((bookmarks / impressions) * 100.0, 4),
                "reply_rate": round((replies / impressions) * 100.0, 4),
                "profile_click_rate": round((profile_clicks / impressions) * 100.0, 4),
                "follow_per_1k": round((follows / impressions) * 1000.0, 4),
            }
            raw_counts: dict[str, Any] = {
                "impressions": int(impressions),
                "bookmarks": int(bookmarks),
                "replies": int(replies),
                "profile_clicks": int(profile_clicks),
                "follows": int(follows),
            }
            if engagements is not None:
                raw_counts["engagements"] = int(engagements)

            rec = record_experiment(
                store,
                version=draft["version"],
                client=draft["client"],
                variant=draft.get("variant"),
                metrics=metrics,
                artifact=post_id or draft["draft_id"],
                predicted_score=draft.get("predicted_score"),
                extra={
                    "draft_id": draft["draft_id"],
                    "x_post_id": post_id,
                    "x_match_key": match_key,
                    "raw_counts": raw_counts,
                },
            )
            existing_keys.add(match_key)
            ingested.append(rec)

    return {
        "file": str(path),
        "rows_total": rows_total,
        "ingested": len(ingested),
        "skipped_unmatched": skipped_unmatched,
        "skipped_duplicates": skipped_duplicates,
        "skipped_zero_impressions": skipped_zero_impressions,
        "mapped_columns": cols,
        "ids": [r["id"] for r in ingested],
    }


def ingest_file(store: Store, path: Path) -> list[dict[str, Any]]:
    """Ingests a batch of real-world experiment outcomes from a JSON array or CSV file."""
    niche = store.load_niche()
    metric_keys = set(niche["metrics"].keys()) | set(niche.get("guardrails", {}).keys())
    dim_keys = set(niche["dimensions"].keys())
    ingested: list[dict[str, Any]] = []

    if path.suffix.lower() == ".json":
        rows = json.loads(path.read_text(encoding="utf-8"))
        if not isinstance(rows, list):
            raise ValueError("JSON import file must contain a list of experiment objects.")
        for row in rows:
            metrics = row.get("metrics") or {k: float(row[k]) for k in metric_keys if k in row and row[k] != ""}
            variant = row.get("variant") or {k: str(row[k]) for k in dim_keys if k in row and row[k]}
            ingested.append(
                record_experiment(
                    store,
                    version=row.get("version"),
                    client=str(row.get("client", "default")),
                    variant=variant,
                    metrics={k: float(v) for k, v in metrics.items()},
                    artifact=str(row.get("artifact", "")),
                    hypothesis=str(row.get("hypothesis", "")),
                    predicted_score=row.get("predicted_score"),
                )
            )
    elif path.suffix.lower() == ".csv":
        with path.open("r", encoding="utf-8-sig", newline="") as fh:
            for row in csv.DictReader(fh):
                metrics = {k: float(row[k]) for k in metric_keys if k in row and row[k] not in (None, "")}
                variant = {k: str(row[k]).strip() for k in dim_keys if k in row and row[k] not in (None, "")}
                pred = float(row["predicted_score"]) if row.get("predicted_score") not in (None, "") else None
                ingested.append(
                    record_experiment(
                        store,
                        version=row.get("version") or None,
                        client=row.get("client") or "default",
                        variant=variant,
                        metrics=metrics,
                        artifact=row.get("artifact") or "",
                        hypothesis=row.get("hypothesis") or "",
                        predicted_score=pred,
                    )
                )
    else:
        raise ValueError("Collector supports .json and .csv files.")
    return ingested


def export_wiki_summary(store: Store) -> str:
    """Produces a compact (<3500 char) playbook digest ready for raiox Wiki or ai-memory."""
    niche = store.load_niche()
    versions_data = store.load_versions()
    champ = store.get_version(versions_data, versions_data["champion"])
    exps = store.load_experiments()
    clients = sorted({e.get("client", "default") for e in exps})
    lines = [
        f"# {niche['name']} — Live RSI Playbook ({champ['id']})",
        "",
        f"Goal: {niche['goal']}",
        f"Evidence base: {len(exps)} experiments across {max(len(clients), 1)} client(s).",
        "",
        "## Winning Levers Now",
    ]
    for dim, val in champ["strategy"].items():
        lines.append(f"- {dim}: {val}")
    lines.append("")
    lines.append("## Active Rules")
    for rule in champ.get("rules", [])[-8:]:
        lines.append(f"- {rule}")
    discovered = store.list_discovered_options(niche)
    if discovered:
        lines.append("")
        lines.append("## Newly Discovered Tactics")
        for item in discovered[-6:]:
            lines.append(f"- {item['dimension']}={item['option']} [{item['status']}] ({item['source']})")
    anti = champ.get("anti_patterns", [])
    if anti:
        lines.append("")
        lines.append("## Do Not Repeat")
        for item in anti[-5:]:
            lines.append(f"- {item}")
    return "\n".join(lines) + "\n"


def raiox_chart_json(store: Store) -> str:
    """Builds a `raiox-chart` JSON block showing mean score per playbook version."""
    niche = store.load_niche()
    versions_data = store.load_versions()
    exps = store.load_experiments()
    rows = []
    for v in versions_data["versions"]:
        scores = [float(e["actual_score"]) for e in exps if e["version"] == v["id"] and e.get("actual_score") is not None]
        if not scores:
            continue
        avg = round(sum(scores) / len(scores), 1)
        tag = " *" if v["id"] == versions_data["champion"] else (" (rej)" if v["status"] == "rejected" else "")
        rows.append({"label": f"{v['id']}{tag}", "value": avg})
    payload = {
        "title": f"{niche['name']} — Playbook Evolution",
        "subtitle": f"{len(exps)} experiments · * = active champion",
        "unit": "/100",
        "rows": rows[-12:],
    }
    return json.dumps(payload, ensure_ascii=False)
