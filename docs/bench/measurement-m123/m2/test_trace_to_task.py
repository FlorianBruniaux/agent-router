"""Behavior tests. Values are synthetic unless explicitly using retained report."""
import copy
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('adapter', HERE / 'trace_to_task.py')
api = importlib.util.module_from_spec(spec)
spec.loader.exec_module(api)


def sample():
    return {
        'schema_version': 1,
        'task': {'task_id': 'fix', 'source_revision': 'a' * 40,
                 'revision': {'digest': 'a' * 64}, 'verdict': 'unknown',
                 'tests': [{'test_id': 'unit', 'status': 'passed', 'evidence_sha256': 'b' * 64}]},
        'attempts': [{'attempt_id': 'try-1', 'session_id': 'session-1',
                      'revision': {'digest': 'a' * 64}, 'test_ids': ['unit'],
                      'verdict': 'unknown', 'evidence_sha256': 'c' * 64}],
        'snapshots': [{'snapshot_id': 'one', 'attempt_id': 'try-1', 'scope_id': 'scope-1',
                       'start': '2026-10-10T10:00:00Z', 'end': '2026-10-10T10:01:00Z',
                       'usage': {'input_tokens': 100, 'cached_input_tokens': 80,
                                 'output_tokens': 20, 'reasoning_output_tokens': 10,
                                 'cache_write_input_tokens': 0, 'total_tokens': 120},
                       'evidence_sha256': 'd' * 64}],
        'disjoint_scopes_evidence_sha256': None,
    }


