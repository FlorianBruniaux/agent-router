"""Offline developer qualification, not a campaign or a provider run."""
import importlib.util
import json
import os
from pathlib import Path
import re
import tempfile

ROOT = Path(__file__).resolve().parents[3]
ENGINE = ROOT / 'crates/router-runtime/examples/campaign/t02-v2/verify.py'
spec = importlib.util.spec_from_file_location('t02v2', ENGINE)
v = importlib.util.module_from_spec(spec)
spec.loader.exec_module(v)


def main():
    os.umask(0o077)
    assert Path('/sys/fs/cgroup/memory.max').read_text().strip() == '8589934592'
    assert Path('/sys/fs/cgroup/memory.swap.max').read_text().strip() == '0'
    checks = ENGINE.with_name('cases.rs').read_bytes()
    names = [n.decode() for n in re.findall(rb'fn (t02_v2_[a-z0-9_]+)\(', checks)]
    frozen = ROOT / 'docs/bench/t02-oracle-qualification-2026-10-21'
    labels = json.loads((frozen / 'labels.json').read_text())
    rows = []
    for name in labels['correct_controls'] + labels['known_defects']:
        rel = f'controls/{name}.rs'
        source = (frozen / rel).read_bytes()
        assert v.digest(source) == labels['reviewed_input_sha256'][rel]
        with tempfile.TemporaryDirectory() as work:
            outcome = v.evaluate_source(source, checks, names, Path(work))
        expected = 'passed' if name in labels['correct_controls'] else 'failed'
        rows.append({'control': name, 'source_sha256': v.digest(source), 'expected': expected, 'observed': outcome})
    failures=[]
    with tempfile.TemporaryDirectory() as temp:
        work=Path(temp)
        failures.append(v.evaluate_source(b'', checks, names, work, compiler='/nonexistent-qa174'))
        failures.append(v.evaluate_source(b'compile_error!("infrastructure probe");', checks, names, work))
        for argv, timeout in [(['/bin/sleep','2'],0.02),(['/usr/bin/python3','-c','print("x"*70000)'],20)]:
            try:
                v.run(argv,work,v.environment(work),timeout=timeout)
                raise AssertionError('probe unexpectedly completed')
            except v.InfrastructureFailure as e:
                failures.append({'outcome':'unknown','reason':str(e)})
    assert all(r['observed']['outcome']==r['expected'] for r in rows), rows
    assert all(r['outcome']=='unknown' for r in failures)
    assert [r['reason'] for r in failures[2:]]==['timeout','output_limit']
    # Synthetic binding fixture, with public authored bytes. No Runtime worker runs.
    packet=next(p for p in json.loads((ROOT/'crates/router-runtime/examples/campaign/packets.json').read_text())['assignments'] if p['assignment_id']=='t02-Lm-r1-H1')
    source=packet['files']['lib.rs'].encode()
    public=ROOT/'docs/bench/native-useful-score-2026-10-21'
    candidate=(public/'submission/lib.rs').read_bytes()
    patch=(public/'change.patch').read_bytes()
    report={'schema_version':1,'assignment_id':packet['assignment_id'],'packet_sha256':packet['packet_sha256'],
            'source_unchanged':True,'runtime_failed':False,'export_failed':False,'cleanup':'namespace_terminated',
            'admission':{'granted':0,'released':1},'human_acceptance':None,
            'submission':{'relative':'lib.rs','bytes':len(candidate),'sha256':v.digest(candidate)},
            'result':{'memory_enforcement':'enforced','run':{'run_id':packet['assignment_id'],'state':'verifying','artifact_sha256':v.digest(patch)},
                      'transport':{'exit_code':0,'termination':'exited'}}}
    with tempfile.TemporaryDirectory() as temp:
        work=Path(temp)
        for n,b in [('lib.rs',candidate),('source-lib.rs',source),('change.patch',patch),('runtime-result.json',json.dumps(report).encode())]:
            (work/n).write_bytes(b)
        out=work/'receipt.json'
        v.run(['/usr/bin/python3','-I','-B',str(ENGINE),'--input',str(work),'--output',str(out)],work,v.environment(work),timeout=120)
        receipt=json.loads(out.read_text())
        assert receipt['outcomes']['candidate']['outcome']=='passed'
        assert receipt['outcomes']['source']['outcome']=='failed'
        assert receipt['decision_cases_executed'] is False and receipt['human_acceptance'] is None
        assert out.stat().st_mode & 0o777 == 0o600
        receipt_bytes=out.read_bytes()
        # Reusing a result path cannot silently rewrite earlier evidence.
        try:
            v.run(['/usr/bin/python3','-I','-B',str(ENGINE),'--input',str(work),'--output',str(out)],work,v.environment(work),timeout=120)
            raise AssertionError('existing receipt overwritten')
        except v.InfrastructureFailure:
            assert out.read_bytes()==receipt_bytes
        # Refuse unreviewed byte variants in the explicit runtime fixture path.
        (work/'lib.rs').write_bytes(candidate+b'\n')
        denied=work/'denied.json'
        try:
            v.run(['/usr/bin/python3','-I','-B',str(ENGINE),'--input',str(work),'--output',str(denied)],work,v.environment(work),timeout=120)
            raise AssertionError('unreviewed candidate accepted')
        except v.InfrastructureFailure:
            assert not denied.exists()
    output=Path('/task/v2-results')
    output.mkdir(mode=0o700)
    (output/'acceptance.json').write_bytes(receipt_bytes)
    evidence={'schema_version':1,'evaluator_id':v.VERSION,'verifier_sha256':v.digest(ENGINE.read_bytes()),
              'cases_sha256':v.digest(checks),'required_cases':names,'controls':rows,'infrastructure':failures,
              'labels_sha256':v.digest((frozen/'labels.json').read_bytes()),'limits':{'memory_bytes':8589934592,'swap_bytes':0},
              'compiler_version':receipt['compiler_version'],'compiler_sha256':receipt['compiler_sha256'],
              'scope':'finite multi-implementation qualification and synthetic binding fixture',
              'runtime_worker_executed':False,'benchmark_executed':False,'decision_cases_executed':False,
              'human_acceptance':None,'adoption_authorized':False,'model_calls':0,'registry_access':False}
    (output/'qualification.json').write_text(json.dumps(evidence,indent=2)+'\n')
    print(json.dumps({'controls':len(rows),'required_cases':len(names),'infrastructure_unknown':len(failures),'fixture_receipt':'passed'}))

if __name__=='__main__':
    main()
