"""Parent-only checks with tiny authored local processes. No native or models."""
import json
import os
from pathlib import Path
import sys
import tempfile
import time
import unittest
import importlib.util
import shutil

import controller as c


class Parsing(unittest.TestCase):
    def test_duplicate_and_nonfinite_json_refused(self):
        for raw in ('{"a":1,"a":2}', '{"a":NaN}', '{"a":Infinity}'):
            with self.assertRaises(ValueError):
                c.loads(raw)

    def test_permission_denial_never_becomes_success(self):
        self.assertTrue(c.denied_output('Tool request refused.'))
        self.assertTrue(c.denied_output([
            {'type': 'input_text', 'text': 'Wall time: 0.0001 seconds\nOutput:'},
            {'type': 'input_text', 'text': 'Tool request refused.'}]))
        for wrong in ('', 'allowed', '{"exit_code":0}', 'Tool request refused. private content', [], {}):
            self.assertFalse(c.denied_output(wrong))

    def test_public_inventory_matches_pins(self):
        task = c.submission()
        self.assertEqual(task['task']['scope'], ['lib.rs'])
        self.assertEqual(task['risk_context']['environment'], 'local')
        for name, row in task['public_files'].items():
            self.assertNotIn('/', name)
            self.assertEqual(c.sha(row['content'].encode()), row['sha256'])
        self.assertNotIn('private-canary', task['public_files'])

    def test_verifier_bytes_import_in_their_repository_layout(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / 'evaluation-repo'
            target = root / 'crates/router-runtime/examples/campaign/t02-v2/verify.py'
            target.parent.mkdir(parents=True)
            source = c.HERE / 'inputs/verify.py'
            shutil.copyfile(source, target)
            self.assertEqual(c.sha(target.read_bytes()), c.sha(source.read_bytes()))
            spec = importlib.util.spec_from_file_location('layout_qualification', target)
            module = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(module)
            self.assertEqual(module.ROOT, root.resolve())
            self.assertTrue(callable(module.evaluate_source))


class Pipes(unittest.TestCase):
    def child(self, code, evidence, deadline=2):
        return c.Session([sys.executable, '-I', '-B', '-c', code],
                         {'PATH': '/usr/bin:/bin'}, evidence, deadline=deadline)

    def test_actual_local_frames_and_both_streams(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            child = self.child('import json,sys; v=json.loads(input()); print(json.dumps(v),flush=True); print("only stderr",file=sys.stderr,flush=True)', root)
            try:
                child.send({'kind': 'authored'})
                self.assertEqual(child.line(), {'kind': 'authored'})
                self.assertIsNone(child.line())
                self.assertEqual(child.finish(), 0)
            finally:
                child.close()
            self.assertIn(b'only stderr', (root / 'stderr.raw').read_bytes())

    def test_hung_parent_is_deadlined_and_child_stopped(self):
        with tempfile.TemporaryDirectory() as d:
            child = self.child('import time; time.sleep(30)', Path(d), deadline=0.05)
            try:
                with self.assertRaises(TimeoutError):
                    child.line()
            finally:
                child.close()
            self.assertIsNotNone(child.child.returncode)

    def test_oversized_line_is_not_accepted(self):
        with tempfile.TemporaryDirectory() as d:
            child = self.child('import sys; sys.stdout.write("x"*1048578); sys.stdout.flush()', Path(d))
            try:
                with self.assertRaises(ValueError):
                    child.line()
            finally:
                child.close()

    def test_duplicate_native_frame_not_silently_coalesced(self):
        with tempfile.TemporaryDirectory() as d:
            child = self.child('print(\'{"sequence":0,"sequence":1}\',flush=True)', Path(d))
            try:
                with self.assertRaises(ValueError):
                    child.line()
            finally:
                child.close()


if __name__ == '__main__':
    unittest.main()
