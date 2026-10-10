#!/usr/bin/env python3
"""Fake-only M3 protocol rehearsal. No host, provider, shell or native dispatcher."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import random
import stat

ARMS = ('strongfixed', 'cheapfixed', 'router')
VERSION = 1
MAX_BYTES = 1024 * 1024
OUTCOMES = {'accepted', 'rejected', 'retryable_failure', 'ambiguous', 'interrupted'}


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':'), allow_nan=False).encode()


def sha(value):
    return hashlib.sha256(canonical(value)).hexdigest()


def pairs(rows):
    result = {}
    for key, value in rows:
        if key in result:
            raise ValueError('duplicate_key')
        result[key] = value
    return result


def read(path):
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(fd, 'rb') as stream:
        if not stat.S_ISREG(os.fstat(stream.fileno()).st_mode):
            raise ValueError('unsafe_path')
        raw = stream.read(MAX_BYTES + 1)
        if len(raw) > MAX_BYTES:
            raise ValueError('input_limit')
    return json.loads(raw, object_pairs_hook=pairs, parse_constant=lambda _: (_ for _ in ()).throw(ValueError('nonfinite')))


def write(path, value):
    temporary = path.with_suffix(path.suffix + '.tmp')
    with temporary.open('x') as stream:
        stream.write(json.dumps(value, indent=2, sort_keys=True, allow_nan=False) + '\n')
        stream.flush()
        os.fsync(stream.fileno())
    os.replace(temporary, path)


def validator(repo, candidate):
    path = repo / 'docs/bench/calibration-v2/validate_manifest.py'
    if hashlib.sha256(path.read_bytes()).hexdigest() != candidate['repository_inputs'][path.relative_to(repo).as_posix()]:
        raise ValueError('hash_drift')
    spec = importlib.util.spec_from_file_location('calibration_validator', path)
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


def prepare(root, repo):
    """Reuse calibration task/path/hash checks; this is a supplement, not a v2 manifest."""
    candidate = read(root / 'candidate.json')
    check_module = validator(repo, candidate)
    for folder, files in ((root, candidate['sealed_files']), (repo, candidate['repository_inputs'])):
        descriptor = os.open(folder, os.O_RDONLY | os.O_DIRECTORY)
        try:
            check = check_module.Check(descriptor)
            for path, digest in files.items():
                check.file(path, digest, 'sealed_input')
            if check.errors:
                raise ValueError(check.errors[0]['code'])
        finally:
            os.close(descriptor)
    descriptor = os.open(root, os.O_RDONLY | os.O_DIRECTORY)
    try:
        check = check_module.Check(descriptor)
        ids, _ = check.tasks(candidate['tasks'], 'tasks')
        if check.errors or check.missing or len(ids) != 3 or len(set(ids)) != 3:
            raise ValueError('task_contract')
    finally:
        os.close(descriptor)
    if any(t['source_commit'] != candidate['source_commit'] for t in candidate['tasks']):
        raise ValueError('source_commit')
    rng = random.Random(candidate['random_seed'])
    blocks = [(task, repetition) for task in ids for repetition in (1, 2)]
    rng.shuffle(blocks)  # Same block design as the historical pilot, one host only.
    order = []
    for task, repetition in blocks:
        arms = list(ARMS)
        rng.shuffle(arms)
        order.extend({'task_id': task, 'arm': arm, 'repetition': repetition} for arm in arms)
    plan = {**candidate, 'harness_version': VERSION, 'sealed_order': order,
            'document_type': 'm3_offline_rehearsal', 'executable_ready': False,
            'authorization': 'UNKNOWN', 'native_admission': 'UNKNOWN',
            'verifier_isolation': 'UNKNOWN', 'models': 'synthetic_fake_only'}
    plan['plan_sha256'] = sha(plan)
    return plan


def worker_input(plan, root, task_id):
    task = next(t for t in plan['tasks'] if t['task_id'] == task_id)
    return {'task_id': task_id, 'source_commit': task['source_commit'],
            'allowed_write_scope': task['allowed_write_scope'],
            'statement': (root / task['statement_path']).read_text(),
            'constraints': plan['constraints']}


def initial(plan, scenario):
    return {'plan_sha256': plan['plan_sha256'], 'mode': 'fake_only', 'scenario': scenario,
            'assignments': [{**entry, 'assignment_id': f'm3-{index + 1:02d}',
                             'workspace_id': f'fresh-{index + 1:02d}', 'attempts': []}
                            for index, entry in enumerate(plan['sealed_order'])]}


def save(path, state):
    state['state_sha256'] = sha({k: v for k, v in state.items() if k != 'state_sha256'})
    write(path, state)


def validate_state(plan, state, allow_ambiguous=False):
    if state.get('state_sha256') != sha({k: v for k, v in state.items() if k != 'state_sha256'}):
        raise ValueError('state_drift')
    expected = initial(plan, state['scenario'])
    if (state['plan_sha256'] != plan['plan_sha256'] or state['mode'] != 'fake_only'
            or len(state['assignments']) != 18):
        raise ValueError('state_drift')
    for row, base in zip(state['assignments'], expected['assignments']):
        if {k: v for k, v in row.items() if k != 'attempts'} != {k: v for k, v in base.items() if k != 'attempts'}:
            raise ValueError('state_drift')
        if len(row['attempts']) > 2:
            raise ValueError('state_drift')
        for index, attempt in enumerate(row['attempts']):
            if attempt['status'] not in OUTCOMES | {'pending'} or (index and row['attempts'][0]['status'] != 'retryable_failure'):
                raise ValueError('state_drift')
            if not allow_ambiguous and attempt['status'] in ('ambiguous', 'interrupted', 'pending'):
                raise ValueError('ambiguous_attempt')


def run_fake(plan, root, repo, path, scenario=None, limit=18, before_call=None):
    if plan != prepare(root, repo):
        raise ValueError('plan_drift')
    if type(limit) is not int or not 1 <= limit <= 18:
        raise ValueError('limit')
    scenario = None if scenario is None else {str(k): v for k, v in scenario.items()}
    if scenario is not None and any(k not in {str(i) for i in range(18)} or not isinstance(v, list)
                                    or not 1 <= len(v) <= 2 or any(x not in OUTCOMES for x in v)
                                    for k, v in scenario.items()):
        raise ValueError('scenario')
    lock = path.with_suffix(path.suffix + '.lock')
    try:
        lock.mkdir()
    except FileExistsError:
        raise ValueError('concurrent_or_interrupted_writer') from None
    try:
        if path.exists():
            state = read(path)
            validate_state(plan, state)
            if scenario is not None and scenario != state['scenario']:
                raise ValueError('scenario_drift')
        else:
            state = initial(plan, scenario or {})
            save(path, state)  # Every assignment exists before the first fake attempt.
        for index, row in enumerate(state['assignments'][:limit]):
            while len(row['attempts']) < 2 and (not row['attempts'] or row['attempts'][-1]['status'] == 'retryable_failure'):
                if plan != prepare(root, repo):
                    raise ValueError('plan_drift')
                attempt = {'attempt': len(row['attempts']) + 1,
                           'session_id': f"fake-{row['assignment_id']}-{len(row['attempts']) + 1}",
                           'status': 'pending', 'cost_microusd': None}
                row['attempts'].append(attempt)
                save(path, state)
                if before_call:
                    before_call(index, worker_input(plan, root, row['task_id']))
                outcomes = state['scenario'].get(str(index), ['accepted'])
                outcome = outcomes[min(len(row['attempts']) - 1, len(outcomes) - 1)]
                attempt['status'] = 'pending' if outcome == 'interrupted' else outcome
                # Fabricated accounting fixture amounts, never measured price or spend.
                attempt['cost_microusd'] = None if outcome in ('ambiguous', 'interrupted') else 1000
                save(path, state)
                if outcome in ('ambiguous', 'interrupted'):
                    return state
        return state
    finally:
        lock.rmdir()


def summarize(state):
    rows = state['assignments']
    attempts = [a for row in rows for a in row['attempts']]
    return {'mode': 'fake_only', 'assigned': len(rows), 'attempts': len(attempts),
            'accepted': sum(bool(r['attempts']) and r['attempts'][-1]['status'] == 'accepted' for r in rows),
            'unrun': sum(not r['attempts'] for r in rows),
            'ambiguous': sum(a['status'] in ('pending', 'ambiguous') for a in attempts),
            'cost_calculator': 'agent-router eval accepted-changes',
            'authorization': 'UNKNOWN', 'executable_ready': False}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command', choices=['prepare', 'fake', 'report'])
    parser.add_argument('--repo', type=Path, default=Path(__file__).resolve().parents[4])
    parser.add_argument('--state', type=Path)
    parser.add_argument('--limit', type=int, default=18)
    parser.add_argument('--scenario', type=Path)
    args = parser.parse_args()
    try:
        root = Path(__file__).resolve().parent
        plan = prepare(root, args.repo)
        if args.command == 'prepare':
            result = plan
        elif args.state is None:
            raise ValueError('state_required')
        elif args.command == 'report':
            state = read(args.state)
            validate_state(plan, state, allow_ambiguous=True)
            result = summarize(state)
        else:
            scenario = read(args.scenario) if args.scenario else None
            result = summarize(run_fake(plan, root, args.repo, args.state, scenario, args.limit))
        print(json.dumps(result, indent=2, sort_keys=True))
    except (ValueError, KeyError, TypeError, OSError):
        print(json.dumps({'error': 'offline_rehearsal_refused', 'executable_ready': False}))
        raise SystemExit(2)
