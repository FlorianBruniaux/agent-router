import importlib.util
from pathlib import Path
import unittest
import os
import json
import tempfile

spec = importlib.util.spec_from_file_location('qualify', Path(__file__).with_name('qualify.py'))
q = importlib.util.module_from_spec(spec)
spec.loader.exec_module(q)

class ResultClassification(unittest.TestCase):
    def test_success_requires_complete_expected_test_population(self):
        output = b'test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n'
        self.assertEqual(q.classify(0, output, 4), 'passed')
        self.assertEqual(q.classify(0, output, 5), 'unknown')

    def test_test_failure_is_a_functional_failure_only_with_a_complete_summary(self):
        output = b'test result: FAILED. 3 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n'
        self.assertEqual(q.classify(101, output, 4), 'failed')
        self.assertEqual(q.classify(1, output, 4), 'unknown')
        self.assertEqual(q.classify(101, b'compiler or runtime failed', 4), 'unknown')

    def test_empty_ignored_filtered_ambiguous_and_oversized_output_are_unknown(self):
        for output in [
            b'test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0s\n',
            b'test result: ok. 3 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0s\n',
            b'test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out; finished in 0s\n',
            b'x' * 65537,
        ]:
            self.assertEqual(q.classify(0, output, 4), 'unknown')
        summary=b'test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0s\n'
        self.assertEqual(q.classify(0, summary+summary, 4), 'unknown')

class BoundedInputs(unittest.TestCase):
    def test_regular_single_link_bounded_input(self):
        with tempfile.TemporaryDirectory() as tmp:
            p = Path(tmp) / 'input'
            p.write_bytes(b'1234')
            self.assertEqual(q.read_input(p, 4), b'1234')
            with self.assertRaises(ValueError):
                q.read_input(p, 3)
            os.link(p, Path(tmp) / 'hardlink')
            with self.assertRaises(ValueError):
                q.read_input(p)

    def test_symlink_and_special_file_are_refused(self):
        with tempfile.TemporaryDirectory() as tmp:
            p = Path(tmp) / 'input'
            p.symlink_to('/dev/null')
            with self.assertRaises(OSError):
                q.read_input(p)
            with self.assertRaises(ValueError):
                q.read_input('/dev/null')

    def test_missing_compiler_is_unknown_not_functional_failure(self):
        with tempfile.TemporaryDirectory() as tmp:
            result = q.run_suite('/nonexistent-qa174', b'', b'', 4, tmp)
            self.assertEqual(result['outcome'], 'unknown')
            self.assertEqual(result['stage'], 'compile')
            self.assertEqual(result['reason'], 'spawn_failed')

class StoredEvidence(unittest.TestCase):
    def setUp(self):
        self.root = Path(__file__).resolve().parent
        self.result = json.loads((self.root / 'result.json').read_text())

    def test_replayed_sources_match_reviewed_labels_and_report(self):
        manifest_bytes = (self.root / 'controls.json').read_bytes()
        manifest = json.loads(manifest_bytes)
        labels = json.loads((self.root / 'labels.json').read_text())
        self.assertEqual(labels['reviewed_input_sha256'], manifest)
        self.assertEqual(self.result['input_sha256'], manifest)
        self.assertEqual(self.result['manifest_sha256'], q.digest(manifest_bytes))
        self.assertEqual(labels['manifest_sha256'], q.digest(manifest_bytes))
        self.assertEqual(self.result['runner_sha256'], q.digest((self.root / 'qualify.py').read_bytes()))
        for path, expected in manifest.items():
            self.assertEqual(q.digest((self.root / path).read_bytes()), expected)
        self.assertFalse(labels['hidden_oracle_read_by_reviewer'])
        self.assertFalse(labels['oracle_results_available_during_review'])
        self.assertIsNone(labels['human_review'])

    def test_recorded_counterexample_and_infrastructure_outcomes(self):
        rows = {row['control']: row for row in self.result['controls']}
        self.assertEqual(set(rows), set(q.CONTROLS))
        for name, row in rows.items():
            expected_public = 'passed' if q.CONTROLS[name] == 'correct' else 'failed'
            expected_oracle = 'passed' if name in ('correct_i128', 'correct_u128', 'sum_u64') else 'failed'
            self.assertEqual(row['public_witnesses']['outcome'], expected_public)
            self.assertEqual(row['frozen_oracle']['outcome'], expected_oracle)
        self.assertEqual(self.result['missed_defects'], ['sum_u64'])
        self.assertEqual(self.result['false_rejections'], [])
        self.assertEqual(self.result['verdict'], 'unqualified')
        self.assertTrue(self.result['public_labels_match'])
        self.assertEqual([r['outcome'] for r in self.result['infrastructure_probes']], ['unknown', 'unknown'])
        self.assertFalse(self.result['adoption_authorized'])
        self.assertEqual(self.result['oracle_authenticity'], 'supplied_unverified')
        self.assertIsNone(self.result['human_acceptance'])
        self.assertEqual(self.result['model_calls'], 0)
        self.assertFalse(self.result['registry_access'])

    def test_acceptance_binding_matches_retained_r2_packet(self):
        provenance = json.loads((self.root / 'provenance.json').read_text())
        canonical = json.dumps(provenance['binding'], sort_keys=True, separators=(',', ':')).encode()
        self.assertEqual(q.digest(canonical), provenance['acceptance_sha256'])
        self.assertEqual(provenance['acceptance_sha256'], '072e26ddcfb71bfd284cbced9acdd3dc3c04ae72960d48f6676f7bc339224563')
        self.assertEqual(provenance['binding']['external_files']['hidden.rs'], q.ORACLE_SHA256)
        self.assertEqual(self.result['oracle_sha256'], q.ORACLE_SHA256)
        self.assertIsNone(provenance['human_review'])

    def test_recorded_linux_limits_and_private_boundary(self):
        isolation = json.loads((self.root / 'isolation.json').read_text())
        self.assertEqual(self.result['limits'], {'memory.max': str(q.MEMORY), 'memory.swap.max': '0'})
        self.assertEqual(isolation['memory_bytes'], q.MEMORY)
        self.assertEqual(isolation['memory_plus_swap_bytes'], q.MEMORY)
        self.assertEqual(isolation['network_mode'], 'none')
        self.assertEqual(isolation['user'], '65534:65534')
        self.assertTrue(isolation['read_only_root'])
        self.assertEqual(isolation['cap_drop'], ['ALL'])
        self.assertIn('no-new-privileges', isolation['security_opt'])
        self.assertFalse(isolation['binds'])
        self.assertEqual(isolation['mounts'], [])

if __name__ == '__main__':
    unittest.main()
