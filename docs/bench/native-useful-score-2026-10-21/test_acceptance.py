import os
import tempfile
import hashlib
import importlib.util
from pathlib import Path
import unittest
HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('acceptance', HERE/'verify.py')
a = importlib.util.module_from_spec(spec)
spec.loader.exec_module(a)
sha = lambda b: hashlib.sha256(b).hexdigest()

class BindingTests(unittest.TestCase):
    def setUp(self):
        self.source, self.candidate, self.patch = b'broken\n', b'fixed\n', b'a patch\n'
        self.packet = {'assignment_id':'t02-Lm-r1-H1', 'packet_sha256':'a'*64, 'acceptance_sha256':'b'*64, 'output':'lib.rs', 'files':{'lib.rs':'broken\n'}}
        self.report = {'schema_version':1, 'assignment_id':'t02-Lm-r1-H1', 'packet_sha256':'a'*64,
            'cleanup':'namespace_terminated', 'admission':{'granted':0, 'released':1}, 'source_unchanged':True,
            'runtime_failed':False, 'export_failed':False, 'human_acceptance':None,
            'submission':{'relative':'lib.rs', 'bytes':6, 'sha256':sha(self.candidate)},
            'result':{'memory_enforcement':'enforced', 'run':{'run_id':'t02-Lm-r1-H1','state':'verifying', 'artifact_sha256':sha(self.patch)},
                      'transport':{'exit_code':0, 'termination':'exited'}}}
    def verify(self):
        return a.verify_bindings(self.packet,self.report,self.source,self.candidate,self.patch)
    def test_binds_real_source_candidate_and_patch(self):
        r=self.verify()
        self.assertEqual(r['candidate_sha256'],sha(b'fixed\n'))
        self.assertEqual(r['source_sha256'],sha(b'broken\n'))
        self.assertEqual(r['artifact_sha256'],sha(b'a patch\n'))
    def test_other_run_is_refused(self):
        self.report['result']['run']['run_id']='another-run'
        with self.assertRaises(ValueError): self.verify()
    def test_changed_candidate_is_refused(self):
        self.candidate=b'replaced\n'
        with self.assertRaises(ValueError): self.verify()
    def test_changed_source_is_refused(self):
        self.source=b'stale\n'
        with self.assertRaises(ValueError): self.verify()
    def test_changed_patch_is_refused(self):
        self.patch=b'replaced diff\n'
        with self.assertRaises(ValueError): self.verify()
    def test_unknown_cleanup_cannot_accept(self):
        self.report['cleanup']='unknown'
        with self.assertRaises(ValueError): self.verify()
    def test_retained_admission_cannot_accept(self):
        self.report['admission']['granted']=1
        with self.assertRaises(ValueError): self.verify()
    def test_other_packet_cannot_borrow_verdict(self):
        self.report['packet_sha256']='c'*64
        with self.assertRaises(ValueError): self.verify()
    def test_outside_file_refused(self):
        self.report['submission']['relative']='../lib.rs'
        with self.assertRaises(ValueError): self.verify()
    def test_transport_failure_not_correctness(self):
        self.report['result']['transport']['exit_code']=1
        with self.assertRaises(ValueError): self.verify()
    def test_source_change_refused(self):
        self.report['source_unchanged']=False
        with self.assertRaises(ValueError): self.verify()
    def test_runtime_failure_refused(self):
        self.report['runtime_failed']=True
        with self.assertRaises(ValueError): self.verify()
    def test_export_failure_refused(self):
        self.report['export_failed']=True
        with self.assertRaises(ValueError): self.verify()
    def test_unenforced_memory_refused(self):
        self.report['result']['memory_enforcement']='unknown'
        with self.assertRaises(ValueError): self.verify()
    def test_replaced_size_refused(self):
        self.report['submission']['bytes']=7
        with self.assertRaises(ValueError): self.verify()
    def test_noop_refused(self):
        self.candidate=self.source
        self.report['submission']={'relative':'lib.rs','bytes':len(self.source),'sha256':sha(self.source)}
        with self.assertRaises(ValueError): self.verify()
class InputTests(unittest.TestCase):
    def test_link_and_oversize_inputs_are_refused(self):
        with tempfile.TemporaryDirectory() as root:
            p=Path(root)/'data';p.write_bytes(b'abcd')
            self.assertEqual(a.read(p,4),b'abcd')
            with self.assertRaises(ValueError): a.read(p,3)
            link=Path(root)/'link';link.symlink_to(p)
            with self.assertRaises(OSError): a.read(link,4)
            hard=Path(root)/'hard';os.link(p,hard)
            with self.assertRaises(ValueError): a.read(p,4)

if __name__=='__main__': unittest.main()
