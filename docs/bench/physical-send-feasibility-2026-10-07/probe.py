"""Bounded local broker feasibility; never production authority or paid inference."""
from __future__ import annotations
import argparse
import hashlib
import http.client
import importlib.util
import json
import os
from pathlib import Path
import queue
import socket
import subprocess
import threading
import time

PIN = '16593cc2f422d5f398a8e40f550ebbaf1245392528957be342c295920a300704'
MODEL = 'gpt-6.1-sol'
PROMPT = 'F03A_LOCAL_SEND_DIAGNOSTIC'
LIMIT = 1024 * 1024
CASES = {'allow': (1, 'enabled', False), 'deny_first': (0, 'enabled', False),
         'retry_allow': (2, 'enabled', True), 'retry_deny': (1, 'enabled', True),
         'guard_disabled': (2, 'disabled', False), 'guard_failed': (2, 'failed', False),
         'broker_absent': (2, 'enabled', False)}

class Gate:
    """Reserve before socket creation; ambiguous sends never refund this local unit."""
    def __init__(self, limit, mode='enabled'):
        if type(limit) is not int or not 0 <= limit <= 2 or mode not in ('enabled', 'disabled', 'failed'):
            raise ValueError('invalid diagnostic policy')
        self.limit, self.mode, self.reserved = limit, mode, 0
        self.lock = threading.Lock()
        self.decisions = []

    def admit(self):
        with self.lock:
            try:
                if self.mode == 'failed':
                    raise RuntimeError('injected admission failure')
                accepted = self.mode == 'enabled' and self.reserved < self.limit
                reason = 'allow' if accepted else ('disabled' if self.mode == 'disabled' else 'ceiling')
            except RuntimeError:
                accepted, reason = False, 'guard_error'
            if accepted:
                self.reserved += 1
            self.decisions.append({'ordinal': len(self.decisions) + 1, 'allowed': accepted, 'reason': reason})
            return accepted


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def profile(port):
    if type(port) is not int or not 1 <= port <= 65535:
        raise ValueError('invalid port')
    # Diagnostic TCP restriction only; file/tool isolation is not admitted here.
    return f'(version 1)\n(allow default)\n(deny network*)\n(allow network-outbound (remote ip "localhost:{port}"))\n'


def handler(h, final_sse, broker):
    class Handler(h.ProviderHandler):
        def parse_request(self):
            with self.server.lock:
                self.server.request_count += 1
            return super().parse_request()

        def reply(self, code, payload, content_type='application/json'):
            self.send_response(code)
            self.send_header('Content-Type', content_type)
            self.send_header('Content-Length', str(len(payload)))
            self.send_header('Connection', 'close')
            self.end_headers()
            self.wfile.write(payload)

        def do_POST(self):
            self.close_connection = True
            if self.path != '/v1/responses' or self.headers.get('Transfer-Encoding') or len(self.headers.get_all('Content-Length', [])) != 1:
                self.reply(400, b'{"error":"request refused"}'); return
            try:
                length = int(self.headers['Content-Length'])
                if not 0 < length <= LIMIT: raise ValueError('body limit')
                raw = self.rfile.read(length)
                if len(raw) != length: raise ValueError('short body')
                body = json.loads(raw)
                if body.get('model') != MODEL: raise ValueError('model refused')
            except (ValueError, TypeError, AttributeError):
                self.reply(400, b'{"error":"body refused"}'); return
            with self.server.lock:
                index = len(self.server.bodies) + 1
                self.server.bodies.append(hashlib.sha256(raw).hexdigest())
            (self.server.raw / f'request-{index}.json').write_bytes(raw)
            if not broker:
                if self.server.retry and index == 1:
                    self.reply(503, b'{"error":{"message":"F03A_RETRY","type":"server_error"}}')
                else:
                    self.reply(200, final_sse(MODEL), 'text/event-stream')
                return
            if not self.server.gate.admit():
                self.reply(403, b'{"error":{"message":"F03A_SEND_DENIED","type":"permission_error"}}'); return
            # Sole physical upstream send site. No retry, redirects, proxy or caller URL.
            upstream = http.client.HTTPConnection('127.0.0.1', self.server.upstream_port, timeout=2)
            try:
                upstream.request('POST', '/v1/responses', raw, {'Content-Type': 'application/json'})
                response = upstream.getresponse()
                payload = response.read(LIMIT + 1)
                if len(payload) > LIMIT: raise ValueError('response limit')
                self.reply(response.status, payload, response.getheader('Content-Type', 'application/json'))
            except (OSError, ValueError, http.client.HTTPException):
                self.reply(502, b'{"error":"upstream ambiguous, reservation retained"}')
            finally:
                upstream.close()
    return Handler


