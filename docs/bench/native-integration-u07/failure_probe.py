"""Qualify native UserPromptSubmit failure behavior with zero inference.

Only fixed hook diagnostics and a rejecting loopback provider are available.
Native trust must come from the real /hooks UI, never from this program.
"""
from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import queue
import re
import shlex
import signal
import stat
import subprocess
import sys
import threading
import time
import tomllib

PROMPTS = {name: 'U07:' + name.upper() for name in
    ('block', 'exit1', 'malformed', 'timeout', 'signal', 'exit2_reason', 'exit2_empty', 'disabled')}
NATIVE_HASH = '16593cc2f422d5f398a8e40f550ebbaf1245392528957be342c295920a300704'
MAX_BYTES = 65536
CASE_SECONDS = 15
PREPARE_SECONDS = 180


class ProbeError(RuntimeError):
    pass


def digest(data):
    return hashlib.sha256(data).hexdigest()


def parse_hook_input(raw):
    if len(raw) > MAX_BYTES: raise ProbeError('hook input limit')
    try:
        value = json.loads(raw)
        if not isinstance(value, dict) or value.get('hook_event_name') != 'UserPromptSubmit':
            raise ProbeError('hook event mismatch')
        mode = next((case for case, prompt in PROMPTS.items() if value.get('prompt') == prompt), None)
        if mode is None or mode == 'disabled': raise ProbeError('unknown fixed prompt')
        if any(not isinstance(value.get(key), str) or not 0 < len(value[key]) <= 256
                for key in ('session_id', 'turn_id')):
            raise ProbeError('hook identity missing')
        return {'case': mode, 'session_id': value['session_id'], 'turn_id': value['turn_id']}
    except (ValueError, UnicodeError, RecursionError) as error:
        raise ProbeError('malformed hook input') from error


def journal_append(path, row):
    payload = (json.dumps(row, separators=(',', ':')) + '\n').encode()
    if len(payload) > 2048: raise ProbeError('journal entry limit')
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_APPEND | os.O_NOFOLLOW, 0o600)
    try:
        info = os.fstat(fd)
        if not stat.S_ISREG(info.st_mode) or info.st_mode & 0o077 or info.st_size + len(payload) > MAX_BYTES:
            raise ProbeError('journal is not bounded private storage')
        if os.write(fd, payload) != len(payload): raise ProbeError('incomplete journal write')
        os.fsync(fd)
    finally:
        os.close(fd)


def hook(journal):
    # Bounds a stalled input and the timeout case independently of host policy.
    signal.alarm(5)
    row = parse_hook_input(sys.stdin.buffer.read(MAX_BYTES + 1))
    row.update(pid=os.getpid(), pgid=os.getpgrp(), monotonic_ns=time.monotonic_ns())
    journal_append(journal, {**row, 'event': 'entered'})
    journal_append(journal, {**row, 'event': 'action'})
    mode = row['case']
    if mode == 'signal': os.kill(os.getpid(), signal.SIGTERM)
    if mode == 'timeout':
        time.sleep(3)
        journal_append(journal, {**row, 'event': 'late_return'})
    if mode == 'exit1':
        sys.stderr.write('U07_EXIT1\n'); return 1
    if mode == 'exit2_reason':
        sys.stderr.write('U07_EXIT2_BLOCK_REASON\n'); return 2
    if mode == 'exit2_empty': return 2
    if mode == 'malformed':
        sys.stdout.write('{not-json\n'); return 0
    reason = 'U07_TIMEOUT_RETURNED_LATE' if mode == 'timeout' else 'U07_BLOCK_CONTROL'
    sys.stdout.write(json.dumps({'decision': 'block', 'reason': reason}, separators=(',', ':')) + '\n')
    return 0


