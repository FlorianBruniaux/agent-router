"""Prove these criteria remain unmet on their sealed base, without solving them."""
import importlib.util
import os
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
REPO = Path(os.environ.get('M3_REPO', ROOT.parents[3]))


class CandidateTests(unittest.TestCase):
    def test_baseline_public_and_private_criteria_unmet(self):
        spec = importlib.util.spec_from_file_location('acceptance', ROOT / 'acceptance.py')
        acceptance = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(acceptance)
        for task in ('t1', 't2', 't3'):
            for hidden in (None, ROOT / 'verifier/hidden_cases.json'):
                with self.subTest(task=task, hidden=bool(hidden)):
                    report = acceptance.check(REPO, task, hidden)
                    self.assertFalse(report['passed'], 'candidate was solved or baseline drifted; reselect corpus')
                    self.assertGreater(report['cases'], report['passed_cases'])

    def test_bug_reproduction_is_observed_one_instead_of_three(self):
        spec = importlib.util.spec_from_file_location('acceptance', ROOT / 'acceptance.py')
        acceptance = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(acceptance)
        passed, row = acceptance.report_case(REPO, {'accepted': 1, 'valid_cost': 1, 'invalid_cost': 2})
        self.assertFalse(passed)
        self.assertEqual(row['cost_list_usd_total'], 1)
        self.assertEqual(row['cost_per_accepted_list_usd'], 1)


if __name__ == '__main__':
    unittest.main()
