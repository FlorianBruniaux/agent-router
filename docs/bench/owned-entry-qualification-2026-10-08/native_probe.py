"""F01r local owned-client diagnostic. No paid provider or product admission."""
from __future__ import annotations
import argparse
import hashlib
import errno
import re
import importlib.util
import io
import json
import math
import os
from pathlib import Path
import queue
import socket
import subprocess
import time
import direct_client as d

PIN = '16593cc2f422d5f398a8e40f550ebbaf1245392528957be342c295920a300704'
TASK = 'F01R_LOCAL_OWNED_ENTRY_TASK'
CASES = ('allow', 'deny_first', 'broker_absent_cancel')

def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module

def provider_matches(raw, task):
    try:
        if not isinstance(raw, bytes) or len(raw) > 1024 * 1024:
            return False
        body = d.strict_json(raw)
        users = [row for row in body['input'] if isinstance(row, dict) and row.get('role') == 'user']
        prompts = [content.get('text') for row in users for content in row.get('content', [])
                   if isinstance(content, dict)]
        return (body.get('model') == d.MODEL and body.get('reasoning', {}).get('effort') == d.EFFORT
            and users and users[-1].get('content') == [{'type': 'input_text', 'text': task}]
            and prompts.count(task) == 1)
    except (KeyError, TypeError, AttributeError, d.Refused):
        return False

def binding_handler(h, p, final_sse):
    """Reuse broker send and unit gate; validate selected body before their effect."""
    class Handler(p.handler(h, final_sse, True)):
        def do_POST(self):
            self.close_connection = True
            if (self.path != '/v1/responses' or self.headers.get('Transfer-Encoding')
                    or len(self.headers.get_all('Content-Length', [])) != 1):
                self.reply(400, b'{"error":"request refused"}'); return
            try:
                size = int(self.headers['Content-Length'])
                if not 0 < size <= p.LIMIT:
                    raise ValueError('body limit')
                raw = self.rfile.read(size)
                if len(raw) != size or not provider_matches(raw, TASK):
                    raise ValueError('selection or task binding refused')
            except (ValueError, OSError):
                self.reply(403, b'{"error":"selection or task binding refused"}'); return
            # Each handler validates its own bytes. No shared "latest body" state.
            self.rfile = io.BytesIO(raw)
            super().do_POST()
    return Handler

def one(rows):
    if len(rows) != 1:
        raise ValueError('missing or duplicate evidence')
    return rows[0]

