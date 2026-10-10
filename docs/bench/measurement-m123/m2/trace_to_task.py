#!/usr/bin/env python3
"""Bounded, offline host-snapshot importer. No provider, store, or runtime API."""
import argparse
import copy
from datetime import datetime
import hashlib
import json
import math
from pathlib import Path
import re
import sys

MAX_BYTES = 4 * 1024 * 1024
MAX_RECORDS = 4096
TOKEN_FIELDS = ('input_tokens', 'cached_input_tokens', 'cache_write_input_tokens',
                'output_tokens', 'reasoning_output_tokens', 'total_tokens')


def bounded_bytes(path):
    with Path(path).open('rb') as stream:
        raw = stream.read(MAX_BYTES + 1)
    if len(raw) > MAX_BYTES:
        raise ValueError('input exceeds 4 MiB')
    return raw


def unique_keys(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError('duplicate JSON key')
        result[key] = value
    return result


def read_json(path):
    return json.loads(bounded_bytes(path), object_pairs_hook=unique_keys,
                      parse_constant=lambda _: reject('non-finite JSON number'))


def reject(reason):
    raise ValueError(reason)


def digest(value):
    if not isinstance(value, str) or not re.fullmatch('[0-9a-f]{64}', value):
        reject('invalid evidence digest')
    return value


def identity(value):
    if not isinstance(value, str) or not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9_.:-]{0,255}', value):
        reject('invalid identity')
    return value


def exact_keys(value, keys):
    if not isinstance(value, dict) or set(value) != set(keys):
        reject('unexpected or missing fields')


def records(value):
    if not isinstance(value, list) or len(value) > MAX_RECORDS:
        reject('invalid or excessive records')
    return value


def source_revision(value):
    if value is not None and (not isinstance(value, str) or not re.fullmatch('[0-9a-f]{40}', value)):
        reject('invalid source revision')


def revision(value):
    if value is None:
        return
    if not isinstance(value, dict):
        reject('invalid revision')
    if set(value) == {'digest'}:
        digest(value['digest'])
    elif set(value) == {'source_files_sha256'}:
        files = value['source_files_sha256']
        if not isinstance(files, dict) or not 1 <= len(files) <= MAX_RECORDS:
            reject('invalid source file set')
        for path, sha in files.items():
            if (not isinstance(path, str) or len(path) > 256
                    or not re.fullmatch(r'[A-Za-z0-9_-]+(?:/[A-Za-z0-9_-]+)*\.rs', path)):
                reject('invalid relative Rust source path')
            digest(sha)
    else:
        reject('unexpected revision fields')


def timestamp(value):
    if value is None:
        return None
    if not isinstance(value, str) or not re.fullmatch(r'\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,6})?Z', value):
        reject('invalid UTC timestamp')
    return datetime.fromisoformat(value[:-1] + '+00:00')


def seconds(value):
    if value is not None and (type(value) not in (int, float) or not 0 <= value <= 2**53 - 1
                              or not math.isfinite(value)):
        reject('invalid duration')
    return value


def configured_parent(baseline):
    model = baseline.get('configured_model')
    effort = baseline.get('configured_effort')
    if model is not None and (not isinstance(model, str) or len(model) > 64
                             or not re.fullmatch(r'gpt-\d+(?:\.\d+)?-(?:astra|sol|luna|terra)', model)):
        reject('invalid configured model')
    if effort not in (None, 'none', 'minimal', 'low', 'medium', 'high', 'xhigh', 'max', 'ultra'):
        reject('invalid configured effort')
    return {'model': model, 'effort': effort}


