#!/usr/bin/env python3
"""Regression fixtures for sealed M3 inputs, without a historical host launch."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest import mock


class BaselineTests(unittest.TestCase):
    def setUp(self):
        spec = importlib.util.spec_from_file_location('baseline', Path(__file__).with_name('check-m3-baseline.py'))
        self.m = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(self.m)
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name) / 'repo'
        self.root.mkdir()
        self.out = Path(self.tmp.name) / 'snapshot'
        trace = mock.patch.dict(os.environ, {'GIT_TRACE2_EVENT': '0'})
        trace.start()
        self.addCleanup(trace.stop)
        self.file = self.root / 'docs/bench/campaign-next/pilot/run_pilot.py'
        self.file.parent.mkdir(parents=True)
        self.file.write_text('broken historical fixture\n')
        for scope in self.m.SCOPES:
            path = self.root / scope
            if path == self.file.parent: continue
            if path.suffix:
                path.parent.mkdir(parents=True, exist_ok=True); path.write_text('fixture\n')
            else:
                path.mkdir(parents=True, exist_ok=True); (path / 'fixture').write_text('fixture\n')
        subprocess.run(['git', 'init', '-q', str(self.root)], check=True)
        subprocess.run(['git', '-C', str(self.root), 'add', '--', *self.m.SCOPES], check=True)
        subprocess.run(['git', '-C', str(self.root), '-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid', '-c', 'commit.gpgsign=false', 'commit', '-qm', 'fixture'], check=True)
        self.revision = subprocess.check_output(['git', '-C', str(self.root), 'rev-parse', 'HEAD'], text=True).strip()
        self.candidate = self.root / 'docs/bench/measurement-m123/m3/candidate.json'
        self.candidate.parent.mkdir(parents=True)
        self.value = {'source_commit': self.revision, 'repository_inputs': {self.file.relative_to(self.root).as_posix(): hashlib.sha256(self.file.read_bytes()).hexdigest()}}
        self.candidate.write_text(json.dumps(self.value))

    def test_archive_preserves_historical_input_after_live_fix(self):
        self.file.write_text('fixed live implementation\n')
        self.assertEqual(self.m.prepare(self.root, self.out), self.revision)
        frozen = self.out / self.file.relative_to(self.root)
        self.assertEqual(frozen.read_text(), 'broken historical fixture\n')
        self.assertEqual(self.file.read_text(), 'fixed live implementation\n')
        self.assertEqual(frozen.stat().st_mode & 0o777, 0o600)

    def test_hash_drift_refuses_frozen_input(self):
        self.value['repository_inputs'][self.file.relative_to(self.root).as_posix()] = '0' * 64
        self.candidate.write_text(json.dumps(self.value))
        with self.assertRaisesRegex(ValueError, 'baseline_hash'):
            self.m.prepare(self.root, self.out)

    def test_oversized_archive_refuses_before_extraction(self):
        with mock.patch.object(self.m, 'MAX_ARCHIVE', 1):
            with self.assertRaisesRegex(ValueError, 'baseline_size'):
                self.m.prepare(self.root, self.out)
        self.assertFalse(self.out.exists())

    def test_archived_symlink_is_never_followed(self):
        link = self.file.parent / 'link'
        link.symlink_to('../outside')
        subprocess.run(['git', '-C', str(self.root), 'add', '--', str(link.relative_to(self.root))], check=True)
        subprocess.run(['git', '-C', str(self.root), '-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid', '-c', 'commit.gpgsign=false', 'commit', '-qm', 'symlink fixture'], check=True)
        self.value['source_commit'] = subprocess.check_output(['git', '-C', str(self.root), 'rev-parse', 'HEAD'], text=True).strip()
        self.candidate.write_text(json.dumps(self.value))
        with self.assertRaisesRegex(ValueError, 'baseline_path'):
            self.m.prepare(self.root, self.out)

    def test_absent_or_invalid_revision_fails_closed(self):
        for revision in ['0' * 40, '--output=elsewhere']:
            self.value['source_commit'] = revision
            self.candidate.write_text(json.dumps(self.value))
            with self.assertRaises((ValueError, subprocess.CalledProcessError)):
                self.m.prepare(self.root, self.out)
            self.assertFalse(self.out.exists())


if __name__ == '__main__':
    unittest.main()
