"""U02a v5: pinned Codex code-mode roundtrip through a scripted local provider."""
from __future__ import annotations
import argparse
import importlib.util
import json
import os
import re
import selectors
from pathlib import Path
import subprocess
import tempfile
import threading
import time

TOOL = 'u02_registration_canary'
NONCE = 'U02_CODE_MODE_NONCE'
FIXED_OUTPUT = 'U02_CODE_MODE_CALLBACK_ACCEPTED'
FINAL_OUTPUT = 'U02_CODE_MODE_ROUNDTRIP_DONE'
CALL_ID = 'call_u02_exec'
ITEM_ID = 'ctc_u02_exec'
CODE = 'text(await tools.u02_registration_canary({nonce: "U02_CODE_MODE_NONCE"}));'
BASE_MODEL = 'gpt-6.1-sol'
SELECTED_MODEL = 'gpt-6-astra'
NATIVE_HASH = '16593cc2f422d5f398a8e40f550ebbaf1245392528957be342c295920a300704'


class CallbackGate:
    """No execution: one exact native callback gets one fixed content response."""
    def __init__(self, thread_id, turn_id):
        self.thread_id, self.turn_id = thread_id, turn_id
        self.accepted = []
        self.refused = 0

    def respond(self, message):
        params = message.get('params') or {}
        request_id = message.get('id')
        valid = (not self.accepted and message.get('method') == 'item/tool/call'
            and type(request_id) in (str, int) and isinstance(params, dict)
            and params.get('threadId') == self.thread_id
            and params.get('turnId') == self.turn_id
            and isinstance(params.get('callId'), str) and 0 < len(params['callId']) < 256
            and params.get('tool') == TOOL and params.get('namespace') is None
            and params.get('arguments') == {'nonce': NONCE})
        if not valid:
            self.refused += 1
            return None
        self.accepted.append({'request_id': request_id, 'thread_id': self.thread_id,
            'turn_id': self.turn_id, 'call_id': params['callId'], 'tool': TOOL,
            'arguments_exact': True})
        return {'id': request_id, 'result': {'contentItems': [
            {'type': 'inputText', 'text': FIXED_OUTPUT}], 'success': True}}


def project(request):
    """Only bounded inventory locations, parameters and output booleans leave scratch."""
    canary, executes = [], []
    def walk(value, pointer='', depth=0):
        if depth > 32:
            raise ValueError('request nesting limit')
        if isinstance(value, str) and TOOL in value:
            canary.append(pointer)
        elif isinstance(value, dict):
            if value.get('type') == 'custom' and value.get('name') == 'exec':
                executes.append(pointer)
            for key, child in value.items():
                walk(child, pointer+'/'+str(key).replace('~','~0').replace('/','~1'), depth+1)
        elif isinstance(value, list):
            for index, child in enumerate(value):
                walk(child, pointer+'/'+str(index), depth+1)
    walk(request)
    outputs = [i for i in request.get('input', []) if isinstance(i,dict)
        and i.get('type') == 'custom_tool_call_output']
    matched = [i for i in outputs if i.get('call_id') == CALL_ID
        and isinstance(i.get('output'),list)
        and sum(c == {'type':'input_text','text':FIXED_OUTPUT} for c in i['output']) == 1]
    return {'model': request.get('model'), 'effort': (request.get('reasoning') or {}).get('effort'),
        'canary_pointers': canary, 'exec_pointers': executes,
        'custom_output_count': len(outputs), 'matching_output': len(matched) == 1}


def call_sse():
    call = {'id':ITEM_ID, 'type':'custom_tool_call', 'status':'completed',
        'call_id':CALL_ID, 'namespace':'functions', 'name':'exec', 'input':CODE}
    response = {'id':'resp_u02_call','object':'response','created_at':0,
        'model':SELECTED_MODEL, 'status':'completed','output':[call],
        'usage':{'input_tokens':1,'output_tokens':1,'total_tokens':2}}
    events = [
        {'type':'response.created', 'response':{**response,'status':'in_progress','output':[]}},
        {'type':'response.output_item.added','output_index':0,'item':{**call,'status':'in_progress','input':''}},
        {'type':'response.custom_tool_call_input.delta','item_id':ITEM_ID,'output_index':0,'delta':CODE},
        {'type':'response.custom_tool_call_input.done','item_id':ITEM_ID,'output_index':0,'input':CODE},
        {'type':'response.output_item.done','output_index':0,'item':call},
        {'type':'response.completed','response':response}]
    return ''.join('event: '+e['type']+'\ndata: '+json.dumps(e,separators=(',',':'))+'\n\n' for e in events).encode()

