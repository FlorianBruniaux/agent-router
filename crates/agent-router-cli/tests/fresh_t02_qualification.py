# SPDX-FileCopyrightText: 2026 Florian Bruniaux
# SPDX-License-Identifier: Apache-2.0
"""Inert binding tests: no native host, compiler, account or model."""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import unittest
import tempfile
import os
import subprocess

ROOT = Path(__file__).resolve().parents[3]
MODULE = ROOT / 'crates/agent-router-cli/src/finite_entry/qualification/fresh_t02.py'
spec = importlib.util.spec_from_file_location('fresh_t02', MODULE)
fresh = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fresh)
sha = lambda b: hashlib.sha256(b).hexdigest()

class Binding(unittest.TestCase):
    def setUp(self):
        p = ROOT / 'crates/router-runtime/examples/campaign'
        self.baseline = json.loads((p / 't02-v2/acceptance.json').read_bytes())
        packet = next(v for v in json.loads((p / 'packets.json').read_bytes())['assignments'] if v['assignment_id'] == 't02-Lm-r1-H1')
        self.source = packet['files']['lib.rs'].encode()
        self.candidate = (ROOT / 'docs/bench/native-useful-score-2026-10-21/submission/lib.rs').read_bytes()
        self.patch = (ROOT / 'docs/bench/native-useful-score-2026-10-21/change.patch').read_bytes()
        self.submission = json.dumps({'output':'lib.rs','public_files':{n:{'content':v,'sha256':sha(v.encode())} for n,v in packet['files'].items()}}).encode()
        run = {'run_id':'entry','state':'verifying','artifact_sha256':sha(self.patch)}
        self.report = {'schema_version':1,'source_revision':'3'*40,'session_id':'1'*64,'submission_sha256':sha(self.submission),'run_id':'entry',
            'summary':{**run,'attempts':1,'receipt_id':'2'*64},'result':{'run':run,'memory_enforcement':'enforced','transport':{'exit_code':0,'termination':'exited'}},
            'runtime_error':False,'broker_error':False,'source_unchanged':True,'cleanup':'namespace_terminated','admission':{'granted':0,'released':1},
            'artifact':{'patch':self.patch.decode(),'sha256':sha(self.patch)},'output':{'name':'lib.rs','content':self.candidate.decode()},
            'human_acceptance':None,'independent_verification_performed':False}

    def bind(self, **kw):
        args = dict(report=self.report,submission=self.submission,source=self.source,candidate=self.candidate,patch=self.patch,compiler_sha256=self.baseline['compiler_sha256'])
        args.update(kw)
        return fresh.bind(**args)

    def test_binds_real_fresh_identity_without_campaign_translation(self):
        result = self.bind()
        self.assertEqual(result.get('session_id'), '1'*64)
        self.assertEqual(result.get('source_revision'), '3'*40)
        self.assertEqual(result.get('run_id'), 'entry')
        self.assertEqual(result.get('submission_sha256'), sha(self.submission))
        self.assertEqual(result.get('artifact_sha256'), '9371af9ffc9f7a79e3bbd24d6787505fdf2353f21587b272005faf950440bb9c')
        self.assertNotIn('assignment_id', result)
        self.assertNotIn('packet_sha256', result)

    def test_changed_inputs_and_compiler_never_bind(self):
        for kwargs in ({'source':self.source+b' '},{'candidate':self.candidate+b' '},{'patch':self.patch+b' '},{'submission':self.submission+b' '},{'compiler_sha256':'0'*64}):
            with self.subTest(kwargs=list(kwargs)), self.assertRaises(ValueError):
                self.bind(**kwargs)

    def test_failed_or_substituted_runtime_evidence_never_binds(self):
        for path,value in [(['source_revision'],'bad'),(['source_revision'],None),(['session_id'],'bad'),(['run_id'],'t02-Lm-r1-H1'),(['summary','state'],'verified'),(['summary','attempts'],2),
             (['result','run','run_id'],'other'),(['result','memory_enforcement'],'unknown'),(['result','transport','exit_code'],1),
             (['result','transport','termination'],'timed_out'),(['runtime_error'],True),(['broker_error'],True),(['source_unchanged'],False),
             (['cleanup'],'group_terminated'),(['admission','granted'],1),(['admission','released'],0),(['artifact','sha256'],'0'*64),
             (['output','content'],'substituted'),(['human_acceptance'],True),(['independent_verification_performed'],True)]:
            report=copy.deepcopy(self.report);node=report
            for key in path[:-1]:node=node[key]
            node[path[-1]]=value
            with self.subTest(path=path),self.assertRaises(ValueError):self.bind(report=report)

    def test_full_tree_reconstruction_preserves_other_public_files(self):
        task = fresh.loads(self.submission)
        engine = fresh.load_engine(ROOT / 'crates/router-runtime/examples/campaign/t02-v2/verify.py')
        with tempfile.TemporaryDirectory() as directory:
            work = Path(directory)
            def run(argv):
                return subprocess.run(argv, cwd=work, env=engine.environment(work), check=True, capture_output=True, timeout=10).stdout
            proof = fresh.reconstruct(task, self.candidate, self.patch, work, run)
            self.assertEqual(set(proof), {'source_tree','candidate_tree','git_mode'})
            self.assertNotEqual(proof['source_tree'], proof['candidate_tree'])
            for name, item in task['public_files'].items():
                expected = self.candidate if name == 'lib.rs' else item['content'].encode()
                self.assertEqual(run(['git','show',':'+name]), expected)
            self.assertEqual(proof['git_mode'], '100644')

    def test_changed_candidate_and_unsafe_public_path_refuse_reconstruction(self):
        engine = fresh.load_engine(ROOT / 'crates/router-runtime/examples/campaign/t02-v2/verify.py')
        for variant in ('candidate','path','mode','extra'):
            with self.subTest(variant=variant), tempfile.TemporaryDirectory() as directory:
                task = fresh.loads(self.submission)
                candidate, patch = self.candidate, self.patch
                if variant == 'candidate': candidate += b'changed'
                if variant == 'path': task['public_files']['../escape'] = {'content':'bad','sha256':sha(b'bad')}
                if variant == 'mode': patch += b'diff --git a/AGENTS.md b/AGENTS.md\nold mode 100644\nnew mode 100755\n'
                if variant == 'extra': task['public_files']['extra.txt'] = {'content':'unexpected','sha256':sha(b'unexpected')}
                work=Path(directory)
                def run(argv): return subprocess.run(argv, cwd=work, env=engine.environment(work), check=True, capture_output=True, timeout=10).stdout
                with self.assertRaises((ValueError, RuntimeError)):
                    fresh.reconstruct(task,candidate,patch,work,run)

class TrustedAssets(unittest.TestCase):
    def test_loader_refuses_changed_script_links_and_oversize(self):
        engine = ROOT / 'crates/router-runtime/examples/campaign/t02-v2/verify.py'
        self.assertEqual(fresh.load_engine(engine).VERSION, 't02-score-v2')
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'engine.py'
            path.write_bytes(b'raise AssertionError("must never execute")')
            with self.assertRaises(ValueError): fresh.load_engine(path)
            path.unlink(); path.symlink_to(engine)
            with self.assertRaises(OSError): fresh.load_engine(path)
            path.unlink(); path.write_bytes(b'x' * 32769)
            with self.assertRaises(ValueError): fresh.read(path, 32768)
            path.write_bytes(b'public'); os.link(path, Path(directory) / 'hardlink')
            with self.assertRaises(ValueError): fresh.read(path, 32768)

    def test_duplicate_and_nonfinite_json_are_refused(self):
        for raw in (b'{"a":1,"a":2}', b'{"a":NaN}'):
            with self.assertRaises(ValueError): fresh.loads(raw)

if __name__ == '__main__':
    unittest.main()