def qualify(e):
    """Reconcile actual transcript, independent bodies, reservations and cleanup."""
    try:
        case, c = e['case'], e['client']
        if case not in CASES or e['failure'] is not None or c['failure'] is not None:
            return False
        if any(e.get(key) is not True for key in ('cleanup_complete', 'process_group_gone',
                'output_complete', 'listeners_closed', 'handlers_stopped', 'pin_unchanged')):
            return False
        group = e.get('group_cleanup')
        if (not isinstance(group, dict) or group.get('group_gone') is not True
                or group.get('deadline_reached') is not False or type(group.get('process_group_id')) is not int
                or group['process_group_id'] <= 0 or type(group.get('timeout_seconds')) is not int
                or group['timeout_seconds'] != 3):
            return False
        wait = group.get('wait_seconds')
        observations = group.get('observations')
        if (type(wait) not in (int, float) or not math.isfinite(wait) or not 0 <= wait <= 3
                or not isinstance(observations, list) or not 1 <= len(observations) <= 64):
            return False
        previous = -1
        for index, row in enumerate(observations):
            elapsed = row.get('elapsed_seconds')
            if (type(elapsed) not in (int, float) or not math.isfinite(elapsed)
                    or not previous <= elapsed <= wait or elapsed < 0
                    or row.get('state') != ('absent' if index == len(observations)-1 else 'present')):
                return False
            previous = elapsed
        if type(e.get('process_exit_code')) is not int or e['process_exit_code'] != 0:
            return False
        if type(e.get('native_launch_count')) is not int or e['native_launch_count'] != 1:
            return False
        if e.get('socket_control') != ['allowed', 1]:
            return False
        listeners = e.get('listener_checks')
        if not isinstance(listeners, list) or len(listeners) != 2:
            return False
        if {row.get('listener') for row in listeners} != {'broker', 'provider'}:
            return False
        if any(row.get('fd_closed') is not True or type(row.get('connect_errno')) is not int
               or row['connect_errno'] != errno.ECONNREFUSED for row in listeners):
            return False
        if any(c.get(key) is not False for key in ('evidence_lost', 'late_result', 'retry_or_refund')) or type(c.get('callbacks_refused')) is not int or c['callbacks_refused'] != 0:
            return False
        if c['requested'] != {'model': d.MODEL, 'effort': d.EFFORT} or c['task_sha256'] != hashlib.sha256(TASK.encode()).hexdigest():
            return False
        sent, received = c['sent'], c['received']
        if not isinstance(sent, list) or not isinstance(received, list) or len(received) > d.MAX_MESSAGES:
            return False
        if any(not isinstance(row, dict) or ('id' not in row and 'method' not in row)
                or ('method' in row and (not isinstance(row['method'], str)
                    or not isinstance(row.get('params', {}), dict) or 'result' in row or 'error' in row))
                or ('result' in row and 'error' in row) for row in received):
            return False
        if len(json.dumps(received, separators=(',', ':'), allow_nan=False).encode()) > d.MAX_TOTAL:
            return False
        replies = [row for row in received if 'id' in row]
        if any('method' in row or type(row['id']) is not int or 'error' in row or not isinstance(row.get('result'), dict) for row in replies):
            return False
        ids = [row['id'] for row in replies]
        expected_ids = [1, 2, 3, 4] if case == 'broker_absent_cancel' else [1, 2, 3]
        if ids != expected_ids:
            return False
        start = one([row for row in replies if row['id'] == 2])['result']
        thread = d.identity(start['thread']['id'])
        turn = d.identity(one([row for row in replies if row['id'] == 3])['result']['turn']['id'])
        if (c['thread_id'], c['turn_id']) != (thread, turn):
            return False
        if (start.get('model'), start.get('reasoningEffort'), start.get('modelProvider'),
            start.get('approvalPolicy'), start.get('approvalsReviewer'), start.get('sandbox')) != (
                d.MODEL, d.EFFORT, 'local_fixture', 'on-request', 'user', {'type': 'readOnly', 'networkAccess': False}):
            return False
        if not isinstance(start.get('sandbox'), dict) or start['sandbox'].get('networkAccess') is not False:
            return False
        if any(key in start['thread'] and start['thread'][key] != value for key, value in
               (('model', d.MODEL), ('reasoningEffort', d.EFFORT), ('modelProvider', 'local_fixture'))):
            return False
        methods = [row['message'].get('method') for row in sent]
        if methods != ['initialize', 'initialized', 'thread/start', 'turn/start'] + (['turn/interrupt'] if case == 'broker_absent_cancel' else []):
            return False
        if [sent[i]['message'].get('id') for i in (0, 2, 3)] != [1, 2, 3]:
            return False
        ts = sent[2]['message']['params']
        if (ts.get('model'), ts.get('modelProvider'), ts.get('ephemeral'), ts.get('approvalPolicy'),
            ts.get('approvalsReviewer'), ts.get('sandbox')) != (d.MODEL, 'local_fixture', True, 'on-request', 'user', 'read-only'):
            return False
        workspace = ts.get('cwd')
        if (not isinstance(workspace, str) or not workspace or ts.get('ephemeral') is not True
                or start.get('cwd') != workspace or start.get('runtimeWorkspaceRoots') != [workspace]
                or start['thread'].get('ephemeral') is not True
                or 'cwd' in start['thread'] and start['thread']['cwd'] != workspace):
            return False
        expected_turn = {'threadId': thread, 'input': [{'type': 'text', 'text': TASK}], 'model': d.MODEL, 'effort': d.EFFORT}
        if sent[3]['message']['params'] != expected_turn:
            return False
        reply_positions = {row['id']: received.index(row) for row in replies}
        boundaries = [row['after_received'] for row in sent]
        if any(type(value) is not int or not 0 <= value <= len(received) for value in boundaries):
            return False
        if boundaries[:4] != [0, reply_positions[1]+1, reply_positions[1]+1, reply_positions[2]+1]:
            return False
        for send_index, identifier in [(0, 1), (2, 2), (3, 3)] + ([(4, 4)] if case == 'broker_absent_cancel' else []):
            if boundaries[send_index] > reply_positions[identifier]:
                return False
        end = one([row for row in received if row.get('method') == 'turn/completed'])
        status = {'allow': 'completed', 'deny_first': 'failed', 'broker_absent_cancel': 'interrupted'}[case]
        if (end['params'].get('threadId'), end['params']['turn'].get('id'), end['params']['turn'].get('status'), c['terminal']) != (thread, turn, status, status):
            return False
        if received.index(end) <= received.index(one([row for row in replies if row['id'] == 3])):
            return False
        for row in received:
            if row.get('method') == 'model/rerouted':
                return False
            if row.get('method') in ('item/started', 'item/completed', 'error', 'turn/started'):
                params = row['params']
                observed_turn = params.get('turnId', params.get('turn', {}).get('id') if isinstance(params.get('turn'), dict) else None)
                if params.get('threadId') != thread or observed_turn != turn:
                    return False
                if row.get('method') != 'error' and received.index(row) > received.index(end):
                    return False
        if case == 'broker_absent_cancel':
            interrupt = sent[4]
            if interrupt['message'] != {'id': 4, 'method': 'turn/interrupt', 'params': {'threadId': thread, 'turnId': turn}}:
                return False
            boundary = interrupt['after_received']
            if type(boundary) is not int or not 1 <= boundary <= len(received):
                return False
            wait = received[boundary - 1]
            if wait.get('method') != 'error' or wait['params'].get('willRetry') is not True:
                return False
            ack = one([row for row in replies if row['id'] == 4])
            if received.index(ack) < boundary or received.index(end) < boundary or received.index(ack) >= received.index(end):
                return False
            if c.get('interrupt_sent') is not True or c.get('interrupt_ack') is not True:
                return False
        elif c.get('interrupt_sent') is not False or c.get('interrupt_ack') is not False:
            return False
        bcount, pcount = {'allow': (1, 1), 'deny_first': (1, 0), 'broker_absent_cancel': (0, 0)}[case]
        for key, value in [('broker_http_requests', bcount), ('provider_http_requests', pcount),
                           ('reserved', pcount), ('broker_rejected_connections', 0), ('provider_rejected_connections', 0)]:
            if type(e.get(key)) is not int or e[key] != value:
                return False
        expected_decisions = ([{'ordinal': 1, 'allowed': case == 'allow', 'reason': 'allow' if case == 'allow' else 'ceiling'}] if bcount else [])
        if any(type(row.get('allowed')) is not bool or type(row.get('ordinal')) is not int for row in e['decisions']):
            return False
        if e['decisions'] != expected_decisions or len(e['provider_requests']) != pcount or len(e['broker_body_hashes']) != bcount:
            return False
        hashes = [hashlib.sha256(raw.encode()).hexdigest() for raw in e['provider_requests']]
        if hashes != e['provider_body_hashes'] or hashes != e['broker_body_hashes'][:pcount]:
            return False
        return all(provider_matches(raw.encode(), TASK) for raw in e['provider_requests'])
    except (KeyError, TypeError, ValueError, AttributeError, IndexError):
        return False

