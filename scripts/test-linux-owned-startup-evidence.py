#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Florian Bruniaux
# SPDX-License-Identifier: Apache-2.0
"""Negative diagnostic receipt witnesses, not native qualification."""
import copy
import importlib.util
import hashlib
import json
from pathlib import Path
import unittest
ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('owned_startup', ROOT/'docs/bench/linux-owned-startup-2026-10-11/run.py')
r = importlib.util.module_from_spec(spec); spec.loader.exec_module(r)

class Startup(unittest.TestCase):
    def setUp(self):
        # Historical receipt plus synthetic startup fields: never a runtime proof.
        old = json.loads((ROOT/'docs/bench/linux-copied-client-2026-10-11/evidence.json').read_text())
        self.row = copy.deepcopy(old['campaign']['cases'][0])
        self.row['provisional'].update(startup_contract='producer-owned-v1', startup_binding_sha256='a'*64, native_environment_sha256='b'*64)
    def test_accepts_complete_synthetic_startup(self):
        r.validate_complete(self.row)
    def test_refuses_absent_changed_or_invalid_startup_binding(self):
        for key in ('startup_contract', 'startup_binding_sha256', 'native_environment_sha256'):
            for bad in (None, False, '', 'unknown', 'z'*64):
                with self.subTest(key=key, bad=bad):
                    row=copy.deepcopy(self.row); row['provisional'][key]=bad
                    with self.assertRaises((AssertionError,KeyError,TypeError)):
                        r.validate_complete(row)
            row=copy.deepcopy(self.row); row['provisional'].pop(key)
            with self.assertRaises((AssertionError,KeyError,TypeError)):
                r.validate_complete(row)

    def test_retained_current_campaign(self):
        evidence = json.loads((ROOT/'docs/bench/linux-owned-startup-2026-10-11/evidence.json').read_text())
        self.assertEqual(evidence['schema'], 'linux-owned-startup-diagnostic-v1')
        self.assertEqual(evidence['f12a'], 'NO_GO')
        summary = evidence['campaign']
        self.assertEqual([row['case'] for row in summary['cases']], list(r.base.CASES))
        for row in summary['cases']:
            r.validate_complete(row)
            self.assertEqual(row['provisional']['copy_hashes'][:4],
                [summary['binaries_sha256'][key] for key in ('client','native','helper','wrapper')])
        for key in ('startup_binding_sha256','native_environment_sha256'):
            self.assertEqual(len({row['provisional'][key] for row in summary['cases']}), 4)
        for key in ('client-build','qualified-build'):
            receipt=evidence['build_receipts'][key]
            self.assertEqual(receipt['exit_code'],0)
            self.assertIsNone(receipt['stop_reason'])
            self.assertIs(receipt['container_absent'],True)
        for source, pin in evidence['source_inputs']['final_controller']['source_sha256'].items():
            self.assertEqual(evidence['source_sha256'][source],pin)
        # Diagnostic fixtures are retained immutable. Product source provenance
        # is frozen build metadata, not an expectation that future HEAD equals it.
        for source, pin in evidence['source_sha256'].items():
            if source.startswith('docs/bench/'):
                self.assertEqual(hashlib.sha256((ROOT/source).read_bytes()).hexdigest(),pin)
        self.assertEqual(summary['inside_sha256'],evidence['source_sha256']['docs/bench/linux-copied-client-2026-10-11/inside.py'])

if __name__ == '__main__': unittest.main(verbosity=2)
