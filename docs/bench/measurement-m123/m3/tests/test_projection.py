"""Projection rejects incomplete bindings and malformed bounded inputs."""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


def checksum(value, field):
    return hashlib.sha256(json.dumps({k: v for k, v in value.items() if k != field},
                                    sort_keys=True, separators=(',', ':')).encode()).hexdigest()


class ProjectionTests(unittest.TestCase):
    def setUp(self):
        spec = importlib.util.spec_from_file_location('projection', ROOT / 'project_accounting.py')
        self.module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(self.module)
        self.plan = json.loads((ROOT / 'fixtures/plan.json').read_text())
        self.state = json.loads((ROOT / 'fixtures/state.json').read_text())

    def test_omitted_assignment_rejected_after_state_rehash(self):
        self.state['assignments'].pop()
        self.state['state_sha256'] = checksum(self.state, 'state_sha256')
        with self.assertRaisesRegex(ValueError, 'state_drift'):
            self.module.project(self.plan, self.state)

    def test_substituted_assignment_rejected_after_state_rehash(self):
        self.state['assignments'][0]['arm'] = 'substituted'
        self.state['state_sha256'] = checksum(self.state, 'state_sha256')
        with self.assertRaisesRegex(ValueError, 'state_drift'):
            self.module.project(self.plan, self.state)

    def test_plan_payload_drift_rejected_before_projection(self):
        self.plan['source_commit'] = 'changed'
        with self.assertRaisesRegex(ValueError, 'plan_drift'):
            self.module.project(self.plan, self.state)

    def test_shortened_plan_rejected_even_after_all_hashes_recomputed(self):
        self.plan['sealed_order'].pop()
        self.plan['plan_sha256'] = checksum(self.plan, 'plan_sha256')
        self.state['plan_sha256'] = self.plan['plan_sha256']
        self.state['state_sha256'] = checksum(self.state, 'state_sha256')
        with self.assertRaisesRegex(ValueError, 'plan_drift'):
            self.module.project(self.plan, self.state)

    def test_ambiguous_projection_preserves_unknown_cost(self):
        self.state['assignments'][0]['attempts'] = [copy.deepcopy(self.state['assignments'][0]['attempts'][0])]
        self.state['assignments'][0]['attempts'][0].update(status='pending', cost_microusd=None)
        self.state['state_sha256'] = checksum(self.state, 'state_sha256')
        result = self.module.project(self.plan, self.state)
        self.assertEqual(len(result['cohort']['assignments']), 18)
        self.assertIsNone(result['observations'][0]['cost_microusd'])
        self.assertEqual(result['observations'][0]['usage_coverage'], 'unknown')

    def test_cli_rejects_duplicate_oversize_and_malformed_inputs_with_static_errors(self):
        valid = {'plan': json.dumps(self.plan), 'state': json.dumps(self.state)}
        for kind in ('plan', 'state'):
            key = 'models' if kind == 'plan' else 'mode'
            value = 'synthetic_fake_only' if kind == 'plan' else 'fake_only'
            duplicate = valid[kind][:-1] + ',' + json.dumps(key) + ':' + json.dumps(value) + '}'
            for bad in (duplicate, valid[kind] + ' ' * (1024 * 1024 + 1),
                        '{"private-payload":"/private/test-marker"', '[]'):
                with self.subTest(kind=kind, length=len(bad)), tempfile.TemporaryDirectory() as directory:
                    root = Path(directory)
                    paths = {k: root / (k + '.json') for k in valid}
                    for key, path in paths.items():
                        path.write_text(bad if key == kind else valid[key])
                    process = subprocess.run([sys.executable, '-B', str(ROOT / 'project_accounting.py'),
                                              '--plan', str(paths['plan']), '--state', str(paths['state'])],
                                             capture_output=True, text=True, timeout=15)
                    self.assertEqual(process.returncode, 2)
                    self.assertEqual(process.stderr, '')
                    self.assertEqual(json.loads(process.stdout),
                                     {'error': 'offline_projection_refused', 'executable_ready': False})


if __name__ == '__main__':
    unittest.main()
