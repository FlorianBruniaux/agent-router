"""Public fixture acceptance only. Never opens a Runtime or starts a worker."""
import copy
import json
import os
from pathlib import Path
import controller as c


def main():
    os.umask(0o077)
    assert os.getuid() == 65534
    assert Path('/sys/fs/cgroup/memory.max').read_text().strip() == '8589934592'
    assert Path('/sys/fs/cgroup/memory.swap.max').read_text().strip() == '0'
    retained = c.HERE / 'retained'
    frames = c.loads((retained / 'frames.json').read_bytes())
    submission = c.loads((retained / 'submission.jsonl').read_bytes())
    readback = c.loads((retained / 'independent-readback.json').read_bytes())
    parent = c.loads((retained / 'parent-result.json').read_bytes())
    raw_submission = (retained / 'submission.jsonl').read_bytes()
    assert raw_submission.endswith(b'\n')
    submission_sha256 = c.sha(raw_submission[:-1])
    assert parent['status'] == 'failed' and parent['error'] == 'IndexError: 4'
    assert parent['exit_code'] == parent['child_exit_code_after_stop'] == 3
    assert parent['worker_requests'] == 6 and parent['observed_tool_results'] == 4
    assert parent['distinct_permission_denials_observed'] == 1
    assert frames[-1]['kind'] == 'entry.result'
    assert [f['sequence'] for f in frames] == list(range(len(frames)))
    assert len({f['session_id'] for f in frames}) == 1
    assert all(f['submission_sha256'] == submission_sha256 for f in frames)
    report = frames[-1]['report']
    assert report['submission_sha256'] == submission_sha256
    assert report['session_id'] == parent['session_id'] == frames[0]['session_id']
    assert report['result']['memory_enforcement'] == 'enforced'
    assert report['summary'] == readback['summary']
    assert report['summary']['state'] == 'verifying' and report['summary']['attempts'] == 1
    assert len(readback['attempts']) == 1
    assert report['runtime_error'] is False and report['broker_error'] is False
    assert report['human_acceptance'] is None and report['source_unchanged'] is True
    assert report['cleanup'] == 'namespace_terminated'
    assert report['admission']['released'] == 1 and report['admission']['granted'] == 0
    assert readback['admission_counts'] == {'released': 1}
    patch_digest = c.sha(report['artifact']['patch'].encode())
    assert patch_digest == report['artifact']['sha256'] == readback['persisted_artifact_sha256']
    assert patch_digest == report['summary']['artifact_sha256']
    route = next(f['receipt'] for f in frames if f['kind'] == 'entry.route')
    assert route['receipt_id'] == report['summary']['receipt_id']
    for name, pin in submission['public_files'].items():
        assert c.sha(pin['content'].encode()) == pin['sha256'] == readback['source_files'][name]
    requests = [f['request'] for f in frames if f['kind'] == 'entry.request']
    assert [f['request_id'] for f in frames if f['kind'] == 'entry.request'] == list(range(1, 7))
    assert len(requests) == 6
    assert all(r['model'] == submission['model'] and r['reasoning']['effort'] == submission['effort'] for r in requests)
    source = submission['public_files']['lib.rs']['content'].encode()
    assert source == (c.HERE / 'inputs/lib.rs').read_bytes()
    out = Path('/task/evidence/positive')
    c.acceptance(report, source, out)
    receipt = c.loads((out / 'public-acceptance.json').read_bytes())
    assert receipt['results']['candidate']['outcome'] == 'passed'
    assert receipt['results']['source']['outcome'] == 'failed'
    assert len(receipt['results']['candidate']['cases']) == 18
    controls = []
    for name in ('candidate_changed', 'artifact_digest_changed'):
        changed = copy.deepcopy(report)
        if name == 'candidate_changed':
            changed['output']['content'] += '\n'
        else:
            changed['artifact']['sha256'] = '0' * 64
        target = Path('/task') / name
        target.mkdir(mode=0o700)
        try:
            c.acceptance(changed, source, target)
        except AssertionError:
            assert not (target / 'public-acceptance.json').exists()
            controls.append({'case': name, 'outcome': 'refused_before_evaluation'})
        else:
            raise AssertionError('mutated binding accepted')
    c.save(out / 'external-resolution.json', {
        'status': 'public_acceptance_qualified_on_retained_native_artifact',
        'original_parent_status': 'failed', 'original_parent_error': 'IndexError: 4',
        'original_parent_record_unchanged': True,
        'retained_hashes': {n: c.sha((retained / n).read_bytes()) for n in
                            ('frames.json', 'submission.jsonl', 'independent-readback.json', 'parent-result.json')},
        'artifact_sha256': patch_digest, 'candidate_sha256': receipt['candidate_sha256'],
        'submission_sha256': submission_sha256, 'session_id': report['session_id'],
        'source_sha256': receipt['source_sha256'], 'controls': controls,
        'verifier_sha256': c.sha(c.EVALUATOR.read_bytes()),
        'oracle_bytes_modified': False, 'new_runtime_opened': False,
        'new_worker_started': False, 'model_calls': 0, 'human_acceptance': None,
        'runtime_verdict_modified': False, 'not_a_benchmark_trial': True})
    print(json.dumps({'candidate_cases': 18, 'candidate': 'passed', 'source': 'failed',
                      'mutated_bindings_refused': len(controls), 'new_workers': 0, 'model_calls': 0}))


if __name__ == '__main__':
    main()