def verdict(case):
    mode = case['mode']
    count = 2 if mode == 'enabled' else 1
    requests = case.get('provider_requests', [])
    common = (not case.get('failure') and case.get('thread_id') and case.get('turn_id')
        and len(requests) == count and case.get('provider_request_count') == count
        and case.get('provider_accepted_connections') == count
        and case.get('provider_rejected_requests') == 0
        and case.get('provider_rejected_connections') == 0
        and all(case.get(k) is True for k in ('turn_started_correlated',
            'invalid_effort_rejected_before_provider','cleanup_complete','temporary_state_removed','owned_process_group_gone')))
    if not common:
        return False
    expected_model, expected_effort = ((BASE_MODEL,'low') if mode == 'disabled' else (SELECTED_MODEL,'high'))
    if any(r.get('model') != expected_model or r.get('effort') != expected_effort for r in requests):
        return False
    if mode == 'refused':
        return (case.get('callback_count') == 0 and case.get('callback_refused_count') == 1
            and case.get('refused_before_fixed_output') is True
            and case.get('expected_nonce_rejection_correlated') is True)
    if not all(case.get(k) is True for k in ('turn_completed_correlated','final_output_correlated')):
        return False
    if mode == 'disabled':
        return (not requests[0]['canary_pointers'] and case.get('callback_count') == 0
            and case.get('callback_refused_count') == 0 and not requests[0]['matching_output'])
    return (bool(requests[0]['canary_pointers']) and bool(requests[0]['exec_pointers'])
        and requests[1]['matching_output'] is True and case.get('callback_count') == 1
        and case.get('callback_refused_count') == 0 and case.get('callback_item_correlated') is True)



