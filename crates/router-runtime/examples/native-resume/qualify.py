# SPDX-FileCopyrightText: 2026 Florian Bruniaux
# SPDX-License-Identifier: Apache-2.0
"""Native product resume protocol after actual owner SIGKILL, public fixtures only.

Assembled beside the frozen interruption.py, parent.py and controller.py.
The outer runner provides a fresh isolated 8 GiB cell for each scenario.
"""
import importlib.util
import json
import os
from pathlib import Path
import selectors
import signal
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('interruption', HERE / 'interruption.py')
q = importlib.util.module_from_spec(spec)
spec.loader.exec_module(q)
b, p = q.b, q.p
SCENARIO = sys.argv[1]
old_assert = q.assert_journal


def assert_context(root, evidence, state, complete):
    rows = [r for r in state['events'] if r[1] == 'integration.context.v1']
    assert len(rows) == 1
    context = b.loads(rows[0][3])
    assert context['version'] == 1 and context['source'] == str(root / 'candidate')
    filtered = dict(state, events=[r for r in state['events'] if r[1] != 'integration.context.v1'])
    old_assert(root, evidence, filtered, complete)


q.assert_journal = assert_context


def first_line(child):
    end = time.monotonic() + 20
    data = bytearray()
    with selectors.DefaultSelector() as selector:
        selector.register(child.stdout, selectors.EVENT_READ)
        while time.monotonic() < end:
            assert selector.select(max(0, end-time.monotonic())), 'resume review timeout'
            part = os.read(child.stdout.fileno(), 4096)
            assert part, 'resume exited before review'
            data.extend(part)
            assert len(data) <= 16384
            if b'\n' in data:
                line, rest = data.split(b'\n', 1)
                assert not rest
                return b.loads(line), bytes(data)
    raise AssertionError('resume review deadline')