def public_projection(e):
    c = e.get('client', {})
    observed = 'UNKNOWN'
    try:
        if len(e.get('provider_requests', [])) == 1:
            raw = e['provider_requests'][0].encode()
            body = d.strict_json(raw)
            observed = {'model': body.get('model') if body.get('model') == d.MODEL else 'UNADMITTED',
                        'effort': body.get('reasoning', {}).get('effort') if body.get('reasoning', {}).get('effort') == d.EFFORT else 'UNADMITTED',
                        'task_digest_matches': bool(provider_matches(raw, TASK))}
    except (d.Refused, AttributeError, TypeError):
        observed = 'UNKNOWN'
    # Explicit field projection, never serializing native IDs, paths, task or RPCs.
    requested = c.get('requested')
    requested = ({'model': requested.get('model') if requested.get('model') == d.MODEL else 'UNADMITTED',
                  'effort': requested.get('effort') if requested.get('effort') == d.EFFORT else 'UNADMITTED'}
                 if isinstance(requested, dict) else 'UNKNOWN')
    digest = c.get('task_sha256')
    digest = digest if isinstance(digest, str) and re.fullmatch('[0-9a-f]{64}', digest) else 'UNKNOWN'
    def count(value, maximum=2):
        return value if type(value) is int and 0 <= value <= maximum else 'UNKNOWN'
    return {'case': e.get('case') if e.get('case') in CASES else 'UNKNOWN', 'evidence_kind': 'local_native_owned_client_broker_diagnostic',
        'requested': requested, 'task_sha256': digest,
        'observed_provider': observed, 'terminal': c.get('terminal') if c.get('terminal') in ('completed', 'failed', 'interrupted') else 'UNKNOWN',
        'native_launch_count': count(e.get('native_launch_count'), 1),
        'broker_http_requests': count(e.get('broker_http_requests')), 'provider_http_requests': count(e.get('provider_http_requests')),
        'reserved_local_request_units': count(e.get('reserved')), 'cleanup_complete': e.get('cleanup_complete') is True,
        'evidence_lost': c.get('evidence_lost') is True, 'late_result': c.get('late_result') is True,
        'checks_pass': qualify(e), 'os_admission': 'UNKNOWN', 'product_integration': 'UNKNOWN',
        'real_provider_inference': 'UNKNOWN', 'paid_provider_calls': 0}

