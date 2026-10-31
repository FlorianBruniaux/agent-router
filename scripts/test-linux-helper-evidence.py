#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Florian Bruniaux
# SPDX-License-Identifier: Apache-2.0
"""Offline negative witnesses for the real-helper diagnostic's acceptance gate."""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import unittest

HERE = Path(__file__).resolve().parents[1] / 'docs/bench/linux-helper-memory-2026-10-11'
spec = importlib.util.spec_from_file_location('linux_helper_diagnostic', HERE / 'run.py')
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)
EVIDENCE = json.loads((HERE / 'evidence.json').read_text())


class EvidenceTests(unittest.TestCase):
    def setUp(self):
        self.row = copy.deepcopy(EVIDENCE['runtime'])
        self.receipt = copy.deepcopy(EVIDENCE['runtime_receipt'])

    def reject(self):
        with self.assertRaises((AssertionError, KeyError)):
            runner.validate(self.row, self.receipt)

    def test_observed_receipt_and_exact_fixture(self):
        runner.validate(self.row, self.receipt)
        self.assertEqual(hashlib.sha256((HERE / 'inside.py').read_bytes()).hexdigest(), self.receipt['fixture_sha256'])
        self.assertEqual(self.receipt['helper_sha256'], EVIDENCE['binary_sha256'])

    def test_container_cleanup_unknown(self):
        self.receipt['container_absent'] = None
        self.reject()

    def test_helper_identity_mismatch(self):
        self.receipt['helper_sha256'] = '0' * 64
        self.reject()

    def test_no_native_promotion(self):
        self.row['f12a'] = 'GO'
        self.reject()

    def test_preinstalled_controller_limit_is_not_causal_evidence(self):
        self.row['controller_limits_before_after'] = [runner.BOUND] * 2
        self.reject()

    def test_requested_limit_changed(self):
        for bound in (None, runner.BOUND + 1, runner.BOUND - 1):
            with self.subTest(bound=bound):
                self.row['requested_address_space_bytes'] = bound
                self.reject()

    def test_worker_or_inherited_limit_missing(self):
        self.row['direct']['fork']['soft_hard'] = [-1, -1]
        self.reject()

    def test_allocation_refusal_missing(self):
        self.row['supervision'][0]['worker']['exec']['memory']['large_allocation_refused'] = False
        self.reject()

    def test_wrong_deadline_exit(self):
        self.row['supervision'][1]['exit_code'] = 0
        self.reject()

    def test_descendant_cleanup_unobserved(self):
        self.row['supervision'][2]['adopted_reaped'] = []
        self.reject()

    def test_group_receipt_cannot_claim_namespace(self):
        self.row['supervision'][3]['lease'] = 'namespace-clean'
        self.reject()

    def test_unbounded_capture(self):
        self.receipt['elapsed_seconds'] = 46
        self.reject()

    def test_cgroup_swap_changed(self):
        self.row['cgroups']['memory.swap.max'] = str(runner.BOUND)
        self.reject()

    def test_run_has_no_pull_network_or_host_mount(self):
        argv = runner.command('/usr/local/bin/docker', 'ar-f12a-test')
        for flag in ('--pull=never', '--network=none', '--read-only', '--cap-drop=ALL',
                     '--security-opt=no-new-privileges', '--memory=8589934592', '--memory-swap=8589934592'):
            self.assertIn(flag, argv)
        self.assertFalse(any(x == '-v' or x.startswith(('--mount', '--volume', '--privileged')) for x in argv))
        self.assertIn(runner.IMAGE, argv)


if __name__ == '__main__':
    unittest.main(verbosity=2)
