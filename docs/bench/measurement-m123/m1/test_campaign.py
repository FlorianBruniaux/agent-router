"""Preparation contracts, with an actual CLI route test when explicitly supplied."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("campaign", ROOT / "campaign.py")
campaign = None
if (ROOT / "campaign.py").is_file():
    campaign = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(campaign)


class CampaignContracts(unittest.TestCase):
    def setUp(self):
        self.assertIsNotNone(campaign, "campaign preparation helper is missing")
        self.catalog = json.loads((ROOT / "catalog.json").read_text())
        self.host = json.loads((ROOT / "host-snapshot.json").read_text())
        self.provenance = json.loads((ROOT / "provenance.json").read_text())
        self.now = self.provenance["observed_at_unix_seconds"]

    def prepare(self):
        return campaign.prepare(self.catalog, self.host, self.provenance, self.now)

    def test_absent_model_excluded_before_scoring(self):
        self.host["supported_models"].remove("gpt-6-astra")
        self.host["supported_efforts"].pop("gpt-6-astra")
        self.provenance["host_sha256"] = campaign.digest(self.host)
        report = self.prepare()
        self.assertNotIn("gpt-6-astra", {c["model"] for c in report["eligible"]})
        self.assertIn({"model": "gpt-6-astra", "effort": None, "reason": "model_absent"}, report["excluded"])

    def test_incompatible_effort_excluded(self):
        self.host["supported_efforts"]["gpt-6-astra"].remove("ultra")
        self.provenance["host_sha256"] = campaign.digest(self.host)
        report = self.prepare()
        self.assertNotIn({"model": "gpt-6-astra", "effort": "ultra", "stages": ["plan", "execute", "verify"]}, report["eligible"])
        self.assertIn({"model": "gpt-6-astra", "effort": "ultra", "reason": "effort_incompatible"}, report["excluded"])

    def test_at_least_two_models_and_intersection(self):
        report = self.prepare()
        self.assertEqual(report["status"], "READY_FOR_PREPARATION")
        self.assertGreaterEqual(len({c["model"] for c in report["eligible"]}), 2)
        self.assertEqual(len(report["eligible"]), 29)
        self.assertFalse(report["execution_authorized"])
        self.assertEqual(report["provider_inference"], "UNKNOWN")
        for c in report["eligible"]:
            self.assertIn(c["model"], self.host["supported_models"])
            self.assertIn(c["effort"], self.host["supported_efforts"][c["model"]])

    def test_one_model_is_insufficient(self):
        self.host["supported_models"] = ["gpt-6.1-sol"]
        self.host["supported_efforts"] = {"gpt-6.1-sol": ["high"]}
        self.provenance["host_sha256"] = campaign.digest(self.host)
        self.assertEqual(self.prepare()["status"], "INSUFFICIENT_MODEL_CHOICE")

    def test_stale_future_and_unbound_provenance_refused(self):
        for now in (self.now - 1, self.now + 86401):
            with self.assertRaises(ValueError):
                campaign.prepare(self.catalog, self.host, self.provenance, now)
        self.provenance["host_sha256"] = "0" * 64
        with self.assertRaises(ValueError):
            self.prepare()

    def test_undeclared_model_and_effort_substitution(self):
        requested = {"model": "gpt-6-astra", "effort": "high"}
        for observed in ({"model": "gpt-6-sol", "effort": "high"}, {"model": "gpt-6-astra", "effort": "medium"}):
            result = campaign.assess_identity(requested, observed)
            self.assertEqual(result["status"], "SUBSTITUTION_UNDECLARED")
            self.assertFalse(result["matches_requested"])
            self.assertFalse(result["provider_identity_verified"])

    def test_unknown_identity_does_not_match(self):
        requested = {"model": "gpt-6-astra", "effort": "high"}
        for observed in (None, {"model": None, "effort": "high"}, {"model": "gpt-6-astra", "effort": None}):
            result = campaign.assess_identity(requested, observed)
            self.assertEqual(result["status"], "UNKNOWN")
            self.assertIsNone(result["matches_requested"])
        result = campaign.assess_identity(requested, requested)
        self.assertEqual(result["status"], "MATCHED_SUPPLIED")
        self.assertFalse(result["provider_identity_verified"])

    def test_declared_substitution_preserved_but_not_match(self):
        requested = {"model": "gpt-6-astra", "effort": "high"}
        observed = {"model": "gpt-6-sol", "effort": "high"}
        declaration = {"requested": requested, "observed": observed, "reason": "synthetic fixture only"}
        result = campaign.assess_identity(requested, observed, declaration)
        self.assertEqual(result["status"], "SUBSTITUTION_DECLARED")
        self.assertFalse(result["matches_requested"])
        declaration["observed"] = requested
        with self.assertRaises(ValueError):
            campaign.assess_identity(requested, observed, declaration)

    def test_bounded_json_and_duplicate_keys(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "bad.json"
            for raw in ('{"x": 1, "x": 2}', ' ' * (campaign.MAX_BYTES + 1)):
                path.write_text(raw)
                with self.assertRaises(ValueError):
                    campaign.load_json(path)
        self.catalog["unexpected"] = True
        with self.assertRaises(ValueError):
            self.prepare()

    def test_malformed_nested_choices_and_model_id_are_refused(self):
        self.catalog["models"][0]["efforts"] = [["high"]]
        with self.assertRaises(ValueError):
            self.prepare()
        self.catalog["models"][0]["efforts"] = ["high"]
        self.catalog["models"][0]["id"] = ["bad"]
        with self.assertRaises(ValueError):
            self.prepare()

    def test_request_embeds_exact_host_without_capability_claims(self):
        request = campaign.load_json(ROOT / "request-template.json")
        self.assertEqual(request["host"], self.host)
        self.assertEqual(request["task"]["required_capabilities"], [])
        self.assertEqual(request["capability_knowledge"], {})

    def test_actual_cli_route_is_deterministic_and_intersection_only(self):
        binary = os.environ.get("AGENT_ROUTER_BIN")
        if not binary:
            self.skipTest("set AGENT_ROUTER_BIN to a reviewed existing binary; no Cargo build")
        args = [binary, "route", "--catalog", str(ROOT / "catalog.json"), "--request", str(ROOT / "request-template.json"), "--json"]
        first = subprocess.run(args, check=True, capture_output=True, timeout=10).stdout
        second = subprocess.run(args, check=True, capture_output=True, timeout=10).stdout
        self.assertEqual(first, second)
        decision = json.loads(first)
        eligible = {(c["model"], c["effort"]) for c in self.prepare()["eligible"]}
        selected = decision["decision"]["selected"]
        plans = [selected["plan"]] + selected["alternatives"]
        recommended_models = set()
        for plan in plans:
            self.assertEqual(len(plan["stages"]), 3)
            for stage in plan["stages"]:
                self.assertIn((stage["model"], stage["effort"]), eligible)
                recommended_models.add(stage["model"])
        self.assertGreaterEqual(len(recommended_models), 2)
        # Exercise actual product constraints with a host projection changed only
        # in temporary requests, without modifying the campaign artifacts.
        request = campaign.load_json(ROOT / "request-template.json")
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "request.json"
            request["host"]["supported_models"].remove("gpt-6-luna")
            request["host"]["supported_efforts"].pop("gpt-6-luna")
            path.write_text(json.dumps(request))
            changed_args = [binary, "route", "--catalog", str(ROOT / "catalog.json"), "--request", str(path), "--json"]
            absent = json.loads(subprocess.run(changed_args, check=True, capture_output=True, timeout=10).stdout)["decision"]["selected"]
            for plan in [absent["plan"]] + absent["alternatives"]:
                self.assertNotIn("gpt-6-luna", {stage["model"] for stage in plan["stages"]})
            request = campaign.load_json(ROOT / "request-template.json")
            request["host"]["supported_efforts"]["gpt-6-luna"] = ["low"]
            path.write_text(json.dumps(request))
            incompatible = json.loads(subprocess.run(changed_args, check=True, capture_output=True, timeout=10).stdout)["decision"]["selected"]
            for plan in [incompatible["plan"]] + incompatible["alternatives"]:
                for stage in plan["stages"]:
                    if stage["model"] == "gpt-6-luna":
                        self.assertEqual(stage["effort"], "low")
            self.assertTrue(any(item["model"] == "gpt-6-luna" and item["code"] == "host.effort.unsupported" for item in incompatible["exclusions"]))


if __name__ == "__main__":
    unittest.main()