def checked_usage(value):
    if value is None:
        return None
    if not isinstance(value, dict) or set(value) - set(TOKEN_FIELDS):
        reject('unexpected usage fields')
    output = {key: value.get(key) for key in TOKEN_FIELDS}
    for number in output.values():
        if number is not None and (type(number) is not int or not 0 <= number <= 2**63 - 1):
            reject('invalid token counter')
    for subset, whole in [('cached_input_tokens', 'input_tokens'),
                          ('reasoning_output_tokens', 'output_tokens')]:
        if output[subset] is not None and output[whole] is not None and output[subset] > output[whole]:
            reject('counter subset exceeds total')
    if all(output[k] is not None for k in ('input_tokens', 'output_tokens', 'total_tokens')):
        if output['total_tokens'] != output['input_tokens'] + output['output_tokens']:
            reject('inconsistent token total')
    return output


def report(value):
    exact_keys(value, ('schema_version', 'task', 'attempts', 'snapshots', 'disjoint_scopes_evidence_sha256'))
    if type(value['schema_version']) is not int or value['schema_version'] != 1:
        reject('unsupported schema')
    task = value['task']
    exact_keys(task, ('task_id', 'source_revision', 'revision', 'verdict', 'tests'))
    identity(task['task_id'])
    source_revision(task['source_revision']); revision(task['revision'])
    if task['verdict'] not in ('accepted', 'rejected', 'unknown'):
        reject('invalid verdict')
    tests = {}
    for test in records(task['tests']):
        exact_keys(test, ('test_id', 'status', 'evidence_sha256'))
        key = identity(test['test_id'])
        if key in tests or test['status'] not in ('passed', 'failed', 'unknown'):
            reject('duplicate test or invalid status')
        digest(test['evidence_sha256']); tests[key] = test
    attempts = {}
    for attempt in records(value['attempts']):
        exact_keys(attempt, ('attempt_id', 'session_id', 'revision', 'test_ids', 'verdict', 'evidence_sha256'))
        key = identity(attempt['attempt_id']); identity(attempt['session_id'])
        revision(attempt['revision'])
        if key in attempts:
            reject('duplicate attempt identity')
        if attempt['verdict'] not in ('accepted', 'rejected', 'unknown'):
            reject('invalid attempt verdict')
        if any(test not in tests for test in records(attempt['test_ids'])):
            reject('unknown test link')
        digest(attempt['evidence_sha256']); attempts[key] = attempt
    disjoint = value['disjoint_scopes_evidence_sha256']
    if disjoint is not None:
        digest(disjoint)
    snapshots = {}; duplicates = 0; windows = []; seen_scopes = set(); reasons = []
    for snapshot in records(value['snapshots']):
        exact_keys(snapshot, ('snapshot_id', 'attempt_id', 'scope_id', 'start', 'end', 'usage', 'evidence_sha256'))
        key = identity(snapshot['snapshot_id']); identity(snapshot['scope_id'])
        start = timestamp(snapshot['start']); end = timestamp(snapshot['end'])
        if start is not None and end is not None and start > end:
            reject('reversed usage window')
        if snapshot['attempt_id'] not in attempts:
            reject('unknown attempt link')
        digest(snapshot['evidence_sha256'])
        if key in snapshots:
            if snapshots[key] != snapshot:
                reject('conflicting snapshot identity')
            duplicates += 1
            continue
        snapshots[key] = snapshot
        usage = checked_usage(snapshot['usage'])
        row = copy.deepcopy(snapshot); row['usage'] = usage
        row['uncached_input_tokens'] = None
        if usage and usage['input_tokens'] is not None and usage['cached_input_tokens'] is not None:
            row['uncached_input_tokens'] = usage['input_tokens'] - usage['cached_input_tokens']
        windows.append(row)
        if snapshot['scope_id'] in seen_scopes:
            reasons.append('multiple_windows_same_scope_not_additive')
        seen_scopes.add(snapshot['scope_id'])
    if len(seen_scopes) > 1 and disjoint is None:
        reasons.append('scope_overlap_unknown')
    if not windows or any(row['usage'] is None or any(v is None for v in row['usage'].values()) for row in windows):
        reasons.append('usage_incomplete')
    if set(attempts) - {row['attempt_id'] for row in windows}:
        reasons.append('attempt_without_usage')
    combined = None
    if not reasons:
        combined = {key: sum(row['usage'][key] for row in windows) for key in TOKEN_FIELDS}
    return {
        'schema_version': 1, 'kind': 'offline_host_trace_to_task',
        'evidence_trust': 'supplied_unverified', 'native_runtime_evidence': False,
        'task': copy.deepcopy(task), 'attempts': copy.deepcopy(list(attempts.values())),
        'usage': {'basis': 'host_reported', 'windows': windows,
                  'duplicate_snapshots': duplicates, 'combined': combined,
                  'combined_unknown_reasons': sorted(set(reasons)),
                  'disjoint_scopes_evidence_sha256': disjoint},
        'costs': {'host_reported_microusd': None, 'estimated_microusd': None, 'invoiced_microusd': None},
        'cost_unknown_reasons': ['no_supplied_money_observations', 'pricing_incomplete', 'invoice_absent'],
        'cost_per_accepted_task_microusd': None,
        'ratio_unknown_reason': 'no_explicitly_accepted_task' if task['verdict'] != 'accepted' else 'cost_incomplete',
        'physical_provider_calls': None, 'provider_inference_identity': None,
        'accepted_changes': {'status': 'unavailable', 'reason': 'host_session_counters_are_not_physical_work_identities'},
    }


