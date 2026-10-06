"""One real AR execute-stage decision applied to an isolated native Codex turn.

This diagnostic uses a scripted loopback provider, not an AR runtime receipt.
"""
from __future__ import annotations

import argparse
import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import queue
import re
import selectors
import signal
import subprocess
import sys
import tempfile
import threading
import time

MAX_ROUTE_BYTES = 1024 * 1024
ROUTE_SECONDS = 10
NATIVE_SECONDS = 15
BASE_MODEL = 'gpt-6.1-sol'
BASE_EFFORT = 'low'
FINAL_OUTPUT = 'U06_NATIVE_ROUTE_APPLIED'
NATIVE_HASH = '16593cc2f422d5f398a8e40f550ebbaf1245392528957be342c295920a300704'
# Explicit fixture subset of platform-supported pairs, not paid-provider eligibility.
SUPPORTED = {'gpt-5.6-terra': {'medium', 'high', 'xhigh'},
    'gpt-5.6-sol': {'high', 'xhigh', 'max'},
    'gpt-5.6-luna': {'low', 'medium', 'high'}}


class FixtureError(RuntimeError):
    pass


def strict_json(raw):
    def pairs(values):
        result = {}
        for key, value in values:
            if key in result:
                raise FixtureError('duplicate JSON field')
            result[key] = value
        return result
    try:
        return json.loads(raw, object_pairs_hook=pairs,
            parse_constant=lambda _: (_ for _ in ()).throw(FixtureError('nonfinite JSON')))
    except (ValueError, UnicodeError, RecursionError) as error:
        raise FixtureError('invalid JSON') from error


def extract_execute(raw):
    if len(raw) > MAX_ROUTE_BYTES:
        raise FixtureError('route output limit')
    value = strict_json(raw)
    try:
        if value['schema_version'] != '1' or re.fullmatch('[0-9a-f]{64}', value['decision_id']) is None:
            raise FixtureError('route schema or decision identity')
        if set(value['decision']) != {'selected'}:
            raise FixtureError('route unavailable or ambiguous')
        stages = value['decision']['selected']['plan']['stages']
        if not isinstance(stages, list) or not 1 <= len(stages) <= 8:
            raise FixtureError('route stages')
        if any(not isinstance(stage, dict) or stage.get('kind') not in ('plan', 'execute', 'verify') for stage in stages):
            raise FixtureError('route stage kind')
        chosen = [stage for stage in stages if stage['kind'] == 'execute']
        if len(chosen) != 1:
            raise FixtureError('execute stage must occur exactly once')
        model, effort = chosen[0]['model'], chosen[0]['effort']
        if not isinstance(model, str) or not isinstance(effort, str) or effort not in SUPPORTED.get(model, set()):
            raise FixtureError('unsupported model/effort')
        return {'decision_id': value['decision_id'], 'kind': 'execute', 'model': model,
            'effort': effort, 'stage_index': stages.index(chosen[0]),
            'not_executed_stages': [stage['kind'] for stage in stages if stage['kind'] != 'execute'],
            'selected_skills_applied': False}
    except (KeyError, TypeError, AttributeError) as error:
        raise FixtureError('malformed selected route') from error


class DispatchGate:
    """One controller-local dispatch, including a dispatch that is refused."""
    def __init__(self, selection):
        self.selection, self.consumed = copy.deepcopy(selection), False

    def consume(self):
        if self.consumed:
            raise FixtureError('decision already consumed')
        self.consumed = True
        return copy.deepcopy(self.selection)


class ProviderGate:
    def __init__(self, model, effort, prompt):
        self.model, self.effort, self.prompt = model, effort, prompt
        self.accepted, self.attempted = 0, 0

    def accept(self, request):
        self.attempted += 1
        try:
            users = [item for item in request['input'] if isinstance(item, dict) and item.get('role') == 'user']
            matching_prompts = sum(content.get('text') == self.prompt for item in users
                for content in item.get('content', []) if isinstance(content, dict))
            valid = (self.attempted == 1 and request.get('model') == self.model
                and request.get('reasoning', {}).get('effort') == self.effort
                and matching_prompts == 1
                and bool(users) and users[-1].get('content') == [{'type': 'input_text', 'text': self.prompt}])
        except (KeyError, TypeError, AttributeError):
            valid = False
        if valid:
            self.accepted += 1
        return valid


