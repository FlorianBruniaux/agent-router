"""Bounded owned-entry diagnostic; no product authority or persistent state."""
from __future__ import annotations
import copy
import hashlib
import json
import math
import time

MODEL = 'gpt-6.1-sol'
EFFORT = 'low'
MAX_LINE = 256 * 1024
MAX_TOTAL = 2 * 1024 * 1024
MAX_MESSAGES = 2048
MAX_TASK = 4096
TIMEOUT = 15

class Refused(ValueError):
    pass

def strict_json(raw):
    def pairs(rows):
        value = {}
        for key, item in rows:
            if key in value:
                raise Refused('duplicate JSON key')
            value[key] = item
        return value
    def finite(value):
        number = float(value)
        if not math.isfinite(number):
            raise Refused('nonfinite JSON number')
        return number
    try:
        return json.loads(raw, object_pairs_hook=pairs, parse_float=finite,
            parse_constant=lambda _: (_ for _ in ()).throw(Refused('nonfinite JSON')))
    except (ValueError, UnicodeError, RecursionError) as error:
        raise Refused('malformed JSON') from error

def identity(value):
    if not isinstance(value, str) or not 0 < len(value) <= 256:
        raise Refused('invalid native identity')
    return value

class Client:
    """One task and one native turn. Transport owns process cleanup separately."""
    def __init__(self, transport, workspace):
        self.transport, self.workspace = transport, str(workspace)
        self.pending, self.replied = set(), set()
        self.total, self.started, self.finished = 0, False, False
        self.evidence = {'task_sha256': None, 'requested': None, 'thread_id': None,
            'turn_id': None, 'terminal': 'UNKNOWN', 'evidence_lost': False,
            'late_result': False, 'interrupt_sent': False, 'interrupt_ack': False,
            'sent': [], 'received': [], 'failure': None, 'callbacks_refused': 0,
            'provider_observation': 'UNKNOWN', 'os_admission': 'UNKNOWN',
            'product_integration': 'UNKNOWN', 'retry_or_refund': False}

    def refuse(self, reason):
        self.evidence['terminal'], self.evidence['failure'] = 'UNKNOWN', reason
        raise Refused(reason)

    def send(self, row):
        if len(self.evidence['sent']) >= MAX_MESSAGES:
            self.evidence['evidence_lost'] = True
            self.refuse('send count limit')
        identifier = row.get('id')
        if 'method' in row and identifier is not None:
            if type(identifier) is not int or identifier in self.pending or identifier in self.replied:
                self.refuse('duplicate request identity')
            self.pending.add(identifier)
        self.evidence['sent'].append({'after_received': len(self.evidence['received']),
                                     'message': copy.deepcopy(row)})
        self.transport.send(row)

    def observe(self, raw):
        if self.evidence['failure'] is not None:
            self.refuse('protocol failure is durable')
        if self.evidence['evidence_lost']:
            self.refuse('evidence already lost')
        if not isinstance(raw, bytes):
            self.refuse('transport must preserve raw messages')
        self.total += len(raw)
        if len(raw) > MAX_LINE or self.total > MAX_TOTAL or len(self.evidence['received']) >= MAX_MESSAGES:
            self.evidence['evidence_lost'] = True
            self.refuse('receive limit')
        try:
            row = strict_json(raw)
        except Refused:
            self.refuse('malformed message')
        if not isinstance(row, dict):
            self.refuse('message must be an object')
        self.evidence['received'].append(row)
        has_id, has_method = 'id' in row, 'method' in row
        if 'jsonrpc' in row and row['jsonrpc'] != '2.0':
            self.refuse('invalid JSON-RPC version')
        if has_method:
            if not isinstance(row['method'], str) or not row['method'] or not isinstance(row.get('params', {}), dict) or 'result' in row or 'error' in row:
                self.refuse('invalid notification envelope')
            if has_id:
                if type(row['id']) not in (int, str) or isinstance(row['id'], str) and not 0 < len(row['id']) <= 256:
                    self.refuse('invalid callback ID')
                self.send({'id': row['id'], 'error': {'code': -32601, 'message': 'diagnostic callback refused'}})
                self.evidence['callbacks_refused'] += 1
                self.refuse('native callback refused')
            params = row.get('params', {})
            if row['method'] == 'model/rerouted':
                self.refuse('native model reroute refused')
            if row['method'] == 'turn/completed':
                if self.finished:
                    self.evidence['late_result'] = True
                    self.refuse('late terminal result')
                turn = params.get('turn')
                if not isinstance(turn, dict) or params.get('threadId') != self.evidence['thread_id'] or turn.get('id') != self.evidence['turn_id'] or self.evidence['turn_id'] is None:
                    self.refuse('terminal identity mismatch')
                status = turn.get('status')
                if status not in ('completed', 'failed', 'interrupted'):
                    self.refuse('unsupported terminal status')
                if self.evidence['interrupt_sent'] and (status != 'interrupted' or not self.evidence['interrupt_ack']):
                    self.refuse('interrupt has no acknowledged terminal')
                self.evidence['terminal'], self.finished = status, True
            elif row['method'] in ('item/started', 'item/completed', 'error', 'turn/started'):
                # Native notifications before a start acknowledgement are preserved;
                # qualification reconstructs exact task/thread/turn order afterward.
                if self.evidence['thread_id'] is not None and params.get('threadId') != self.evidence['thread_id']:
                    self.refuse('notification thread mismatch')
                observed_turn = params.get('turnId', params.get('turn', {}).get('id') if isinstance(params.get('turn'), dict) else None)
                if self.evidence['turn_id'] is not None and observed_turn != self.evidence['turn_id']:
                    self.refuse('notification turn mismatch')
                if self.finished and row['method'] in ('item/started', 'item/completed', 'turn/started'):
                    self.evidence['late_result'] = True
                    self.refuse('late result')
        else:
            if not has_id or type(row['id']) is not int or ('result' in row) == ('error' in row):
                self.refuse('invalid reply envelope')
            if row['id'] not in self.pending or row['id'] in self.replied:
                self.refuse('wrong or duplicate RPC ID')
            self.pending.remove(row['id']); self.replied.add(row['id'])
            if 'error' in row or not isinstance(row['result'], dict):
                self.refuse('native RPC refused')
            if row['id'] == 4:
                self.evidence['interrupt_ack'] = True
        return row

    def receive(self, deadline):
        try:
            return self.observe(self.transport.receive(deadline))
        except (TimeoutError, OSError) as error:
            self.refuse('transport unavailable or interrupt ambiguous')

    def rpc(self, identifier, method, params, deadline):
        self.send({'id': identifier, 'method': method, 'params': params})
        while True:
            row = self.receive(deadline)
            if row.get('id') == identifier and 'method' not in row:
                return row['result']

    def execute(self, task, model, effort, *, cancel_on_wait=False):
        if self.started:
            self.refuse('one task only, replay refused')
        self.started = True
        if (model, effort) != (MODEL, EFFORT):
            self.refuse('selection not admitted in local diagnostic')
        if not isinstance(task, str) or not 0 < len(task.encode()) <= MAX_TASK:
            self.refuse('task limit')
        self.evidence['task_sha256'] = hashlib.sha256(task.encode()).hexdigest()
        self.evidence['requested'] = {'model': model, 'effort': effort}
        deadline = time.monotonic() + TIMEOUT
        self.rpc(1, 'initialize', {'clientInfo': {'name': 'ar-f01r-diagnostic', 'version': '1.0'},
            'capabilities': {'experimentalApi': True}}, deadline)
        self.send({'method': 'initialized'})
        start = self.rpc(2, 'thread/start', {'model': model, 'modelProvider': 'local_fixture',
            'cwd': self.workspace, 'ephemeral': True, 'approvalPolicy': 'on-request',
            'approvalsReviewer': 'user', 'sandbox': 'read-only'}, deadline)
        if (start.get('model'), start.get('reasoningEffort'), start.get('modelProvider')) != (model, effort, 'local_fixture'):
            self.refuse('native thread choice rerouted or unknown')
        thread = start.get('thread')
        if (not isinstance(thread, dict) or any(key in thread and thread[key] != value for key, value in
                (('model', model), ('reasoningEffort', effort), ('modelProvider', 'local_fixture')))):
            self.refuse('native thread metadata contradicts selection')
        if (start.get('cwd') != self.workspace or start.get('runtimeWorkspaceRoots') != [self.workspace]
                or thread.get('ephemeral') is not True
                or 'cwd' in thread and thread['cwd'] != self.workspace):
            self.refuse('native workspace or ephemeral binding changed or unknown')
        sandbox = start.get('sandbox')
        if (not isinstance(sandbox, dict) or sandbox.get('networkAccess') is not False
                or (start.get('approvalPolicy'), start.get('approvalsReviewer'), sandbox)
                != ('on-request', 'user', {'type': 'readOnly', 'networkAccess': False})):
            self.refuse('native permissions changed or unknown')
        try:
            self.evidence['thread_id'] = identity(start.get('thread', {}).get('id'))
        except (Refused, AttributeError):
            self.refuse('invalid thread identity')
        turn = self.rpc(3, 'turn/start', {'threadId': self.evidence['thread_id'],
            'input': [{'type': 'text', 'text': task}], 'model': model, 'effort': effort}, deadline)
        try:
            self.evidence['turn_id'] = identity(turn.get('turn', {}).get('id'))
        except (Refused, AttributeError):
            self.refuse('invalid turn identity')
        while not self.finished:
            row = self.receive(deadline)
            params = row.get('params', {})
            if (cancel_on_wait and not self.evidence['interrupt_sent'] and row.get('method') == 'error'
                    and params.get('willRetry') is True and params.get('threadId') == self.evidence['thread_id']
                    and params.get('turnId') == self.evidence['turn_id']):
                self.evidence['interrupt_sent'] = True
                self.send({'id': 4, 'method': 'turn/interrupt', 'params': {
                    'threadId': self.evidence['thread_id'], 'turnId': self.evidence['turn_id']}})
        return self.evidence