class AdapterTests(unittest.TestCase):
    def test_nested_and_invalid_metadata_refused_without_payload_echo(self):
        cases = [
            (('task', 'source_revision'), {'private': 'SYNTHETIC-PRIVATE-CANARY'}),
            (('task', 'revision'), {'private': 'SYNTHETIC-PRIVATE-CANARY'}),
            (('attempts', 0, 'revision'), {'digest': {'private': 'SYNTHETIC-PRIVATE-CANARY'}}),
            (('snapshots', 0, 'start'), {'private': 'SYNTHETIC-PRIVATE-CANARY'}),
            (('snapshots', 0, 'end'), ['SYNTHETIC-PRIVATE-CANARY']),
            (('snapshots', 0, 'start'), '2026-99-99T99:00:00Z'),
            (('task', 'revision'), {'source_files_sha256': {'/private/SYNTHETIC-PRIVATE-CANARY': 'a' * 64}}),
        ]
        for path, payload in cases:
            with self.subTest(field=path):
                x = sample(); node = x
                for key in path[:-1]: node = node[key]
                node[path[-1]] = payload
                with tempfile.TemporaryDirectory() as directory:
                    file = Path(directory) / 'input.json'; file.write_text(json.dumps(x))
                    proc = subprocess.run([sys.executable, str(HERE / 'trace_to_task.py'), 'report', '--input', str(file)], capture_output=True, text=True)
                    self.assertEqual(proc.returncode, 2)
                    self.assertEqual(proc.stdout, '')
                    self.assertEqual(proc.stderr.strip(), 'trace_to_task.invalid_input')

    def test_repeated_snapshot_counted_once(self):
        x = sample(); x['snapshots'].append(copy.deepcopy(x['snapshots'][0]))
        r = api.report(x)
        self.assertEqual(r['usage']['combined']['total_tokens'], 120)
        self.assertEqual(r['usage']['duplicate_snapshots'], 1)

    def test_retry_with_distinct_identity_counted_separately(self):
        x = sample()
        a = copy.deepcopy(x['attempts'][0]); a.update(attempt_id='try-2', session_id='session-2')
        s = copy.deepcopy(x['snapshots'][0]); s.update(snapshot_id='two', attempt_id='try-2', scope_id='scope-2')
        x['attempts'].append(a); x['snapshots'].append(s)
        x['disjoint_scopes_evidence_sha256'] = 'e' * 64
        self.assertEqual(api.report(x)['usage']['combined']['total_tokens'], 240)

    def test_unknown_overlap_blocks_combined_total(self):
        x = sample(); s = copy.deepcopy(x['snapshots'][0]); s.update(snapshot_id='two', scope_id='child')
        x['snapshots'].append(s)
        self.assertIsNone(api.report(x)['usage']['combined'])

    def test_attempt_without_snapshot_blocks_total(self):
        x = sample(); a = copy.deepcopy(x['attempts'][0]); a.update(attempt_id='try-2', session_id='session-2')
        x['attempts'].append(a)
        self.assertIsNone(api.report(x)['usage']['combined'])

    def test_missing_usage_stays_unknown(self):
        x = sample(); x['snapshots'][0]['usage'] = None
        self.assertIsNone(api.report(x)['usage']['combined'])

    def test_cache_and_reasoning_are_subsets(self):
        r = api.report(sample())
        self.assertEqual(r['usage']['combined']['total_tokens'], 120)
        self.assertEqual(r['usage']['windows'][0]['uncached_input_tokens'], 20)

    def test_no_accepted_task_no_ratio_and_unknown_money(self):
        r = api.report(sample())
        self.assertIsNone(r['cost_per_accepted_task_microusd'])
        self.assertEqual(r['costs'], {'host_reported_microusd': None,
                                    'estimated_microusd': None, 'invoiced_microusd': None})
        self.assertEqual(r['accepted_changes']['status'], 'unavailable')

    def test_missing_counter_field_stays_unknown(self):
        x = sample(); del x['snapshots'][0]['usage']['cached_input_tokens']
        self.assertIsNone(api.report(x)['usage']['combined'])

    def test_cli_refuses_invalid_input_without_echoing_private_path(self):
        proc = subprocess.run([sys.executable, str(HERE / 'trace_to_task.py'), 'report', '--input', '/private/SECRET-missing-input'], capture_output=True, text=True)
        self.assertEqual(proc.returncode, 2)
        self.assertEqual(proc.stderr.strip(), 'trace_to_task.invalid_input')
        self.assertNotIn('SECRET', proc.stdout + proc.stderr)

    def test_conflicting_snapshot_identity_refused(self):
        x = sample(); s = copy.deepcopy(x['snapshots'][0]); s['usage']['input_tokens'] = 101
        x['snapshots'].append(s)
        with self.assertRaises(ValueError): api.report(x)

    def test_duplicate_attempt_identity_refused(self):
        x = sample(); x['attempts'].append(copy.deepcopy(x['attempts'][0]))
        with self.assertRaises(ValueError): api.report(x)

    def test_unknown_attempt_and_test_refused(self):
        for field in ('attempt', 'test'):
            x = sample()
            if field == 'attempt': x['snapshots'][0]['attempt_id'] = 'missing'
            else: x['attempts'][0]['test_ids'] = ['missing']
            with self.assertRaises(ValueError): api.report(x)

    def test_overlapping_windows_same_scope_not_added(self):
        x = sample(); s = copy.deepcopy(x['snapshots'][0]); s.update(snapshot_id='later', end='2026-10-10T10:02:00Z')
        x['snapshots'].append(s)
        x['disjoint_scopes_evidence_sha256'] = 'e' * 64
        self.assertIsNone(api.report(x)['usage']['combined'])

    def test_invalid_usage_refused(self):
        for key, value in [('input_tokens', -1), ('cached_input_tokens', 101), ('total_tokens', 999), ('output_tokens', True)]:
            x = sample(); x['snapshots'][0]['usage'][key] = value
            with self.assertRaises(ValueError): api.report(x)

    def test_duplicate_json_keys_refused(self):
        with tempfile.TemporaryDirectory() as d:
            p = Path(d) / 'bad.json'; p.write_text('{"a":1,"a":2}')
            with self.assertRaises(ValueError): api.read_json(p)

    def test_importer_allowlists_private_sources_and_keeps_cutoffs(self):
        with tempfile.TemporaryDirectory() as d:
            p = Path(d)
            usage = sample()['snapshots'][0]['usage']
            docs = {
                'parent-usage-baseline.json': {'source_session': 'PRIVATE', 'start_timestamp': '2026-10-10T10:00:00Z',
                    'configured_model': 'gpt-6-astra', 'configured_effort': 'high',
                    'before_turn': {'info': {'total_token_usage': usage}}},
                'parent-usage-cutoff.json': {'start_timestamp': '2026-10-10T10:00:00Z', 'end_timestamp': '2026-10-10T10:05:00Z', 'delta': usage},
                'child-usage-cutoff.json': {'first_token_event': {'timestamp': '2026-10-10T10:00:01Z'},
                    'latest_token_event': {'timestamp': '2026-10-10T10:03:00Z', 'info': {'total_token_usage': usage}}},
                'measurements.json': {'source': {'base_commit': 'a' * 40, 'modified_source_sha256': {'src.rs': 'a'*64}},
                    'gates': [{'id': 'unit', 'exit_code': 0, 'duration_seconds': 1, 'log_sha256': 'b'*64}],
                    'review': {'verdict': 'local pass'}, 'acceptance': {'user_acceptance': 'unknown'},
                    'usage': {'implementation_subagent': {'cutoff': '2026-10-10T10:04:00Z', 'reported_totals': usage}}},
            }
            for name, doc in docs.items(): (p / name).write_text(json.dumps(doc))
            (p / 'capture-parent-usage.py').write_text('# retained provenance only\n')
            r = api.import_b1b(p, p / 'measurements.json')
            serialized = json.dumps(r)
            self.assertNotIn('PRIVATE', serialized)
            self.assertNotIn(str(p), serialized)
            self.assertEqual(len(r['usage']['windows']), 3)
            self.assertIsNone(r['usage']['combined'])
            self.assertEqual(r['task']['verdict'], 'unknown')
            self.assertEqual(r['task']['tests'][0]['status'], 'passed')
            attacks = [
                ('parent-usage-cutoff.json', ('delta',), []),
                ('measurements.json', ('worker',), {'elapsed_seconds': 10**1000}),
                ('measurements.json', ('gates', 0, 'exit_code'), False),
                ('measurements.json', ('gates', 0, 'exit_code'), 0.0),
                ('measurements.json', ('gates', 0, 'exit_code'), None),
                ('parent-usage-baseline.json', ('configured_model',), {'private': 'SYNTHETIC-PRIVATE-CANARY'}),
                ('parent-usage-baseline.json', ('configured_effort',), {'private': 'SYNTHETIC-PRIVATE-CANARY'}),
                ('measurements.json', ('source', 'modified_source_sha256'), {'/private/SYNTHETIC-PRIVATE-CANARY': 'a' * 64}),
                ('measurements.json', ('source', 'modified_source_sha256'), {'src.rs': {'private': 'SYNTHETIC-PRIVATE-CANARY'}}),
                ('measurements.json', ('gates', 0, 'duration_seconds'), {'private': 'SYNTHETIC-PRIVATE-CANARY'}),
                ('measurements.json', ('worker',), {'elapsed_seconds': {'private': 'SYNTHETIC-PRIVATE-CANARY'}}),
            ]
            for name, path, payload in attacks:
                with self.subTest(import_field=path):
                    changed = copy.deepcopy(docs[name]); node = changed
                    for key in path[:-1]: node = node[key]
                    node[path[-1]] = payload
                    (p / name).write_text(json.dumps(changed))
                    try:
                        proc = subprocess.run([sys.executable, str(HERE / 'trace_to_task.py'), 'import-b1b', '--private-dir', str(p), '--measurements', str(p / 'measurements.json')], capture_output=True, text=True)
                        self.assertEqual(proc.returncode, 2)
                        self.assertEqual(proc.stdout, '')
                        self.assertEqual(proc.stderr.strip(), 'trace_to_task.invalid_input')
                    finally:
                        (p / name).write_text(json.dumps(docs[name]))
            for attempt in r['attempts']:
                self.assertEqual(attempt['session_identity_kind'], 'report_local_alias')
                self.assertIsNone(attempt['observed_native_session_id'])

    def test_oversized_file_refused(self):
        with tempfile.TemporaryDirectory() as d:
            p = Path(d) / 'huge'; p.write_bytes(b' ' * (4 * 1024 * 1024 + 1))
            with self.assertRaises(ValueError): api.read_json(p)


