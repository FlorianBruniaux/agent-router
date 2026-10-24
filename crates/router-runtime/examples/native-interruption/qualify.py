"""Instrumented native owner death; imports the frozen #207 public parent.

No provider, registry or model call. The driver arms only after accept_apply
and observes SIGSTOP before killing/reaping the exact owner. Runtime reopening
uses a separate Rust executable; it does not reconstruct application authority.
"""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import signal
import sqlite3
import subprocess
import sys

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('previous_parent', HERE / 'parent.py')
p = importlib.util.module_from_spec(spec)
spec.loader.exec_module(p)
b = p.base
sha = lambda data: hashlib.sha256(data).hexdigest()


def journal(root):
    with sqlite3.connect((root / 'runtime/runtime.sqlite3').as_uri() + '?mode=ro', uri=True) as c:
        events = c.execute('SELECT sequence,kind,state,detail FROM events ORDER BY sequence').fetchmany(257)
        assert len(events) <= 256
        summary = b.loads(c.execute("SELECT summary FROM runs WHERE run_id='entry'").fetchone()[0])
        record = b.loads(c.execute("SELECT record FROM verifications WHERE run_id='entry'").fetchone()[0])
        binding = b.loads(c.execute('SELECT binding FROM admission_binding WHERE id=1').fetchone()[0])
    return {'events':events, 'summary':summary, 'verification':record,'admission_binding':binding}


def source(root, evidence):
    files = p.packet_files(root, evidence)
    return {'files':{n:sha(data) for n,data in files.items()},
            'modes':{n:oct((root/'candidate'/n).stat().st_mode & 0o7777) for n in files},
            'head':p.read_git(root/'candidate','rev-parse','HEAD'),
            'index':p.read_git(root/'candidate','ls-files','--stage'),
            'refs':p.read_git(root/'candidate','for-each-ref','refs/agent-router/')}


def assert_journal(root, evidence, state, complete):
    review = b.loads((evidence/'candidate-review.json').read_bytes())['review']
    events = state['events']
    integration = [(e[1], e[3]) for e in events if e[1].startswith('integration.')]
    assert integration == [('integration.intent', json.dumps(review, separators=(',', ':')))] or (
        len(integration) == (2 if complete else 1)
        and integration[0][0] == 'integration.intent' and b.loads(integration[0][1]) == review
        and (not complete or integration[1] == ('integration.applied', review['artifact_sha256'])))
    assert len(integration) == (2 if complete else 1)
    records = [b.loads(e[3]) for e in events if e[1] == 'verification.attested.v1']
    assert len(records) == 2
    assert records[0]['objective'] == 'unknown' and records[0]['validation'] == 'passed'
    assert records[1] == state['verification'] and records[1]['objective'] == 'passed'
    assert records[1]['verifier'] == 'explicit-controller-candidate-acceptance-v1'
    assert state['summary']['state'] == 'verified'
    raw_report = p.read_owned(root/'fresh-t02-verification/report.json', 1048576)
    raw_receipt = p.read_owned(root/'fresh-t02-receipt.json', 32768)
    receipt = b.loads(raw_receipt)
    assert records[0]['evidence_sha256'] == sha(raw_receipt)
    assert receipt['runtime_report_sha256'] == sha(raw_report)
    for name, data in [('pre-verification-report.json',raw_report),('fresh-t02-receipt.json',raw_receipt)]:
        destination = evidence/name
        if destination.exists():
            assert destination.read_bytes() == data
        else:
            b.write_new(destination, data)
    pre = b.loads(raw_report)
    assert pre['cleanup'] == 'namespace_terminated'
    assert pre['admission']['granted'] == 0 and pre['admission']['released'] == 1
    b.save(evidence/'cleanup-before-owner-interruption.json', {'cleanup':pre['cleanup'], 'admission':pre['admission']})


