import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class LedgerCliTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.folder = Path(self.tmp.name)
        self.protocol = self.folder / 'protocol'
        self.protocol.mkdir()
        for name in ('gate.py', 'capture.py'):
            shutil.copyfile(ROOT / name, self.protocol / name)
        for name in ('README.md', 'prompt.md'):
            (self.protocol / name).write_text('Fixture only.\n')
        (self.protocol / 'manifest.json').write_text(json.dumps({'assignments': [
            {'arm_id': name, 'allocation_tokens': 100} for name in ('a', 'b', 'c')], 'envelope_tokens': 300}))
        self.seal()
        self.state = self.folder / 'state.json'

    def seal(self):
        files = {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in self.protocol.iterdir() if p.name != 'seal.json'}
        path = self.protocol / 'seal.json'
        path.write_text(json.dumps({'files_sha256': files}))
        self.digest = hashlib.sha256(path.read_bytes()).hexdigest()

    def run_gate(self, action, *extra, approval=None):
        return subprocess.run([sys.executable, '-B', str(self.protocol / 'gate.py'), action,
            '--state', str(self.state), '--approval-seal', approval or self.digest, *extra], capture_output=True, text=True)

    def test_unapproved_protocol_does_not_create_state(self):
        r = self.run_gate('init', '--parent-thread-id', 'parent', approval='wrong')
        self.assertEqual(r.returncode, 3, r.stderr)
        self.assertFalse(self.state.exists())

    def test_pending_reservation_survives_process_restart(self):
        self.assertEqual(self.run_gate('init', '--parent-thread-id', 'parent').returncode, 0)
        self.assertEqual(self.run_gate('reserve').returncode, 0)
        original = self.state.read_bytes()
        self.assertEqual(self.run_gate('reserve').returncode, 3)
        self.assertEqual(self.run_gate('init', '--parent-thread-id', 'parent').returncode, 3)
        self.assertEqual(self.state.read_bytes(), original)

    def test_protocol_drift_refuses_reservation_without_rewriting_state(self):
        self.assertEqual(self.run_gate('init', '--parent-thread-id', 'parent').returncode, 0)
        original = self.state.read_bytes()
        (self.protocol / 'prompt.md').write_text('Changed task.\n')
        r = self.run_gate('reserve')
        self.assertEqual(r.returncode, 3)
        self.assertEqual(json.loads(r.stdout)['reason'], 'protocol_drift')
        self.assertEqual(self.state.read_bytes(), original)

    def test_duplicate_json_key_refused(self):
        self.assertEqual(self.run_gate('init', '--parent-thread-id', 'parent').returncode, 0)
        self.state.write_text('{"status":"running","status":"ready"}')
        self.assertEqual(self.run_gate('reserve').returncode, 3)


if __name__ == '__main__':
    unittest.main()
