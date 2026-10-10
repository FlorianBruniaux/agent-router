"""Offline harness fixtures, never product or provider evidence."""
import copy
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
REPO = Path(os.environ.get('M3_REPO', ROOT.parents[3]))


class RehearsalTests(unittest.TestCase):
    def setUp(self):
        self.assertTrue((ROOT / 'rehearse.py').exists(), 'offline helper is missing')
        spec = importlib.util.spec_from_file_location('m3', ROOT / 'rehearse.py')
        self.m = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(self.m)
        self.plan = self.m.prepare(ROOT, REPO)
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.state = Path(self.tmp.name) / 'state.json'

    def run_fake(self, **kw):
        return self.m.run_fake(self.plan, ROOT, REPO, self.state, **kw)

    def test_sealed_cartesian_order_and_first_block(self):
        rows = self.plan['sealed_order']
        self.assertEqual(len(rows), 18)
        self.assertEqual(len({(r['task_id'], r['arm'], r['repetition']) for r in rows}), 18)
        self.assertEqual(len({(r['task_id'], r['repetition']) for r in rows[:3]}), 1)
        self.assertEqual({r['arm'] for r in rows[:3]}, set(self.m.ARMS))
        self.assertEqual(self.plan, self.m.prepare(ROOT, REPO))
        self.assertFalse(self.plan['executable_ready'])

    def test_assignment_persisted_before_fake_call_and_no_hidden_worker_input(self):
        observed = []
        def inspect(row, payload):
            persisted = json.loads(self.state.read_text())
            self.assertEqual(len(persisted['assignments']), 18)
            self.assertEqual(persisted['assignments'][row]['attempts'][-1]['status'], 'pending')
            self.assertNotIn('hidden', json.dumps(payload))
            self.assertEqual(set(payload), {'task_id', 'source_commit', 'allowed_write_scope', 'statement', 'constraints'})
            observed.append(row)
        state = self.run_fake(before_call=inspect, limit=3)
        self.assertEqual(observed, [0, 1, 2])
        self.assertEqual(self.m.summarize(state)['unrun'], 15)

    def test_failures_retries_and_unrun_are_retained(self):
        state = self.run_fake(limit=3, scenario={0: ['retryable_failure', 'accepted'], 1: ['rejected']})
        report = self.m.summarize(state)
        self.assertEqual((report['assigned'], report['attempts'], report['accepted'], report['unrun']), (18, 4, 2, 15))
        self.assertEqual(report['cost_calculator'], 'agent-router eval accepted-changes')
        self.assertEqual(state['assignments'][0]['attempts'][0]['status'], 'retryable_failure')
        self.assertEqual(len({a['session_id'] for r in state['assignments'] for a in r['attempts']}), 4)
        self.assertEqual(len({r['workspace_id'] for r in state['assignments']}), 18)

    def test_resume_completed_does_not_replay(self):
        first = self.run_fake(limit=3)
        second = self.run_fake()
        self.assertEqual(second['assignments'][:3], first['assignments'][:3])
        self.assertEqual(self.m.summarize(second)['attempts'], 18)
        self.assertEqual(self.run_fake(), second)

    def test_ambiguous_and_interrupted_attempt_stop_resume(self):
        for outcome in ('ambiguous', 'interrupted'):
            with self.subTest(outcome=outcome):
                self.state = Path(self.tmp.name) / (outcome + '.json')
                state = self.run_fake(scenario={0: [outcome]})
                self.assertEqual(self.m.summarize(state)['attempts'], 1)
                self.assertIsNone(state['assignments'][0]['attempts'][0]['cost_microusd'])
                with self.assertRaisesRegex(ValueError, 'ambiguous_attempt'):
                    self.run_fake()

    def test_retry_ceiling_is_two(self):
        state = self.run_fake(limit=1, scenario={0: ['retryable_failure', 'retryable_failure']})
        self.assertEqual(len(state['assignments'][0]['attempts']), 2)
        self.assertEqual(self.m.summarize(state)['accepted'], 0)
        self.assertEqual(self.run_fake(limit=1), state)

    def test_plan_and_version_drift_refuse_before_state_or_call(self):
        for key in ('source_commit', 'harness_version'):
            plan = copy.deepcopy(self.plan)
            plan[key] = 'changed'
            with self.assertRaisesRegex(ValueError, 'plan_drift'):
                self.m.run_fake(plan, ROOT, REPO, self.state)
            self.assertFalse(self.state.exists())

    def test_hash_drift_and_symlink_refuse(self):
        import shutil
        for mode in ('changed', 'symlink'):
            root = Path(self.tmp.name) / mode
            shutil.copytree(ROOT, root)
            path = root / 'tasks/t1.md'
            path.unlink()
            if mode == 'changed':
                path.write_text('changed')
            else:
                path.symlink_to(ROOT / 'tasks/t1.md')
            with self.assertRaisesRegex(ValueError, 'hash_drift|unsafe_path'):
                self.m.prepare(root, REPO)

    def test_state_corruption_and_concurrent_lock_refuse(self):
        self.run_fake(limit=1)
        state = json.loads(self.state.read_text())
        state['assignments'].pop()
        self.state.write_text(json.dumps(state))
        with self.assertRaisesRegex(ValueError, 'state_drift'):
            self.run_fake()
        self.state.with_suffix('.json.lock').mkdir()
        with self.assertRaisesRegex(ValueError, 'concurrent_or_interrupted_writer'):
            self.run_fake()

    def test_invalid_scenario_rejected_before_any_attempt(self):
        with self.assertRaisesRegex(ValueError, 'scenario'):
            self.run_fake(scenario={0: ['real_provider']})
        self.assertFalse(self.state.exists())

    def test_accounting_projection_keeps_every_assignment_and_attempt(self):
        self.assertTrue((ROOT / 'project_accounting.py').exists(), 'accounting projection missing')
        spec = importlib.util.spec_from_file_location('projection', ROOT / 'project_accounting.py')
        projection = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(projection)
        state = self.run_fake(limit=3, scenario={0: ['retryable_failure', 'accepted'], 1: ['rejected']})
        result = projection.project(self.plan, state)
        self.assertEqual(len(result['cohort']['assignments']), 18)
        self.assertEqual(len(result['selections']), 18)
        self.assertEqual(len(result['observations']), 4)
        self.assertEqual(len(result['inventories'][0]['expected']), 4)
        self.assertEqual(len({r['key']['physical_sha256'] for r in result['observations']}), 4)
        self.assertEqual(sum(r['revision_sha256'] is None for r in result['selections']), 15)
        self.assertEqual(sum(v['status'] == 'accepted' for v in result['verdicts']), 2)


if __name__ == '__main__':
    unittest.main()
