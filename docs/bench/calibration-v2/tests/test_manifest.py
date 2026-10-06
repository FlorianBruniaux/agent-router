"""Synthetic preparation fixtures only: no host execution, real criteria or approval."""
import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]


class ManifestTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        shutil.copytree(ROOT / 'fixtures/synthetic', self.root, dirs_exist_ok=True)
        self.manifest = json.loads((self.root / 'manifest.json').read_text())

    def run_validator(self, manifest=None, raw=None):
        source = self.root / 'manifest.json'
        source.write_text(raw if raw is not None else json.dumps(manifest or self.manifest))
        script = ROOT / 'validate_manifest.py'
        self.assertTrue(script.exists(), 'U08 executable validator is missing')
        result = subprocess.run([sys.executable, str(script), str(source)], capture_output=True, text=True, timeout=15)
        self.assertEqual(result.stderr, '', 'validator must not emit a traceback')
        report = json.loads(result.stdout)
        self.assertIn(result.returncode, (0, 1, 2))
        return result.returncode, report

    def reject(self, mutate, expected):
        mutate(self.manifest)
        code, report = self.run_validator()
        self.assertEqual(code, 2)
        self.assertFalse(report['structurally_valid'])
        self.assertIn(expected, {entry['code'] for entry in report['errors']})
        self.assertFalse(report['executable_ready'])
        return report

    def test_synthetic_fixture_valid_but_never_executable(self):
        code, report = self.run_validator()
        self.assertEqual(code, 1)
        self.assertTrue(report['structurally_valid'])
        self.assertFalse(report['offline_preparation_ready'])
        self.assertFalse(report['executable_ready'])
        self.assertEqual(report['authorization'], 'UNKNOWN')
        self.assertEqual(report['native_admission'], 'UNKNOWN')
        self.assertEqual(report['benefit'], 'UNKNOWN')
        self.assertIn('spending.global_cap_usd', report['missing_inputs'])

    def test_inert_template_reports_missing_inputs(self):
        template = json.loads((ROOT / 'manifest.template.json').read_text())
        code, report = self.run_validator(template)
        self.assertEqual(code, 1)
        self.assertFalse(report['offline_preparation_ready'])
        self.assertIn('tasks', report['missing_inputs'])
        self.assertIn('campaign_id', report['missing_inputs'])

    def test_missing_field(self):
        self.reject(lambda m: m.pop('analysis'), 'fields')

    def test_unknown_field(self):
        self.reject(lambda m: m['spending'].update(bypass_cap=True), 'fields')

    def test_duplicate_json_key(self):
        code, report = self.run_validator(raw='{"campaign_id":"one","campaign_id":"two"}')
        self.assertEqual(code, 2)
        self.assertEqual(report['errors'][0]['code'], 'json')

    def test_traversal_and_absolute_paths(self):
        for value in ('../private', '/private/secret', 'assets/../shared', 'C:/private'):
            with self.subTest(value=value):
                m = copy.deepcopy(self.manifest)
                m['input_files']['corpus'] = value
                code, report = self.run_validator(m)
                self.assertEqual(code, 2)
                self.assertIn('unsafe_path', {e['code'] for e in report['errors']})
                self.assertNotIn(value, json.dumps(report))

    def test_symlink_rejected_even_with_matching_hash(self):
        file = self.root / 'assets/corpus.txt'
        file.unlink()
        file.symlink_to(self.root / 'assets/policy_and_catalog.txt')
        self.manifest['input_hashes']['corpus'] = self.manifest['input_hashes']['policy_and_catalog']
        code, report = self.run_validator()
        self.assertEqual(code, 2)
        self.assertIn('unsafe_path', {e['code'] for e in report['errors']})

    def test_sealed_file_missing_or_drifted(self):
        file = self.root / 'assets/corpus.txt'
        file.write_text('changed synthetic corpus')
        code, report = self.run_validator()
        self.assertEqual(code, 2)
        self.assertIn('hash_drift', {e['code'] for e in report['errors']})
        file.unlink()
        code, report = self.run_validator()
        self.assertEqual(code, 2)
        self.assertIn('missing_file', {e['code'] for e in report['errors']})

    def test_duplicate_task_and_assignment(self):
        self.reject(lambda m: m['tasks'].append(copy.deepcopy(m['tasks'][0])), 'duplicate_task')
        self.manifest['tasks'].pop()
        self.reject(lambda m: m['sealed_order'].append(copy.deepcopy(m['sealed_order'][0])), 'assignments')

    def test_missing_arm_assignment(self):
        self.reject(lambda m: m['sealed_order'].pop(), 'assignments')

    def test_bad_start_reference(self):
        self.reject(lambda m: m['tasks'][0].update(source_commit='main'), 'source_commit')

    def test_hidden_test_leak(self):
        self.reject(lambda m: m['tasks'][0].update(hidden_tests_separated=False), 'hidden_test_leak')

    def test_confirmation_overlap_by_identity_or_content(self):
        self.reject(lambda m: m['confirmation_tasks'][0].update(task_id=m['tasks'][0]['task_id']), 'heldout_overlap')
        self.manifest['confirmation_tasks'][0]['task_id'] = 'synthetic-heldout'
        self.reject(lambda m: m['confirmation_tasks'][0].update(statement_sha256=m['tasks'][0]['statement_sha256'], statement_path=m['tasks'][0]['statement_path']), 'heldout_overlap')

    def test_unknown_semantics_cannot_be_weakened(self):
        self.reject(lambda m: m['analysis'].update(missing_usage_or_price='assume_zero'), 'analysis_contract')

    def test_unknown_identity_stops_and_external_receipt_cannot_be_invented(self):
        self.reject(lambda m: m['stop_on'].remove('missing_session_identity'), 'stop_contract')
        self.manifest['stop_on'].append('missing_session_identity')
        self.reject(lambda m: m['export'].update(runtime_receipt_for_external_native_session=True), 'export_contract')

    def test_nonfinite_money_and_unknown_prior_spend(self):
        self.reject(lambda m: m['spending'].update(global_cap_usd=float('inf')), 'json')
        self.manifest['spending']['global_cap_usd'] = 1
        self.manifest['spending']['known_prior_spend_usd'] = 0
        self.manifest['spending']['estimated_remaining_usd'] = 0
        self.manifest['spending']['unknown_prior_spend'] = True
        code, report = self.run_validator()
        self.assertEqual(code, 1)
        self.assertIn('spending.unknown_prior_spend', report['missing_inputs'])

    def test_budget_excess_and_approval_drift(self):
        self.manifest['spending'].update(global_cap_usd=1, known_prior_spend_usd=1, estimated_remaining_usd=1, unknown_prior_spend=False)
        code, report = self.run_validator()
        self.assertEqual(code, 2)
        self.assertIn('budget_exceeded', {e['code'] for e in report['errors']})
        self.manifest['spending']['estimated_remaining_usd'] = 0
        self.reject(lambda m: m['spending'].update(approved_manifest_sha256='0' * 64), 'approval_drift')

    def test_fully_recorded_preparation_still_grants_no_authority(self):
        # Test-only declarations are intentionally fabricated, never persisted as approval.
        self.manifest['synthetic'] = False
        self.manifest['collection_consent'] = 'test-only consent reference'
        self.manifest['spending'].update(global_cap_usd=1, known_prior_spend_usd=0,
                                        estimated_remaining_usd=1, unknown_prior_spend=False,
                                        enforcement_evidence_path='assets/corpus.txt',
                                        enforcement_evidence_sha256=self.manifest['input_hashes']['corpus'])
        for evidence in self.manifest['preconditions'].values():
            evidence.update(status='verified', evidence_path='assets/corpus.txt',
                            evidence_sha256=self.manifest['input_hashes']['corpus'])
        raw = json.dumps(self.manifest, sort_keys=True, separators=(',', ':'), allow_nan=False).encode()
        self.manifest['spending']['approved_manifest_sha256'] = hashlib.sha256(raw).hexdigest()
        code, report = self.run_validator()
        self.assertEqual(code, 0)
        self.assertTrue(report['offline_preparation_ready'])
        self.assertFalse(report['executable_ready'])
        self.assertEqual(report['authorization'], 'UNKNOWN')
        self.manifest['random_seed'] += 1
        code, report = self.run_validator()
        self.assertEqual(code, 2)
        self.assertIn('approval_drift', {e['code'] for e in report['errors']})

    def test_hidden_file_must_not_be_any_exposed_input(self):
        t = self.manifest['tasks'][0]
        self.manifest['input_files']['corpus'] = t['hidden_tests_path']
        self.manifest['input_hashes']['corpus'] = t['hidden_tests_sha256']
        code, report = self.run_validator()
        self.assertEqual(code, 2)
        self.assertIn('hidden_test_leak', {e['code'] for e in report['errors']})

    def test_oversized_or_nonregular_file_rejected(self):
        file = self.root / 'assets/corpus.txt'
        with file.open('wb') as stream:
            stream.truncate(16 * 1024 * 1024 + 1)
        code, report = self.run_validator()
        self.assertEqual(code, 2)
        self.assertIn('file_limit', {e['code'] for e in report['errors']})
        file.unlink()
        file.mkdir()
        code, report = self.run_validator()
        self.assertEqual(code, 2)
        self.assertIn('unsafe_path', {e['code'] for e in report['errors']})

    def test_unknown_task_field_rejected(self):
        self.reject(lambda m: m['tasks'][0].update(private_prompt='do not echo'), 'fields')

    def test_null_evidence_cannot_satisfy_verified_precondition(self):
        self.manifest['preconditions']['U07']['status'] = 'verified'
        code, report = self.run_validator()
        self.assertEqual(code, 1)
        self.assertIn('preconditions.U07', report['missing_inputs'])

    def test_realistic_sparse_binary_sizes_accept_streamed_hashes(self):
        # Observed binary sizes, synthetic zero-filled bytes, no native executables.
        for role, size in [('host_binary', 240184096), ('router_binary', 19063984)]:
            file = self.root / self.manifest['input_files'][role]
            with file.open('wb') as stream:
                stream.truncate(size)
            digest = hashlib.sha256()
            chunk = bytes(65536)
            complete, remainder = divmod(size, len(chunk))
            for _ in range(complete):
                digest.update(chunk)
            digest.update(chunk[:remainder])
            self.manifest['input_hashes'][role] = digest.hexdigest()
        code, report = self.run_validator()
        self.assertEqual(code, 1)
        self.assertTrue(report['structurally_valid'])
        # Size allowance applies to the role, never to a generic corpus reference.
        self.manifest['input_files']['corpus'] = self.manifest['input_files']['router_binary']
        self.manifest['input_hashes']['corpus'] = self.manifest['input_hashes']['router_binary']
        code, report = self.run_validator()
        self.assertEqual(code, 2)
        self.assertIn({'code': 'file_limit', 'field': 'input_hashes.corpus'}, report['errors'])

    def test_money_large_integer_returns_fixed_error_without_traceback(self):
        self.manifest['spending']['global_cap_usd'] = 10**400
        code, report = self.run_validator()
        self.assertEqual(code, 2)
        self.assertIn({'code': 'money', 'field': 'spending.global_cap_usd'}, report['errors'])

    def test_manifest_swap_to_symlink_is_rejected_at_open(self):
        spec = importlib.util.spec_from_file_location('u08_validator', ROOT / 'validate_manifest.py')
        validator = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(validator)
        manifest = self.root / 'manifest.json'
        substitute = self.root / 'substitute.json'
        substitute.write_text(json.dumps(self.manifest))
        original_path_open, original_os_open = Path.open, os.open
        swapped = False

        def swap():
            nonlocal swapped
            if not swapped:
                manifest.unlink()
                manifest.symlink_to(substitute)
                swapped = True

        def path_open(path, *args, **kwargs):
            if path == manifest:
                swap()
            return original_path_open(path, *args, **kwargs)

        def descriptor_open(path, flags, *args, **kwargs):
            if path == 'manifest.json' and kwargs.get('dir_fd') is not None:
                swap()
            return original_os_open(path, flags, *args, **kwargs)

        with patch.object(Path, 'open', path_open), patch.object(validator.os, 'open', descriptor_open):
            report = validator.validate(manifest)
        self.assertTrue(swapped, 'replacement must happen exactly at manifest open')
        self.assertFalse(report['structurally_valid'])
        self.assertIn({'code': 'unsafe_path', 'field': 'manifest'}, report['errors'])

    def test_fifo_manifest_is_rejected_without_waiting_for_writer(self):
        source = self.root / 'manifest.json'
        source.unlink()
        os.mkfifo(source)
        result = subprocess.run([sys.executable, str(ROOT / 'validate_manifest.py'), str(source)],
                                capture_output=True, text=True, timeout=5)
        self.assertEqual(result.returncode, 2)
        self.assertEqual(result.stderr, '')
        report = json.loads(result.stdout)
        self.assertIn({'code': 'unsafe_path', 'field': 'manifest'}, report['errors'])

    def test_binary_role_limit_cannot_be_exceeded(self):
        file = self.root / self.manifest['input_files']['host_binary']
        with file.open('wb') as stream:
            stream.truncate(256 * 1024 * 1024 + 1)
        code, report = self.run_validator()
        self.assertEqual(code, 2)
        self.assertIn({'code': 'file_limit', 'field': 'input_hashes.host_binary'}, report['errors'])

    def test_aggregate_limit_includes_both_binary_reads(self):
        size = 256 * 1024 * 1024
        digest = hashlib.sha256()
        for _ in range(size // 65536):
            digest.update(bytes(65536))
        for role in ('host_binary', 'router_binary'):
            file = self.root / self.manifest['input_files'][role]
            with file.open('wb') as stream:
                stream.truncate(size)
            self.manifest['input_hashes'][role] = digest.hexdigest()
        # Each binary individually fits; remaining sealed bytes exceed 512 MiB combined.
        code, report = self.run_validator()
        self.assertEqual(code, 2)
        self.assertIn({'code': 'file_limit', 'field': 'input_hashes.router_binary'}, report['errors'])

    def test_money_microusd_representation_boundary(self):
        self.manifest['spending']['global_cap_usd'] = 9223372036854
        code, report = self.run_validator()
        self.assertEqual(code, 1)
        self.assertTrue(report['structurally_valid'])
        self.manifest['spending']['global_cap_usd'] += 1
        code, report = self.run_validator()
        self.assertEqual(code, 2)
        self.assertIn({'code': 'money', 'field': 'spending.global_cap_usd'}, report['errors'])

    def test_bounded_input(self):
        code, report = self.run_validator(raw=' ' * (1024 * 1024 + 1))
        self.assertEqual(code, 2)
        self.assertEqual(report['errors'][0]['code'], 'input_limit')


if __name__ == '__main__':
    unittest.main()