def import_b1b(directory, measurements):
    """Read five named retained files, allowlist fields, never execute capture code."""
    directory = Path(directory)
    sources = {}
    def source(label, path):
        raw = bounded_bytes(path)
        sources[label] = hashlib.sha256(raw).hexdigest()
        return json.loads(raw, object_pairs_hook=unique_keys,
                          parse_constant=lambda _: reject('non-finite JSON number'))
    baseline = source('parent_baseline', directory / 'parent-usage-baseline.json')
    parent = source('parent_cutoff', directory / 'parent-usage-cutoff.json')
    child = source('child_retained_cutoff', directory / 'child-usage-cutoff.json')
    measured = source('public_measurements', measurements)
    sources['parent_capture_code'] = hashlib.sha256(bounded_bytes(directory / 'capture-parent-usage.py')).hexdigest()
    # Source paths, source_session, payloads, prompts and arbitrary notes are never copied.
    revision = {'source_files_sha256': measured['source']['modified_source_sha256']}
    tests = []
    for gate in records(measured['gates']):
        if not isinstance(gate, dict):
            reject('invalid gate record')
        code = gate.get('exit_code')
        if type(code) is not int or not -(2**31) <= code < 2**31:
            reject('invalid gate exit status')
        tests.append({'test_id': gate['id'], 'status': 'passed' if code == 0 else 'failed',
                      'evidence_sha256': gate['log_sha256']})
    verdict = measured['acceptance']['user_acceptance']
    if verdict not in ('accepted', 'rejected', 'unknown'):
        reject('unsupported supplied user verdict')
    attempts = [{'attempt_id': role, 'session_id': role + '-retained-session',
                 'revision': revision, 'test_ids': [t['test_id'] for t in tests],
                 'verdict': verdict, 'evidence_sha256': sources['public_measurements']}
                for role in ('coordinator', 'implementation')]
    def snapshot(name, attempt, scope, start, end, usage, evidence):
        # Missing allowlisted usage fields stay null, never silently zero.
        if usage is not None and not isinstance(usage, dict):
            reject('invalid usage object')
        allowed = None if usage is None else {k: usage.get(k) for k in TOKEN_FIELDS}
        return dict(snapshot_id=name, attempt_id=attempt, scope_id=scope, start=start,
                    end=end, usage=allowed, evidence_sha256=sources[evidence])
    latest = child.get('latest_token_event') or {}
    later = measured.get('usage', {}).get('implementation_subagent', {})
    snapshots = [
        snapshot('parent_delta', 'coordinator', 'parent_scope', parent.get('start_timestamp'),
                 parent.get('end_timestamp'), parent.get('delta'), 'parent_cutoff'),
        snapshot('child_retained_cumulative', 'implementation', 'child_scope', None,
                 latest.get('timestamp'), latest.get('info', {}).get('total_token_usage'), 'child_retained_cutoff'),
        snapshot('child_public_cumulative', 'implementation', 'child_scope', None,
                 later.get('cutoff'), later.get('reported_totals'), 'public_measurements'),
    ]
    output = report({'schema_version': 1,
                     'task': {'task_id': 'b1b-attempt-correlation-cli',
                              'source_revision': measured['source']['base_commit'], 'revision': revision,
                              'tests': tests, 'verdict': verdict},
                     'attempts': attempts, 'snapshots': snapshots,
                     'disjoint_scopes_evidence_sha256': None})
    output['sources_sha256'] = sources
    output['configured_parent'] = configured_parent(baseline)
    for attempt in output['attempts']:
        attempt['session_identity_kind'] = 'report_local_alias'
        attempt['observed_native_session_id'] = None
    output['coverage'] = {
        'source_kind': 'retained_allowlisted_host_exports', 'raw_session_read': False,
        'parent_delta_recomputed': False, 'parent_baseline_present': True,
        'child_counter_origin': 'unknown', 'session_aliases': 'producer_assigned_labels_not_native_ids',
        'attempt_granularity': 'coordinator_and_implementation_work_not_physical_requests',
        'revision_kind': 'supplied_source_file_digest_set_not_git_commit_acceptance',
        'local_checks': 'supplied_gate_results_linked_to_task_revision',
        'user_acceptance': verdict, 'human_active_time_seconds': None,
        'cutoff_excludes_later_work': True,
    }
    output['local_check_seconds'] = {g['id']: seconds(g.get('duration_seconds')) for g in measured['gates']}
    output['implementation_elapsed_seconds'] = seconds(measured.get('worker', {}).get('elapsed_seconds'))
    return output