def start_server(h, cls, raw):
    raw.mkdir(mode=0o700)
    server = h.LocalProvider()
    server.RequestHandlerClass, server.raw, server.bodies = cls, raw, []
    thread = threading.Thread(target=lambda: server.serve_forever(poll_interval=.05), daemon=True)
    thread.start()
    return server, thread


def run_case(h, prior, binary, name, raw):
    raw.mkdir(mode=0o700)
    home, workspace = raw / 'home', raw / 'workspace'
    for directory in (home, workspace, home / 'tmp', home / 'config', home / 'cache', home / 'data'):
        directory.mkdir(mode=0o700, parents=True, exist_ok=True)
    maximum, mode, retry = CASES[name]
    provider = broker = client = None
    messages, result = [], {'case': name, 'failure': None, 'cleanup_complete': False}
    try:
        provider, pt = start_server(h, handler(h, prior.final_sse, False), raw / 'provider')
        provider.retry = retry
        broker, bt = start_server(h, handler(h, prior.final_sse, True), raw / 'broker')
        broker.gate, broker.upstream_port = Gate(maximum, mode), provider.server_address[1]
        policy = profile(broker.server_address[1])
        (raw / 'controller.sb').write_text(policy)
        # Positive socket control and forbidden direct provider connection in same profile.
        control_script = 'import socket,sys,json; out=[]\nfor p in sys.argv[1:]:\n s=socket.socket(); s.settimeout(1)\n try: s.connect(("127.0.0.1",int(p))); out.append("allowed")\n except OSError as e: out.append(e.errno)\n finally: s.close()\nprint(json.dumps(out))'
        control = prior.bounded_command(['/usr/bin/sandbox-exec', '-p', policy, '/usr/bin/python3', '-c', control_script,
            str(broker.server_address[1]), str(provider.server_address[1])], workspace, h.isolated_env(binary, home), timeout=5)
        result['socket_control'] = json.loads(control)
        if result['socket_control'] != ['allowed', 1]: raise ValueError('socket confinement control failed')
        if name == 'broker_absent':
            broker.shutdown(); broker.server_close(); bt.join(timeout=3)
        (home / 'config.toml').write_text(f'''model = "{MODEL}"
model_provider = "local_fixture"
model_reasoning_effort = "low"
approval_policy = "on-request"
approvals_reviewer = "user"
sandbox_mode = "read-only"
web_search = "disabled"
[features]
shell_tool = false
unified_exec = false
apps = false
[model_providers.local_fixture]
name = "F03a loopback broker"
base_url = "http://127.0.0.1:{broker.server_address[1]}/v1"
wire_api = "responses"
requires_openai_auth = false
supports_websockets = false
request_max_retries = 1
stream_max_retries = 0
''')
        process = subprocess.Popen(['/usr/bin/sandbox-exec', '-p', policy, str(binary), 'app-server', '--listen', 'stdio://'],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            cwd=workspace, env=h.isolated_env(binary, home), start_new_session=True)
        client = h.AppServer(process)
        deadline = time.monotonic() + 15
        def receive():
            message = client.receive(deadline)
            messages.append(message)
            if 'id' in message and 'method' in message:
                raise ValueError('unexpected native callback')
            return message
        def rpc(identifier, method, params):
            client.send({'id': identifier, 'method': method, 'params': params})
            while True:
                message = receive()
                if message.get('id') == identifier:
                    if 'error' in message: raise ValueError('native RPC refused')
                    return message['result']
        rpc(1, 'initialize', {'clientInfo': {'name': 'f03a', 'version': '1.0'}, 'capabilities': {'experimentalApi': True}})
        client.send({'method': 'initialized'})
        start = rpc(2, 'thread/start', {'model': MODEL, 'modelProvider': 'local_fixture', 'cwd': str(workspace),
            'ephemeral': True, 'approvalPolicy': 'on-request', 'approvalsReviewer': 'user', 'sandbox': 'read-only'})
        tid = start['thread']['id']
        turn = rpc(3, 'turn/start', {'threadId': tid, 'input': [{'type': 'text', 'text': PROMPT}], 'model': MODEL, 'effort': 'low'})
        turnid = turn['turn']['id']
        while True:
            message = receive()
            if message.get('method') == 'turn/completed':
                params = message['params']
                if params['threadId'] != tid or params['turn']['id'] != turnid: raise ValueError('turn identity mismatch')
                result['turn_status'] = params['turn']['status']
                result['turn_correlated'] = True
                break
        result['final_output_observed'] = any(m.get('method') == 'item/completed'
            and m.get('params', {}).get('threadId') == tid and m['params'].get('turnId') == turnid
            and m['params'].get('item', {}).get('text') == prior.FINAL_OUTPUT for m in messages)
    except (OSError, ValueError, KeyError, h.FixtureError, prior.FixtureError) as error:
        result['failure'] = type(error).__name__
        (raw / 'error.txt').write_text(str(error))
    finally:
        cleanup = True
        if client:
            try:
                result['process_exit_code'] = client.stop()
                result['process_group_gone'] = prior._group_absent(client.process.pid, 0)
                eof = False
                while True:
                    try: line = client.lines.get_nowait()
                    except queue.Empty: break
                    if line is None: eof = True
                    else: messages.append(json.loads(line))
                result['output_complete'] = eof and not client.exceeded.is_set()
                cleanup &= result['process_group_gone'] and result['output_complete']
            except (OSError, ValueError, h.FixtureError): cleanup = False
        else: cleanup = False
        for server, thread, label in ((broker, locals().get('bt'), 'broker'), (provider, locals().get('pt'), 'provider')):
            if server:
                try:
                    server.shutdown(); server.server_close(); thread.join(timeout=3)
                    cleanup &= not thread.is_alive()
                except (OSError, RuntimeError, h.FixtureError): cleanup = False
                result[label + '_receives'] = len(server.bodies)
                result[label + '_body_hashes'] = server.bodies
                result[label + '_http_requests'] = server.request_count
                result[label + '_rejected_connections'] = server.rejected_connections
        if broker:
            result['decisions'], result['reserved'] = broker.gate.decisions, broker.gate.reserved
        result['cleanup_complete'] = bool(cleanup)
        (raw / 'rpc.json').write_text(json.dumps(messages, indent=2))
    result['checks_pass'] = case_verdict(result)
    (raw / 'result.json').write_text(json.dumps(result, indent=2))
    return result