def verify_protocol(messages):
    """Recompute success from observed RPC identities and exact output, not flags."""
    try:
        if any('error' in message or ('method' in message and 'id' in message)
                or message.get('method') == 'error' for message in messages):
            return False
        if any('id' in message and (type(message['id']) is not int or message['id'] not in (1, 2, 3)) for message in messages):
            return False
        responses = {identifier: [message for message in messages if message.get('id') == identifier]
            for identifier in (1, 2, 3)}
        if any(len(values) != 1 for values in responses.values()):
            return False
        start = responses[2][0]['result']
        thread_id = start['thread']['id']
        turn_id = responses[3][0]['result']['turn']['id']
        if not all(isinstance(identity, str) and 0 < len(identity) < 256 for identity in (thread_id, turn_id)):
            return False
        if (start.get('model'), start.get('reasoningEffort'), start.get('modelProvider')) != (BASE_MODEL, BASE_EFFORT, 'local_fixture'):
            return False
        starts = [m['params'] for m in messages if m.get('method') == 'turn/started']
        ends = [m['params'] for m in messages if m.get('method') == 'turn/completed']
        if len(starts) != 1 or len(ends) != 1:
            return False
        if any(e.get('threadId') != thread_id or e['turn'].get('id') != turn_id for e in starts + ends):
            return False
        if ends[0]['turn'].get('status') != 'completed' or ends[0]['turn'].get('error') is not None:
            return False
        items = [m for m in messages if m.get('method') in ('item/started', 'item/completed')]
        if any(m['params'].get('threadId') != thread_id or m['params'].get('turnId') != turn_id for m in items):
            return False
        if any(m['params']['item'].get('type') not in ('userMessage', 'agentMessage') for m in items):
            return False
        agents = [m for m in items if m['params']['item'].get('type') == 'agentMessage']
        opened = [m for m in agents if m['method'] == 'item/started']
        closed = [m for m in agents if m['method'] == 'item/completed']
        return (len(opened) == len(closed) == 1
            and opened[0]['params']['item']['id'] == closed[0]['params']['item']['id'] == 'msg_u06'
            and closed[0]['params']['item'].get('text') == FINAL_OUTPUT
            and messages.index(opened[0]) < messages.index(closed[0])
            and messages.index(closed[0]) < next(i for i, m in enumerate(messages) if m.get('method') == 'turn/completed'))
    except (KeyError, TypeError, AttributeError, ValueError):
        return False


def case_verdict(case, model, effort):
    return (not case.get('failure') and verify_protocol(case.get('rpc', []))
        and type(case.get('process_exit_code')) is int and case['process_exit_code'] == 0
        and all(case.get(field) is True for field in ('cleanup_complete', 'temporary_state_removed',
            'owned_process_group_gone', 'output_complete'))
        and case.get('callback_count') == 0 and case.get('provider_request_count') == 1
        and case.get('provider_accepted_connections') == 1
        and case.get('provider_rejected_requests') == 0 and case.get('provider_rejected_connections') == 0
        and case.get('provider_requests') == [{'model': model, 'effort': effort, 'prompt_exact': True}])


def _group_absent(pid, _timeout):
    try:
        os.killpg(pid, 0)
    except ProcessLookupError:
        return True
    except PermissionError:
        return False
    return False


