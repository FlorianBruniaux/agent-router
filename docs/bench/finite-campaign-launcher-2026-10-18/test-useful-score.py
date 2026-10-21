"""Offline regression: successful transport must not stand in for acceptance."""
import subprocess
import sys
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent

class UsefulScoreMode(unittest.TestCase):
    def test_explicit_correctness_mode_is_available_without_dispatch(self):
        result = subprocess.run([sys.executable, '-B', str(HERE/'qualify-offline.py'), '--help'], capture_output=True, text=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('--useful-score', result.stdout)

    def test_other_assignment_refused_before_any_output_or_container_read(self):
        result = subprocess.run([sys.executable, '-B', str(HERE/'qualify-offline.py'), 't05-Lm-r1-H0', '--useful-score', '--output', '/does-not-exist', '--recipe', '/does-not-exist'], capture_output=True, text=True, timeout=10)
        self.assertEqual(result.returncode, 2)
        self.assertIn('requires the sealed t02-Lm-r1-H1 packet', result.stderr)
        self.assertNotIn('Traceback', result.stderr)

    def test_evaluator_requires_explicit_useful_score_before_reading_container(self):
        result = subprocess.run([sys.executable, '-B', str(HERE/'qualify-offline.py'), 't02-Lm-r1-H1', '--evaluate-public-score', '--output', '/does-not-exist', '--recipe', '/does-not-exist'], capture_output=True, text=True, timeout=10)
        self.assertEqual(result.returncode, 2)
        self.assertIn('--evaluate-public-score requires --useful-score', result.stderr)
        self.assertNotIn('Traceback', result.stderr)

if __name__ == '__main__':
    unittest.main()