class NativeTransport:
    """Reuse bounded stdout reader; preserve raw JSON until strict client parsing."""
    def __init__(self, app):
        self.app = app
    def send(self, row):
        self.app.send(row)
    def receive(self, deadline):
        if self.app.exceeded.is_set():
            raise d.Refused('native output overflow')
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise TimeoutError('native deadline')
        try:
            raw = self.app.lines.get(timeout=remaining)
        except queue.Empty as error:
            raise TimeoutError('native deadline') from error
        if raw is None:
            raise d.Refused('native EOF or overflow')
        return raw

def config(port):
    if type(port) is not int or not 1 <= port <= 65535:
        raise ValueError('invalid broker port')
    return f'''model = "{d.MODEL}"
model_provider = "local_fixture"
model_reasoning_effort = "{d.EFFORT}"
approval_policy = "on-request"
approvals_reviewer = "user"
sandbox_mode = "read-only"
web_search = "disabled"
[features]
shell_tool = false
unified_exec = false
apps = false
[model_providers.local_fixture]
name = "F01r loopback diagnostic broker"
base_url = "http://127.0.0.1:{port}/v1"
wire_api = "responses"
requires_openai_auth = false
supports_websockets = false
request_max_retries = 1
stream_max_retries = 0
'''

def observe_group_cleanup(pid):
    """Observe, never signal beyond signal0. Late/unknown absence cannot grant."""
    if type(pid) is not int or pid <= 0:
        raise ValueError('invalid supervised process group')
    started = time.monotonic()
    result = {'process_group_id': pid, 'group_gone': False, 'timeout_seconds': 3,
              'wait_seconds': 0.0, 'deadline_reached': False, 'observations': []}
    for _ in range(64):
        elapsed = max(0.0, time.monotonic()-started)
        if elapsed >= 3:
            result['deadline_reached'] = True
            break
        try:
            os.killpg(pid, 0)
            state = 'present'
        except ProcessLookupError:
            state = 'absent'
        except PermissionError:
            state = 'permission_denied'
        except OSError:
            state = 'os_error'
        elapsed = max(0.0, time.monotonic()-started)
        result['observations'].append({'elapsed_seconds': elapsed, 'state': state})
        if state == 'absent':
            result['group_gone'] = elapsed <= 3
            result['deadline_reached'] = elapsed > 3
            break
        if state != 'present':
            break
        remaining = 3-elapsed
        if remaining <= 0:
            result['deadline_reached'] = True
            break
        time.sleep(min(.05, remaining))
    result['wait_seconds'] = max(0.0, time.monotonic()-started)
    return result