def invoke(root, evidence, label, approve=False, supplied=None, cut=False, wrong_admission=False):
    state = q.journal(root)
    config = evidence / ('wrong-admission-config.json' if wrong_admission else 'resume-config.json')
    value = {'schema_version':1, 'root':str(root/'runtime'), 'admission':{
        'schema_version':1, 'registry':str(root/'admission'),
        'expected_policy_sha256':state['admission_binding']['policy_sha256'],
        'runtime_id':'entry', 'session_budget':None}}
    if wrong_admission: value['admission']['expected_policy_sha256']='0'*64
    if not config.exists(): b.save(config, value)
    else: assert b.loads(config.read_bytes()) == value
    argv = ['/task/programs/agent-router','finite-resume','--config',str(config),'--run-id','entry','--json']
    if approve: argv.append('--approve')
    env = {'HOME':'/task/home','PATH':'/usr/bin:/bin'}
    if cut: env.update(LD_PRELOAD='/task/programs/cut.so', AR_CUT_STAGE='before',
        AR_CUT_TARGET=str(root/'candidate/lib.rs'), AR_CUT_ARM=str(evidence/'armed'))
    b.save(evidence/(label+'-launch.json'), {'argv':argv,'env':env})
    child = subprocess.Popen(argv, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    raw = b''
    try:
        if approve:
            event, raw = first_line(child)
            digest = supplied if supplied is not None else event['review']['approval_digest']
            if digest is not None:
                child.stdin.write(json.dumps({'schema_version':1,'kind':'resume.approve','approval_digest':digest}).encode()+b'\n')
                child.stdin.flush()
            if cut:
                end = time.monotonic()+20
                while time.monotonic()<end:
                    pid, status = os.waitpid(child.pid, os.WNOHANG|os.WUNTRACED)
                    if pid:
                        assert os.WIFSTOPPED(status) and os.WSTOPSIG(status)==signal.SIGSTOP
                        assert len([e for e in q.journal(root)['events'] if e[1]=='integration.resume_intent.v1'])==1
                        b.save(evidence/'resume-stopped.json', {'pid':pid,'signal':signal.SIGSTOP,'journal':q.journal(root),'source':q.source(root,evidence)})
                        os.kill(pid,signal.SIGKILL)
                        break
                    time.sleep(.02)
                else: raise AssertionError('resume interruption deadline')
        out, err = child.communicate(timeout=20)
        raw += out
        b.write_new(evidence/(label+'-stdout.jsonl'), raw)
        b.write_new(evidence/(label+'-stderr.txt'), err)
        b.save(evidence/(label+'-exit.json'), {'returncode':child.returncode,'reaped':True})
        if cut: assert child.returncode == -signal.SIGKILL
        events = [b.loads(line) for line in raw.splitlines()]
        return child.returncode, events
    finally:
        if child.poll() is None: child.kill(); child.wait(timeout=10)


def interrupted(root, task, evidence, readback):
    assert (evidence/'owner-reaped.json').exists()
    if SCENARIO == 'later_edit':
        (root/'candidate/lib.rs').write_bytes(b'// later edit with unchanged mode0600\n')
        assert (root/'candidate/lib.rs').stat().st_mode & 0o777 == 0o600
    before = q.source(root,evidence)
    history = q.journal(root)
    b.save(evidence/'resume-before.json', {'source':before,'journal':history})
    code,_=invoke(root,evidence,'wrong-admission',wrong_admission=True)
    assert code!=0 and q.source(root,evidence)==before and q.journal(root)==history
    code, events = invoke(root,evidence,'inspect')
    assert code==0 and len(events)==1 and events[0]['kind']=='resume.review'
    review = events[0]['review']
    expected = {'kill_before':'preimage_intact','kill_after':'candidate_observed','kill_partial':'divergent','resume_interrupted':'preimage_intact','later_edit':'divergent'}[SCENARIO]
    assert review['state']==expected
    assert q.source(root,evidence)==before and q.journal(root)==history
    if expected=='preimage_intact':
        code, _ = invoke(root,evidence,'stale',True,supplied=review['approval_digest'])
        assert code!=0 and q.source(root,evidence)==before and q.journal(root)==history
        code, events = invoke(root,evidence,'fresh',True,cut=SCENARIO=='resume_interrupted')
        if SCENARIO=='resume_interrupted':
            assert code==-signal.SIGKILL and q.source(root,evidence)==before
            terminal=False
        else:
            assert code==0 and events[-1]['kind']=='resume.applied'
            after=q.source(root,evidence)
            assert after['files']['lib.rs']==review['candidate_sha256']
            assert all(after['files'][n]==h for n,h in before['files'].items() if n!='lib.rs')
            assert all(after[k]==before[k] for k in ['modes','head','index','refs'])
            terminal=True
        after_history=q.journal(root)
        assert after_history['events'][:len(history['events'])]==history['events']
        assert after_history['verification']==history['verification'] and after_history['summary']==history['summary']
        assert [e[1] for e in after_history['events'][len(history['events']):]]==(['integration.resume_intent.v1','integration.resume_applied.v1'] if terminal else ['integration.resume_intent.v1'])
        snapshot=q.source(root,evidence)
        b.save(evidence/'resume-after-attempt.json', {'source':snapshot,'journal':after_history,'terminal':terminal})
        code, events=invoke(root,evidence,'replay',True)
        assert code!=0 and events[0]['review']['approval_digest'] is None
        assert events[0]['review']['state']==('completed' if terminal else 'consumed')
        assert q.source(root,evidence)==snapshot and q.journal(root)==after_history
        b.save(evidence/'resume-after-replay.json', {'source':snapshot,'journal':after_history,'preserved':True})
    else:
        code, events=invoke(root,evidence,'refused',True)
        assert code!=0 and events[0]['review']['approval_digest'] is None
        assert q.source(root,evidence)==before and q.journal(root)==history
        b.save(evidence/'resume-refusal-preservation.json', {'source':before,'journal':history,'preserved':True})
    # Simulated later user edit must survive even a fresh invocation.
    target=root/'candidate/lib.rs';target.write_bytes(b'// later authored edit, preserve me\n');target.chmod(0o400)
    snapshot=q.source(root,evidence);history=q.journal(root)
    # Metadata refusal occurs before a review line, so use finite inspection.
    code,_=invoke(root,evidence,'edited')
    assert code!=0 and q.source(root,evidence)==snapshot and q.journal(root)==history
    b.save(evidence/'resume-later-edit.json', {'source':snapshot,'journal':history,'preserved':True})
    b.save(evidence/'resume-result.json',{'status':'passed','scenario':SCENARIO,'production_command':True,
        'original_state':expected,'resume_applied':SCENARIO=='kill_before','later_edit_preserved':True,
        'models_started':0,'real_registry_access':False,'journal':history})


if __name__=='__main__':
    assert SCENARIO in ['kill_before','kill_after','kill_partial','resume_interrupted','later_edit']
    b.Session=q.InterruptedSession
    b.interruption_acceptance=interrupted
    b.main('kill_before' if SCENARIO in ('resume_interrupted','later_edit') else SCENARIO)
