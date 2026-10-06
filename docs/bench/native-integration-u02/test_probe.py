"""Offline checks for the U02 evidence projection and fail-closed verdict."""
import copy
import http.client
from pathlib import Path
from unittest.mock import patch
import unittest
import probe


class EvidenceTests(unittest.TestCase):
    def case(self):
        return {'enabled': True, 'thread_id': 'thread', 'turn_id': 'turn',
                'provider_requests': [{'model': probe.SELECTED_MODEL, 'effort': 'high',
                    'function_tools': [probe.SENTINEL_TOOL]}],
                'events': [{'method':'turn/started','thread_id':'thread','turn_id':'turn'},
                    {'method':'turn/completed','thread_id':'thread','turn_id':'turn','status':'completed'}],
                'output_correlated':True,'invalid_effort_rejected_before_provider':True,
                'invalid_effort_error_code':-32600,
                'provider_request_count':1,'provider_accepted_connections':1,
                'cleanup_complete':True,'provider_rejected_connections':0,'provider_rejected_requests':0}

    def test_positive_contract(self):
        self.assertTrue(probe.verdict(self.case()))

    def test_missing_canary_cannot_be_green(self):
        case = self.case()
        case['provider_requests'][0]['function_tools'] = []
        self.assertFalse(probe.verdict(case))

    def test_cross_turn_completion_cannot_be_green(self):
        case = self.case()
        case['events'][1]['turn_id'] = 'other-turn'
        self.assertFalse(probe.verdict(case))

    def test_extra_physical_request_cannot_be_green(self):
        case = self.case()
        case['provider_requests'].append(copy.deepcopy(case['provider_requests'][0]))
        self.assertFalse(probe.verdict(case))

    def test_cleanup_and_negative_control_required(self):
        for field in ('cleanup_complete','invalid_effort_rejected_before_provider','output_correlated'):
            with self.subTest(field=field):
                case = self.case(); case[field] = False
                self.assertFalse(probe.verdict(case))

    def test_wrong_model_or_effort_cannot_be_green(self):
        for field in ('model','effort'):
            with self.subTest(field=field):
                case = self.case(); case['provider_requests'][0][field] = 'wrong'
                self.assertFalse(probe.verdict(case))

    def test_disabled_means_baseline_and_no_canary(self):
        case = self.case(); case['enabled'] = False
        case['provider_requests'] = [{'model':probe.BASE_MODEL,'effort':'low','function_tools':[]}]
        self.assertTrue(probe.verdict(case))
        case['provider_requests'][0]['function_tools'] = [probe.SENTINEL_TOOL]
        self.assertFalse(probe.verdict(case))

    def test_projection_omits_prompt_and_key_and_reads_namespaces(self):
        actual = probe.request_projection({'model':'m','reasoning':{'effort':'high'},
            'input':'private prompt','api_key':'private key','tools':[
                {'type':'namespace','name':'n','tools':[{'type':'function','name':'f','description':'private'}]}]})
        self.assertEqual(actual, {'model':'m','effort':'high','function_tools':['f'],'tool_types':['namespace']})