def require_trust(listed, state, command, enabled):
    try:
        entries = listed['data']
        if any(entry['errors'] or entry['warnings'] for entry in entries): raise ProbeError('native discovery warning')
        hooks = [hook for entry in entries for hook in entry['hooks']]
        if len(hooks) != 1: raise ProbeError('unexpected hook inventory')
        hook = hooks[0]
        current = hook['currentHash']
        if (hook['eventName'] != 'userPromptSubmit' or hook['handlerType'] != 'command'
                or hook['command'] != command or hook.get('async', False) is not False
                or hook['timeoutSec'] != 1 or hook['enabled'] is not enabled
                or hook['trustStatus'] != 'trusted' or re.fullmatch('sha256:[0-9a-f]{64}', current) is None
                or state[hook['key']]['trusted_hash'] != current):
            raise ProbeError('hook is not exactly UI-trusted')
        return current
    except (KeyError, TypeError, AttributeError) as error:
        raise ProbeError('native trust evidence unavailable') from error


def qualify(case):
    """No contact is insufficient: require correlated terminal native evidence."""
    result = {'case': case['case'], 'verdict': 'UNKNOWN', 'safe_failure_refusal': False,
        'parent_http_requests': case.get('provider_request_count'),
        'accepted_tcp_connections': case.get('provider_accepted_connections'),
        'rejected_tcp_connections': case.get('provider_rejected_connections'), 'hook_status': None,
        'terminal_parent_status': None, 'hook_run_id_sha256': None,
        'thread_id': None, 'turn_id': None}
    try:
        if case.get('failure') is not None: return result
        messages = case['rpc']
        if not all(case.get(field) is True for field in ('trust_verified', 'output_complete',
                'owned_process_group_gone', 'hook_processes_gone', 'provider_stopped')):
            return result
        if type(case.get('exit_code')) is not int or case['exit_code'] != 0: return result
        if any('id' in message and 'method' in message for message in messages): return result
        def one(predicate):
            found = [message for message in messages if predicate(message)]
            if len(found) != 1: raise ProbeError('missing or duplicate protocol identity')
            return found[0]
        thread = one(lambda m: m.get('id') == 3)['result']['thread']['id']
        turn = one(lambda m: m.get('id') == 4)['result']['turn']['id']
        if not all(isinstance(value, str) and 0 < len(value) <= 256 for value in (thread, turn)): return result
        started = one(lambda m: m.get('method') == 'turn/started')['params']
        completed = one(lambda m: m.get('method') == 'turn/completed')['params']
        if any(event.get('threadId') != thread or event['turn'].get('id') != turn for event in (started, completed)): return result
        terminal = completed['turn'].get('status')
        if terminal not in ('completed', 'failed', 'interrupted'): return result
        result.update(thread_id=thread, turn_id=turn, terminal_parent_status=terminal)
        requests = case['http']
        if (len(requests) != case['provider_request_count'] or case['provider_accepted_connections'] < len(requests)
                or any(r.get('prompt_case') != case['case'] or r.get('rejected') is not True for r in requests)):
            return result
        hooks = [m for m in messages if m.get('method', '').startswith('hook/')]
        rows = case['journal']
        if case['case'] == 'disabled':
            if not hooks and not rows and requests: result['verdict'] = 'DISABLED_PARENT_CONTACT_OBSERVED'
            return result
        hook_start = one(lambda m: m.get('method') == 'hook/started')
        hook_end = one(lambda m: m.get('method') == 'hook/completed')
        if len(hooks) != 2: return result
        order = [messages.index(one(lambda m: m.get('method') == name))
            for name in ('turn/started', 'hook/started', 'hook/completed', 'turn/completed')]
        if order != sorted(order): return result
        for notification in (hook_start, hook_end):
            event = notification['params']; run = event['run']
            if (event.get('threadId') != thread or event.get('turnId') != turn
                    or run.get('eventName') != 'userPromptSubmit' or run.get('handlerType') != 'command'
                    or run.get('executionMode') != 'sync'): return result
        before, after = hook_start['params']['run'], hook_end['params']['run']
        if (not isinstance(before.get('id'), str) or not 0 < len(before['id']) <= 4096
                or before['id'] != after['id'] or before.get('status') != 'running'): return result
        if (len(rows) != 2 or [row.get('event') for row in rows] != ['entered', 'action']
                or any(row.get('session_id') != thread or row.get('turn_id') != turn
                    or row.get('case') != case['case'] for row in rows)):
            return result
        status = after.get('status')
        result.update(hook_status=status, hook_run_id_sha256=digest(after['id'].encode()))
        if requests:
            result['verdict'] = 'PARENT_FALLTHROUGH_OBSERVED'
        elif (case['provider_accepted_connections'] == 0 and case.get('provider_rejected_connections') == 0
                and status in ('blocked', 'stopped')):
            result['verdict'] = 'BLOCK_OBSERVED'
            result['safe_failure_refusal'] = case['case'] in ('exit1', 'exit2_empty', 'malformed', 'signal')
        return result
    except (KeyError, TypeError, AttributeError, ProbeError):
        return result


