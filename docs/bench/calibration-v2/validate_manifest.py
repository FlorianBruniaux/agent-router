#!/usr/bin/env python3
"""Bounded offline U08 preparation checks. Never executes sealed files or grants authority."""
import argparse
import copy
import hashlib
import json
import math
import os
from pathlib import Path
import re
import stat

MAX_JSON = 1024 * 1024
MAX_FILE = 16 * 1024 * 1024
MAX_BINARY = 256 * 1024 * 1024
MAX_TOTAL = 512 * 1024 * 1024
MAX_USD = (2**63 - 1) // 1_000_000
HASH = re.compile(r'[0-9a-f]{64}\Z')
ID = re.compile(r'[A-Za-z0-9][A-Za-z0-9._-]{0,79}\Z')
ROLES = {'corpus', 'router_binary', 'policy_and_catalog', 'host_binary',
         'sanitized_configuration', 'dated_price_table', 'analysis_code'}
PRECONDITIONS = {'U02b', 'U04b', 'U05', 'U06', 'U07',
                 'send_control_verified', 'provenance_review'}
BASE = Path(__file__).with_name('manifest.template.json')


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':'), allow_nan=False).encode()


def pairs(entries):
    result = {}
    for key, value in entries:
        if key in result:
            raise ValueError('duplicate key')
        result[key] = value
    return result


def relative(value):
    return (isinstance(value, str) and 0 < len(value) <= 240
            and re.fullmatch(r'[A-Za-z0-9._/-]+', value) is not None
            and not value.startswith('/')
            and all(part not in ('', '.', '..') for part in value.split('/')))


