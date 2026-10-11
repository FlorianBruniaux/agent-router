"""Failed attempts remain charged even when they cannot be accepted."""
import contextlib
import importlib.util
import io
import json
import tempfile
import unittest
from pathlib import Path


SPEC = importlib.util.spec_from_file_location(
    "report_cost_pilot", Path(__file__).resolve().parents[1] / "pilot/run_pilot.py")
PILOT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PILOT)


class ReportCostTests(unittest.TestCase):
    def report(self, costs, accepted=True):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            pilot = PILOT.Pilot.__new__(PILOT.Pilot)
            pilot.results = root
            pilot.runs_dir = root / "runs"
            pilot.pricing = {"version": "synthetic"}
            pilot._manifest = {"arms": ["E"], "mode": "fake",
                               "manifest_sha256": "synthetic", "host_config": "isolated"}
            for index, (list_cost, product_cost) in enumerate(costs):
                row = {
                    "provider": "fake", "arm": "E", "task": f"t{index}", "rep": 1,
                    "invalid": [] if index == 0 else ["infrastructure_failure"],
                    "accepted": accepted and index == 0,
                    "cost_list_microusd": list_cost, "cost_product_microusd": product_cost,
                    "host_cost_usd": None, "time_to_acceptance_ms": 1,
                    "tokens": {key: None for key in ("input_tokens", "cached_input_tokens",
                               "cache_write_input_tokens", "output_tokens", "reasoning_output_tokens")},
                    "phases": [], "agent_verdict": "PASS",
                }
                target = pilot.runs_dir / str(index)
                target.mkdir(parents=True)
                (target / "result.json").write_text(json.dumps(row))
            with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
                pilot.report()
            return json.loads((root / "report.json").read_text())["per_arm"]["fake"]["E"]

    def test_invalid_attempt_costs_remain_in_total_and_accepted_ratio(self):
        result = self.report([(7000000, 14000000), (11000000, 22000000)])
        self.assertEqual(result["cost_list_usd_total"], 18)
        self.assertEqual(result["cost_product_usd_total"], 36)
        self.assertEqual(result["cost_per_accepted_list_usd"], 18)
        self.assertEqual(result["cost_per_accepted_product_usd"], 36)
        self.assertEqual((result["valid"], result["invalid"], result["accepted"]), (1, 1, 1))

    def test_unknown_invalid_cost_is_not_treated_as_free(self):
        result = self.report([(7000000, 14000000), (None, None)])
        for key in ("cost_list_usd_total", "cost_product_usd_total",
                    "cost_per_accepted_list_usd", "cost_per_accepted_product_usd"):
            self.assertIsNone(result[key])
        self.assertEqual(result["cost_unknown_runs"], 1)

    def test_unknown_product_cost_does_not_erase_known_list_cost(self):
        result = self.report([(7000000, 14000000), (11000000, None)])
        self.assertEqual(result["cost_list_usd_total"], 18)
        self.assertEqual(result["cost_per_accepted_list_usd"], 18)
        self.assertIsNone(result["cost_product_usd_total"])
        self.assertIsNone(result["cost_per_accepted_product_usd"])
        self.assertEqual(result["cost_unknown_runs"], 0)

    def test_no_accepted_patch_keeps_cost_but_has_no_ratio(self):
        result = self.report([(7000000, 14000000), (11000000, 22000000)], accepted=False)
        self.assertEqual(result["cost_list_usd_total"], 18)
        self.assertEqual(result["cost_product_usd_total"], 36)
        self.assertIsNone(result["cost_per_accepted_list_usd"])
        self.assertIsNone(result["cost_per_accepted_product_usd"])
        self.assertEqual(result["accepted"], 0)


if __name__ == "__main__":
    unittest.main()