def bounded_command(command, cwd, env, timeout=ROUTE_SECONDS, cap=MAX_ROUTE_BYTES, group_check=_group_absent):
    """Bound both pipes while reading; always reap the owned process group."""
    output = {'stdout': bytearray(), 'stderr': bytearray()}
    selector = selectors.DefaultSelector()
    process = None
    try:
        process = subprocess.Popen(command, cwd=cwd, env=env, stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
        for name in output:
            stream = getattr(process, name)
            os.set_blocking(stream.fileno(), False)
            selector.register(stream, selectors.EVENT_READ, name)
        deadline = time.monotonic() + timeout
        while selector.get_map():
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise FixtureError('command timeout')
            for key, _ in selector.select(remaining):
                try:
                    chunk = os.read(key.fileobj.fileno(), min(32768, cap + 1 - len(output[key.data])))
                except BlockingIOError:
                    continue
                if not chunk:
                    selector.unregister(key.fileobj)
                    continue
                output[key.data].extend(chunk)
                if len(output[key.data]) > cap:
                    raise FixtureError('command output limit')
        if process.wait(timeout=max(.01, deadline - time.monotonic())) != 0:
            raise FixtureError('command nonzero exit')
        return bytes(output['stdout'])
    except (OSError, subprocess.TimeoutExpired) as error:
        raise FixtureError('command unavailable or timeout') from error
    finally:
        selector.close()
        if process is not None:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            except PermissionError:
                process.kill()
            process.wait(timeout=3)
            for stream in (process.stdout, process.stderr):
                stream.close()
            if not group_check(process.pid, 3):
                raise FixtureError('command process cleanup unknown')


def final_sse(model):
    item = {'id': 'msg_u06', 'type': 'message', 'status': 'completed', 'role': 'assistant',
        'content': [{'type': 'output_text', 'text': FINAL_OUTPUT, 'annotations': []}]}
    response = {'id': 'resp_u06', 'object': 'response', 'created_at': 0, 'model': model,
        'status': 'completed', 'output': [item], 'usage': {'input_tokens': 1, 'output_tokens': 1, 'total_tokens': 2}}
    events = [
        {'type': 'response.created', 'response': {**response, 'status': 'in_progress', 'output': []}},
        {'type': 'response.output_item.added', 'output_index': 0,
            'item': {**item, 'status': 'in_progress', 'content': [{'type': 'output_text', 'text': '', 'annotations': []}]}},
        {'type': 'response.output_text.delta', 'item_id': 'msg_u06', 'output_index': 0, 'content_index': 0, 'delta': FINAL_OUTPUT},
        {'type': 'response.output_text.done', 'item_id': 'msg_u06', 'output_index': 0, 'content_index': 0, 'text': FINAL_OUTPUT},
        {'type': 'response.output_item.done', 'output_index': 0, 'item': item},
        {'type': 'response.completed', 'response': response}]
    return ''.join('event: ' + e['type'] + '\ndata: ' + json.dumps(e, separators=(',', ':')) + '\n\n' for e in events).encode()


def handler_class(h):
    class Handler(h.ProviderHandler):
        def parse_request(self):
            with self.server.lock:
                self.server.request_count += 1
            return super().parse_request()

        def send_error(self, code, message=None, explain=None):
            with self.server.lock:
                self.server.rejected_requests += 1
            super().send_error(code, message, explain)

        def do_POST(self):
            self.close_connection = True
            if self.path != '/v1/responses':
                self.send_error(404); return
            try:
                if self.headers.get('Transfer-Encoding') or len(self.headers.get_all('Content-Length', [])) != 1:
                    self.send_error(400); return
                length = int(self.headers['Content-Length'])
                if not 0 < length <= h.MAX_REQUEST_BYTES:
                    self.send_error(413); return
                body = self.rfile.read(length)
                if len(body) != length:
                    self.send_error(400); return
                request = strict_json(body)
                with self.server.lock:
                    accepted = self.server.request_count == 1 and self.server.gate.accept(request)
                    if accepted:
                        self.server.projections.append({'model': request['model'],
                            'effort': request['reasoning']['effort'], 'prompt_exact': True})
                if not accepted:
                    self.send_error(400); return
            except (ValueError, TypeError, AttributeError, FixtureError):
                self.send_error(400); return
            (self.server.raw_dir / 'provider-request.json').write_bytes(body)
            payload = final_sse(self.server.gate.model)
            self.send_response(200)
            self.send_header('Content-Type', 'text/event-stream')
            self.send_header('Content-Length', str(len(payload)))
            self.send_header('Connection', 'close')
            self.end_headers()
            self.wfile.write(payload)
            self.wfile.flush()
    return Handler


def run_case(h, prior, binary, model, effort, prompt, enabled, raw_dir):
    case = {'mode': 'enabled' if enabled else 'disabled', 'rpc': [], 'callback_count': 0,
        'failure': None, 'cleanup_complete': False, 'owned_process_group_gone': False,
        'output_complete': False, 'provider_requests': []}
    provider = listener = client = None
    with tempfile.TemporaryDirectory(prefix='ar-u06-native-', dir='/private/tmp') as scratch:
        root = Path(scratch); home, workspace = root / 'home', root / 'workspace'
        for directory in (home, workspace, home / 'tmp', home / 'config', home / 'cache', home / 'data'):
            directory.mkdir(mode=0o700, parents=True, exist_ok=True)
        def observe(message):
            case['rpc'].append(message)
            if 'method' in message and 'id' in message:
                case['callback_count'] += 1
                raise FixtureError('unexpected native callback')
            if message.get('method') == 'error':
                raise FixtureError('native protocol error')
        def response(identifier, deadline):
            while True:
                message = client.receive(deadline); observe(message)
                if message.get('id') == identifier:
                    return message
        try:
            provider = h.LocalProvider(); provider.RequestHandlerClass = handler_class(h)
            provider.projections = []; provider.gate = ProviderGate(model, effort, prompt); provider.raw_dir = raw_dir
            listener = threading.Thread(target=lambda: provider.serve_forever(poll_interval=.05), daemon=True)
            listener.start()
            (home / 'config.toml').write_text(f'''model = "{BASE_MODEL}"
model_provider = "local_fixture"
model_reasoning_effort = "{BASE_EFFORT}"
approval_policy = "never"
sandbox_mode = "read-only"
web_search = "disabled"
[features]
shell_tool = false
unified_exec = false
apps = false
[model_providers.local_fixture]
name = "U06 scripted loopback"
base_url = "http://127.0.0.1:{provider.server_address[1]}/v1"
wire_api = "responses"
requires_openai_auth = false
supports_websockets = false
request_max_retries = 0
stream_max_retries = 0
''')
            process = subprocess.Popen([str(binary), 'app-server', '--listen', 'stdio://'],
                stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                cwd=workspace, env=h.isolated_env(binary, home), start_new_session=True)
            client = h.AppServer(process); deadline = time.monotonic() + NATIVE_SECONDS
            client.send({'id': 1, 'method': 'initialize', 'params': {'clientInfo': {'name': 'u06-route', 'version': '1.0.0'},
                'capabilities': {'experimentalApi': True}}})
            if 'error' in response(1, deadline): raise FixtureError('initialize refused')
            client.send({'method': 'initialized'})
            client.send({'id': 2, 'method': 'thread/start', 'params': {'model': BASE_MODEL,
                'modelProvider': 'local_fixture', 'cwd': str(workspace), 'ephemeral': True,
                'approvalPolicy': 'never', 'sandbox': 'read-only'}})
            start = response(2, deadline)
            if 'error' in start: raise FixtureError('thread/start refused')
            case['thread_id'] = start['result']['thread']['id']
            case['thread_defaults'] = {key: start['result'].get(key) for key in ('model', 'reasoningEffort', 'modelProvider')}
            params = {'threadId': case['thread_id'], 'input': [{'type': 'text', 'text': prompt}]}
            if enabled: params.update(model=model, effort=effort)
            case['turn_requested'] = {key: params.get(key) for key in ('model', 'effort')}
            client.send({'id': 3, 'method': 'turn/start', 'params': params})
            turn = response(3, deadline)
            if 'error' in turn:
                case['attempt_outcome'] = 'refused'
                raise FixtureError('turn/start refused')
            case['turn_id'] = turn['result']['turn']['id']
            while not any(m.get('method') == 'turn/completed' for m in case['rpc']):
                observe(client.receive(deadline))
        except (OSError, h.FixtureError, FixtureError, KeyError, TypeError, ValueError) as error:
            case['failure'] = type(error).__name__
            (raw_dir / 'error.txt').write_text(str(error))
        finally:
            cleanup_ok = True
            if client is not None:
                try:
                    case['process_exit_code'] = client.stop()
                    case['owned_process_group_gone'] = prior.wait_group_gone(client.process.pid, h.CLEANUP_TIMEOUT_SECONDS, case)
                    eof = False
                    while True:
                        try: line = client.lines.get_nowait()
                        except queue.Empty: break
                        if line is None: eof = True
                        else: observe(strict_json(line))
                    case['output_complete'] = eof and not client.exceeded.is_set()
                except (OSError, h.FixtureError, FixtureError, ValueError, TypeError):
                    cleanup_ok = False
            if provider is not None:
                try:
                    if listener is not None and listener.ident is not None: provider.shutdown()
                    provider.server_close()
                    if listener is not None and listener.ident is not None:
                        listener.join(timeout=h.CLEANUP_TIMEOUT_SECONDS)
                        if listener.is_alive(): cleanup_ok = False
                except (OSError, h.FixtureError, RuntimeError):
                    cleanup_ok = False
                case['provider_requests'] = provider.projections
                for key in ('request_count', 'accepted_connections', 'rejected_requests', 'rejected_connections'):
                    case['provider_' + key] = getattr(provider, key)
            case['cleanup_complete'] = cleanup_ok and client is not None
            (raw_dir / 'rpc-messages.json').write_text(json.dumps(case['rpc'], indent=2))
    case['temporary_state_removed'] = not root.exists()
    case['success'] = bool(case_verdict(case, model, effort))
    case.setdefault('attempt_outcome', 'completed_and_locally_verified' if case['success'] else 'failed_or_unknown')
    case['local_verification'] = verify_protocol(case['rpc'])
    del case['rpc']
    return case


def load_module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    previous = sys.dont_write_bytecode
    try:
        sys.dont_write_bytecode = True
        spec.loader.exec_module(module)
    finally:
        sys.dont_write_bytecode = previous
    return module


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo-root', type=Path, required=True)
    parser.add_argument('--router-binary', type=Path, required=True)
    parser.add_argument('--router-sha256', required=True)
    parser.add_argument('--native-binary', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    os.umask(0o077)
    repo = args.repo_root.resolve(strict=True)
    helper = repo / 'docs/bench/egress-admission/codex_appserver_local.py'
    prior_path = repo / 'docs/bench/native-integration-u02/probe_v5.py'
    h = load_module('u06_egress', helper); prior = load_module('u06_prior', prior_path)
    binary = args.native_binary.resolve(strict=True); router = args.router_binary.resolve(strict=True)
    if not re.fullmatch('[0-9a-f]{64}', args.router_sha256) or h.sha256_file(router) != args.router_sha256:
        raise FixtureError('router binary hash mismatch')
    if h.sha256_file(binary) != NATIVE_HASH:
        raise FixtureError('native binary hash mismatch')
    raw = Path(tempfile.mkdtemp(prefix='ar-u06-evidence-', dir='/private/tmp'))
    frozen = {}
    for label, relative in [('request', 'tests/fixtures/requests/codex-implementation.json'),
            ('catalog', 'assets/catalogs/codex-core-v1.json')]:
        with (repo / relative).open('rb') as stream: contents = stream.read(MAX_ROUTE_BYTES + 1)
        if len(contents) > MAX_ROUTE_BYTES: raise FixtureError('route input byte limit')
        destination = raw / (label + '.json'); destination.write_bytes(contents)
        frozen[label] = {'path': destination, 'sha256': hashlib.sha256(contents).hexdigest()}
    with tempfile.TemporaryDirectory(prefix='ar-u06-route-', dir='/private/tmp') as scratch:
        home = Path(scratch)
        for name in ('tmp', 'config', 'cache', 'data'): (home / name).mkdir(mode=0o700)
        output = bounded_command([str(router), 'route', '--request', str(frozen['request']['path']),
            '--catalog', str(frozen['catalog']['path']), '--json'], home, h.isolated_env(router, home),
            group_check=prior.wait_group_gone)
    (raw / 'route-output.json').write_bytes(output)
    selection = extract_execute(output)
    selected = DispatchGate(selection).consume()
    version, native_hash = h.verify_codex(binary)
    if version != '0.159.2' or native_hash != NATIVE_HASH: raise FixtureError('native version mismatch')
    cases = []
    for enabled in (True, False):
        mode = 'enabled' if enabled else 'disabled'
        directory = raw / mode; directory.mkdir(mode=0o700)
        model, effort = (selected['model'], selected['effort']) if enabled else (BASE_MODEL, BASE_EFFORT)
        prompt = f"U06 decision {selected['decision_id']} stage execute control {mode}. Return exactly {FINAL_OUTPUT}."
        cases.append(run_case(h, prior, binary, model, effort, prompt, enabled, directory))
    report = {'schema_version': 1, 'kind': 'external_native_fixture',
        'route': {'router_binary_sha256': args.router_sha256,
            'request_sha256': frozen['request']['sha256'], 'catalog_sha256': frozen['catalog']['sha256'],
            'output_sha256': hashlib.sha256(output).hexdigest(), 'selection': selected},
        'native': {'version': version, 'binary_sha256': native_hash},
        'source_hashes': {'probe': h.sha256_file(Path(__file__)), 'u02_helper': h.sha256_file(prior_path),
            'appserver_helper': h.sha256_file(helper)},
        'admission_basis': 'explicit_platform_supported_fixture_subset; paid_provider_eligibility_UNKNOWN',
        'cases': cases, 'ar_runtime_receipt': 'NOT_APPLICABLE', 'parent_session_model_changed': False,
        'automatic_daily_interception': 'NOT_TESTED', 'external_egress_denial': 'UNKNOWN',
        'real_provider_availability': 'UNKNOWN', 'real_task_quality': 'UNKNOWN', 'cost': 'UNKNOWN',
        'usage': 'SYNTHETIC_SCRIPTED_PROVIDER', 'selected_skills_applied': False,
        'disabled_withdrawal_verified': bool(cases[1]['success'] and (selected['model'], selected['effort']) != (BASE_MODEL, BASE_EFFORT)),
        'success': all(case['success'] for case in cases)}
    args.output.write_text(json.dumps(report, indent=2, sort_keys=True) + '\n')
    print(json.dumps({'report': str(args.output), 'private_evidence': str(raw), 'success': report['success']}))
    return 0 if report['success'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
