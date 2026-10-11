#!/usr/bin/env python3
"""Replay recorded data and reject nine corrupted claims. No native child runs."""
import copy, json, pathlib, subprocess, sys, tempfile, unittest
ROOT = pathlib.Path(__file__).resolve().parents[1]
EVIDENCE = json.loads((ROOT / "evidence.json").read_text())
CASES = {r["case"]: r for r in EVIDENCE["campaign"]["cases"]}

class Receipts(unittest.TestCase):
    def run_checker(self, case, rows):
        with tempfile.TemporaryDirectory() as directory:
            path = pathlib.Path(directory) / "receipt.jsonl"
            path.write_text("".join(json.dumps(row)+"\n" for row in rows))
            return subprocess.run([sys.executable, str(ROOT/"tests/check_receipt.py"),
                case, str(CASES[case]["exit_code"]), str(path)],
                capture_output=True, timeout=5).returncode

    def test_six_recorded_outcomes(self):
        for case, result in CASES.items():
            with self.subTest(case=case):
                rows=[json.loads(line) for line in result["stdout"].splitlines()]
                self.assertEqual(self.run_checker(case, rows), 0)

    def test_nine_false_claims_are_rejected(self):
        mutations = [
            ("normal", "qualification", True),
            ("normal", "reaped", False),
            ("normal", "core_zero", False),
            ("normal", "peak_observed_bytes", 9306304),
            ("growth", "peak_observed_bytes", 9289919),
            ("failread", "decision", "completed"),
            ("mismatch", "injected", False),
            ("self", "allocation_budget_bytes", 1),
            ("self", "classification", "InstalledOnly"),
        ]
        for case, field, value in mutations:
            with self.subTest(case=case, field=field):
                rows=[json.loads(line) for line in CASES[case]["stdout"].splitlines()]
                rows[0][field]=value
                self.assertNotEqual(self.run_checker(case, rows), 0)

if __name__ == "__main__": unittest.main()
