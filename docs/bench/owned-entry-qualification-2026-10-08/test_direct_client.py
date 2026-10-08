"""Scripted native peer fixtures; no native/provider execution in this suite."""
import copy
import hashlib
import json
import math
from pathlib import Path
import sys
import unittest
sys.path.insert(0, str(Path(__file__).parent))
import direct_client as d

# Complete native 0.159.2 thread/start response from the first controlled allow
# attempt; only disposable workspace paths and native identities are redacted.
NATIVE_THREAD_START = {'id': 2,
 'result': {'activePermissionProfile': None,
            'approvalPolicy': 'on-request',
            'approvalsReviewer': 'user',
            'cwd': '/private/tmp/fixture-workspace',
            'disabledPluginIds': [],
            'instructionSources': [],
            'model': 'gpt-6.1-sol',
            'modelProvider': 'local_fixture',
            'multiAgentMode': 'explicitRequestOnly',
            'reasoningEffort': 'low',
            'runtimeWorkspaceRoots': ['/private/tmp/fixture-workspace'],
            'sandbox': {'networkAccess': False, 'type': 'readOnly'},
            'serviceTier': None,
            'thread': {'agentNickname': None,
                       'agentRole': None,
                       'canAcceptDirectInput': True,
                       'cliVersion': '0.159.2',
                       'createdAt': 1791435178,
                       'cwd': '/private/tmp/fixture-workspace',
                       'daybreakEnabled': None,
                       'environments': [{'cwd': '/private/tmp/fixture-workspace',
                                         'environmentId': 'local',
                                         'runtimeWorkspaceRoots': ['/private/tmp/fixture-workspace']}],
                       'ephemeral': True,
                       'extra': None,
                       'forkedFromId': None,
                       'gitInfo': None,
                       'historyMode': 'paginated',
                       'id': 'thread_fixture',
                       'model': 'gpt-6.1-sol',
                       'modelProvider': 'local_fixture',
                       'name': None,
                       'originator': 'ar-f01r-diagnostic',
                       'parentThreadId': None,
                       'path': None,
                       'preview': '',
                       'projectId': None,
                       'reasoningEffort': 'low',
                       'recencyAt': 1791435178,
                       'section': None,
                       'sectionEnteredAt': None,
                       'sessionId': 'thread_fixture',
                       'source': 'vscode',
                       'status': {'type': 'idle'},
                       'threadSource': None,
                       'turns': [],
                       'updatedAt': 1791435178}}}

class Peer:
    def __init__(self, rows):
        self.rows, self.sent = list(rows), []
    def send(self, row):
        self.sent.append(copy.deepcopy(row))
    def receive(self, deadline):
        if not self.rows:
            raise TimeoutError('scripted interruption is ambiguous')
        row = self.rows.pop(0)
        return row if isinstance(row, bytes) else json.dumps(row).encode()

def opening(model='gpt-6.1-sol', effort='low'):
    start = copy.deepcopy(NATIVE_THREAD_START)
    start['result']['model'], start['result']['reasoningEffort'] = model, effort
    start['result']['thread']['model'], start['result']['thread']['reasoningEffort'] = model, effort
    return [{'id': 1, 'result': {'userAgent': 'fixture'}}, start,
            {'id': 3, 'result': {'turn': {'id': 'turn_fixture', 'status': 'inProgress'}}}]


def complete(status='completed', tid='turn_fixture'):
    return {'method': 'turn/completed', 'params': {'threadId': 'thread_fixture',
        'turn': {'id': tid, 'status': status}}}

