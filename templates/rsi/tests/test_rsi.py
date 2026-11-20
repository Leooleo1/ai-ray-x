"""Unit and end-to-end tests for the raiox-rsi engine."""

from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
FIXTURES = Path(__file__).resolve().parent / "fixtures"
sys.path.insert(0, str(ROOT))

from engine.collector import export_wiki_summary, ingest_file, ingest_x_export, raiox_chart_json, record_experiment
from engine.evaluator import compute_score, detect_drift, evaluate_challenger, lever_attribution
from engine.presets import build_custom_niche, get_preset
from engine.simulator import simulate_outcome
from engine.store import Store


class TestRsiEngine(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        self.store = Store(self.root)

    def tearDown(self) -> None:
        self.tmp.cleanup()

    def test_custom_niche_and_scoring(self) -> None:
        niche = build_custom_niche(
            name="YouTube Thumbnails",
            goal="Maximize CTR and first-30s retention",
            feedback_window_hours=6,
            metrics={
                "ctr": {"weight": 60, "scale": 10.0, "higher_is_better": True, "unit": "%"},
                "retention_30s": {"weight": 40, "scale": 80.0, "higher_is_better": True, "unit": "%"},
            },
            dimensions={"face_expression": ["shocked", "curious", "none"], "text_words": ["2_words", "none"]},
            guardrails={"clickbait_drop_rate": {"max": 40.0, "unit": "%"}},
            min_samples=3,
        )
        v1 = self.store.init_niche(niche)
        self.assertEqual(v1["id"], "v001")
        self.assertTrue(self.store.current_playbook.exists())
        self.assertEqual(compute_score(niche, {"ctr": 10.0, "retention_30s": 80.0}), 100.0)
        self.assertEqual(compute_score(niche, {"ctr": 5.0, "retention_30s": 40.0}), 50.0)

    def test_dynamic_option_with_provenance_and_statistical_gate(self) -> None:
        niche = get_preset("x-viral")
        niche["min_samples"] = 3
        self.store.init_niche(niche)

        # Unknown option without provenance must still fail (catches typos like format=thread).
        with self.assertRaises(ValueError):
            self.store.create_challenger("Typo test", {"format": "thread"})

        # Registering a genuinely new tactic stores provenance and keeps it as a candidate.
        added = self.store.add_option(
            "hook_style",
            "native_poll",
            source="https://x.com/creators/status/12345",
            proposed_by="agent",
        )
        self.assertEqual(added["status"], "candidate")
        self.assertEqual(added["option"], "native_poll")

        saved_niche = self.store.load_niche()
        self.assertIn("native_poll", saved_niche["dimensions"]["hook_style"])
        self.assertEqual(
            saved_niche["option_provenance"]["hook_style"]["native_poll"]["source"],
            "https://x.com/creators/status/12345",
        )
        # Appears in playbook immediately as candidate.
        playbook_before = self.store.current_playbook.read_text(encoding="utf-8")
        self.assertIn("native_poll", playbook_before)
        self.assertIn("https://x.com/creators/status/12345", playbook_before)

        # Inline registration via propose --source also works.
        chal = self.store.create_challenger(
            "Test native poll + quote card",
            {"hook_style": "native_poll", "format": "quote_card"},
            source="Observed 3 viral threads using quote cards this week",
            proposed_by="person",
        )
        self.assertEqual(chal["id"], "v002")
        self.assertEqual(self.store.load_versions()["champion"], "v001")

        # Must pass the exact same statistical gate (1 sample -> keep_testing).
        record_experiment(
            self.store,
            version="v001",
            client="c1",
            variant=None,
            metrics={"bookmark_rate": 1.0, "reply_rate": 0.8, "profile_click_rate": 1.2, "follow_per_1k": 1.5},
        )
        record_experiment(
            self.store,
            version="v002",
            client="c1",
            variant=None,
            metrics={"bookmark_rate": 3.6, "reply_rate": 2.8, "profile_click_rate": 4.4, "follow_per_1k": 7.0},
        )
        self.assertEqual(evaluate_challenger(self.store, apply=True)["decision"], "keep_testing")

        # Reach min_samples -> promotes and marks discovered options as 'proven'.
        for i in range(2):
            record_experiment(
                self.store,
                version="v001",
                client="c1",
                variant=None,
                metrics={"bookmark_rate": 1.0 + i * 0.1, "reply_rate": 0.8, "profile_click_rate": 1.2, "follow_per_1k": 1.6},
            )
            record_experiment(
                self.store,
                version="v002",
                client="c1",
                variant=None,
                metrics={"bookmark_rate": 3.7 + i * 0.1, "reply_rate": 2.7, "profile_click_rate": 4.5, "follow_per_1k": 7.2},
            )
        ev = evaluate_challenger(self.store, apply=True)
        self.assertEqual(ev["decision"], "promote")
        prov = self.store.load_niche()["option_provenance"]
        self.assertEqual(prov["hook_style"]["native_poll"]["status"], "proven")
        self.assertEqual(prov["format"]["quote_card"]["status"], "proven")

        # Appears in CLI status and report.
        st = subprocess.run(
            [sys.executable, str(ROOT / "rsi.py"), "--root", str(self.root), "status"],
            capture_output=True,
            text=True,
            check=True,
        )
        status_data = json.loads(st.stdout)
        self.assertIn("native_poll", status_data["dimensions"]["hook_style"])
        self.assertEqual(len(status_data["discovered_options"]), 2)

        rep = subprocess.run(
            [sys.executable, str(ROOT / "rsi.py"), "--root", str(self.root), "report"],
            capture_output=True,
            text=True,
            check=True,
        )
        self.assertIn("hook_style=native_poll [proven]", rep.stdout)
        self.assertIn("https://x.com/creators/status/12345", rep.stdout)

    def test_simulator_unknown_option_is_deterministic_by_name(self) -> None:
        niche = get_preset("x-viral")
        variant = dict(niche["initial_strategy"])
        variant["hook_style"] = "native_poll"
        a = simulate_outcome(niche, variant, phase=0, seed=1234)
        b = simulate_outcome(niche, variant, phase=0, seed=1234)
        self.assertEqual(a, b)

    def test_x_analytics_ingest_en_and_pt_and_matching_and_errors(self) -> None:
        niche = get_preset("x-viral")
        self.store.init_niche(niche)
        self.store.create_challenger("Test visual before/after", {"hook_style": "visual_before_after"})

        # Register 3 drafts: one matched by text (ignoring appended t.co URL), one matched by post_id, one zero-impression.
        self.store.register_draft(
            version="v002",
            client="acme",
            text="We cut Claude context rereads by 59% with one local setting. Here is the before/after breakdown.",
            predicted_score=78.0,
        )
        self.store.register_draft(
            version="v001",
            client="acme",
            text="Different local wording matched via explicit post id",
            post_id="https://x.com/acme/status/1890000000000000002",
            predicted_score=55.0,
        )
        self.store.register_draft(
            version="v002",
            client="acme",
            text="Draft with zero impressions yet should be skipped cleanly.",
        )

        # Ingest EN X Analytics fixture.
        summary_en = ingest_x_export(self.store, FIXTURES / "x_analytics_sample_en.csv")
        self.assertEqual(summary_en["rows_total"], 4)
        self.assertEqual(summary_en["ingested"], 2)
        self.assertEqual(summary_en["skipped_unmatched"], 1)
        self.assertEqual(summary_en["skipped_zero_impressions"], 1)
        self.assertEqual(summary_en["skipped_duplicates"], 0)

        exps = self.store.load_experiments()
        self.assertEqual(len(exps), 2)
        first = exps[0]
        self.assertEqual(first["version"], "v002")
        self.assertEqual(first["client"], "acme")
        # 120/4000=3.0%, 60/4000=1.5%, 160/4000=4.0%, 24/4000*1000=6.0
        self.assertEqual(first["actual_metrics"]["bookmark_rate"], 3.0)
        self.assertEqual(first["actual_metrics"]["reply_rate"], 1.5)
        self.assertEqual(first["actual_metrics"]["profile_click_rate"], 4.0)
        self.assertEqual(first["actual_metrics"]["follow_per_1k"], 6.0)
        self.assertEqual(exps[1]["version"], "v001")
        self.assertEqual(exps[1]["x_post_id"], "1890000000000000002")

        # Re-ingesting the same export skips duplicates cleanly.
        again = ingest_x_export(self.store, FIXTURES / "x_analytics_sample_en.csv")
        self.assertEqual(again["ingested"], 0)
        self.assertEqual(again["skipped_duplicates"], 2)

        # Ingest PT X Analytics fixture via CLI.
        self.store.register_draft(
            version="v002",
            client="beta",
            text="Cortamos 59% das releituras de contexto em 10 minutos. Passo a passo nos comentários.",
        )
        proc = subprocess.run(
            [sys.executable, str(ROOT / "rsi.py"), "--root", str(self.root), "ingest-x", str(FIXTURES / "x_analytics_sample_pt.csv")],
            capture_output=True,
            text=True,
            check=True,
        )
        summary_pt = json.loads(proc.stdout)
        self.assertEqual(summary_pt["ingested"], 1)
        self.assertEqual(summary_pt["skipped_unmatched"], 1)
        pt_rec = self.store.load_experiments()[-1]
        self.assertEqual(pt_rec["client"], "beta")
        # 150/5000=3.0%, 75/5000=1.5%, 200/5000=4.0%, 30/5000*1000=6.0
        self.assertEqual(pt_rec["actual_metrics"]["bookmark_rate"], 3.0)
        self.assertEqual(pt_rec["actual_metrics"]["follow_per_1k"], 6.0)

        # Missing essential impressions column produces a clear error.
        bad_csv = self.root / "bad_x.csv"
        bad_csv.write_text("Post id,Post text,Bookmarks,Replies\n1,hi,10,5\n", encoding="utf-8")
        with self.assertRaises(ValueError) as ctx:
            ingest_x_export(self.store, bad_csv)
        self.assertIn("impressions", str(ctx.exception).lower())

    def test_min_samples_gate_and_promotion_and_rollback(self) -> None:
        niche = get_preset("x-viral")
        niche["min_samples"] = 4
        self.store.init_niche(niche)
        chal = self.store.create_challenger("Test specific number proof", {"hook_style": "specific_number_proof"})
        self.assertEqual(chal["id"], "v002")

        for _ in range(2):
            record_experiment(
                self.store,
                version="v001",
                client="c1",
                variant=None,
                metrics={"bookmark_rate": 1.2, "reply_rate": 0.8, "profile_click_rate": 1.5, "follow_per_1k": 2.0},
            )
            record_experiment(
                self.store,
                version="v002",
                client="c1",
                variant=None,
                metrics={"bookmark_rate": 3.4, "reply_rate": 2.5, "profile_click_rate": 4.2, "follow_per_1k": 6.5},
            )
        early = evaluate_challenger(self.store, apply=True)
        self.assertEqual(early["decision"], "keep_testing")
        self.assertEqual(self.store.load_versions()["champion"], "v001")

        for i in range(2):
            record_experiment(
                self.store,
                version="v001",
                client="c2",
                variant=None,
                metrics={"bookmark_rate": 1.3 + i * 0.1, "reply_rate": 0.9, "profile_click_rate": 1.4, "follow_per_1k": 2.1},
            )
            record_experiment(
                self.store,
                version="v002",
                client="c2",
                variant=None,
                metrics={"bookmark_rate": 3.5 + i * 0.1, "reply_rate": 2.6, "profile_click_rate": 4.1, "follow_per_1k": 6.8},
            )
        ev = evaluate_challenger(self.store, apply=True)
        self.assertEqual(ev["decision"], "promote")
        self.assertEqual(self.store.load_versions()["champion"], "v002")
        self.assertIn("Playbook v002", self.store.current_playbook.read_text(encoding="utf-8"))

        self.store.rollback("v001", "regression in live traffic")
        self.assertEqual(self.store.load_versions()["champion"], "v001")
        self.assertIn("Playbook v001", self.store.current_playbook.read_text(encoding="utf-8"))

    def test_guardrail_vetoes_high_scoring_spam_challenger(self) -> None:
        niche = get_preset("x-viral")
        niche["min_samples"] = 3
        self.store.init_niche(niche)
        self.store.create_challenger("Aggressive spammy CTA", {"cta_type": "hard_sell"})

        for _ in range(3):
            record_experiment(
                self.store,
                version="v001",
                client="c1",
                variant=None,
                metrics={"bookmark_rate": 1.5, "reply_rate": 1.0, "profile_click_rate": 1.5, "follow_per_1k": 2.0, "unfollow_spike_rate": 0.2},
            )
            record_experiment(
                self.store,
                version="v002",
                client="c1",
                variant=None,
                metrics={"bookmark_rate": 3.9, "reply_rate": 2.9, "profile_click_rate": 4.8, "follow_per_1k": 7.5, "unfollow_spike_rate": 2.4},
            )

        ev = evaluate_challenger(self.store, apply=True)
        self.assertEqual(ev["decision"], "reject")
        self.assertIn("Guardrail", ev["reason"])
        self.assertEqual(self.store.load_versions()["champion"], "v001")
        self.assertIn("Anti-Patterns", self.store.current_playbook.read_text(encoding="utf-8"))

    def test_csv_and_json_collector_and_exports(self) -> None:
        niche = get_preset("cold-outbound")
        self.store.init_niche(niche)

        csv_path = self.root / "batch.csv"
        csv_path.write_text(
            "client,version,open_rate,positive_reply_rate,meeting_rate,spam_complaint_rate,predicted_score,artifact\n"
            "acme,v001,60,5.2,2.1,0.1,68,seq-1\n"
            "beta,v001,64,6.0,2.5,0.05,72,seq-2\n",
            encoding="utf-8",
        )
        rows = ingest_file(self.store, csv_path)
        self.assertEqual(len(rows), 2)

        json_path = self.root / "batch.json"
        json_path.write_text(
            json.dumps([{"client": "gamma", "metrics": {"open_rate": 58, "positive_reply_rate": 4.8, "meeting_rate": 1.9}}]),
            encoding="utf-8",
        )
        rows2 = ingest_file(self.store, json_path)
        self.assertEqual(len(rows2), 1)

        wiki = export_wiki_summary(self.store)
        self.assertIn("Cold Outbound", wiki)
        self.assertIn("3 experiments across 3 client(s)", wiki)

        chart = json.loads(raiox_chart_json(self.store))
        self.assertEqual(len(chart["rows"]), 1)
        self.assertTrue(chart["rows"][0]["label"].startswith("v001"))

    def test_drift_detection_triggers_after_score_drop(self) -> None:
        niche = get_preset("x-viral")
        self.store.init_niche(niche)
        versions = self.store.load_versions()
        versions["versions"][0]["promoted_score"] = 80.0
        self.store.save_versions(versions)

        for _ in range(6):
            record_experiment(
                self.store,
                version="v001",
                client="c1",
                variant=None,
                metrics={"bookmark_rate": 1.2, "reply_rate": 0.8, "profile_click_rate": 1.5, "follow_per_1k": 2.0},
            )
        drift = detect_drift(self.store)
        self.assertTrue(drift["drifting"])
        self.assertGreater(drift["drop_pct"], 15.0)
        attr = lever_attribution(self.store.load_experiments(), niche)
        self.assertIn("hook_style", attr)

    def test_cli_demo_evolves_and_improves_over_baseline(self) -> None:
        proc = subprocess.run(
            [sys.executable, str(ROOT / "rsi.py"), "--root", str(self.root), "demo", "--preset", "x-viral", "--cycles", "8", "--shift-at", "5", "--seed", "7"],
            capture_output=True,
            text=True,
            check=True,
        )
        data = json.loads(proc.stdout)
        self.assertEqual(data["preset"], "x-viral")
        self.assertNotEqual(data["final_champion"], "v001", "RSI loop should promote at least one better playbook")
        baseline = data["chart"]["rows"][0]["value"]
        self.assertGreater(data["final_score"], baseline, "final champion should outperform initial baseline")


if __name__ == "__main__":
    unittest.main()
