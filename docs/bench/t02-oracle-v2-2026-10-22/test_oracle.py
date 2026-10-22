import importlib.util
from pathlib import Path
import tempfile
import json
import hashlib
import copy
import unittest

ROOT = Path(__file__).resolve().parents[3]
ENGINE = ROOT / 'crates/router-runtime/examples/campaign/t02-v2/verify.py'
spec = importlib.util.spec_from_file_location('t02v2', ENGINE)
v = importlib.util.module_from_spec(spec)
spec.loader.exec_module(v)

class Outcomes(unittest.TestCase):
    def test_exact_population_and_summary_are_required(self):
        out = b'test a ... ok\ntest b ... FAILED\n\ntest result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n'
        self.assertEqual(v.classify(101, out, ['a', 'b'])['outcome'], 'failed')
        for code, data, names in [(0, out, ['a','b']), (101, out, ['a']), (101, out + b'test a ... ok\n', ['a','b']), (101,b'crash',['a','b']), (101,out.replace(b'0 ignored',b'1 ignored'),['a','b'])]:
            self.assertEqual(v.classify(code, data, names)['outcome'], 'unknown')

    def test_all_passes_need_zero_exit_and_no_extra_or_missing_case(self):
        out = b'test a ... ok\n\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0s\n'
        self.assertEqual(v.classify(0,out,['a'])['outcome'],'passed')
        for data in [out+b'x'*65537,out+out,out.replace(b'test a',b'test other')]:
            self.assertEqual(v.classify(0,data,['a'])['outcome'],'unknown')

    def test_missing_compiler_is_unknown(self):
        with tempfile.TemporaryDirectory() as tmp:
            row = v.evaluate_source(b'', b'#[test] fn a() {}', ['a'], Path(tmp), compiler='/nonexistent-qa174')
            self.assertEqual(row['outcome'], 'unknown')
            self.assertEqual(row['stage'], 'compile')

class EvidenceBindings(unittest.TestCase):
    def test_source_hashes_and_all_multi_implementation_outcomes_match(self):
        report=json.loads(Path(__file__).with_name('qualification.json').read_text())
        self.assertEqual(report['verifier_sha256'],v.digest(ENGINE.read_bytes()))
        self.assertEqual(report['cases_sha256'],v.digest(ENGINE.with_name('cases.rs').read_bytes()))
        self.assertEqual(len(report['required_cases']),18)
        self.assertEqual(len(set(report['required_cases'])),18)
        self.assertEqual(len(report['controls']),6)
        for row in report['controls']:
            self.assertEqual(row['expected'], row['observed']['outcome'])
            self.assertEqual(set(row['observed']['cases']),set(report['required_cases']))
        self.assertEqual([r['outcome'] for r in report['infrastructure']], ['unknown']*4)
        self.assertFalse(report['runtime_worker_executed'])
        self.assertFalse(report['decision_cases_executed'])
        self.assertFalse(report['adoption_authorized'])

    def test_runtime_receipt_binding_cannot_change_artifact_or_claim_human_acceptance(self):
        packet=next(p for p in json.loads((ROOT/'crates/router-runtime/examples/campaign/packets.json').read_text())['assignments'] if p['assignment_id']=='t02-Lm-r1-H1')
        source=packet['files']['lib.rs'].encode()
        base=ROOT/'docs/bench/native-useful-score-2026-10-21'
        candidate=(base/'submission/lib.rs').read_bytes(); patch=(base/'change.patch').read_bytes()
        report={'schema_version':1,'assignment_id':packet['assignment_id'],'packet_sha256':packet['packet_sha256'],
                'source_unchanged':True,'runtime_failed':False,'export_failed':False,'cleanup':'namespace_terminated',
                'admission':{'granted':0,'released':1},'human_acceptance':None,
                'submission':{'relative':'lib.rs','bytes':len(candidate),'sha256':v.digest(candidate)},
                'result':{'memory_enforcement':'enforced','run':{'run_id':packet['assignment_id'],'state':'verifying','artifact_sha256':v.digest(patch)},
                          'transport':{'exit_code':0,'termination':'exited'}}}
        self.assertEqual(v.verify_bindings(packet,report,source,candidate,patch)['artifact_sha256'],v.digest(patch))
        for key, value in [('human_acceptance',True),('cleanup','unknown'),('source_unchanged',False),('runtime_failed',True),('export_failed',True),('submission',{})]:
            bad=copy.deepcopy(report);bad[key]=value
            with self.assertRaises(ValueError):v.verify_bindings(packet,bad,source,candidate,patch)
        for values in [(source+b'x',candidate,patch),(source,candidate+b'x',patch),(source,candidate,patch+b'x')]:
            with self.assertRaises(ValueError):v.verify_bindings(packet,report,*values)

if __name__ == '__main__':
    unittest.main()