def helpers(repo):
    # Reuse the existing no-bytecode loader and bounded transport unchanged.
    path = repo / 'docs/bench/native-integration-u06/probe.py'
    previous = sys.dont_write_bytecode
    try:
        sys.dont_write_bytecode = True
        spec = importlib.util.spec_from_file_location('u07_u06', path)
        module = importlib.util.module_from_spec(spec); spec.loader.exec_module(module)
    finally:
        sys.dont_write_bytecode = previous
    return (module.load_module('u07_transport', repo / 'docs/bench/egress-admission/codex_appserver_local.py'),
        module.load_module('u07_prior', repo / 'docs/bench/native-integration-u02/probe_v5.py'))


def validate_config(baseline, config, key, workspace, enabled):
    # Parse semantic native UI deltas; never write native trust state ourselves.
    value = json.loads(json.dumps(config))
    try:
        states = value['hooks'].pop('state')
        state = states[key]
        if (set(states) != {key} or set(state) not in ({'trusted_hash'}, {'trusted_hash', 'enabled'})
                or state.get('enabled', True) is not enabled
                or re.fullmatch(r'sha256:[0-9a-f]{64}', state['trusted_hash']) is None):
            raise ProbeError('unexpected native hook state')
        if value.pop('projects', {}) != {workspace: {'trust_level': 'trusted'}}:
            raise ProbeError('workspace trust unavailable')
        tui = value.pop('tui', {})
        if set(tui) - {'screen_reader_detection_done', 'model_availability_nux'}:
            raise ProbeError('unexpected UI configuration')
        if tui.get('screen_reader_detection_done', True) is not True:
            raise ProbeError('unexpected UI detection setting')
        nux = tui.get('model_availability_nux', {})
        if nux and (set(nux) != {'gpt-6.1-sol'} or type(nux['gpt-6.1-sol']) is not int or not 1 <= nux['gpt-6.1-sol'] <= 8):
            raise ProbeError('unexpected UI counter')
        if value != baseline: raise ProbeError('configuration changed beyond native trust UI')
        return states
    except (KeyError, TypeError, AttributeError) as error:
        raise ProbeError('native UI state unavailable') from error


def start_provider(h, port):
    class RejectingProvider(h.LocalProvider):
        def server_bind(self):
            self.server_address = ('127.0.0.1', port)
            super().server_bind()
    class Handler(h.ProviderHandler):
        def parse_request(self):
            with self.server.lock: self.server.request_count += 1
            return super().parse_request()

        def do_POST(self):
            self.close_connection = True
            mode = None
            try:
                if self.headers.get('Transfer-Encoding') or len(self.headers.get_all('Content-Length', [])) != 1:
                    raise ValueError('invalid framing')
                size = int(self.headers['Content-Length'])
                if not 0 < size <= h.MAX_REQUEST_BYTES: raise ValueError('request limit')
                raw = self.rfile.read(size)
                if len(raw) != size: raise ValueError('short request')
                body = json.loads(raw)
                users = [item for item in body.get('input', []) if item.get('role') == 'user']
                texts = [part.get('text') for part in users[-1].get('content', []) if part.get('type') in ('input_text', 'text')] if users else []
                mode = next((name for name, prompt in PROMPTS.items() if texts == [prompt]), None)
            except (ValueError, TypeError, AttributeError, UnicodeError):
                pass
            with self.server.lock:
                self.server.rejected_requests += 1
                self.server.projections.append({'prompt_case': mode, 'rejected': True})
            payload = b'{"error":{"message":"U07_FIXED_REFUSAL","type":"diagnostic_refusal"}}'
            self.send_response(503)
            self.send_header('Content-Type', 'application/json')
            self.send_header('Content-Length', str(len(payload)))
            self.send_header('Connection', 'close'); self.end_headers()
            self.wfile.write(payload)
    provider = RejectingProvider(); provider.RequestHandlerClass = Handler
    provider.projections = []
    listener = threading.Thread(target=lambda: provider.serve_forever(poll_interval=.05), daemon=True)
    try: listener.start()
    except RuntimeError:
        provider.server_close(); raise
    return provider, listener