class ClientTests(unittest.TestCase):
    def client(self, rows):
        peer = Peer(rows)
        return d.Client(peer, '/private/tmp/fixture-workspace'), peer

    def test_unadmitted_selection_refused_before_any_send(self):
        for model, effort in [('unadmitted', 'low'), ('gpt-6.1-sol', 'ultra')]:
            with self.subTest(model=model, effort=effort):
                client, peer = self.client([])
                with self.assertRaises(d.Refused):
                    client.execute('synthetic task', model, effort)
                self.assertEqual(peer.sent, [])

    def test_observed_native_readonly_network_false_is_admitted(self):
        client, peer = self.client(opening() + [complete()])
        try:
            evidence = client.execute('synthetic task', 'gpt-6.1-sol', 'low')
        except d.Refused as error:
            self.fail('observed safe native permission policy was refused: '+str(error))
        self.assertEqual(evidence['terminal'], 'completed')
        self.assertEqual(sum(row.get('method') == 'turn/start' for row in peer.sent), 1)

    def test_task_thread_turn_and_fixed_permission_request(self):
        client, peer = self.client(opening() + [complete()])
        evidence = client.execute('synthetic task', 'gpt-6.1-sol', 'low')
        self.assertEqual(evidence['task_sha256'], hashlib.sha256(b'synthetic task').hexdigest())
        self.assertEqual((evidence['thread_id'], evidence['turn_id'], evidence['terminal']),
                         ('thread_fixture', 'turn_fixture', 'completed'))
        thread = peer.sent[2]['params']
        self.assertEqual((thread['approvalPolicy'], thread['approvalsReviewer'], thread['sandbox']),
                         ('on-request', 'user', 'read-only'))
        self.assertEqual(peer.sent[3]['params'], {'threadId': 'thread_fixture',
            'input': [{'type': 'text', 'text': 'synthetic task'}], 'model': 'gpt-6.1-sol', 'effort': 'low'})

    def test_network_access_true_missing_malformed_or_unknown_permission_refused(self):
        policies = [{'type': 'readOnly', 'networkAccess': True}, {'type': 'readOnly'},
                    {'type': 'readOnly', 'networkAccess': None},
                    {'type': 'readOnly', 'networkAccess': 0},
                    {'type': 'readOnly', 'networkAccess': 'false'},
                    {'type': 'workspaceWrite', 'networkAccess': False},
                    {'type': 'readOnly', 'networkAccess': False, 'unknownGrant': True},
                    [], 'readOnly']
        for policy in policies:
            with self.subTest(policy=policy):
                rows = opening()
                rows[1]['result']['sandbox'] = policy
                client, peer = self.client(rows)
                with self.assertRaises(d.Refused):
                    client.execute('synthetic task', 'gpt-6.1-sol', 'low')
                self.assertFalse(any(x.get('method') == 'turn/start' for x in peer.sent))
                self.assertEqual(client.evidence['terminal'], 'UNKNOWN')

    def test_rerouted_thread_choice_refused_before_turn(self):
        for model, effort in [('wrong-model', 'low'), ('gpt-6.1-sol', 'high')]:
            with self.subTest(model=model, effort=effort):
                client, peer = self.client(opening(model, effort))
                with self.assertRaises(d.Refused):
                    client.execute('synthetic task', 'gpt-6.1-sol', 'low')
                self.assertFalse(any(x.get('method') == 'turn/start' for x in peer.sent))

    def test_native_reroute_notification_is_durable_refusal(self):
        notification = {'method': 'model/rerouted', 'params': {'threadId': 'thread_fixture',
            'turnId': 'turn_fixture', 'fromModel': 'gpt-6.1-sol',
            'toModel': 'unadmitted-model', 'reason': 'highRiskCyberActivity'}}
        client, peer = self.client(opening()+[notification, complete()])
        with self.assertRaises(d.Refused):
            client.execute('synthetic task', 'gpt-6.1-sol', 'low')
        self.assertEqual(client.evidence['received'][-1], notification)
        self.assertEqual(client.evidence['terminal'], 'UNKNOWN')
        self.assertEqual(sum(row.get('method') == 'turn/start' for row in peer.sent), 1)
        with self.assertRaises(d.Refused):
            client.observe(json.dumps(complete()).encode())
        self.assertEqual(client.evidence['terminal'], 'UNKNOWN')

    def test_nested_native_thread_selection_contradiction_is_refused(self):
        for key, wrong in [('model', 'unadmitted-model'), ('reasoningEffort', 'high'),
                           ('modelProvider', 'unadmitted-provider')]:
            with self.subTest(key=key):
                rows = opening()
                rows[1]['result']['thread'][key] = wrong
                client, peer = self.client(rows)
                with self.assertRaises(d.Refused):
                    client.execute('synthetic task', 'gpt-6.1-sol', 'low')
                self.assertFalse(any(row.get('method') == 'turn/start' for row in peer.sent))
                self.assertEqual(client.evidence['terminal'], 'UNKNOWN')

    def test_optional_nested_selection_omitted_preserves_top_level_binding(self):
        rows = opening()
        for key in ('model', 'reasoningEffort', 'modelProvider'):
            del rows[1]['result']['thread'][key]
        client, _ = self.client(rows+[complete()])
        self.assertEqual(client.execute('synthetic task', 'gpt-6.1-sol', 'low')['terminal'], 'completed')

    def test_returned_workspace_and_ephemeral_contradictions_refused(self):
        mutations = [lambda r: r.update(cwd='/private/tmp/other-workspace'),
                     lambda r: r.update(runtimeWorkspaceRoots=['/private/tmp/other-workspace']),
                     lambda r: r.update(runtimeWorkspaceRoots=['/private/tmp/fixture-workspace', '/private/tmp/other-workspace']),
                     lambda r: r['thread'].update(ephemeral=False),
                     lambda r: r['thread'].update(ephemeral=1),
                     lambda r: r['thread'].update(cwd='/private/tmp/other-workspace'),
                     lambda r: r.pop('cwd'), lambda r: r.pop('runtimeWorkspaceRoots')]
        for mutate in mutations:
            with self.subTest(mutate=mutate):
                rows = opening(); mutate(rows[1]['result'])
                client, peer = self.client(rows)
                with self.assertRaises(d.Refused):
                    client.execute('synthetic task', 'gpt-6.1-sol', 'low')
                self.assertFalse(any(row.get('method') == 'turn/start' for row in peer.sent))

    def test_permission_callback_receives_no_authority(self):
        callback = {'id': 71, 'method': 'item/commandExecution/requestApproval',
                    'params': {'threadId': 'thread_fixture', 'turnId': 'turn_fixture'}}
        client, peer = self.client(opening() + [callback])
        with self.assertRaises(d.Refused):
            client.execute('synthetic task', 'gpt-6.1-sol', 'low')
        self.assertEqual(peer.sent[-1], {'id': 71, 'error': {'code': -32601,
            'message': 'diagnostic callback refused'}})
        self.assertEqual(client.evidence['terminal'], 'UNKNOWN')

    def test_wrong_duplicate_and_boolean_rpc_ids_fail_closed(self):
        for rows in [[{'id': 99, 'result': {}}], [{'id': True, 'result': {}}], opening()[:1] + opening()]:
            with self.subTest(rows=rows):
                client, peer = self.client(rows)
                with self.assertRaises(d.Refused):
                    client.execute('synthetic task', 'gpt-6.1-sol', 'low')
                self.assertFalse(any(x.get('method') == 'turn/start' for x in peer.sent))

    def test_malformed_duplicate_keys_nonfinite_and_conflicting_envelopes(self):
        for raw in [b'[]', b'{', b'{"id":1,"id":1,"result":{}}',
                    b'{"id":1,"result":{"v":NaN}}', b'{"id":1,"result":{},"error":{}}']:
            with self.subTest(raw=raw):
                client, _ = self.client([raw])
                with self.assertRaises(d.Refused):
                    client.execute('synthetic task', 'gpt-6.1-sol', 'low')

    def test_exponent_overflow_is_refused_by_raw_parser_and_client(self):
        for numeric in (b'1e999', b'-1e999'):
            raw = b'{"method":"diagnostic/extension","params":{"metric":'+numeric+b'}}'
            with self.subTest(numeric=numeric):
                with self.assertRaises(d.Refused):
                    d.strict_json(raw)
                client, _ = self.client(opening()+[raw, complete()])
                with self.assertRaises(d.Refused):
                    client.execute('synthetic task', 'gpt-6.1-sol', 'low')
                self.assertEqual(client.evidence['terminal'], 'UNKNOWN')
                self.assertFalse(client.evidence['evidence_lost'])
                with self.assertRaises(d.Refused):
                    client.observe(json.dumps(complete()).encode())
                self.assertEqual(client.evidence['terminal'], 'UNKNOWN')

    def test_legitimate_finite_metrics_remain_preserved_and_complete(self):
        values = [(b'1e308', 1e308), (b'-1e308', -1e308), (b'1.25', 1.25),
                  (b'0.0', 0.0), (b'1e-999', 0.0)]
        for numeric, expected in values:
            raw = b'{"method":"diagnostic/extension","params":{"metric":'+numeric+b'}}'
            with self.subTest(numeric=numeric):
                parsed = d.strict_json(raw)
                self.assertTrue(math.isfinite(parsed['params']['metric']))
                self.assertEqual(parsed['params']['metric'], expected)
                client, _ = self.client(opening()+[raw, complete()])
                evidence = client.execute('synthetic task', 'gpt-6.1-sol', 'low')
                self.assertEqual(evidence['terminal'], 'completed')
                self.assertIn(parsed, evidence['received'])

    def test_overflow_records_irreversible_evidence_loss(self):
        client, _ = self.client([b' ' * (d.MAX_LINE + 1)])
        with self.assertRaises(d.Refused):
            client.execute('synthetic task', 'gpt-6.1-sol', 'low')
        self.assertTrue(client.evidence['evidence_lost'])
        with self.assertRaises(d.Refused):
            client.observe(json.dumps(complete()).encode())
        self.assertEqual(client.evidence['terminal'], 'UNKNOWN')

    def test_matching_wait_interrupts_once_needs_ack_and_terminal(self):
        wait = {'method': 'error', 'params': {'threadId': 'thread_fixture',
                'turnId': 'turn_fixture', 'willRetry': True}}
        client, peer = self.client(opening() + [wait, {'id': 4, 'result': {}}, complete('interrupted')])
        evidence = client.execute('synthetic task', 'gpt-6.1-sol', 'low', cancel_on_wait=True)
        self.assertEqual(evidence['terminal'], 'interrupted')
        self.assertEqual([x for x in peer.sent if x.get('method') == 'turn/interrupt'],
             [{'id': 4, 'method': 'turn/interrupt', 'params': {'threadId': 'thread_fixture', 'turnId': 'turn_fixture'}}])
        self.assertTrue(evidence['interrupt_ack'])

    def test_ambiguous_interrupt_retains_unknown_no_resubmit(self):
        wait = {'method': 'error', 'params': {'threadId': 'thread_fixture',
                'turnId': 'turn_fixture', 'willRetry': True}}
        client, peer = self.client(opening() + [wait])
        with self.assertRaises(d.Refused):
            client.execute('synthetic task', 'gpt-6.1-sol', 'low', cancel_on_wait=True)
        self.assertEqual(client.evidence['terminal'], 'UNKNOWN')
        self.assertEqual(sum(x.get('method') == 'turn/start' for x in peer.sent), 1)
        self.assertEqual(sum(x.get('method') == 'turn/interrupt' for x in peer.sent), 1)

    def test_late_success_cannot_finalize_interrupted_turn(self):
        wait = {'method': 'error', 'params': {'threadId': 'thread_fixture',
                'turnId': 'turn_fixture', 'willRetry': True}}
        client, _ = self.client(opening() + [wait, {'id': 4, 'result': {}}, complete('interrupted')])
        client.execute('synthetic task', 'gpt-6.1-sol', 'low', cancel_on_wait=True)
        with self.assertRaises(d.Refused):
            client.observe(json.dumps(complete()).encode())
        self.assertEqual(client.evidence['terminal'], 'UNKNOWN')
        self.assertTrue(client.evidence['late_result'])

    def test_malformed_after_turn_start_is_sticky(self):
        client, _ = self.client(opening() + [b'{'])
        with self.assertRaises(d.Refused):
            client.execute('synthetic task', 'gpt-6.1-sol', 'low')
        with self.assertRaises(d.Refused):
            client.observe(json.dumps(complete()).encode())
        self.assertEqual(client.evidence['terminal'], 'UNKNOWN')

    def test_total_and_message_limits_retain_loss(self):
        for raw, count in [(json.dumps({'method': 'fixture', 'params': {'text': 'x'*100000}}).encode(), 22),
                           (b'{"method":"fixture"}', d.MAX_MESSAGES+1)]:
            with self.subTest(count=count):
                client, _ = self.client([])
                with self.assertRaises(d.Refused):
                    for _ in range(count):
                        client.observe(raw)
                self.assertTrue(client.evidence['evidence_lost'])
                self.assertEqual(client.evidence['terminal'], 'UNKNOWN')

    def test_task_cannot_be_replayed(self):
        client, peer = self.client(opening() + [complete()])
        client.execute('synthetic task', 'gpt-6.1-sol', 'low')
        before = len(peer.sent)
        with self.assertRaises(d.Refused):
            client.execute('synthetic task', 'gpt-6.1-sol', 'low')
        self.assertEqual(len(peer.sent), before)

    def test_wrong_terminal_identity_is_not_completion(self):
        client, _ = self.client(opening() + [complete(tid='different_turn')])
        with self.assertRaises(d.Refused):
            client.execute('synthetic task', 'gpt-6.1-sol', 'low')
        self.assertEqual(client.evidence['terminal'], 'UNKNOWN')

if __name__ == '__main__':
    unittest.main()
