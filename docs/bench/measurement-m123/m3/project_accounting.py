#!/usr/bin/env python3
"""Project fabricated fake attempts to accepted-changes v1; no cost calculation."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path

_spec = importlib.util.spec_from_file_location('m3_rehearsal', Path(__file__).with_name('rehearse.py'))
rehearse = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(rehearse)


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(',', ':')).encode()).hexdigest()


def project(plan, state):
    if plan['plan_sha256'] != rehearse.sha({k: v for k, v in plan.items() if k != 'plan_sha256'}):
        raise ValueError('plan_drift')
    expected = {(task['task_id'], arm, repetition) for task in plan['tasks']
                for arm in rehearse.ARMS for repetition in (1, 2)}
    actual = [(row['task_id'], row['arm'], row['repetition']) for row in plan['sealed_order']]
    if (len(expected) != 18 or len(actual) != 18 or set(actual) != expected
            or plan['harness_version'] != rehearse.VERSION or plan['models'] != 'synthetic_fake_only'):
        raise ValueError('plan_drift')
    rehearse.validate_state(plan, state, allow_ambiguous=True)
    tasks = {t['task_id']: t for t in plan['tasks']}
    assignments = []
    for index, row in enumerate(state['assignments'], 1):
        task = tasks[row['task_id']]
        assignments.append({'assignment_id': row['assignment_id'], 'start_sequence': index,
                            'task_start_sha256': digest(row['assignment_id']),
                            'source_sha256': digest(task['source_commit']),
                            'criteria_sha256': digest([task['acceptance_script_sha256'], task['hidden_tests_sha256']])})
    cohort = {'complete': True, 'assignments': assignments}
    # Rust accepted_cohort_sha256 domain and serialized struct field order, ADR 0024.
    cohort_sha = hashlib.sha256(b'accepted-changes/cohort/v1\0' + json.dumps(cohort, separators=(',', ':')).encode()).hexdigest()
    result = {'schema_version': 1, 'cohort': cohort, 'cohort_sha256': cohort_sha,
              'analysis_sequence': 0, 'selections': [], 'revisions': [], 'verdicts': [],
              'inventories': [{'kind': k, 'coverage': 'complete', 'expected': []} for k in ('model', 'ci', 'human')],
              'observations': []}
    sequence = len(assignments)
    for row, assignment in zip(state['assignments'], assignments):
        selected = None
        for attempt in row['attempts']:
            binding = {k: assignment[k] for k in ('assignment_id', 'source_sha256', 'criteria_sha256')}
            binding.update(candidate_sha256=None, revision_sha256=None)
            status = attempt['status']
            if status in ('accepted', 'rejected'):
                binding['candidate_sha256'] = digest(['fake-patch', attempt['session_id']])
                binding['revision_sha256'] = digest(['fake-revision', attempt['session_id']])
                sequence += 1
                result['revisions'].append({'sequence': sequence, 'binding': binding.copy(), 'predecessor_sha256': selected})
                selected = binding['revision_sha256']
            sequence += 1
            key = {'authority_sha256': digest('m3-synthetic-not-provider-authority'), 'physical_sha256': digest(attempt['session_id'])}
            result['inventories'][0]['expected'].append({'key': key, 'sequence': sequence})
            result['observations'].append({'key': key, 'sequence': sequence, 'cohort_sha256': cohort_sha,
                                          'kind': 'model', 'attribution': 'cohort_once', 'links': [binding],
                                          'external_request_id': attempt['session_id'], 'evidence_sha256': digest(attempt),
                                          'agent_outcome': 'exited' if status == 'accepted' else 'failed' if status in ('rejected', 'retryable_failure') else 'unknown',
                                          'usage_coverage': 'complete' if attempt['cost_microusd'] is not None else 'unknown',
                                          'pricing_coverage': 'complete' if attempt['cost_microusd'] is not None else 'unknown',
                                          'cost_microusd': attempt['cost_microusd']})
            if selected:
                frontier = sequence
                sequence += 1
                result['verdicts'].append({'sequence': sequence, 'binding': binding.copy(), 'status': status,
                                           'verifier_outcome': 'passed' if status == 'accepted' else 'failed',
                                           'verifier_sha256': digest(['fake-verifier', attempt['session_id']]),
                                           'reviewer_sha256': digest('fake-reviewer-not-independent'),
                                           'acceptance_sequence': None,
                                           'cost_frontier_sequence': frontier if status == 'accepted' else None})
        result['selections'].append({'assignment_id': assignment['assignment_id'], 'revision_sha256': selected})
    result['analysis_sequence'] = sequence + 1
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--plan', required=True, type=Path)
    parser.add_argument('--state', required=True, type=Path)
    args = parser.parse_args()
    try:
        print(json.dumps(project(rehearse.read(args.plan), rehearse.read(args.state)), indent=2, allow_nan=False))
    except (ValueError, KeyError, TypeError, AttributeError, OSError, RecursionError, OverflowError):
        print(json.dumps({'error': 'offline_projection_refused', 'executable_ready': False}))
        raise SystemExit(2)