def stop_provider(h, provider, listener):
    provider.shutdown(); provider.server_close(); listener.join(timeout=h.CLEANUP_TIMEOUT_SECONDS)
    return not listener.is_alive() and not any(t.is_alive() for t in provider.handler_threads)


def read_bounded(path, limit=MAX_BYTES):
    with path.open('rb') as stream: data = stream.read(limit + 1)
    if len(data) > limit: raise ProbeError('private input size limit')
    return data


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2) + '\n')


def prepare(args, h):
    state = args.state.absolute()
    state.mkdir(mode=0o700)
    home, workspace = state / 'home', state / 'workspace'
    for path in (home, workspace, home / 'tmp', home / 'config', home / 'cache', home / 'data'):
        path.mkdir(mode=0o700)
    provider, listener = start_provider(h, 0)
    stopped = threading.Event()
    old = {sig: signal.signal(sig, lambda *_: stopped.set()) for sig in (signal.SIGINT, signal.SIGTERM)}
    try:
        script = state / 'hook.py'; script.write_bytes(Path(__file__).read_bytes())
        command = shlex.join([sys.executable, '-B', str(script), 'hook', '--journal', str(state / 'hook.jsonl')])
        config = f'''model = "gpt-6.1-sol"
model_provider = "local_fixture"
model_reasoning_effort = "low"
approval_policy = "never"
sandbox_mode = "read-only"
web_search = "disabled"
[features]
shell_tool = false
unified_exec = false
apps = false
[model_providers.local_fixture]
name = "U07 rejecting loopback"
base_url = "http://127.0.0.1:{provider.server_address[1]}/v1"
wire_api = "responses"
requires_openai_auth = false
supports_websockets = false
request_max_retries = 0
stream_max_retries = 0
[[hooks.UserPromptSubmit]]
hooks = [{{type = "command", command = {json.dumps(command)}, timeout = 1, async = false}}]
'''
        (home / 'config.toml').write_text(config)
        (state / 'config-before.toml').write_text(config)
        pins = {str(path): h.sha256_file(path) for path in (script, args.binary,
            args.repo_root / 'docs/bench/native-integration-u06/probe.py',
            args.repo_root / 'docs/bench/native-integration-u02/probe_v5.py',
            args.repo_root / 'docs/bench/egress-admission/codex_appserver_local.py')}
        write_json(state / 'preimages.json', {'pins': pins, 'command': command,
            'port': provider.server_address[1], 'repo': str(args.repo_root), 'binary': str(args.binary),
            'baseline_sha256': digest(config.encode()), 'source_sha256': h.sha256_file(Path(__file__))})
        launch = {'argv': [str(args.binary)], 'env': {**h.isolated_env(args.binary, home), 'TERM': 'xterm-256color'},
            'cwd': str(workspace), 'deadline_seconds': PREPARE_SECONDS,
            'pty_capture_limit_bytes': 2 * 1024 * 1024}
        write_json(state / 'ui-launch.json', launch)
        print(json.dumps({'ready': True, 'ui_launch': str(state / 'ui-launch.json'), 'state': str(state)}), flush=True)
        stopped.wait(PREPARE_SECONDS)
    finally:
        ok = stop_provider(h, provider, listener)
        for sig, handler in old.items(): signal.signal(sig, handler)
        write_json(state / 'prepare-cleanup.json', {'provider_stopped': ok,
            'accepted_connections': provider.accepted_connections, 'http_requests': provider.request_count,
            'preimages_sha256': digest(read_bounded(state / 'preimages.json'))})
    if not ok: raise ProbeError('prepare listener cleanup unknown')
    return 0


