"""Offline controller tests; the fake upstream is never a model-quality result."""
import base64
import copy
import importlib.util
import json
import pathlib
import unittest
import http.client
import http.server
import threading
import hashlib
import io
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location('subscription_gateway', ROOT / 'subscription_gateway.py')
gateway = importlib.util.module_from_spec(SPEC) if SPEC else None
if SPEC and pathlib.Path(SPEC.origin).exists(): SPEC.loader.exec_module(gateway)


def body():
    r = json.loads(pathlib.Path(str(ROOT/'relay-controller-native-request.json')).read_text())
    return r


def completion(identifier='resp_1', input_tokens=3000, cached=1000, output=1000):
    r = {'type': 'response.completed', 'response': {'id': identifier, 'model': 'gpt-6-luna',
         'status': 'completed', 'output': [], 'usage': {'input_tokens': input_tokens,
         'input_tokens_details': {'cached_tokens': cached}, 'output_tokens': output,
         'output_tokens_details': {'reasoning_tokens': 100}, 'total_tokens': input_tokens + output}}}
    return b'event: response.completed\ndata: ' + json.dumps(r).encode() + b'\n\n'


class GatewayTests(unittest.TestCase):
    def setUp(self):
        self.assertTrue(callable(getattr(gateway, 'Gateway', None)), 'Missing bounded subscription gateway')

    def test_cumulative_usage_cache_and_unmodified_sse(self):
        sent = []
        def send(raw, remaining):
            sent.append(raw)
            wire = completion(f'resp_{len(sent)}')
            return 200, 'text/event-stream', iter((wire[:35], wire[35:]))
        g = gateway.Gateway({'model': 'gpt-6-luna', 'effort': 'medium'}, send)
        r = body()
        before = copy.deepcopy(r)
        for n in (1, 2):
            rows = list(g.handle({'schema_version': 1, 'id': n, 'body': r}))
            wire = b''.join(base64.b64decode(x['data']) for x in rows if x['kind'] == 'chunk')
            self.assertEqual(wire, completion(f'resp_{n}'))
            self.assertTrue(rows[-1]['complete'])
        self.assertEqual(r, before)
        self.assertEqual(rows[-1]['cumulative_usage']['input_tokens'], 6000)
        self.assertEqual(rows[-1]['cumulative_usage']['cached_input_tokens'], 2000)
        self.assertEqual(rows[-1]['cumulative_usage']['output_tokens'], 2000)
        self.assertEqual(rows[-1]['cumulative_equivalent_microcredits'], 35500)
        self.assertEqual(rows[-1]['stop_reason'], 'observed_token_limit')
        self.assertEqual(len(list(g.handle({'schema_version': 1, 'id': 3, 'body': r}))), 1)
        self.assertEqual(len(sent), 2)

    def test_wrong_identity_tier_url_headers_and_duplicate_id_never_send(self):
        for mutation in ('model', 'effort', 'tier', 'url', 'headers', 'duplicate_id',
                         'body_background', 'body_url', 'body_api_key'):
            with self.subTest(mutation=mutation):
                calls = []
                def send(raw, remaining):
                    calls.append(raw)
                    return 200, 'text/event-stream', iter((completion(input_tokens=100, cached=0, output=100),))
                g = gateway.Gateway({'model': 'gpt-6-luna', 'effort': 'medium'}, send)
                r = body()
                frame = {'schema_version': 1, 'id': 1, 'body': r}
                if mutation == 'model': r['model'] = 'gpt-6-astra'
                if mutation == 'effort': r['reasoning']['effort'] = 'xhigh'
                if mutation == 'tier': r['service_tier'] = 'priority'
                if mutation == 'url': frame['url'] = 'https://api.openai.com/v1/responses'
                if mutation == 'headers': frame['headers'] = {'Authorization': 'bad'}
                if mutation == 'body_background': r['background'] = True
                if mutation == 'body_url': r['url'] = 'https://api.openai.com/v1/responses'
                if mutation == 'body_api_key': r['api_key'] = 'synthetic-key'
                if mutation == 'duplicate_id':
                    list(g.handle(frame))
                    calls.clear()
                rows = list(g.handle(frame))
                self.assertFalse(rows[-1]['complete'])
                self.assertEqual(calls, [])

    def test_unknown_usage_zero_eof_failure_and_duplicate_completion_stop(self):
        cases = [b'', b'event: response.failed\ndata: {"type":"response.failed"}\n\n',
                 completion(input_tokens=0, cached=0, output=0), completion() + completion()]
        missing = completion().replace(b'"usage": {', b'"unrecognized_usage": {')
        cases.append(missing)
        for wire in cases:
            with self.subTest(wire=wire[:40]):
                calls = []
                def send(raw, remaining):
                    calls.append(raw)
                    return 200, 'text/event-stream', iter((wire,))
                g = gateway.Gateway({'model': 'gpt-6-luna', 'effort': 'medium'}, send)
                rows = list(g.handle({'schema_version': 1, 'id': 1, 'body': body()}))
                self.assertFalse(rows[-1]['complete'])
                self.assertIsNone(rows[-1]['usage'])
                list(g.handle({'schema_version': 1, 'id': 2, 'body': body()}))
                self.assertEqual(len(calls), 1)

    def test_upstream_errors_are_not_retried_or_leaked(self):
        calls = []
        def send(raw, remaining):
            calls.append(raw)
            return 401, 'application/json', iter((b'private-provider-error',))
        g = gateway.Gateway({'model': 'gpt-6-luna', 'effort': 'medium'}, send)
        rows = list(g.handle({'schema_version': 1, 'id': 1, 'body': body()}))
        self.assertEqual(len(calls), 1)
        self.assertNotIn('private-provider-error', json.dumps(rows))
        self.assertFalse(rows[-1]['complete'])

    def test_oversize_stream_and_expired_trial_stop_without_next_call(self):
        now = [0]
        calls = []
        def send(raw, remaining):
            calls.append(raw)
            return 200, 'text/event-stream', iter((b'x' * (8 * 1024 * 1024 + 1),))
        g = gateway.Gateway({'model': 'gpt-6-luna', 'effort': 'medium'}, send, clock=lambda: now[0])
        rows = list(g.handle({'schema_version': 1, 'id': 1, 'body': body()}))
        self.assertFalse(rows[-1]['complete'])
        g = gateway.Gateway({'model': 'gpt-6-luna', 'effort': 'medium'}, send, clock=lambda: now[0])
        now[0] = 61
        calls.clear()
        rows = list(g.handle({'schema_version': 1, 'id': 1, 'body': body()}))
        self.assertFalse(rows[-1]['complete'])
        self.assertEqual(calls, [])

    def test_invalid_usage_is_unknown(self):
        source = json.loads(completion().split(b'data: ', 1)[1])['response']
        for name in ('cache_exceeds_input', 'bool_count', 'wrong_total', 'missing_cache', 'reasoning_exceeds_output'):
            with self.subTest(name=name):
                value = copy.deepcopy(source)
                if name == 'cache_exceeds_input': value['usage']['input_tokens_details']['cached_tokens'] = 4000
                if name == 'bool_count': value['usage']['input_tokens'] = True
                if name == 'wrong_total': value['usage']['total_tokens'] = 999
                if name == 'missing_cache': value['usage'].pop('input_tokens_details')
                if name == 'reasoning_exceeds_output': value['usage']['output_tokens_details']['reasoning_tokens'] = 1001
                with self.assertRaises(ValueError): gateway.observed_usage(value, 'gpt-6-luna')

    def test_real_http_transport_pins_destination_and_keeps_auth_out_of_output(self):
        self.assertTrue(callable(getattr(gateway, 'subscription_sender', None)), 'Missing subscription HTTP transport')
        received = []
        wire = completion()
        class Handler(http.server.BaseHTTPRequestHandler):
            def do_POST(self):
                received.append((self.path, dict(self.headers), self.rfile.read(int(self.headers['Content-Length']))))
                self.send_response(200)
                self.send_header('Content-Type', 'text/event-stream')
                self.send_header('Content-Length', str(len(wire)))
                self.end_headers()
                self.wfile.write(wire)
            def log_message(self, *_): pass
        server = http.server.HTTPServer(('127.0.0.1', 0), Handler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            def connection(host, timeout):
                self.assertEqual(host, 'chatgpt.com')
                return http.client.HTTPConnection('127.0.0.1', server.server_port, timeout=timeout)
            send = gateway.subscription_sender(lambda: {'Authorization': 'Bearer synthetic-secret',
                'ChatGPT-Account-ID': 'synthetic-account'}, connection_factory=connection)
            g = gateway.Gateway({'model': 'gpt-6-luna', 'effort': 'medium'}, send)
            rows = list(g.handle({'schema_version': 1, 'id': 1, 'body': body()}))
            self.assertTrue(rows[-1]['complete'])
            self.assertEqual(len(received), 1)
            self.assertEqual(received[0][0], '/backend-api/codex/responses')
            self.assertEqual(received[0][1]['Authorization'], 'Bearer synthetic-secret')
            self.assertNotIn('synthetic-secret', json.dumps(rows))
            self.assertNotIn('synthetic-account', json.dumps(rows))
        finally:
            server.shutdown()
            server.server_close()
            thread.join(2)

    def test_auth_validation_refuses_api_other_account_and_expired_credentials(self):
        self.assertTrue(callable(getattr(gateway, 'validate_auth', None)), 'Missing subscription-only auth validation')
        token = 'header.' + base64.urlsafe_b64encode(b'{"exp":10000}').decode().rstrip('=') + '.signature'
        raw = {'auth_mode': 'chatgpt', 'OPENAI_API_KEY': None,
               'tokens': {'account_id': 'synthetic-account', 'access_token': token}}
        expected = hashlib.sha256(b'synthetic-account').hexdigest()
        headers = gateway.validate_auth(raw, expected, now=0)
        self.assertEqual(headers['Authorization'], 'Bearer ' + token)
        for change in ('mode', 'api_key', 'account', 'expired', 'header_injection'):
            with self.subTest(change=change):
                value = copy.deepcopy(raw)
                now = 0
                if change == 'mode': value['auth_mode'] = 'apikey'
                if change == 'api_key': value['OPENAI_API_KEY'] = 'synthetic-api-key'
                if change == 'account': value['tokens']['account_id'] = 'other-account'
                if change == 'expired': now = 10001
                if change == 'header_injection': value['tokens']['access_token'] += '\r\nBad: value'
                with self.assertRaises(ValueError): gateway.validate_auth(value, expected, now=now)

    def test_pipe_protocol_empty_eof_does_not_send_and_duplicate_json_stops(self):
        self.assertTrue(callable(getattr(gateway, 'drive', None)), 'Missing bounded JSONL pipe driver')
        calls = []
        def send(raw, remaining):
            calls.append(raw)
            return 200, 'text/event-stream', iter((completion(),))
        g = gateway.Gateway({'model': 'gpt-6-luna', 'effort': 'medium'}, send)
        output = io.BytesIO()
        self.assertEqual(gateway.drive(io.BytesIO(), output, lambda: g), 0)
        self.assertEqual(output.getvalue(), b'')
        self.assertEqual(calls, [])
        source = io.BytesIO(b'{"schema_version":1,"id":1,"id":2,"body":{}}\n')
        self.assertEqual(gateway.drive(source, output, lambda: g), 1)
        self.assertEqual(calls, [])
        self.assertIsNone(json.loads(output.getvalue())['usage'])

    def test_crlf_stream_is_preserved_and_events_after_terminal_are_refused(self):
        wire = completion().replace(b'\n', b'\r\n')
        g = gateway.Gateway({'model': 'gpt-6-luna', 'effort': 'medium'},
                            lambda raw, remaining: (200, 'text/event-stream', iter((wire,))))
        rows = list(g.handle({'schema_version': 1, 'id': 1, 'body': body()}))
        self.assertTrue(rows[-1]['complete'])
        self.assertEqual(b''.join(base64.b64decode(r['data']) for r in rows if r['kind'] == 'chunk'), wire)
        wire = completion() + b'event: response.output_text.delta\ndata: {"type":"response.output_text.delta","delta":"late"}\n\n'
        g = gateway.Gateway({'model': 'gpt-6-luna', 'effort': 'medium'},
                            lambda raw, remaining: (200, 'text/event-stream', iter((wire,))))
        rows = list(g.handle({'schema_version': 1, 'id': 1, 'body': body()}))
        self.assertFalse(rows[-1]['complete'])

    def test_historical_cli_refuses_synthetic_intent_before_transport(self):
        result = subprocess.run([sys.executable, '-B', str(ROOT / 'subscription_gateway.py'),
                                 '--assignment', 't02-Lx-r1-H0'], input=b'',
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=5)
        # The portable capsule deliberately excludes the historical launch seal.
        # The frozen standalone CLI must refuse it; approved v2 is separate.
        self.assertEqual(result.returncode, 1)
        self.assertEqual(result.stdout, b'')
        self.assertEqual(result.stderr, b'subscription relay refused before completion\n')


if __name__ == '__main__': unittest.main()
