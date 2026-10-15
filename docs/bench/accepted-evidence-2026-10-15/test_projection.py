"""Offline regressions for the existing M4 evidence projection."""
import json
from pathlib import Path
import project
import verify
import subprocess
import sys
import unittest


HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]


class ProjectionTests(unittest.TestCase):
    def source_values(self):
        def load(path):
            return json.loads((REPO / path).read_text())
        return (
            load(f"{project.RESULTS}/result.json"),
            load(f"{project.PROTOCOL}/manifest.json"),
            load(f"{project.RESULTS}/integration-acceptance.json"),
            project.project()["dossier"]["source_files_sha256"],
        )

    def test_preserves_three_host_attempts_and_only_router_acceptance(self):
        completed = subprocess.run(
            [sys.executable, "-B", str(HERE / "project.py")],
            cwd=REPO, capture_output=True, text=True, check=False,
        )
        self.assertEqual(completed.returncode, 0, completed.stderr)
        data = json.loads(completed.stdout)
        self.assertEqual(data["dossier"]["cohort"]["host_attempt_count"], 3)
        self.assertEqual(data["dossier"]["cohort"]["host_tokens"], 156434)
        self.assertEqual(data["dossier"]["accepted_revision_count"], 1)
        selected = data["accepted_changes_input"]
        self.assertEqual(len(selected["cohort"]["assignments"]), 3)
        self.assertEqual(selected["observations"], [])
        self.assertEqual(
            [v["binding"]["assignment_id"] for v in selected["verdicts"]
             if v["status"] == "accepted"],
            ["m4v2-t1-router-r1"],
        )
        self.assertEqual([v["status"] for v in selected["verdicts"]],
                         ["pending", "pending", "accepted"])
        self.assertTrue(all(value is None for value in data["dossier"]["incomplete_scopes"].values()))
        self.assertEqual(data["dossier"]["comparison"]["verdict"], "inconclusive")

    def test_duplicate_host_response_cannot_supply_two_assigned_attempts(self):
        result, manifest, receipt, hashes = self.source_values()
        result["participants"][1]["host_evidence"]["response_records"][0]["response_identity_sha256"] = (
            result["participants"][0]["host_evidence"]["response_records"][0]["response_identity_sha256"]
        )
        with self.assertRaisesRegex(ValueError, "duplicate host response"):
            project.build(result, manifest, receipt, hashes)

    def test_omitted_rejected_assignment_is_not_a_complete_cohort(self):
        result, manifest, receipt, hashes = self.source_values()
        del result["participants"][1]
        with self.assertRaisesRegex(ValueError, "missing assigned work"):
            project.build(result, manifest, receipt, hashes)

    def test_acceptance_cannot_be_transferred_to_a_different_revision(self):
        result, manifest, receipt, hashes = self.source_values()
        receipt["candidate_file_sha256"] = "0" * 64
        with self.assertRaisesRegex(ValueError, "accepted revision substitution"):
            project.build(result, manifest, receipt, hashes)

    def test_equivalent_output_cannot_be_marked_historically_accepted(self):
        result, manifest, receipt, hashes = self.source_values()
        result["participants"][2]["evaluation"]["human_acceptance"] = "accepted"
        with self.assertRaisesRegex(ValueError, "historical verdict changed"):
            project.build(result, manifest, receipt, hashes)

    def test_report_cannot_turn_missing_money_into_zero_total_or_known_ratio(self):
        original = json.loads((HERE / "accepted-changes-report.json").read_text())
        for kind in ("model", "ci", "human"):
            with self.subTest(kind=kind, change="total"):
                report = json.loads(json.dumps(original))
                report["cohort_costs"][kind]["total_microusd"] = 0
                with self.assertRaisesRegex(ValueError, f"unknown {kind} total became known"):
                    verify.check_report(report)
            with self.subTest(kind=kind, change="ratio"):
                report = json.loads(json.dumps(original))
                report["cost_per_selected_accepted_change"][kind] = {
                    "numerator_microusd": 0, "denominator": 1,
                }
                with self.assertRaisesRegex(ValueError, f"unknown {kind} ratio became known"):
                    verify.check_report(report)


if __name__ == "__main__":
    unittest.main()