def check_images(args, h, enabled):
    state = args.state
    pins_raw = read_bounded(state / 'preimages.json')
    info = json.loads(pins_raw)
    if info['repo'] != str(args.repo_root) or info['binary'] != str(args.binary): raise ProbeError('source selection changed')
    if h.sha256_file(Path(__file__)) != info['source_sha256']: raise ProbeError('diagnostic source changed')
    if any(h.sha256_file(Path(path)) != expected for path, expected in info['pins'].items()): raise ProbeError('pinned input changed')
    before = read_bounded(state / 'config-before.toml')
    if digest(before) != info['baseline_sha256']: raise ProbeError('baseline changed')
    home = state / 'home'; config = read_bounded(home / 'config.toml')
    key = str(home / 'config.toml') + ':user_prompt_submit:0:0'
    states = validate_config(tomllib.loads(before.decode()), tomllib.loads(config.decode()), key, str(state / 'workspace'), enabled)
    cleanup = json.loads(read_bounded(state / 'prepare-cleanup.json'))
    if cleanup.get('provider_stopped') is not True or cleanup.get('preimages_sha256') != digest(pins_raw):
        raise ProbeError('prepare cleanup or preimages digest unavailable')
    info['preimages_sha256'] = digest(pins_raw)
    return info, states, key, config


def run_case(args, h, prior):
    mode = args.case; enabled = mode != 'disabled'; state = args.state
    info, states, key, config = check_images(args, h, enabled)
    raw = state / ('case-' + mode); raw.mkdir(mode=0o700)
    (raw / 'config-before.toml').write_bytes(config)
    case = {'case': mode, 'rpc': [], 'http': [], 'journal': [], 'trust_verified': False,
        'output_complete': False, 'owned_process_group_gone': False, 'hook_processes_gone': False, 'provider_stopped': False,
        'exit_code': None, 'provider_request_count': None, 'provider_accepted_connections': None}
    provider = listener = client = None
    journal = state / 'hook.jsonl'
    offset = len(read_bounded(journal)) if journal.exists() else 0
    def observe(message):
        case['rpc'].append(message)
        if 'id' in message and 'method' in message: raise ProbeError('unexpected native callback')
    def response(identifier, deadline):
        while True:
            message = client.receive(deadline); observe(message)
            if message.get('id') == identifier:
                if 'error' in message: raise ProbeError('native RPC refused')
                return message['result']
    try:
        provider, listener = start_provider(h, info['port'])
        process = subprocess.Popen([str(args.binary), 'app-server', '--listen', 'stdio://'],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            cwd=state / 'workspace', env=h.isolated_env(args.binary, state / 'home'), start_new_session=True)
        client = h.AppServer(process); deadline = time.monotonic() + CASE_SECONDS
        client.send({'id': 1, 'method': 'initialize', 'params': {'clientInfo': {'name': 'u07-failure', 'version': '1.0.0'},
            'capabilities': {'experimentalApi': True}}})
        response(1, deadline); client.send({'method': 'initialized'})
        client.send({'id': 2, 'method': 'hooks/list', 'params': {'cwds': [str(state / 'workspace')]}})
        listed = response(2, deadline)
        hook_info = [item for entry in listed['data'] for item in entry['hooks']]
        if len(hook_info) != 1: raise ProbeError('unexpected hook discovery')
        item = hook_info[0]
        if (item.get('key') != key or item.get('sourcePath') != str(state / 'home/config.toml')
                or item.get('source') != 'user' or item.get('matcher') is not None
                or item.get('pluginId') is not None or item.get('isManaged') is not False):
            raise ProbeError('unexpected hook provenance')
        case['trusted_hash'] = require_trust(listed, states, info['command'], enabled)
        case['trust_verified'] = True
        client.send({'id': 3, 'method': 'thread/start', 'params': {'model': 'gpt-6.1-sol',
            'modelProvider': 'local_fixture', 'cwd': str(state / 'workspace'), 'ephemeral': True,
            'approvalPolicy': 'never', 'sandbox': 'read-only'}})
        thread = response(3, deadline)['thread']['id']
        client.send({'id': 4, 'method': 'turn/start', 'params': {'threadId': thread,
            'input': [{'type': 'text', 'text': PROMPTS[mode]}]}})
        response(4, deadline)
        while not any(m.get('method') == 'turn/completed' for m in case['rpc']): observe(client.receive(deadline))
    except (OSError, ValueError, TypeError, KeyError, ProbeError, h.FixtureError) as error:
        case['failure'] = type(error).__name__
        (raw / 'error.txt').write_text(str(error))
    finally:
        if client is not None:
            try:
                case['exit_code'] = client.stop()
                case['owned_process_group_gone'] = prior.wait_group_gone(client.process.pid, 3)
                eof = False
                while True:
                    try: line = client.lines.get_nowait()
                    except queue.Empty: break
                    if line is None: eof = True
                    else: observe(json.loads(line))
                case['output_complete'] = eof and not client.exceeded.is_set()
            except (OSError, ValueError, ProbeError, h.FixtureError):
                case['output_complete'] = False
        if provider is not None:
            try: case['provider_stopped'] = stop_provider(h, provider, listener)
            except (OSError, RuntimeError, h.FixtureError): pass
            case.update(http=provider.projections, provider_request_count=provider.request_count,
                provider_accepted_connections=provider.accepted_connections,
                provider_rejected_connections=provider.rejected_connections)
        if journal.exists():
            try: case['journal'] = [json.loads(line) for line in read_bounded(journal)[offset:].splitlines()]
            except (ValueError, ProbeError): case['output_complete'] = False
        post = read_bounded(state / 'home/config.toml'); (raw / 'config-after.toml').write_bytes(post)
        if post != config: case['trust_verified'] = False
        try:
            after_info, _, _, _ = check_images(args, h, enabled)
            if after_info != info: case['trust_verified'] = False
        except (OSError, ValueError, KeyError, ProbeError):
            case['trust_verified'] = False
        case['hook_processes_gone'] = hooks_gone(case['journal'], prior, mode == 'disabled')
        write_json(raw / 'private-evidence.json', case)
    result = qualify(case)
    result.update(kind='external_native_hook_failure_diagnostic', u07_complete=False,
        native_binary_sha256=NATIVE_HASH, hook_script_sha256=info['pins'][str(state / 'hook.py')],
        quality='UNKNOWN', cost='UNKNOWN', router_child_executed=False,
        timeout_failure_refusal='UNKNOWN' if mode == 'timeout' else None)
    write_json(raw / 'report.json', result)
    print(json.dumps(result))
    return 0 if result['verdict'] != 'UNKNOWN' else 2


