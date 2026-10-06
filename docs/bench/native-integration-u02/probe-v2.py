"""Bounded Codex app-server contract probe. No router or global installation."""
from __future__ import annotations
import argparse
import importlib.util
import json
from pathlib import Path
import platform
import subprocess
import tempfile
import threading
import time
import uuid

BASE_MODEL = 'gpt-6.1-sol'
SELECTED_MODEL = 'gpt-6-astra'
SENTINEL_TOOL = 'u02_registration_canary'
OUTPUT = 'U02_SCRIPTED_OUTPUT'


def load_helpers(path):
    spec = importlib.util.spec_from_file_location('u02_existing_helpers', path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def function_names(tools):
    result = []
    for tool in tools:
        if not isinstance(tool, dict):
            continue
        if tool.get('type') == 'function' and isinstance(tool.get('name'), str):
            result.append(tool['name'])
        if isinstance(tool.get('tools'), list):
            result.extend(function_names(tool['tools']))
    return result


def request_projection(request):
    """Retain parameters and inventory, never input prompts or credentials."""
    reasoning = request.get('reasoning') or {}
    return {
        'model': request.get('model'),
        'effort': reasoning.get('effort'),
        'function_tools': function_names(request.get('tools', [])),
        'tool_types': sorted({t.get('type', '<missing>') for t in request.get('tools', []) if isinstance(t, dict)}),
    }


def verdict(case):
    expected_model = SELECTED_MODEL if case['enabled'] else BASE_MODEL
    expected_effort = 'high' if case['enabled'] else 'low'
    requests = case.get('provider_requests', [])
    events = case.get('events', [])
    thread_id, turn_id = case.get('thread_id'), case.get('turn_id')
    return bool(thread_id and turn_id and len(requests) == 1
        and requests[0]['model'] == expected_model
        and requests[0]['effort'] == expected_effort
        and (SENTINEL_TOOL in requests[0]['function_tools']) == case['enabled']
        and any(e.get('method') == 'turn/started' and e.get('thread_id') == thread_id
                and e.get('turn_id') == turn_id for e in events)
        and any(e.get('method') == 'turn/completed' and e.get('thread_id') == thread_id
                and e.get('turn_id') == turn_id and e.get('status') == 'completed' for e in events)
        and case.get('output_correlated') is True
        and case.get('invalid_effort_rejected_before_provider') is True
        and case.get('cleanup_complete') is True
        and case.get('provider_rejected_connections') == 0
        and case.get('provider_rejected_requests') == 0)


def run_case(h, binary, enabled, timeout):
    case = {'enabled': enabled, 'task_id': str(uuid.uuid4()), 'events': [],
            'cleanup_complete': False, 'output_correlated': False}
    provider = listener = client = None
    with tempfile.TemporaryDirectory(prefix='ar-u02-') as scratch:
        root = Path(scratch)
        home, workspace = root/'home', root/'workspace'
        for d in [home, workspace, home/'tmp', home/'config', home/'cache', home/'data']:
            d.mkdir(mode=0o700, parents=True, exist_ok=True)

        class Handler(h.ProviderHandler):
            def do_POST(self):
                self.close_connection = True
                if self.path != '/v1/responses':
                    self.send_error(404); return
                try:
                    length = int(self.headers.get('Content-Length', '0'))
                    if not 0 < length <= h.MAX_REQUEST_BYTES:
                        self.send_error(413); return
                    body = self.rfile.read(length)
                    if len(body) != length:
                        self.send_error(400); return
                    request = json.loads(body)
                    if not isinstance(request, dict):
                        self.send_error(400); return
                    projection = request_projection(request)
                except (ValueError, TypeError, AttributeError, UnicodeError):
                    self.send_error(400); return
                with self.server.lock:
                    if self.server.request_count >= 1:
                        self.server.rejected_requests += 1
                        self.send_error(429); return
                    self.server.request_count += 1
                    self.server.projections.append(projection)
                # Reuse the existing bounded SSE final-message fixture, no tool call.
                payload = h.response_sse(2, True).replace(h.CALLBACK_RESULT.encode(), OUTPUT.encode())
                self.send_response(200)
                self.send_header('Content-Type', 'text/event-stream')
                self.send_header('Content-Length', str(len(payload)))
                self.send_header('Connection', 'close')
                self.end_headers()
                self.wfile.write(payload)
                self.wfile.flush()

        def observe(message):
            method = message.get('method')
            p = message.get('params') or {}
            if method in ('turn/started', 'turn/completed'):
                turn = p.get('turn') or {}
                case['events'].append({'method': method, 'thread_id': p.get('threadId'),
                    'turn_id': turn.get('id'), 'status': turn.get('status')})
            if method == 'item/completed' and p.get('item', {}).get('type') == 'agentMessage':
                item = p['item']
                case['events'].append({'method': method, 'thread_id': p.get('threadId'),
                    'turn_id': p.get('turnId'), 'item_type': item.get('type'),
                    'exact_synthetic_output': item.get('text') == OUTPUT})

        def response(request_id, deadline):
            while True:
                message = client.receive(deadline)
                observe(message)
                if message.get('id') == request_id:
                    return message
                if 'id' in message and 'method' in message:
                    raise h.FixtureError('unexpected server request, no tool execution permitted')

        try:
            provider = h.LocalProvider()
            provider.RequestHandlerClass = Handler
            provider.projections = []
            listener = threading.Thread(target=provider.serve_forever, daemon=True)
            listener.start()
            port = provider.server_address[1]
            (home/'config.toml').write_text(f'''model = "{BASE_MODEL}"
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
name = "U02 local scripted fixture"
base_url = "http://127.0.0.1:{port}/v1"
wire_api = "responses"
requires_openai_auth = false
supports_websockets = false
''')
            process = subprocess.Popen([str(binary), 'app-server', '--listen', 'stdio://'],
                stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                cwd=workspace, env=h.isolated_env(binary, home), start_new_session=True)
            client = h.AppServer(process)
            deadline = time.monotonic() + timeout
            client.send({'id':1, 'method':'initialize', 'params':{
                'clientInfo':{'name':'u02-native-contract','version':'1.0.0'},
                'capabilities':{'experimentalApi':True}}})
            if 'error' in response(1, deadline):
                raise h.FixtureError('initialize rejected')
            client.send({'method':'initialized'})
            params = {'model':BASE_MODEL,'modelProvider':'local_fixture','cwd':str(workspace),
                      'ephemeral':True,'approvalPolicy':'never','sandbox':'read-only'}
            if enabled:
                params['dynamicTools'] = [{'name':SENTINEL_TOOL,'description':'Registration canary; never invoked.',
                    'inputSchema':{'type':'object','properties':{},'additionalProperties':False}}]
            client.send({'id':2,'method':'thread/start','params':params})
            start = response(2, deadline)
            if 'error' in start:
                raise h.FixtureError('thread/start rejected')
            case['thread_id'] = start['result']['thread']['id']
            case['thread_defaults'] = {k:start['result'].get(k) for k in ('model','reasoningEffort','modelProvider')}
            base = {'threadId':case['thread_id'],'clientUserMessageId':case['task_id'],
                    'input':[{'type':'text','text':'Return the fixed synthetic fixture output.'}]}
            client.send({'id':3,'method':'turn/start','params':{**base,'effort':{'invalid_type':True}}})
            rejected = response(3, deadline)
            case['invalid_effort_error_code'] = rejected.get('error',{}).get('code')
            with provider.lock:
                case['invalid_effort_rejected_before_provider'] = 'error' in rejected and provider.request_count == 0
            if not case['invalid_effort_rejected_before_provider']:
                raise h.FixtureError('invalid effort type unexpectedly accepted; stopping before another turn')
            if enabled:
                base.update(model=SELECTED_MODEL, effort='high')
            case['turn_requested'] = {'model':base.get('model'),'effort':base.get('effort'),
                                      'omission_means_inherit':not enabled}
            client.send({'id':4,'method':'turn/start','params':base})
            turn = response(4, deadline)
            if 'error' in turn:
                raise h.FixtureError('turn/start rejected')
            case['turn_id'] = turn['result']['turn']['id']
            while not any(e['method']=='turn/completed' for e in case['events']):
                message = client.receive(deadline)
                observe(message)
                if 'id' in message and 'method' in message:
                    raise h.FixtureError('unexpected server request, no tool execution permitted')
            case['output_correlated'] = any(e['method']=='item/completed'
                and e['thread_id']==case['thread_id'] and e['turn_id']==case['turn_id']
                and e['exact_synthetic_output'] for e in case['events'])
        except (OSError, h.FixtureError, KeyError, TypeError, ValueError) as exc:
            case['failure'] = type(exc).__name__ + ': ' + str(exc)[:200]
        finally:
            if client is not None:
                case['process_exit_code'] = client.stop()
            if provider is not None:
                if listener is not None and listener.ident is not None:
                    provider.shutdown()
                provider.server_close()
                if listener is not None and listener.ident is not None:
                    listener.join(timeout=h.CLEANUP_TIMEOUT_SECONDS)
                    if listener.is_alive():
                        raise h.FixtureError('listener cleanup failed')
                case['provider_requests'] = provider.projections
                case['provider_rejected_connections'] = provider.rejected_connections
                case['provider_rejected_requests'] = provider.rejected_requests
            case['cleanup_complete'] = True
    case['temporary_state_removed'] = not root.exists()
    case['success'] = verdict(case) and case['temporary_state_removed']
    return case


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--timeout', type=int, default=30)
    args = parser.parse_args()
    if not 5 <= args.timeout <= 60:
        parser.error('timeout must be between 5 and 60 seconds per case')
    helpers = Path(__file__).resolve().parent.parent/'egress-admission'/'codex_appserver_local.py'
    h = load_helpers(helpers)
    binary, verified = h.resolve_codex(None)
    version, digest = h.verify_codex(binary, verified)
    result = {'schema_version':1,'surface':'Codex CLI app-server stdio client API',
              'codex_version':version,'codex_binary_sha256':digest,
              'script_sha256':h.sha256_file(Path(__file__).resolve()),
              'helper_sha256':h.sha256_file(helpers),
              'host':platform.platform(),'provider_spend':'USD 0, scripted loopback provider',
              'synthetic_key_only':True,'os_network_sandbox_status':'NOT_APPLIED',
              'external_egress':'UNKNOWN, no packet capture or OS denial proof',
              'automatic_cli_prompt_hook':'NOT_TESTED','ar_runtime_receipt':'NOT_APPLICABLE',
              'cases':[]}
    for enabled in (True, False):
        result['cases'].append(run_case(h,binary,enabled,args.timeout))
    result['success'] = all(c['success'] for c in result['cases'])
    args.output.write_text(json.dumps(result,indent=2,sort_keys=True)+'\n')
    print(json.dumps({'success':result['success'],'output':str(args.output)}))
    return 0 if result['success'] else 1

if __name__ == '__main__':
    raise SystemExit(main())
