"""Qualification assertions and actual loopback broker tests, no native launch."""
import copy
import hashlib
import errno
import http.client
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock
sys.path.insert(0, str(Path(__file__).parent))
import direct_client as d
import native_probe as n
from test_direct_client import Peer, opening, complete

TASK = 'F01R_LOCAL_OWNED_ENTRY_TASK'

def body(model='gpt-6.1-sol', effort='low', task=TASK):
    return {'model': model, 'reasoning': {'effort': effort}, 'input': [
        {'role': 'user', 'content': [{'type': 'input_text', 'text': task}]}]}

def evidence():
    peer = Peer(opening() + [complete()])
    client = d.Client(peer, '/private/tmp/fixture-workspace')
    client.execute(TASK, 'gpt-6.1-sol', 'low')
    raw = json.dumps(body(), separators=(',', ':')).encode()
    digest = hashlib.sha256(raw).hexdigest()
    return {'case': 'allow', 'client': client.evidence, 'native_launch_count': 1,
        'provider_requests': [raw.decode()], 'provider_body_hashes': [digest],
        'broker_body_hashes': [digest], 'broker_http_requests': 1, 'provider_http_requests': 1,
        'reserved': 1, 'decisions': [{'ordinal': 1, 'allowed': True, 'reason': 'allow'}],
        'cleanup_complete': True, 'process_group_gone': True, 'output_complete': True,
        'listeners_closed': True, 'handlers_stopped': True, 'process_exit_code': 0,
        'socket_control': ['allowed', 1], 'pin_unchanged': True,
        'provider_rejected_connections': 0, 'broker_rejected_connections': 0,
        'failure': None, 'group_cleanup': {'process_group_id': 4242, 'group_gone': True,
             'timeout_seconds': 3, 'wait_seconds': .1, 'deadline_reached': False,
             'observations': [{'elapsed_seconds': 0.0, 'state': 'present'},
                              {'elapsed_seconds': .1, 'state': 'absent'}]},
        'listener_checks': [{'listener': 'broker', 'fd_closed': True, 'connect_errno': errno.ECONNREFUSED},
                                          {'listener': 'provider', 'fd_closed': True, 'connect_errno': errno.ECONNREFUSED}]}