def project_physical_evidence(value, provenance):
    """Gate passthrough to the existing calculator, never invent physical identities."""
    if provenance not in ('synthetic_supplied', 'explicit_physical_observations'):
        reject('session snapshots cannot establish physical work identity')
    required = ('schema_version', 'cohort', 'cohort_sha256', 'analysis_sequence',
                'selections', 'revisions', 'verdicts', 'inventories', 'observations')
    exact_keys(value, required)
    for observation in records(value['observations']):
        for key in ('authority_sha256', 'physical_sha256'):
            digest(observation['key'][key])
    # Full strict identity, coverage, binding and arithmetic checks belong to eval accepted-changes.
    return copy.deepcopy(value)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='command', required=True)
    normalized = sub.add_parser('report'); normalized.add_argument('--input', required=True, type=Path)
    retained = sub.add_parser('import-b1b')
    retained.add_argument('--private-dir', required=True, type=Path)
    retained.add_argument('--measurements', required=True, type=Path)
    physical = sub.add_parser('physical-input'); physical.add_argument('--input', required=True, type=Path)
    physical.add_argument('--provenance', required=True, choices=['synthetic_supplied', 'explicit_physical_observations'])
    args = parser.parse_args()
    try:
        if args.command == 'report': output = report(read_json(args.input))
        elif args.command == 'import-b1b': output = import_b1b(args.private_dir, args.measurements)
        else: output = project_physical_evidence(read_json(args.input), args.provenance)
        print(json.dumps(output, indent=2, sort_keys=True, allow_nan=False))
    except (OSError, ValueError, KeyError, TypeError, AttributeError, OverflowError, RecursionError):
        # Fixed error prevents accidental disclosure of private paths or source payloads.
        print('trace_to_task.invalid_input', file=sys.stderr)
        return 2
    return 0


if __name__ == '__main__':
    sys.exit(main())