class ExistingCalculatorTests(unittest.TestCase):
    @unittest.skipUnless(os.environ.get('AGENT_ROUTER_BIN'), 'set AGENT_ROUTER_BIN for offline integration')
    def test_synthetic_physical_costs_use_existing_calculator(self):
        # Removing pricing coverage must remove the total, not keep a false exact cost.
        root = HERE.parents[3]
        evidence = json.loads((root / 'tests/fixtures/evaluation/accepted-changes-v1.json').read_text())
        with tempfile.TemporaryDirectory() as d:
            path = Path(d) / 'input.json'
            for incomplete in (False, True):
                data = copy.deepcopy(evidence)
                if incomplete: data['observations'][0]['pricing_coverage'] = 'unknown'; data['observations'][0]['cost_microusd'] = None
                projected = api.project_physical_evidence(data, 'synthetic_supplied')
                path.write_text(json.dumps(projected))
                proc = subprocess.run([os.environ['AGENT_ROUTER_BIN'], 'eval', 'accepted-changes', '--input', str(path), '--json'], capture_output=True, text=True)
                self.assertEqual(proc.returncode, 0, proc.stderr)
                output = json.loads(proc.stdout)
                self.assertIn('supplied_unverified', json.dumps(output))
                # The selected later revision is not accepted even though an older one was.
                self.assertEqual(output['selected_accepted_changes'], 0)
                self.assertIsNone(output['cost_per_selected_accepted_change']['model'])
                self.assertEqual(output['cohort_costs']['model']['total_microusd'], None if incomplete else 700)
                self.assertEqual(output['cohort_costs']['model']['known_subtotal_microusd'], 600 if incomplete else 700)

    def test_session_snapshot_cannot_become_physical_event(self):
        with self.assertRaises(ValueError): api.project_physical_evidence(sample(), 'host_session_snapshot')


if __name__ == '__main__': unittest.main()
