#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Florian Bruniaux
# SPDX-License-Identifier: Apache-2.0
"""Regression checks for retention of first-launch evidence; no product runs."""
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("startup_measurement", Path(__file__).with_name("benchmark-startup.py"))
measurement = importlib.util.module_from_spec(spec)
spec.loader.exec_module(measurement)


class StartupEvidenceTests(unittest.TestCase):
    def arguments(self, binary, report):
        return ["benchmark-startup.py", "--binary", str(binary), "--output", str(report),
                "--label", "synthetic-collector-test", "--source-sha256", "a" * 64,
                "--samples", "10"]

    def test_metadata_failure_preserves_every_launch_including_first(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            report = root / "report.json"
            # The later executable inspection fails. Timings must already be
            # durable in the independently readable journal.
            samples = [{"elapsed_ms": value, "exit_code": 0, "output_limit_exceeded": False}
                       for value in [503.0, 2.0, 2.0, 2.0, 177.0, 2.0, 2.0, 2.0, 2.0, 2.0]]
            with patch.object(sys, "argv", self.arguments(root / "missing", report)), \
                 patch.object(measurement, "launch", side_effect=samples):
                with self.assertRaises(FileNotFoundError):
                    measurement.main()
            retained = [json.loads(line) for line in report.with_suffix(".launches.jsonl").read_text().splitlines()]
            self.assertEqual(retained, samples)
            self.assertFalse(report.exists())

    def test_failed_spawn_and_timeout_are_observations(self):
        for error, status in [(FileNotFoundError(), "spawn_failed"),
                              (subprocess.TimeoutExpired("synthetic", 10), "timeout")]:
            with patch.object(measurement.subprocess, "run", side_effect=error):
                sample = measurement.launch(Path("synthetic"), "--version")
            self.assertEqual(sample["termination"], status)
            self.assertIsNone(sample["exit_code"])
            self.assertGreaterEqual(sample["elapsed_ms"], 0)

    def test_existing_report_is_never_replaced_or_remeasured(self):
        with tempfile.TemporaryDirectory() as directory:
            report = Path(directory) / "report.json"
            report.write_text("earlier evidence")
            with patch.object(sys, "argv", self.arguments(Path("synthetic"), report)), \
                 patch.object(measurement, "launch") as launch, \
                 patch.object(sys, "stderr", io.StringIO()):
                with self.assertRaises(SystemExit):
                    measurement.main()
            launch.assert_not_called()
            self.assertEqual(report.read_text(), "earlier evidence")


if __name__ == "__main__":
    unittest.main()