def case_verdict(result):
    name = result['case']
    expected = {'allow': (1, 1, 'completed'), 'deny_first': (1, 0, 'failed'),
        'retry_allow': (2, 2, 'completed'), 'retry_deny': (2, 1, 'failed'),
        'guard_disabled': (1, 0, 'failed'), 'guard_failed': (1, 0, 'failed'),
        'broker_absent': (0, 0, 'failed')}[name]
    return (result['failure'] is None and result['cleanup_complete']
        and result.get('turn_correlated') is True and result.get('process_exit_code') == 0
        and (result.get('broker_receives'), result.get('provider_receives'), result.get('turn_status')) == expected
        and (result.get('broker_http_requests'), result.get('provider_http_requests')) == expected[:2]
        and all(type(result.get(label + '_http_requests')) is int for label in ('broker', 'provider'))
        and all(isinstance(result.get(label + '_body_hashes'), list)
            and len(result[label + '_body_hashes']) == count
            for label, count in zip(('broker', 'provider'), expected[:2]))
        and result.get('reserved') == expected[1]
        and result.get('final_output_observed') == (expected[2] == 'completed')
        and result.get('broker_rejected_connections') == result.get('provider_rejected_connections') == 0
        and result.get('broker_body_hashes', [])[:expected[1]] == result.get('provider_body_hashes'))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--state', type=Path, required=True)
    args = parser.parse_args()
    os.umask(0o077)
    binary = args.binary.resolve(strict=True)
    if hashlib.sha256(binary.read_bytes()).hexdigest() != PIN: raise ValueError('native pin mismatch')
    args.state.mkdir(mode=0o700, parents=False, exist_ok=False)
    repo = Path(__file__).resolve().parents[3]
    h = load('f03a_helpers', repo / 'docs/bench/egress-admission/codex_appserver_local.py')
    prior = load('f03a_prior', repo / 'docs/bench/native-integration-u06/probe.py')
    report = {'schema_version': 1, 'native_sha256': PIN, 'native_version': h.verify_codex(binary)[0],
        'evidence_kind': 'local_native_broker_diagnostic', 'budget_unit': 'one reserved upstream HTTP request',
        'paid_provider_calls': 0, 'global_admission': 'NO_GO', 'os_admission': 'UNKNOWN',
        'cases': [run_case(h, prior, binary, case, args.state / case) for case in CASES]}
    report['native_pin_unchanged'] = hashlib.sha256(binary.read_bytes()).hexdigest() == PIN
    report['local_transport_feasibility'] = 'GO' if report['native_pin_unchanged'] and all(c['checks_pass'] for c in report['cases']) else 'NO_GO'
    (args.state / 'report.json').write_text(json.dumps(report, indent=2))
    print(json.dumps(report, indent=2))
    return 0 if report['local_transport_feasibility'] == 'GO' else 2

if __name__ == '__main__':
    raise SystemExit(main())