class Check:
    def __init__(self, root):
        self.root = root
        self.errors = []
        self.missing = set()
        self.total = 0

    def error(self, code, field):
        self.errors.append({'code': code, 'field': field})

    def fields(self, value, expected, field):
        if not isinstance(value, dict) or set(value) != set(expected):
            self.error('fields', field)
            return False
        return True

    def text(self, value, field):
        if value is None:
            self.missing.add(field)
        elif not isinstance(value, str) or not value.strip() or len(value) > 2000:
            self.error('text', field)

    def file(self, name, digest, field, maximum=MAX_FILE):
        if name is None or digest is None:
            self.missing.add(field)
            return
        if not relative(name):
            self.error('unsafe_path', field)
            return
        if not isinstance(digest, str) or not HASH.fullmatch(digest):
            self.error('sha256', field)
            return
        # Open each component without following links, including the final file.
        # O_NONBLOCK prevents a FIFO/device from blocking before regular-file rejection.
        descriptor = None
        try:
            descriptor = os.dup(self.root)
            parts = name.split('/')
            for part in parts[:-1]:
                next_fd = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=descriptor)
                os.close(descriptor)
                descriptor = next_fd
            file_fd = os.open(parts[-1], os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=descriptor)
            with os.fdopen(file_fd, 'rb') as stream:
                metadata = os.fstat(stream.fileno())
                if not stat.S_ISREG(metadata.st_mode):
                    self.error('unsafe_path', field)
                    return
                if metadata.st_size > maximum:
                    self.error('file_limit', field)
                    return
                digest_state = hashlib.sha256()
                amount = 0
                while True:
                    chunk = stream.read(min(65536, maximum - amount + 1))
                    if not chunk:
                        break
                    amount += len(chunk)
                    self.total += len(chunk)
                    if amount > maximum or self.total > MAX_TOTAL:
                        self.error('file_limit', field)
                        return
                    digest_state.update(chunk)
                if digest_state.hexdigest() != digest:
                    self.error('hash_drift', field)
        except FileNotFoundError:
            self.error('missing_file', field)
        except OSError:
            self.error('unsafe_path', field)
        finally:
            if descriptor is not None:
                os.close(descriptor)

    def tasks(self, rows, field):
        if not isinstance(rows, list) or len(rows) > 256:
            self.error('task_limit', field)
            return [], []
        if not rows:
            self.missing.add(field)
        ids, statements = [], []
        keys = {'task_id', 'source_commit', 'allowed_write_scope', 'statement_sha256',
                'acceptance_script_sha256', 'hidden_tests_separated', 'human_review_rule',
                'statement_path', 'acceptance_script_path', 'hidden_tests_path', 'hidden_tests_sha256'}
        for index, row in enumerate(rows):
            location = f'{field}[{index}]'
            if not self.fields(row, keys, location):
                continue
            task_id = row['task_id']
            if not isinstance(task_id, str) or not ID.fullmatch(task_id):
                self.error('task_id', location)
                continue
            if task_id in ids:
                self.error('duplicate_task', location)
            ids.append(task_id)
            statements.append(row['statement_sha256'])
            if not isinstance(row['source_commit'], str) or not re.fullmatch(r'[0-9a-f]{40}|[0-9a-f]{64}', row['source_commit']):
                self.error('source_commit', location)
            scope = row['allowed_write_scope']
            if not isinstance(scope, list) or not 0 < len(scope) <= 64 or not all(relative(p) for p in scope) or len(set(scope)) != len(scope):
                self.error('unsafe_path', location + '.allowed_write_scope')
            self.text(row['human_review_rule'], location + '.human_review_rule')
            if row['hidden_tests_separated'] is not True or row['hidden_tests_path'] in (row['statement_path'], row['acceptance_script_path']) or row['hidden_tests_sha256'] == row['statement_sha256']:
                self.error('hidden_test_leak', location)
            for path_key, hash_key in [('statement_path', 'statement_sha256'), ('acceptance_script_path', 'acceptance_script_sha256'), ('hidden_tests_path', 'hidden_tests_sha256')]:
                self.file(row[path_key], row[hash_key], location + '.' + path_key)
        return ids, statements

    def validate(self, m, template):
        keys = set(template) | {'synthetic', 'input_files', 'confirmation_tasks', 'preconditions'}
        if not self.fields(m, keys, 'manifest'):
            return
        for key, value in {'document_type': 'campaign_preparation_manifest', 'template_version': 2,
                           'status': 'sealed_preparation_not_authorized_to_run', 'stage': 'calibration',
                           'execution_path': 'native_external', 'treatment': 'A-v2',
                           'include_D_if_distinct': True, 'required_task_fields': template['required_task_fields'],
                           'retry_policy': template['retry_policy']}.items():
            if canonical(m[key]) != canonical(value):
                self.error('contract', key)
        if type(m['synthetic']) is not bool:
            self.error('type', 'synthetic')
        elif m['synthetic']:
            self.missing.add('real_corpus_and_evidence')
        for key in ('campaign_id', 'host_surface_version', 'evidence_source', 'collection_consent', 'task_selection_rule'):
            self.text(m[key], key)
        if isinstance(m['campaign_id'], str) and not ID.fullmatch(m['campaign_id']):
            self.error('campaign_id', 'campaign_id')
        arms = m['comparators']
        if not isinstance(arms, list) or not arms or not all(isinstance(a, str) and a in ('E', 'D', 'S') for a in arms) or len(set(arms)) != len(arms) or 'E' not in arms:
            self.error('arms', 'comparators')
            arms = []
        arms = ['A-v2'] + arms
        usual = m['usual_usage_differs_from_E']
        if usual is None:
            self.missing.add('usual_usage_differs_from_E')
        elif type(usual) is not bool or (usual and 'D' not in arms):
            self.error('arms', 'usual_usage_differs_from_E')
        if 'S' in arms:
            self.text(m['static_rule_hypothesis'], 'static_rule_hypothesis')
        elif m['static_rule_hypothesis'] is not None:
            self.error('arms', 'static_rule_hypothesis')
        configurations = m['arm_model_effort_configurations']
        if self.fields(configurations, arms, 'arm_model_effort_configurations'):
            for arm in arms:
                value = configurations[arm]
                if self.fields(value, {'model', 'effort', 'configuration_sha256'}, 'arm_model_effort_configurations.' + arm):
                    for key in ('model', 'effort'):
                        self.text(value[key], 'arm_model_effort_configurations.' + arm + '.' + key)
                    if value['configuration_sha256'] != m['input_hashes'].get('sanitized_configuration'):
                        self.error('configuration_drift', 'arm_model_effort_configurations.' + arm)
        if self.fields(m['input_files'], ROLES, 'input_files') and self.fields(m['input_hashes'], ROLES, 'input_hashes'):
            for role in sorted(ROLES):
                self.file(m['input_files'][role], m['input_hashes'][role], 'input_hashes.' + role,
                          MAX_BINARY if role in ('router_binary', 'host_binary') else MAX_FILE)
        all_tasks = m['tasks'] + m['confirmation_tasks'] if isinstance(m['tasks'], list) and isinstance(m['confirmation_tasks'], list) else []
        exposed_paths = set(m['input_files'].values()) if isinstance(m['input_files'], dict) and all(isinstance(p, str) for p in m['input_files'].values()) else set()
        exposed_hashes = set(m['input_hashes'].values()) if isinstance(m['input_hashes'], dict) and all(isinstance(h, str) for h in m['input_hashes'].values()) else set()
        for task in all_tasks:
            if isinstance(task, dict):
                for field in ('statement_path', 'acceptance_script_path'):
                    if isinstance(task.get(field), str):
                        exposed_paths.add(task[field])
                for field in ('statement_sha256', 'acceptance_script_sha256'):
                    if isinstance(task.get(field), str):
                        exposed_hashes.add(task[field])
        if any(isinstance(t, dict) and (t.get('hidden_tests_path') in exposed_paths or t.get('hidden_tests_sha256') in exposed_hashes) for t in all_tasks):
            self.error('hidden_test_leak', 'tasks.hidden_tests')
        task_ids, statements = self.tasks(m['tasks'], 'tasks')
        held_ids, held_statements = self.tasks(m['confirmation_tasks'], 'confirmation_tasks')
        # Holding task IDs alone is insufficient: reject reused statement content too.
        if set(task_ids) & set(held_ids) or set(statements) & set(held_statements):
            self.error('heldout_overlap', 'confirmation_tasks')
        repetitions = m['repetitions']
        if type(repetitions) is not int or not 1 <= repetitions <= 32:
            self.error('repetitions', 'repetitions')
            repetitions = 0
        if type(m['random_seed']) is not int or not 0 <= m['random_seed'] < 2**64:
            self.error('seed', 'random_seed')
        order = m['sealed_order']
        expected = {(task, arm, rep) for task in task_ids for arm in arms for rep in range(1, repetitions + 1)}
        actual = []
        if not isinstance(order, list) or len(order) > 8192 or len(expected) > 8192:
            self.error('assignments', 'sealed_order')
        else:
            for row in order:
                if self.fields(row, {'task_id', 'arm', 'repetition'}, 'sealed_order'):
                    if not isinstance(row['task_id'], str) or not isinstance(row['arm'], str) or type(row['repetition']) is not int:
                        self.error('assignments', 'sealed_order')
                    else:
                        actual.append((row['task_id'], row['arm'], row['repetition']))
            if set(actual) != expected or len(actual) != len(expected):
                self.error('assignments', 'sealed_order')
        if canonical(m['analysis']) != canonical(template['analysis']):
            self.error('analysis_contract', 'analysis')
        stops = m['stop_on']
        if not isinstance(stops, list) or not all(isinstance(s, str) for s in stops) or sorted(stops) != sorted(template['stop_on']):
            self.error('stop_contract', 'stop_on')
        export = m['export']
        if self.fields(export, template['export'], 'export'):
            if not relative(export['destination']):
                self.error('unsafe_path', 'export.destination')
            for key in template['export']:
                if key != 'destination' and export[key] is not False:
                    self.error('export_contract', 'export.' + key)
        spending = m['spending']
        if self.fields(spending, set(template['spending']) | {'estimated_remaining_usd', 'enforcement_evidence_path'}, 'spending'):
            numbers = []
            for key in ('global_cap_usd', 'known_prior_spend_usd', 'estimated_remaining_usd'):
                value = spending[key]
                if value is None:
                    self.missing.add('spending.' + key)
                elif type(value) not in (int, float) or value < 0 or value > MAX_USD or not math.isfinite(value) or (key == 'global_cap_usd' and value == 0):
                    self.error('money', 'spending.' + key)
                else:
                    numbers.append(value)
            if len(numbers) == 3 and numbers[1] + numbers[2] > numbers[0]:
                self.error('budget_exceeded', 'spending')
            if spending['unknown_prior_spend'] is not False:
                if spending['unknown_prior_spend'] not in (None, True):
                    self.error('type', 'spending.unknown_prior_spend')
                self.missing.add('spending.unknown_prior_spend')
            self.file(spending['enforcement_evidence_path'], spending['enforcement_evidence_sha256'], 'spending.enforcement_evidence_sha256')
            approved = spending['approved_manifest_sha256']
            if approved is None:
                self.missing.add('spending.approved_manifest_sha256')
            elif approved != manifest_digest(m):
                self.error('approval_drift', 'spending.approved_manifest_sha256')
        if self.fields(m['preconditions'], PRECONDITIONS, 'preconditions'):
            for key in sorted(PRECONDITIONS):
                value = m['preconditions'][key]
                location = 'preconditions.' + key
                if self.fields(value, {'status', 'evidence_path', 'evidence_sha256'}, location):
                    if value['status'] == 'UNKNOWN':
                        self.missing.add(location)
                        if value['evidence_path'] is not None or value['evidence_sha256'] is not None:
                            self.error('evidence_contract', location)
                    elif value['status'] == 'verified':
                        self.file(value['evidence_path'], value['evidence_sha256'], location)
                    else:
                        self.error('evidence_contract', location)