class QualificationTests(unittest.TestCase):
    def test_independent_body_and_correlated_transcript_qualify(self):
        self.assertTrue(n.qualify(evidence()))

    def test_native_sandbox_requires_explicit_false_network_access(self):
        for policy in [{'type': 'readOnly'}, {'type': 'readOnly', 'networkAccess': True},
                       {'type': 'readOnly', 'networkAccess': 0},
                       {'type': 'readOnly', 'networkAccess': None},
                       {'type': 'workspaceWrite', 'networkAccess': False},
                       {'type': 'readOnly', 'networkAccess': False, 'unknownGrant': True}]:
            with self.subTest(policy=policy):
                e = evidence()
                e['client']['received'][1]['result']['sandbox'] = policy
                self.assertFalse(n.qualify(e))

    def test_qualifier_refuses_native_reroute_even_with_bound_provider_body(self):
        e = evidence()
        e['client']['received'].insert(-1, {'method': 'model/rerouted', 'params': {
            'threadId': 'thread_fixture', 'turnId': 'turn_fixture', 'fromModel': 'gpt-6.1-sol',
            'toModel': 'unadmitted-model', 'reason': 'highRiskCyberActivity'}})
        self.assertFalse(n.qualify(e))

    def test_qualifier_refuses_predecoded_nonfinite_notification_metrics(self):
        for metric in (float('inf'), float('-inf'), float('nan')):
            with self.subTest(metric=metric):
                e = evidence()
                e['client']['received'].insert(-1, {'method': 'diagnostic/extension',
                                                     'params': {'metric': metric}})
                self.assertFalse(n.qualify(e))

    def test_qualifier_preserves_legitimate_finite_notification_metrics(self):
        for metric in (1e308, -1e308, 1.25, 0.0):
            with self.subTest(metric=metric):
                e = evidence()
                e['client']['received'].insert(-1, {'method': 'diagnostic/extension',
                                                     'params': {'metric': metric}})
                self.assertTrue(n.qualify(e))

    def test_qualifier_refuses_contradictory_nested_thread_selection(self):
        for key, wrong in [('model', 'unadmitted-model'), ('reasoningEffort', 'high'),
                           ('modelProvider', 'unadmitted-provider')]:
            with self.subTest(key=key):
                e = evidence()
                e['client']['received'][1]['result']['thread'][key] = wrong
                self.assertFalse(n.qualify(e))

    def test_qualifier_accepts_omitted_optional_nested_metadata(self):
        e = evidence()
        for key in ('model', 'reasoningEffort', 'modelProvider'):
            del e['client']['received'][1]['result']['thread'][key]
        self.assertTrue(n.qualify(e))

    def test_qualifier_refuses_returned_workspace_or_ephemeral_contradiction(self):
        mutations = [lambda r: r.update(cwd='/private/tmp/other-workspace'),
                     lambda r: r.update(runtimeWorkspaceRoots=['/private/tmp/other-workspace']),
                     lambda r: r['thread'].update(ephemeral=False),
                     lambda r: r['thread'].update(ephemeral=1),
                     lambda r: r['thread'].update(cwd='/private/tmp/other-workspace'),
                     lambda r: r.pop('cwd'), lambda r: r.pop('runtimeWorkspaceRoots')]
        for mutate in mutations:
            with self.subTest(mutate=mutate):
                e = evidence(); mutate(e['client']['received'][1]['result'])
                self.assertFalse(n.qualify(e))

    def test_qualifier_requires_send_before_own_reply_and_terminal(self):
        for index, boundary in [(0, 1), (1, 4), (2, 2), (3, 4)]:
            with self.subTest(index=index, boundary=boundary):
                e = evidence()
                e['client']['sent'][index]['after_received'] = boundary
                self.assertFalse(n.qualify(e))

    def test_qualifier_interrupt_ack_precedes_terminal(self):
        e = evidence()
        wait = {'method': 'error', 'params': {'threadId': 'thread_fixture',
                 'turnId': 'turn_fixture', 'willRetry': True}}
        client = d.Client(Peer(opening()+[wait, {'id': 4, 'result': {}}, complete('interrupted')]),
                          '/private/tmp/fixture-workspace')
        e['client'] = client.execute(TASK, 'gpt-6.1-sol', 'low', cancel_on_wait=True)
        e.update(case='broker_absent_cancel', provider_requests=[], provider_body_hashes=[],
                 broker_body_hashes=[], broker_http_requests=0, provider_http_requests=0,
                 reserved=0, decisions=[])
        self.assertTrue(n.qualify(e))
        rows = e['client']['received']
        rows[-2], rows[-1] = rows[-1], rows[-2]
        self.assertFalse(n.qualify(e))

    def test_classifier_requires_observed_group_absence_inside_bound(self):
        mutations = [lambda g: g.update(group_gone=False),
                     lambda g: g.update(deadline_reached=True),
                     lambda g: g.update(wait_seconds=3.1),
                     lambda g: g.update(observations=[]),
                     lambda g: g['observations'][-1].update(state='present'),
                     lambda g: g['observations'][-1].update(state='permission_denied'),
                     lambda g: g['observations'][-1].update(elapsed_seconds=3.1)]
        for mutate in mutations:
            with self.subTest(mutate=mutate):
                e = evidence(); mutate(e['group_cleanup'])
                self.assertFalse(n.qualify(e))
        e = evidence(); e.pop('group_cleanup')
        self.assertFalse(n.qualify(e))

    def test_observed_model_effort_and_task_contradictions_refused(self):
        for change in [('wrong', 'low', TASK), ('gpt-6.1-sol', 'high', TASK),
                       ('gpt-6.1-sol', 'low', 'different task')]:
            with self.subTest(change=change):
                e = evidence()
                raw = json.dumps(body(*change)).encode()
                e['provider_requests'] = [raw.decode()]
                e['provider_body_hashes'] = [hashlib.sha256(raw).hexdigest()]
                e['broker_body_hashes'] = e['provider_body_hashes'][:]
                self.assertFalse(n.qualify(e))

    def test_contradictory_or_missing_native_evidence_cannot_qualify(self):
        mutations = [lambda e: e.update(reserved=0), lambda e: e.update(process_exit_code=-9),
            lambda e: e.update(output_complete=False), lambda e: e.update(pin_unchanged=False),
            lambda e: e['client'].update(evidence_lost=True), lambda e: e['client'].update(late_result=True),
            lambda e: e['client'].update(task_sha256='0'*64),
            lambda e: e['client']['received'].append(copy.deepcopy(e['client']['received'][0])),
            lambda e: e['client']['sent'][3]['message']['params'].update(effort='high'),
            lambda e: e['client']['received'][-1]['params']['turn'].update(id='wrong'),
            lambda e: e['client']['sent'][2]['message']['params'].update(approvalsReviewer='auto_review'),
            lambda e: e.update(provider_requests=[]), lambda e: e.update(provider_http_requests=True),
            lambda e: e['decisions'][0].update(allowed=False)]
        for mutate in mutations:
            with self.subTest(mutate=mutate):
                e = evidence(); mutate(e)
                self.assertFalse(n.qualify(e))

    def test_projection_redacts_paths_identity_task_and_raw_messages(self):
        e = evidence()
        e['private_secret'] = 'secret-should-not-leak'
        projection = n.public_projection(e)
        encoded = json.dumps(projection)
        for private in ['fixture-workspace', 'thread_fixture', 'turn_fixture', TASK,
                        'private_secret', 'secret-should-not-leak']:
            self.assertNotIn(private, encoded)
        self.assertEqual(projection, n.public_projection(copy.deepcopy(e)))
        self.assertEqual(projection['observed_provider'], {'model': 'gpt-6.1-sol', 'effort': 'low', 'task_digest_matches': True})
        self.assertEqual(projection['os_admission'], 'UNKNOWN')

    def test_binding_validation_rejects_unsupported_selection_and_duplicate_json(self):
        for raw in [json.dumps(body(effort='high')).encode(), json.dumps(body(model='wrong')).encode(),
                    json.dumps(body(task='wrong')).encode(), b'{"model":"gpt-6.1-sol","model":"gpt-6.1-sol"}']:
            self.assertFalse(n.provider_matches(raw, TASK))

    def test_projection_cannot_exfiltrate_untrusted_strings(self):
        e = evidence()
        e['case'] = 'secret-case'
        e['client']['requested'] = {'model': 'secret-model', 'effort': 'secret-effort'}
        e['client']['task_sha256'] = 'secret-digest'
        raw = json.dumps(body(model='secret-model', effort='secret-effort')).encode()
        e['provider_requests'] = [raw.decode()]
        encoded = json.dumps(n.public_projection(e))
        self.assertNotIn('secret', encoded)

    def test_cleanup_flags_do_not_override_contradictory_listener_evidence(self):
        e = evidence()
        e['listener_checks'] = [{'listener': 'broker', 'fd_closed': False, 'connect_errno': 0},
                                {'listener': 'provider', 'fd_closed': True, 'connect_errno': errno.ECONNREFUSED}]
        self.assertFalse(n.qualify(e))

    def test_ambiguous_send_keeps_reservation_without_retry_or_refund(self):
        repo = Path(__file__).resolve().parents[3]
        p = n.load('f01r_ambiguous_broker', repo/'docs/bench/physical-send-feasibility-2026-10-07/probe.py')
        h = p.load('f01r_ambiguous_helper', repo/'docs/bench/egress-admission/codex_appserver_local.py')
        prior = p.load('f01r_ambiguous_prior', repo/'docs/bench/native-integration-u06/probe.py')
        with tempfile.TemporaryDirectory(prefix='ar-f01r-ambiguous-') as scratch:
            provider, pt = p.start_server(h, p.handler(h, prior.final_sse, False), Path(scratch)/'provider')
            provider.retry = False
            port = provider.server_address[1]
            provider.shutdown(); provider.server_close(); pt.join(timeout=3)
            broker, bt = p.start_server(h, n.binding_handler(h, p, prior.final_sse), Path(scratch)/'broker')
            broker.gate, broker.upstream_port = p.Gate(1), port
            try:
                conn = http.client.HTTPConnection('127.0.0.1', broker.server_address[1], timeout=2)
                conn.request('POST', '/v1/responses', json.dumps(body()), {'Content-Type': 'application/json'})
                response = conn.getresponse(); payload = response.read(); conn.close()
                self.assertEqual(response.status, 502)
                self.assertIn(b'reservation retained', payload)
                self.assertEqual(broker.gate.reserved, 1)
                self.assertEqual(len(broker.gate.decisions), 1)
            finally:
                broker.shutdown(); broker.server_close(); bt.join(timeout=3)

    def test_live_loopback_binding_denies_before_provider_socket(self):
        repo = Path(__file__).resolve().parents[3]
        p = n.load('f01r_test_broker', repo/'docs/bench/physical-send-feasibility-2026-10-07/probe.py')
        h = p.load('f01r_test_helper', repo/'docs/bench/egress-admission/codex_appserver_local.py')
        prior = p.load('f01r_test_prior', repo/'docs/bench/native-integration-u06/probe.py')
        with tempfile.TemporaryDirectory(prefix='ar-f01r-http-') as scratch:
            root = Path(scratch)
            provider, pt = p.start_server(h, p.handler(h, prior.final_sse, False), root/'provider')
            provider.retry = False
            broker, bt = p.start_server(h, n.binding_handler(h, p, prior.final_sse), root/'broker')
            broker.gate, broker.upstream_port = p.Gate(1), provider.server_address[1]
            try:
                for effort, status in [('high', 403), ('low', 200)]:
                    conn = http.client.HTTPConnection('127.0.0.1', broker.server_address[1], timeout=2)
                    conn.request('POST', '/v1/responses', json.dumps(body(effort=effort)), {'Content-Type': 'application/json'})
                    response = conn.getresponse(); response.read(); conn.close()
                    self.assertEqual(response.status, status)
                    self.assertEqual(len(provider.bodies), 0 if effort == 'high' else 1)
                self.assertEqual(broker.gate.reserved, 1)
            finally:
                for server, thread in [(broker, bt), (provider, pt)]:
                    server.shutdown(); server.server_close(); thread.join(timeout=3)
                    self.assertFalse(thread.is_alive())
                    self.assertFalse(any(x.is_alive() for x in server.handler_threads))

