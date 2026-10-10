#!/usr/bin/env python3
"""Offline admission ledger. Never launches, interrupts or calls a model."""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import tempfile
import capture

require = capture.require


def read_json(path):
    with Path(path).open('rb') as stream:
        raw = stream.read(1024 * 1024 + 1)
    require(len(raw) <= 1024 * 1024, 'input_size')
    return json.loads(raw, object_pairs_hook=capture.no_duplicates)


def protocol(folder, approval):
    folder = Path(folder).resolve()
    with (folder / 'seal.json').open('rb') as stream:
        raw = stream.read(65537)
    require(len(raw) <= 65536, 'seal_size')
    digest = hashlib.sha256(raw).hexdigest()
    require(approval == digest, 'approval_missing_or_stale')
    files = json.loads(raw, object_pairs_hook=capture.no_duplicates)['files_sha256']
    require(isinstance(files, dict) and 0 < len(files) <= 32, 'seal_inventory')
    for name, expected in files.items():
        file = (folder / name).resolve()
        require(file.is_relative_to(folder) and not Path(name).is_absolute(), 'seal_path')
        with file.open('rb') as stream:
            content = stream.read(1024 * 1024 + 1)
        require(len(content) <= 1024 * 1024, 'sealed_file_size')
        require(hashlib.sha256(content).hexdigest() == expected, 'protocol_drift')
    require({'manifest.json', 'gate.py', 'capture.py', 'prompt.md', 'README.md'} <= files.keys(), 'seal_incomplete')
    manifest = read_json(folder / 'manifest.json')
    assignments = manifest['assignments']
    require(isinstance(assignments, list) and len(assignments) == 3, 'assignment_count')
    require(len({a['arm_id'] for a in assignments}) == 3, 'assignment_collision')
    require(all(type(a['allocation_tokens']) is int and a['allocation_tokens'] > 0 for a in assignments), 'allocation')
    require(sum(a['allocation_tokens'] for a in assignments) == manifest['envelope_tokens'], 'allocation_sum')
    return manifest, digest


def reserve(state, manifest):
    require(state['status'] == 'ready', 'not_ready_or_running')
    records = state['records']
    require(len(records) < len(manifest['assignments']), 'cohort_complete')
    require(all(r['status'] == 'completed' for r in records), 'previous_incomplete')
    total = sum(r['capture']['cumulative']['total_tokens'] for r in records)
    assignment = manifest['assignments'][len(records)]
    require(total + assignment['allocation_tokens'] <= manifest['envelope_tokens'], 'insufficient_envelope')
    records.append({'arm_id': assignment['arm_id'], 'status': 'running', 'capture': None, 'acceptance': None})
    state['status'] = 'running'
    return assignment


def finish(state, manifest, raw, identity):
    require(state['status'] == 'running', 'no_running_assignment')
    row = state['records'][-1]
    assignment = manifest['assignments'][len(state['records']) - 1]
    row['thread_id'] = identity
    reason = None
    try:
        require(identity not in [r.get('thread_id') for r in state['records'][:-1]], 'session_reused')
        proof = capture.parse(raw, identity, state['parent_thread_id'], assignment['agent_path'], assignment['model'], assignment['effort'])
        row['capture'] = proof
        require(proof['terminal'] and proof['task_started_count'] == proof['task_complete_count'] == 1, 'incomplete_or_repeated_turn')
        require(proof['tool_call_count'] == 0 and proof['response_record_count'] == 1, 'one_response_contract_violated')
        require(proof['cumulative']['total_tokens'] <= assignment['allocation_tokens'], 'participant_budget_exceeded')
    except (ValueError, KeyError, TypeError, AttributeError, UnicodeError) as error:
        reason = str(error) if isinstance(error, ValueError) and str(error).replace('_', '').isalnum() else 'invalid_trace'
    row['status'] = 'stopped' if reason else 'completed'
    row['reason'] = reason
    state['status'] = 'stopped' if reason else ('complete' if len(state['records']) == len(manifest['assignments']) else 'ready')
    return {'status': state['status'], 'reason': reason, 'tokens': (row.get('capture') or {}).get('cumulative', {}).get('total_tokens')}


def save(path, state):
    descriptor, name = tempfile.mkstemp(prefix='.m4-state-', dir=path.parent)
    try:
        with os.fdopen(descriptor, 'w') as stream:
            json.dump(state, stream, indent=2)
            stream.write('\n')
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(name, path)
    finally:
        if os.path.exists(name):
            os.unlink(name)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=('init', 'reserve', 'finish'))
    parser.add_argument('--protocol', type=Path, default=Path(__file__).resolve().parent)
    parser.add_argument('--state', type=Path, required=True)
    parser.add_argument('--approval-seal', required=True)
    parser.add_argument('--parent-thread-id')
    parser.add_argument('--thread-id')
    parser.add_argument('--trace', type=Path)
    args = parser.parse_args()
    try:
        manifest, digest = protocol(args.protocol, args.approval_seal)
        args.state.parent.mkdir(parents=True, exist_ok=True)
        with Path(str(args.state) + '.lock').open('a') as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            if args.action == 'init':
                require(not args.state.exists(), 'existing_cohort_cannot_reset')
                require(bool(args.parent_thread_id), 'parent_required')
                state = {'protocol_sha256': digest, 'status': 'ready', 'parent_thread_id': args.parent_thread_id, 'records': []}
                result = {'status': 'ready', 'launch_authority': 'operator_only'}
            else:
                state = read_json(args.state)
                require(state['protocol_sha256'] == digest, 'state_protocol_mismatch')
                if args.action == 'reserve':
                    result = {'status': 'reserved', 'assignment': reserve(state, manifest), 'launch_authority': 'operator_only'}
                else:
                    require(args.trace is not None and bool(args.thread_id), 'trace_binding_required')
                    try:
                        with args.trace.open('rb') as stream:
                            raw = stream.read(capture.MAX_BYTES + 1)
                    except OSError:
                        raw = b''
                    result = finish(state, manifest, raw, args.thread_id)
            save(args.state, state)
        print(json.dumps(result))
        return 3 if state['status'] == 'stopped' else 0
    except (ValueError, KeyError, TypeError, AttributeError, OSError) as error:
        reason = str(error) if isinstance(error, ValueError) and str(error).replace('_', '').isalnum() else 'invalid_or_unavailable_input'
        print(json.dumps({'status': 'refused', 'reason': reason}))
        return 3


if __name__ == '__main__':
    raise SystemExit(main())