class ProviderContactTests(unittest.TestCase):
    """Real loopback HTTP handler, scripted RPC peer; never runs a native binary."""

    def run_case(self, *, negative=None, extra=None, error_code=-32600):
        helpers = probe.load_helpers(Path(probe.__file__).resolve().parent.parent /
            'egress-admission' / 'codex_appserver_local.py')
        original_provider = helpers.LocalProvider
        providers = []

        def provider_factory():
            provider = original_provider()
            providers.append(provider)
            return provider

        def contact(kind):
            connection = http.client.HTTPConnection('127.0.0.1',
                providers[0].server_address[1], timeout=3)
            try:
                if kind == 'connection-only':
                    with providers[0].lock:
                        before = providers[0].accepted_connections
                    connection.connect()
                    connection.close()
                    deadline = probe.time.monotonic() + 3
                    while probe.time.monotonic() < deadline:
                        with providers[0].lock:
                            if providers[0].accepted_connections > before:
                                return
                        probe.time.sleep(0.001)
                    self.fail('loopback connection was not observed')
                body = probe.json.dumps({'model': probe.SELECTED_MODEL,
                    'reasoning': {'effort': 'high'},
                    'tools': [{'type': 'function', 'name': probe.SENTINEL_TOOL}]})
                path, method, headers = '/v1/responses', 'POST', {}
                expected = 200
                if kind == 'malformed':
                    body, expected = '{', 400
                elif kind == 'wrong-path':
                    path, expected = '/unexpected', 404
                elif kind == 'oversized':
                    body, expected = '', 413
                    headers['Content-Length'] = str(helpers.MAX_REQUEST_BYTES + 1)
                elif kind == 'wrong-method':
                    method, expected = 'GET', 501
                elif kind == 'retry':
                    expected = 429
                connection.request(method, path, body, headers)
                response = connection.getresponse()
                self.assertEqual(response.status, expected)
                response.read()
            finally:
                connection.close()

        class ScriptedAppServer:
            def __init__(self, process):
                self.messages = []
                self.sent = []
                clients.append(self)

            def send(self, message):
                self.sent.append(message)
                request_id = message.get('id')
                if request_id == 1:
                    self.messages.append({'id': 1, 'result': {}})
                elif request_id == 2:
                    self.messages.append({'id': 2, 'result': {'thread': {'id': 'thread'}}})
                elif request_id == 3:
                    if negative:
                        contact(negative)
                    self.messages.append({'id': 3, 'error': {'code': error_code}})
                elif request_id == 4:
                    contact('valid')
                    if extra:
                        contact(extra)
                    self.messages.extend([
                        {'id': 4, 'result': {'turn': {'id': 'turn'}}},
                        {'method': 'turn/started', 'params': {'threadId': 'thread',
                            'turn': {'id': 'turn'}}},
                        {'method': 'item/completed', 'params': {'threadId': 'thread',
                            'turnId': 'turn', 'item': {'type': 'agentMessage', 'text': probe.OUTPUT}}},
                        {'method': 'turn/completed', 'params': {'threadId': 'thread',
                            'turn': {'id': 'turn', 'status': 'completed'}}},
                    ])

            def receive(self, deadline):
                return self.messages.pop(0)

            def stop(self):
                return 0

        clients = []
        with patch.object(helpers, 'LocalProvider', provider_factory), \
                patch.object(helpers, 'AppServer', ScriptedAppServer), \
                patch.object(probe.subprocess, 'Popen', return_value=object()):
            case = probe.run_case(helpers, Path('/unused-native-binary'), True, 5)
        self.assertTrue(clients, case)
        return case, clients[0].sent

    def test_one_valid_request_is_accepted(self):
        case, _ = self.run_case()
        self.assertTrue(case['success'], case)

    def test_refused_contact_during_negative_control_stops_before_valid_turn(self):
        for kind in ('malformed', 'wrong-path', 'oversized', 'wrong-method'):
            with self.subTest(kind=kind):
                case, sent = self.run_case(negative=kind)
                self.assertFalse(case['success'], case)
                self.assertFalse(case['invalid_effort_rejected_before_provider'])
                self.assertFalse(any(message.get('id') == 4 for message in sent))
                self.assertEqual(case['provider_rejected_requests'], 1)

    def test_wrong_rpc_error_stops_before_valid_turn(self):
        case, sent = self.run_case(error_code=-32000)
        self.assertFalse(case['success'], case)
        self.assertFalse(any(message.get('id') == 4 for message in sent))

    def test_connection_without_http_is_still_provider_contact(self):
        case, sent = self.run_case(negative='connection-only')
        self.assertFalse(case['success'], case)
        self.assertFalse(case['invalid_effort_rejected_before_provider'])
        self.assertFalse(any(message.get('id') == 4 for message in sent))
        self.assertEqual(case['provider_accepted_connections'], 1)
        self.assertEqual(case['provider_request_count'], 0)
        case, _ = self.run_case(extra='connection-only')
        self.assertFalse(case['success'], case)
        self.assertEqual(case['provider_accepted_connections'], 2)
        self.assertEqual(case['provider_request_count'], 1)

    def test_extra_refused_http_or_retry_cannot_be_green(self):
        for kind in ('malformed', 'wrong-path', 'oversized', 'wrong-method', 'retry'):
            with self.subTest(kind=kind):
                case, _ = self.run_case(extra=kind)
                self.assertFalse(case['success'], case)
                self.assertEqual(len(case['provider_requests']), 1)
                self.assertEqual(case['provider_rejected_requests'], 1)


if __name__ == '__main__':
    unittest.main()
