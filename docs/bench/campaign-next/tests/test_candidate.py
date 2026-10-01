import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace


ROOT = Path(__file__).resolve().parents[1]
PILOT = ROOT / "pilot"
sys.path.insert(0, str(PILOT))
SPEC = importlib.util.spec_from_file_location("candidate_run_pilot", PILOT / "run_pilot.py")
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)
EXPORT_SPEC = importlib.util.spec_from_file_location("candidate_export_results", PILOT / "export_results.py")
EXPORT_MODULE = importlib.util.module_from_spec(EXPORT_SPEC)
EXPORT_SPEC.loader.exec_module(EXPORT_MODULE)


class CandidateRunnerContractTests(unittest.TestCase):
    def test_cheaper_role_must_beat_frontier_in_every_token_class(self):
        prices = {
            "models": {
                "frontier": {"input_microusd_per_million": 20, "cached_input_microusd_per_million": 2,
                             "output_microusd_per_million": 80},
                "cheap": {"input_microusd_per_million": 10, "cached_input_microusd_per_million": 1,
                          "output_microusd_per_million": 40},
            }
        }
        writes = {"models": {
            "frontier": {"write_5m": 30, "write_1h": 40},
            "cheap": {"write_5m": 15, "write_1h": 20},
        }}
        pools = {"codex": {"frontier": "frontier", "cheaper": "cheap"}}
        MODULE.validate_pool_prices(pools, prices, writes)
        prices["models"]["cheap"]["cached_input_microusd_per_million"] = 2
        with self.assertRaisesRegex(ValueError, "cached_input"):
            MODULE.validate_pool_prices(pools, prices, writes)
        prices["models"]["cheap"]["cached_input_microusd_per_million"] = 1
        writes["models"]["cheap"]["write_1h"] = 40
        with self.assertRaisesRegex(ValueError, "write_1h"):
            MODULE.validate_pool_prices(pools, prices, writes)
        required = {"codex": {"frontier": "frontier", "cheaper": "cheap",
                              "cache_write_classes": ["write_5m", "write_1h"]}}
        writes["models"].pop("cheap")
        with self.assertRaisesRegex(ValueError, "missing required cache-write"):
            MODULE.validate_pool_prices(required, prices, writes)

    def test_copied_historical_price_snapshot_is_not_ready_for_candidate_pool(self):
        pilot = json.loads((PILOT / "pilot-config.example.json").read_text())
        pricing = json.loads((PILOT / "pricing.json").read_text())
        writes = json.loads((PILOT / "pricing-cache-writes.json").read_text())
        with self.assertRaises(ValueError):
            MODULE.validate_pool_prices(pilot["providers"], pricing, writes)

    def test_codex_completion_events_are_counted_per_process(self):
        pilot = MODULE.Pilot.__new__(MODULE.Pilot)
        parsed = pilot.parse_stream("codex", '\n'.join([
            '{"type":"turn.completed","usage":{"input_tokens":3,"cached_input_tokens":0,"output_tokens":1}}',
            '{"type":"turn.completed","usage":{"input_tokens":4,"cached_input_tokens":0,"output_tokens":1}}',
        ]))
        self.assertEqual(parsed["turns_per_process"], 2)
        exported = EXPORT_MODULE.reduce_phase({"turns_per_process": parsed["turns_per_process"], "errors": []})
        self.assertEqual(exported["turns_per_process"], 2)

    def test_real_calibration_requires_confirmation_and_explicit_caps(self):
        pilot = MODULE.Pilot.__new__(MODULE.Pilot)
        pilot.fake = False
        pilot.args = SimpleNamespace(confirm_spend=False, global_budget_cap_usd=None, estimated_remaining_usd=None)
        pilot.providers = ["codex"]
        pilot.validate_pricing = lambda: None
        pilot.has_unknown_total_cost = lambda: False
        pilot.spent_total_usd = lambda: 0.0
        with self.assertRaisesRegex(SystemExit, "confirm-spend"):
            pilot.require_spend_authorization()
        pilot.args.confirm_spend = True
        with self.assertRaisesRegex(SystemExit, "global-budget-cap-usd"):
            pilot.require_spend_authorization()
        pilot.args.global_budget_cap_usd = "5"
        with self.assertRaisesRegex(SystemExit, "UNKNOWN"):
            pilot.require_spend_authorization()
        pilot.args.global_budget_cap_usd = "5.01"
        pilot.args.estimated_remaining_usd = 1
        with self.assertRaisesRegex(SystemExit, "current 5.00 USD authorization"):
            pilot.require_spend_authorization()
        pilot.args.global_budget_cap_usd = "5"
        pilot.args.estimated_remaining_usd = 5.01
        with self.assertRaisesRegex(SystemExit, "would be exceeded"):
            pilot.require_spend_authorization()
        pilot.args.estimated_remaining_usd = 4.0
        with self.assertRaisesRegex(SystemExit, "no verified per-process"):
            pilot.require_spend_authorization()

    def test_spent_total_includes_overhead_probes(self):
        pilot = MODULE.Pilot.__new__(MODULE.Pilot)
        with tempfile.TemporaryDirectory() as tmp:
            pilot.results = Path(tmp)
            pilot.runs_dir = pilot.results / "runs"
            overhead = pilot.results / "overhead" / "codex-model-user-config"
            overhead.mkdir(parents=True)
            (overhead / "result.json").write_text(json.dumps({"provider": "codex", "cost": {"list_microusd": 250000}}))
            self.assertEqual(pilot.spent_usd("codex"), 0.25)
            self.assertFalse(pilot.has_unknown_cost("codex"))
            pilot.providers = ["codex"]
            pilot.config = {"providers": {"codex": {}}}
            self.assertEqual(pilot.spent_total_usd(), 0.25)

    def test_run_entries_fake_logs_spend_before_execution_without_name_error(self):
        pilot = MODULE.Pilot.__new__(MODULE.Pilot)
        pilot.providers = ["codex"]
        pilot.fake = True
        with tempfile.TemporaryDirectory() as tmp:
            pilot.results = Path(tmp)
            pilot.runs_dir = pilot.results / "runs"
            pilot.check_manifest = lambda: None
            pilot.execute_run = lambda _entry: {
                "invalid": [], "accepted": True, "cost_list_microusd": 0,
            }
            entry = {"provider": "codex", "arm": "A", "task": "t01", "rep": 1}
            from unittest.mock import patch
            with patch.object(MODULE, "log"):
                pilot.run_entries([entry], budget=None)

    def test_total_spend_and_unknowns_cover_configured_providers_not_cli_filter(self):
        pilot = MODULE.Pilot.__new__(MODULE.Pilot)
        pilot.providers = ["codex"]
        pilot.config = {"providers": {"claude": {}, "codex": {}}}
        with tempfile.TemporaryDirectory() as tmp:
            pilot.results = Path(tmp)
            pilot.runs_dir = pilot.results / "runs"
            overhead = pilot.results / "overhead" / "codex-probe"
            overhead.mkdir(parents=True)
            (overhead / "result.json").write_text(json.dumps({"cost": {"list_microusd": 250000}}))
            claude_result = pilot.runs_dir / "claude-A-t01-r1" / "result.json"
            claude_result.parent.mkdir(parents=True)
            claude_result.write_text(json.dumps({"cost": {"list_microusd": 500000}}))
            self.assertEqual(pilot.spent_total_usd(), 0.75)
            self.assertFalse(pilot.has_unknown_total_cost())
            (pilot.runs_dir / "claude-B-t02-r1.interrupted-1").mkdir()
            self.assertTrue(pilot.has_unknown_total_cost())

    def test_incomplete_process_record_makes_prior_spend_unknown(self):
        pilot = MODULE.Pilot.__new__(MODULE.Pilot)
        with tempfile.TemporaryDirectory() as tmp:
            pilot.results = Path(tmp)
            pilot.runs_dir = pilot.results / "runs"
            (pilot.runs_dir / "codex-B-t01-r1.interrupted-1").mkdir(parents=True)
            self.assertTrue(pilot.has_unknown_cost("codex"))

    def test_catalog_budget_cost_without_list_price_is_unknown(self):
        pilot = MODULE.Pilot.__new__(MODULE.Pilot)
        with tempfile.TemporaryDirectory() as tmp:
            pilot.results = Path(tmp)
            pilot.runs_dir = pilot.results / "runs"
            result_path = pilot.runs_dir / "claude-A-t01-r1" / "result.json"
            result_path.parent.mkdir(parents=True)
            result_path.write_text(json.dumps({
                "cost_list_microusd": None,
                "cost_product_microusd": 700000,
                "tokens": {"cache_write_input_tokens": 1000},
            }))
            self.assertEqual(pilot.spent_usd("claude"), 0)
            self.assertTrue(pilot.has_unknown_cost("claude"))

    def test_calibration_reports_real_global_budget_options(self):
        pilot = MODULE.Pilot.__new__(MODULE.Pilot)
        pilot.fake = True
        pilot.results = Path("unused")
        pilot.config = {"tasks": ["t01"], "repetitions": 1}
        pilot.manifest = lambda: {"order": [{"task": "t01", "rep": 1}]}
        pilot.check_manifest = lambda: None
        pilot.measure_overhead = lambda _budget: None
        pilot.run_entries = lambda _block, _budget: None
        pilot.report = lambda: None
        messages = []
        from unittest.mock import patch
        with patch.object(MODULE, "load_json", return_value={"per_arm": {}}), \
                patch.object(MODULE, "log", side_effect=messages.append):
            pilot.calibrate()
        joined = "\n".join(messages)
        self.assertIn("--global-budget-cap-usd", joined)
        self.assertIn("--estimated-remaining-usd", joined)
        self.assertNotIn("--budget-cap-usd", joined)

    def test_existing_base_repo_divergence_is_rejected_without_rebuilding(self):
        pilot = MODULE.Pilot.__new__(MODULE.Pilot)
        with tempfile.TemporaryDirectory() as tmp:
            pilot.results = Path(tmp)
            base = pilot.build_base_repo()
            target = base / "pilot_lib" / "pagination.py"
            target.write_text("def collect_pages(fetch_page, max_pages=100): return []\n")
            pilot.check_manifest = lambda: pilot.validate_base_repo(base)
            invoked = []
            pilot.execute_run = lambda _entry: invoked.append(True)
            pilot.providers = ["claude"]
            pilot.runs_dir = pilot.results / "runs"
            entry = {"provider": "claude", "arm": "A", "task": "t01", "rep": 1}
            with self.assertRaisesRegex(SystemExit, "does not match current corpus/base"):
                pilot.run_entries([entry], budget=None)
            self.assertEqual(invoked, [])
            self.assertEqual(target.read_text(), "def collect_pages(fetch_page, max_pages=100): return []\n")

    def test_global_budget_rejects_spend_plus_reservation_above_cap(self):
        pilot = MODULE.Pilot.__new__(MODULE.Pilot)
        pilot.has_unknown_total_cost = lambda: False
        pilot.spent_total_usd = lambda: 4.2
        with self.assertRaisesRegex(SystemExit, "would be exceeded"):
            pilot.check_global_budget({"cap_usd": 5.0, "estimated_remaining_usd": 0.9})

    def test_pycache_is_ignored_by_git_but_other_writes_are_visible(self):
        import subprocess

        ignore_file = PILOT / "corpus" / "base" / ".gitignore"
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "tests" / "__pycache__").mkdir(parents=True)
            (root / "tests" / "__pycache__" / "test.cpython.pyc").write_text("cache")
            (root / "outside.txt").write_text("visible")
            (root / ".gitignore").write_text(ignore_file.read_text())
            subprocess.run(["git", "init", "-q"], cwd=root, check=True)
            status = subprocess.run(["git", "status", "--porcelain", "--untracked-files=all"],
                                    cwd=root, check=True, capture_output=True, text=True).stdout
            self.assertIn("outside.txt", status)
            self.assertNotIn("__pycache__", status)


if __name__ == "__main__":
    unittest.main()