def hooks_gone(rows, prior, disabled):
    if disabled: return not rows
    try:
        identities = {(row['pid'], row['pgid']) for row in rows}
        if len(identities) != 1: return False
        pid, pgid = identities.pop()
        if type(pid) is not int or type(pgid) is not int or min(pid, pgid) <= 1 or pgid == os.getpgrp(): return False
        if not prior.wait_group_gone(pgid, 3): return False
        try: os.kill(pid, 0)
        except ProcessLookupError: return True
        except PermissionError: return False
        return False
    except (KeyError, TypeError):
        return False


def private_state(path):
    resolved = path.resolve()
    if path.is_symlink() or resolved == Path('/private/tmp') or not resolved.is_relative_to('/private/tmp'):
        raise ProbeError('state must remain inside private temporary storage')
    return resolved


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='action', required=True)
    hook_parser = commands.add_parser('hook')
    hook_parser.add_argument('--journal', type=Path, required=True)
    for action in ('prepare', 'run'):
        command = commands.add_parser(action)
        command.add_argument('--repo-root', type=Path, required=True)
        command.add_argument('--binary', type=Path, required=True)
        command.add_argument('--state', type=Path, required=True)
        if action == 'run': command.add_argument('--case', choices=PROMPTS, required=True)
    args = parser.parse_args()
    if args.action == 'hook': return hook(args.journal)
    os.umask(0o077)
    args.repo_root = args.repo_root.resolve(strict=True)
    args.binary = args.binary.resolve(strict=True)
    args.state = private_state(args.state)
    h, prior = helpers(args.repo_root)
    if h.sha256_file(args.binary) != NATIVE_HASH: raise ProbeError('native binary pin mismatch')
    if args.action == 'prepare': return prepare(args, h)
    if args.state.stat().st_mode & 0o077: raise ProbeError('state directory is not private')
    return run_case(args, h, prior)


if __name__ == '__main__':
    raise SystemExit(main())