def load_helper(path):
    spec = importlib.util.spec_from_file_location('u02_v5_helper', path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


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
            with self.server.lock:
                index = self.server.request_count
            if index > self.server.request_limit:
                self.send_error(429); return
            try:
                if self.headers.get('Transfer-Encoding') or len(self.headers.get_all('Content-Length', [])) != 1:
                    self.send_error(400); return
                size = int(self.headers['Content-Length'])
                if not 0 < size <= h.MAX_REQUEST_BYTES:
                    self.send_error(413); return
                body = self.rfile.read(size)
                if len(body) != size:
                    self.send_error(400); return
                request = json.loads(body)
                if not isinstance(request,dict):
                    self.send_error(400); return
                projection = project(request)
            except (ValueError, TypeError, AttributeError, UnicodeError, RecursionError):
                self.send_error(400); return
            with self.server.lock:
                self.server.projections.append(projection)
            if self.server.raw_dir is not None:
                (self.server.raw_dir/f'provider-request-{index}.json').write_bytes(body)
            if self.server.mode != 'disabled' and index == 1:
                payload = call_sse()
                if self.server.mode == 'refused':
                    payload = payload.replace(NONCE.encode(), b'U02_UNEXPECTED_NONCE')
            else:
                payload = h.response_sse(2, True).replace(h.CALLBACK_RESULT.encode(), FINAL_OUTPUT.encode())
            self.send_response(200)
            self.send_header('Content-Type','text/event-stream')
            self.send_header('Content-Length',str(len(payload)))
            self.send_header('Connection','close')
            self.end_headers()
            self.wfile.write(payload)
            self.wfile.flush()
    return Handler


def run_case(h, binary, mode, timeout, raw_dir=None):
    case = {'mode':mode,'events':[],'cleanup_complete':False,
        'invalid_effort_rejected_before_provider':False}
    provider = listener = client = gate = None
    with tempfile.TemporaryDirectory(prefix='ar-u02-v5-',dir='/private/tmp') as scratch:
        root = Path(scratch)
        home, workspace = root/'home', root/'workspace'
        for directory in (home,workspace,home/'tmp',home/'config',home/'cache',home/'data'):
            directory.mkdir(mode=0o700,parents=True,exist_ok=True)
        raw_messages = []
        def observe(message):
            raw_messages.append(message)
            method, params = message.get('method'), message.get('params') or {}
            if method in ('turn/started','turn/completed'):
                turn=params.get('turn') or {}
                case['events'].append({'method':method,'thread_id':params.get('threadId'),
                    'turn_id':turn.get('id'),'status':turn.get('status')})
            if method in ('item/started','item/completed'):
                item=params.get('item') or {}
                case['events'].append({'method':method,'thread_id':params.get('threadId'),
                    'turn_id':params.get('turnId'),'item_type':item.get('type'),
                    'item_id':item.get('id'),'tool':item.get('tool'),
                    'exact_callback_completion': bool(gate and gate.accepted) and callback_item_matches(item,gate.accepted[0]),
                    'exact_final_output':item.get('type') == 'agentMessage' and item.get('text') == FINAL_OUTPUT})
        def response(request_id,deadline):
            while True:
                message=client.receive(deadline); observe(message)
                if 'method' in message and 'id' in message:
                    raise h.FixtureError('unexpected callback before correlated turn')
                if message.get('id') == request_id:
                    return message
        try:
            provider=h.LocalProvider()
            provider.RequestHandlerClass=handler_class(h)
            provider.projections=[]; provider.mode=mode
            provider.request_limit=2 if mode == 'enabled' else 1
            provider.raw_dir=raw_dir
            listener=threading.Thread(target=provider.serve_forever,daemon=True);listener.start()
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
name = "U02 scripted loopback"
base_url = "http://127.0.0.1:{provider.server_address[1]}/v1"
wire_api = "responses"
requires_openai_auth = false
supports_websockets = false
request_max_retries = 0
stream_max_retries = 0
''')
            process=subprocess.Popen([str(binary),'app-server','--listen','stdio://'],
                stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,
                cwd=workspace,env=h.isolated_env(binary,home),start_new_session=True)
            client=h.AppServer(process);deadline=time.monotonic()+timeout
            client.send({'id':1,'method':'initialize','params':{'clientInfo':{'name':'u02-v5','version':'5.0.0'},'capabilities':{'experimentalApi':True}}})
            if 'error' in response(1,deadline):raise h.FixtureError('initialize rejected')
            client.send({'method':'initialized'})
            params={'model':BASE_MODEL,'modelProvider':'local_fixture','cwd':str(workspace),
                'ephemeral':True,'approvalPolicy':'never','sandbox':'read-only'}
            if mode != 'disabled':
                params['dynamicTools']=[{'type':'function','name':TOOL,'description':'Return fixed synthetic canary output.',
                    'inputSchema':{'type':'object','properties':{'nonce':{'type':'string'}},'required':['nonce'],'additionalProperties':False}}]
            client.send({'id':2,'method':'thread/start','params':params})
            start=response(2,deadline)
            if 'error' in start:raise h.FixtureError('thread/start rejected')
            case['thread_id']=start['result']['thread']['id']
            case['thread_defaults']={k:start['result'].get(k) for k in ('model','reasoningEffort','modelProvider')}
            base={'threadId':case['thread_id'],'input':[{'type':'text','text':'Run the fixed local fixture.'}]}
            client.send({'id':3,'method':'turn/start','params':{**base,'effort':{'invalid':True}}})
            invalid=response(3,deadline)
            with provider.lock:
                case['invalid_effort_rejected_before_provider']=(invalid.get('error',{}).get('code') == -32600
                    and provider.accepted_connections == 0 and provider.request_count == 0)
            if not case['invalid_effort_rejected_before_provider']:
                raise h.FixtureError('negative control contacted provider or wrong error')
            if mode != 'disabled':base.update(model=SELECTED_MODEL,effort='high')
            case['turn_requested']={k:base.get(k) for k in ('model','effort')}
            client.send({'id':4,'method':'turn/start','params':base})
            start=response(4,deadline)
            if 'error' in start:raise h.FixtureError('turn/start rejected')
            case['turn_id']=start['result']['turn']['id']
            gate=CallbackGate(case['thread_id'],case['turn_id'])
            while not any(e['method'] == 'turn/completed' for e in case['events']):
                message=client.receive(deadline);observe(message)
                if 'id' in message and 'method' in message:
                    answer=gate.respond(message)
                    if answer is None:
                        client.send({'id':message['id'],'error':{'code':-32602,'message':'U02 refuses unexpected callback'}})
                        case['refused_before_fixed_output']=not gate.accepted
                        case['expected_nonce_rejection_correlated']=expected_nonce_rejection(
                            message,case['thread_id'],case['turn_id'])
                        if mode == 'refused' and case['expected_nonce_rejection_correlated']:
                            break
                        raise h.FixtureError('unexpected callback refused')
                    client.send(answer)
            def correlated(event):
                return event.get('thread_id') == case['thread_id'] and event.get('turn_id') == case['turn_id']
            case['turn_started_correlated']=any(correlated(e) and e['method']=='turn/started' for e in case['events'])
            case['turn_completed_correlated']=any(correlated(e) and e['method']=='turn/completed'
                and e.get('status')=='completed' for e in case['events'])
            case['final_output_correlated']=any(correlated(e) and e['method']=='item/completed'
                and e.get('exact_final_output') for e in case['events'])
            case['callback_item_correlated']=bool(gate.accepted) and any(correlated(e)
                and e['method']=='item/completed' and e.get('item_type')=='dynamicToolCall'
                and e.get('exact_callback_completion') is True for e in case['events'])
        except (OSError,h.FixtureError,KeyError,TypeError,ValueError) as error:
            # Exception details can contain private paths or protocol text.
            case['failure']=type(error).__name__
            if raw_dir is not None:(raw_dir/'error.txt').write_text(str(error))
        finally:
            if gate is not None:
                case['callbacks']=gate.accepted
                case['callback_count']=len(gate.accepted)
                case['callback_refused_count']=gate.refused
            if client is not None:
                case['process_exit_code']=client.stop()
                case['owned_process_group_gone']=wait_group_gone(client.process.pid,h.CLEANUP_TIMEOUT_SECONDS,case)
                if raw_dir is not None:
                    (raw_dir/'process-group.json').write_text(json.dumps({'pgid':client.process.pid,
                        'observations':{k:v for k,v in case.items() if k.startswith('group_')},
                        'owned_process_group_gone':case['owned_process_group_gone']},indent=2))
            if provider is not None:
                if listener is not None and listener.ident is not None:provider.shutdown()
                provider.server_close()
                if listener is not None and listener.ident is not None:
                    listener.join(timeout=h.CLEANUP_TIMEOUT_SECONDS)
                    if listener.is_alive():raise h.FixtureError('provider listener cleanup failed')
                case['provider_requests']=provider.projections
                for key in ('request_count','accepted_connections','rejected_requests','rejected_connections'):
                    case['provider_'+key]=getattr(provider,key)
            case['cleanup_complete']=True
            if raw_dir is not None:(raw_dir/'rpc-messages.json').write_text(json.dumps(raw_messages,indent=2))
    case['temporary_state_removed']=not root.exists()
    case['success']=verdict(case)
    return case


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary',type=Path,required=True,help='Installed native executable, not the npm launcher')
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--raw-dir',type=Path,required=True,help='New private directory below /private/tmp')
    parser.add_argument('--timeout',type=int,default=20)
    args=parser.parse_args()
    if not 5 <= args.timeout <= 60:parser.error('timeout must be 5..60 seconds per case')
    raw=args.raw_dir.resolve()
    if not raw.is_relative_to(Path('/private/tmp')) or raw == Path('/private/tmp') or raw.exists():
        parser.error('raw-dir must be a new directory below /private/tmp')
    os.umask(0o077);raw.mkdir(mode=0o700)
    helper=Path(__file__).resolve().parent.parent/'egress-admission'/'codex_appserver_local.py'
    h=load_helper(helper)
    binary=args.binary.resolve(strict=True)
    if h.sha256_file(binary) != NATIVE_HASH:parser.error('native binary drift; re-inspect before running')
    version,digest=h.verify_codex(binary)
    if version != '0.159.2':parser.error('version drift')
    result={'schema_version':5,'codex_version':version,'codex_native_binary_sha256':digest,
        'codex_code_mode_host_sha256':h.sha256_file(binary.with_name('codex-code-mode-host')),
        'script_sha256':h.sha256_file(Path(__file__).resolve()),'helper_sha256':h.sha256_file(helper),
        'surface':'Codex CLI app-server stdio','provider':'scripted loopback only','provider_spend':'USD 0',
        'real_inference':'NOT_TESTED','automatic_prompt_interception':'NOT_TESTED',
        'ar_runtime_receipt':'NOT_APPLICABLE','os_network_sandbox_status':'NOT_APPLIED',
        'external_egress':'UNKNOWN: no packet capture or OS denial proof',
        'limits':{'timeout_per_case_seconds':args.timeout,'request_bytes':h.MAX_REQUEST_BYTES,
            'protocol_line_bytes':h.MAX_LINE_BYTES,'stdout_bytes':h.MAX_OUTPUT_BYTES,
            'stderr_bytes':h.MAX_OUTPUT_BYTES,'connections_per_case':h.MAX_PROVIDER_CONNECTIONS,
            'requests_per_case':[2,1,1]},'cases':[]}
    for mode in ('enabled','refused','disabled'):
        directory=raw/mode;directory.mkdir(mode=0o700)
        result['cases'].append(run_case(h,binary,mode,args.timeout,directory))
    result['fresh_session_withdrawal']=len({c.get('thread_id') for c in result['cases']})==3 and result['cases'][-1]['success']
    result['success']=all(c['success'] for c in result['cases']) and result['fresh_session_withdrawal']
    args.output.write_text(json.dumps(result,indent=2,sort_keys=True)+'\n')
    print(json.dumps({'success':result['success'],'cases':[{k:c.get(k) for k in ('mode','success','failure','callback_count','provider_request_count')} for c in result['cases']]}))
    return 0 if result['success'] else 1


def callback_item_matches(item, callback):
    return (item.get('type') == 'dynamicToolCall' and item.get('id') == callback.get('call_id')
        and item.get('tool') == TOOL and item.get('namespace') is None
        and item.get('arguments') == {'nonce':NONCE} and item.get('status') == 'completed'
        and item.get('success') is True
        and item.get('contentItems') == [{'type':'inputText','text':FIXED_OUTPUT}])

def wait_group_gone(pid,timeout,evidence=None):
    evidence={} if evidence is None else evidence
    deadline=time.monotonic()+timeout
    while True:
        try:
            os.killpg(pid,0)
            evidence['group_signal_observation']='present'
        except ProcessLookupError:
            evidence['group_signal_observation']='absent'
            return True
        except PermissionError:
            evidence['group_signal_observation']='permission_error'
            observed=numeric_group_inventory(max(0.01,deadline-time.monotonic()))
            evidence['group_inventory_status']=observed['status']
            if observed['status'] != 'complete':return False
            evidence['group_inventory_contains_owned']=pid in observed['groups']
            return not evidence['group_inventory_contains_owned']
        if time.monotonic() >= deadline:
            return False
        time.sleep(min(0.01,max(0,deadline-time.monotonic())))


def expected_nonce_rejection(message,thread_id,turn_id):
    params=message.get('params') or {}
    return (message.get('method') == 'item/tool/call' and type(message.get('id')) in (str,int)
        and isinstance(params,dict) and params.get('threadId') == thread_id
        and params.get('turnId') == turn_id and params.get('tool') == TOOL
        and params.get('namespace') is None and isinstance(params.get('callId'),str)
        and 0 < len(params['callId']) < 256
        and params.get('arguments') == {'nonce':'U02_UNEXPECTED_NONCE'})

MAX_GROUP_INVENTORY_BYTES=256*1024


def parse_group_inventory(output,returncode):
    if returncode != 0:return {'status':'nonzero_exit'}
    if len(output)>MAX_GROUP_INVENTORY_BYTES:return {'status':'oversized'}
    if not output or not output.endswith(b'\n'):return {'status':'incomplete'}
    lines=output.splitlines(keepends=True)
    if any(re.fullmatch(rb'[ \t]*[0-9]{1,10}[ \t]*\n',line) is None for line in lines):
        return {'status':'malformed'}
    groups={int(line.strip()) for line in lines}
    if any(group>2147483647 for group in groups):return {'status':'malformed'}
    return {'status':'complete','groups':groups}


def numeric_group_inventory(timeout,command=None):
    """Read numeric PGIDs only; EOF, exit status, byte/time limits all required."""
    process=None
    selector=selectors.DefaultSelector()
    deadline=time.monotonic()+timeout
    output=bytearray()
    try:
        process=subprocess.Popen(command or ['/bin/ps','-axo','pgid='],
            stdin=subprocess.DEVNULL,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,
            start_new_session=True)
        os.set_blocking(process.stdout.fileno(),False)
        selector.register(process.stdout,selectors.EVENT_READ)
        while True:
            remaining=deadline-time.monotonic()
            if remaining<=0 or not selector.select(remaining):return {'status':'timed_out'}
            try:
                chunk=os.read(process.stdout.fileno(),min(32768,MAX_GROUP_INVENTORY_BYTES+1-len(output)))
            except BlockingIOError:
                continue
            if not chunk:break
            output.extend(chunk)
            if len(output)>MAX_GROUP_INVENTORY_BYTES:return {'status':'oversized'}
        returncode=process.wait(timeout=max(0.01,deadline-time.monotonic()))
        return parse_group_inventory(bytes(output),returncode)
    except subprocess.TimeoutExpired:
        return {'status':'timed_out'}
    except OSError:
        return {'status':'unavailable'}
    finally:
        selector.close()
        if process is not None:
            if process.poll() is None:process.kill()
            process.wait(timeout=1)
            process.stdout.close()

if __name__ == '__main__':raise SystemExit(main())