def run_case(h, p, prior, binary, case, raw):
    raw.mkdir(mode=0o700)
    home, workspace = raw/'home', raw/'workspace'
    for path in (home, workspace, home/'tmp', home/'config', home/'cache', home/'data'):
        path.mkdir(mode=0o700, parents=True, exist_ok=True)
    e = {'case': case, 'failure': None, 'native_launch_count': 0, 'cleanup_complete': False}
    provider = broker = app = client = None
    servers = []
    try:
        provider, pt = p.start_server(h, p.handler(h, prior.final_sse, False), raw/'provider')
        provider.retry = False
        servers.append((provider, pt, 'provider'))
        broker, bt = p.start_server(h, binding_handler(h, p, prior.final_sse), raw/'broker')
        broker.gate, broker.upstream_port = p.Gate(0 if case == 'deny_first' else 1), provider.server_address[1]
        servers.append((broker, bt, 'broker'))
        policy = p.profile(broker.server_address[1])
        (raw/'controller.sb').write_text(policy)
        control_script = 'import socket,sys,json; out=[]\nfor p in sys.argv[1:]:\n s=socket.socket(); s.settimeout(1)\n try: s.connect(("127.0.0.1",int(p))); out.append("allowed")\n except OSError as e: out.append(e.errno)\n finally: s.close()\nprint(json.dumps(out))'
        control = prior.bounded_command(['/usr/bin/sandbox-exec', '-p', policy, '/usr/bin/python3', '-c', control_script,
            str(broker.server_address[1]), str(provider.server_address[1])], workspace, h.isolated_env(binary, home), timeout=5)
        e['socket_control'] = json.loads(control)
        if e['socket_control'] != ['allowed', 1]:
            raise d.Refused('socket control failed')
        if case == 'broker_absent_cancel':
            broker.shutdown(); broker.server_close(); bt.join(timeout=3)
        (home/'config.toml').write_text(config(broker.server_address[1]))
        process = subprocess.Popen(['/usr/bin/sandbox-exec', '-p', policy, str(binary), 'app-server', '--listen', 'stdio://'],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            cwd=workspace, env=h.isolated_env(binary, home), start_new_session=True)
        e['native_launch_count'] += 1
        app = h.AppServer(process)
        client = d.Client(NativeTransport(app), workspace)
        client.execute(TASK, d.MODEL, d.EFFORT, cancel_on_wait=case == 'broker_absent_cancel')
    except (OSError, ValueError, KeyError, h.FixtureError, prior.FixtureError) as error:
        e['failure'] = type(error).__name__
        (raw/'error.txt').write_text(str(error))
    finally:
        cleanup = True
        if app:
            try:
                e['process_exit_code'] = app.stop()
                e['group_cleanup'] = observe_group_cleanup(app.process.pid)
                e['process_group_gone'] = e['group_cleanup']['group_gone']
                eof = False
                while True:
                    try:
                        line = app.lines.get_nowait()
                    except queue.Empty:
                        break
                    if line is None:
                        eof = True
                    elif client:
                        try:
                            client.observe(line)
                        except d.Refused:
                            e['failure'] = 'post_terminal_protocol_refused'
                e['output_complete'] = eof and not app.exceeded.is_set()
                if app.exceeded.is_set() and client:
                    client.evidence['evidence_lost'] = True
                    client.evidence['terminal'] = 'UNKNOWN'
                cleanup &= e['process_group_gone'] and e['output_complete']
            except (OSError, ValueError, h.FixtureError):
                cleanup = False
        else:
            cleanup = False
        handlers, listeners = True, True
        checks = []
        for server, thread, label in reversed(servers):
            address = server.server_address
            try:
                server.shutdown(); server.server_close(); thread.join(timeout=3)
                stopped = not thread.is_alive() and not any(worker.is_alive() for worker in server.handler_threads)
                handlers &= stopped
                fd_closed = server.socket.fileno() == -1
                s = socket.socket(); s.settimeout(.5)
                try:
                    s.connect(address); connect_errno = 0
                except OSError as error:
                    connect_errno = error.errno
                finally:
                    s.close()
                closed = fd_closed and connect_errno == errno.ECONNREFUSED
                listeners &= closed
                checks.append({'listener': label, 'fd_closed': fd_closed, 'connect_errno': connect_errno})
            except (OSError, RuntimeError, h.FixtureError):
                handlers = listeners = False
            e[label+'_http_requests'] = server.request_count
            e[label+'_body_hashes'] = list(server.bodies)
            e[label+'_rejected_connections'] = server.rejected_connections
        e['listener_checks'], e['handlers_stopped'], e['listeners_closed'] = checks, bool(handlers), bool(listeners)
        e['reserved'] = broker.gate.reserved if broker else None
        e['decisions'] = broker.gate.decisions if broker else []
        e['provider_requests'] = [path.read_text() for path in sorted((raw/'provider').glob('request-*.json'))] if provider else []
        e['client'] = client.evidence if client else {}
        e['pin_unchanged'] = h.sha256_file(binary) == PIN
        e['cleanup_complete'] = bool(cleanup and handlers and listeners)
        (raw/'evidence.json').write_text(json.dumps(e, indent=2, sort_keys=True))
    return e

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--state', type=Path, required=True)
    parser.add_argument('--case', choices=CASES)
    args = parser.parse_args()
    os.umask(0o077)
    repo = Path(__file__).resolve().parents[3]
    h = load('f01r_helpers', repo/'docs/bench/egress-admission/codex_appserver_local.py')
    p = load('f01r_broker', repo/'docs/bench/physical-send-feasibility-2026-10-07/probe.py')
    prior = load('f01r_prior', repo/'docs/bench/native-integration-u06/probe.py')
    binary = args.binary.resolve(strict=True)
    if h.sha256_file(binary) != PIN:
        raise ValueError('native binary pin mismatch')
    if not binary.is_file() or not os.access(binary, os.X_OK):
        raise ValueError('native binary not executable')
    args.state.mkdir(mode=0o700, parents=False, exist_ok=False)
    selected = [args.case] if args.case else CASES
    cases = [public_projection(run_case(h, p, prior, binary, case, args.state/case)) for case in selected]
    report = {'schema_version': 1, 'native_sha256': PIN, 'native_version_for_pin': '0.159.2',
        'evidence_kind': 'local_native_owned_client_broker_diagnostic',
        'budget_unit': 'one reserved local upstream HTTP request', 'paid_provider_calls': 0,
        'global_admission': 'NO_GO', 'os_admission': 'UNKNOWN', 'product_integration': 'UNKNOWN',
        'cases': cases, 'local_qualification': 'GO' if all(row['checks_pass'] for row in cases) else 'NO_GO'}
    (args.state/'report.json').write_text(json.dumps(report, indent=2, sort_keys=True)+'\n')
    print(json.dumps(report, indent=2, sort_keys=True))
    return 0 if report['local_qualification'] == 'GO' else 2

if __name__ == '__main__':
    raise SystemExit(main())