class GroupCleanupTests(unittest.TestCase):
    def controlled_observation(self, side_effect):
        clock = [0.0]
        sleeps = []
        def advance(duration):
            sleeps.append(duration)
            clock[0] += duration
        with mock.patch.object(n.time, 'monotonic', side_effect=lambda: clock[0]), \
             mock.patch.object(n.time, 'sleep', side_effect=advance), \
             mock.patch.object(n.os, 'killpg', side_effect=side_effect) as probe:
            result = n.observe_group_cleanup(4242)
            calls = probe.call_args_list
        return result, sleeps, calls

    def test_delayed_absence_observed_within_three_seconds(self):
        result, sleeps, calls = self.controlled_observation([None, None, ProcessLookupError()])
        self.assertTrue(result['group_gone'])
        self.assertEqual(result['process_group_id'], 4242)
        self.assertEqual([row['state'] for row in result['observations']], ['present', 'present', 'absent'])
        self.assertGreater(result['wait_seconds'], 0)
        self.assertLessEqual(result['wait_seconds'], 3)
        self.assertFalse(result['deadline_reached'])
        self.assertEqual(calls, [mock.call(4242, 0)]*3)

    def test_persistent_group_never_granted_after_bounded_wait(self):
        result, sleeps, calls = self.controlled_observation(lambda *_: None)
        self.assertFalse(result['group_gone'])
        self.assertTrue(result['deadline_reached'])
        # Python 3.13 compensated sum differs from sequential clock additions.
        self.assertLessEqual(sum(sleeps), 3+1e-12)
        self.assertLessEqual(result['wait_seconds'], 3)
        self.assertGreater(len(calls), 1)
        self.assertLessEqual(len(calls), 64)
        self.assertTrue(all(row['state'] == 'present' for row in result['observations']))

    def test_permission_failure_never_claims_absence_or_retries(self):
        result, sleeps, calls = self.controlled_observation(PermissionError())
        self.assertFalse(result['group_gone'])
        self.assertEqual([row['state'] for row in result['observations']], ['permission_denied'])
        self.assertEqual(sleeps, [])
        self.assertEqual(calls, [mock.call(4242, 0)])

if __name__ == '__main__':
    unittest.main()