class InterruptedSession(p.QualifiedSession):
    def __init__(self, argv, env, evidence, deadline=120):
        self.cut = evidence.name in ('kill_before','kill_after')
        self.armed = False
        self.killed = False
        self.root = Path(b.loads((evidence/'config.json').read_bytes())['root'])
        if self.cut:
            env = dict(env, LD_PRELOAD='/task/programs/cut.so',
                AR_CUT_STAGE=evidence.name.removeprefix('kill_'),
                AR_CUT_TARGET=str(self.root/'candidate/lib.rs'),
                AR_CUT_ARM=str(evidence/'armed'))
        b.save(evidence/'actual-owner-launch.json', {'argv':argv,'env':env})
        super().__init__(argv, env, evidence, deadline)
        b.save(evidence/'owner-pid.json', {'pid':self.child.pid})

    def send(self, value):
        if self.cut and value.get('kind') == 'approve' and value.get('phase') == 'accept_apply':
            assert not self.armed
            b.write_new(self.evidence/'armed', b'after native cleanup and candidate review\n')
            self.armed = True
        super().send(value)

    def tick(self, end=None):
        if self.armed and not self.killed:
            pid, status = os.waitpid(self.child.pid, os.WNOHANG | os.WUNTRACED)
            if pid:
                assert pid == self.child.pid and os.WIFSTOPPED(status)
                assert os.WSTOPSIG(status) == signal.SIGSTOP
                j = journal(self.root)
                assert_journal(self.root, self.evidence, j, False)
                observed = source(self.root, self.evidence)
                review = b.loads((self.evidence/'candidate-review.json').read_bytes())['review']
                before = b.loads((self.evidence/'pre-application-source.json').read_bytes())
                expected = review['preimage_sha256'] if self.evidence.name == 'kill_before' else review['candidate_sha256']
                assert observed['files']['lib.rs'] == expected and observed['modes']['lib.rs'] == '0o600'
                assert all(observed['files'][n] == digest for n,digest in before['files'].items() if n != 'lib.rs')
                assert observed['head'] == before['head'] and not observed['refs']
                assert sha(observed['index'].encode()) == before['index_entries_sha256']
                b.save(self.evidence/'stopped-boundary.json', {'pid':pid,'wait_status':status,
                    'signal':os.WSTOPSIG(status),'source':observed,'journal':j})
                os.kill(pid, signal.SIGKILL)
                code = self.child.wait(timeout=10)
                assert code == -signal.SIGKILL
                self.killed = True
                b.save(self.evidence/'owner-reaped.json', {'pid':pid,'returncode':code,'reaped':True})
        super().tick(end)


def reopen(root, evidence, label):
    state = journal(root)
    before = source(root, evidence)
    proc = subprocess.run(['/task/programs/finite-reopen', str(root/'runtime'),state['admission_binding']['policy_sha256']],
        env={'HOME':'/task/home','PATH':'/usr/bin:/bin'}, capture_output=True, timeout=20)
    b.write_new(evidence/(label+'-stdout.json'), proc.stdout)
    b.write_new(evidence/(label+'-stderr.txt'), proc.stderr)
    assert proc.returncode == 0
    result = b.loads(proc.stdout)
    assert result['opened_by'] == 'Runtime::open_with_admission' and result['integrity'] == 'passed'
    assert result['summary'] == state['summary'] and result['verification'] == state['verification']
    assert result['cleanup'] == 'namespace_terminated'
    assert not result['replay_attempted'] and not result['application_authority_reconstructed']
    assert journal(root) == state and source(root, evidence) == before
    # Correlate Rust output to the full SQLite history, not just counts.
    actual = [[e['sequence'],e['kind'],json.dumps(e['state'],separators=(',',':')),e['detail']] for e in result['events']]
    assert actual == [list(e) for e in state['events']]
    b.save(evidence/(label+'-preservation.json'), {'source':before,'journal':state,'preserved':True})


def recovery(root, evidence, complete):
    state = journal(root)
    if not (evidence/'cleanup-before-owner-interruption.json').exists():
        assert_journal(root, evidence, state, complete)
    reopen(root, evidence, 'first-reopen')
    # Authored simulation of a later user edit, after owner death and before
    # another actual reopening. No claim that a human authored these bytes.
    target = root/'candidate/lib.rs'
    prior = source(root, evidence)
    target.write_bytes(b'// authored later user edit; must survive reopening\n')
    target.chmod(0o400)
    # packet_files accepts owner-readable, private permissions, including 0400.
    edited = source(root, evidence)
    assert edited['files']['lib.rs'] != prior['files']['lib.rs']
    b.save(evidence/'authored-later-edit.json', {'before':prior,'after':edited,'authored':True})
    reopen(root, evidence, 'edited-reopen')
    b.save(evidence/'recovery-result.json', {'status':'passed','application': 'applied' if complete else 'unknown',
        'candidate_accepted':True, 'production_resume_api':False,'native_replay_attempted':False,
        'runtime_reopened':2,'later_authored_edit_preserved':True,'model_calls':0})


def interrupted(root, task, evidence, readback):
    assert (evidence/'owner-reaped.json').exists()
    assert readback['summary']['state'] == 'verified'
    recovery(root, evidence, False)


def completed(report, raw, evidence):
    p.verify_recorded_result(report, raw, evidence)
    root = Path(b.loads((evidence/'config.json').read_bytes())['root'])
    recovery(root, evidence, True)


if __name__ == '__main__':
    assert sys.argv[1] in ('kill_before','kill_after','positive')
    b.Session = InterruptedSession
    b.interruption_acceptance = interrupted
    b.acceptance = completed
    b.main(sys.argv[1])