def manifest_digest(m):
    payload = copy.deepcopy(m)
    payload['spending']['approved_manifest_sha256'] = None
    return hashlib.sha256(canonical(payload)).hexdigest()


def validate(path):
    check = Check(None)
    digest = None
    root_fd = None
    try:
        # Pin the directory once: manifest and seals share this exact descriptor.
        # Its ancestors use normal path resolution only during this initial open.
        root_fd = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY)
        check.root = root_fd
        try:
            manifest_fd = os.open(path.name, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK,
                                  dir_fd=root_fd)
        except OSError:
            check.error('unsafe_path', 'manifest')
        else:
            with os.fdopen(manifest_fd, 'rb') as stream:
                if not stat.S_ISREG(os.fstat(stream.fileno()).st_mode):
                    check.error('unsafe_path', 'manifest')
                    raw = None
                else:
                    raw = stream.read(MAX_JSON + 1)
            if raw is None:
                pass
            elif len(raw) > MAX_JSON:
                check.error('input_limit', 'manifest')
            else:
                m = json.loads(raw, object_pairs_hook=pairs, parse_constant=lambda _: (_ for _ in ()).throw(ValueError('nonfinite')))
                template = json.loads(BASE.read_text())
                if m == template and canonical(m) == canonical(template):
                    # The exact historical template is inert, not a concrete manifest.
                    check.missing.update(key for key, value in m.items() if (value is None or value == []) and key != 'static_rule_hypothesis')
                    check.missing.update('input_hashes.' + key for key in ROLES)
                    check.missing.update('spending.' + key for key in template['spending'])
                    check.missing.update({'input_files', 'confirmation_tasks', 'preconditions', 'export.destination', 'spending.estimated_remaining_usd', 'spending.enforcement_evidence_path'})
                else:
                    check.validate(m, template)
                    if not check.errors:
                        digest = manifest_digest(m)
    except (ValueError, UnicodeError, RecursionError, TypeError, AttributeError, KeyError, OSError, OverflowError):
        check.error('json', 'manifest')
    finally:
        if root_fd is not None:
            os.close(root_fd)
    report = {'structurally_valid': not check.errors,
              'offline_preparation_ready': not check.errors and not check.missing,
              'executable_ready': False, 'native_admission': 'UNKNOWN',
              'authorization': 'UNKNOWN', 'benefit': 'UNKNOWN',
              'manifest_sha256': digest, 'errors': check.errors,
              'missing_inputs': sorted(check.missing)}
    return report


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('manifest', type=Path)
    args = parser.parse_args()
    report = validate(args.manifest)
    print(json.dumps(report, indent=2, allow_nan=False))
    raise SystemExit(2 if report['errors'] else 0 if report['offline_preparation_ready'] else 1)
