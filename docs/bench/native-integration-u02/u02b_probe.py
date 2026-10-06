"""U02b: discover an inert untrusted hook without invoking a model turn."""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import threading
import time
import probe_v5 as p

COMMAND = '''/usr/bin/printf '%s\\n' '{"decision":"block","reason":"U02B_FIXED_CANARY"}' '''.strip()


def project_hooks(response):
    entries=response.get('data',[])
    hooks=[hook for entry in entries for hook in entry.get('hooks',[])
        if hook.get('eventName') == 'userPromptSubmit']
    return {'candidate_count':len(hooks),
        'error_count':sum(len(entry.get('errors',[])) for entry in entries),
        'warning_count':sum(len(entry.get('warnings',[])) for entry in entries),
        'candidates':[{'event':hook.get('eventName'),'enabled':hook.get('enabled'),
            'trust_status':hook.get('trustStatus'),'current_hash':hook.get('currentHash'),
            'source':hook.get('source')} for hook in hooks]}


def embedded_hook_schemas(binary):
    data=binary.read_bytes()
    schemas={}
    for match in re.finditer(rb'\{\n  "\$schema"',data):
        try:
            schema,_=json.JSONDecoder().raw_decode(data[match.start():match.start()+200000].decode('utf-8','replace'))
        except ValueError:
            continue
        title=schema.get('title')
        if title in ('user-prompt-submit.command.input','user-prompt-submit.command.output'):
            schemas[title]=schema
    if len(schemas) != 2:raise ValueError('pinned embedded hook schemas unavailable')
    return schemas


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--raw-dir',type=Path,required=True)
    args=parser.parse_args()
    raw=args.raw_dir.resolve()
    if not raw.is_relative_to(Path('/private/tmp')) or raw==Path('/private/tmp') or raw.exists():
        parser.error('raw-dir must be a new private directory below /private/tmp')
    os.umask(0o077);raw.mkdir(mode=0o700)
    binary=args.binary.resolve(strict=True)
    helper=Path(__file__).resolve().parent.parent/'egress-admission'/'codex_appserver_local.py'
    h=p.load_helper(helper)
    if h.sha256_file(binary) != p.NATIVE_HASH:parser.error('native drift; inspect contract before running')
    version,digest=h.verify_codex(binary)
    if version != '0.159.2':parser.error('native version drift')
    schemas=embedded_hook_schemas(binary)
    (raw/'hook-schemas.json').write_text(json.dumps(schemas,indent=2))
    result={'schema_version':1,'codex_version':version,'codex_native_binary_sha256':digest,
        'script_sha256':h.sha256_file(Path(__file__)), 'helper_sha256':h.sha256_file(helper),
        'v5_probe_sha256':h.sha256_file(Path(p.__file__)), 'candidate_command':COMMAND,
        'hook_output_model_effort_application':'UNSUPPORTED_BY_EMBEDDED_OUTPUT_SCHEMA',
        'automatic_prompt_trigger':'NOT_TESTED_UNTRUSTED_CANDIDATE',
        'turn_start_sent':False,'trusted_hash_written':False,'global_config_changed':False,
        'os_network_sandbox_status':'NOT_APPLIED','external_egress':'UNKNOWN',
        'schema_evidence':{title:{'canonical_sha256':hashlib.sha256(json.dumps(schema,sort_keys=True,separators=(',',':')).encode()).hexdigest(),
            'properties':sorted(schema['properties']), 'additionalProperties':schema.get('additionalProperties')}
            for title,schema in schemas.items()}}
    provider=listener=client=None
    with tempfile.TemporaryDirectory(prefix='ar-u02b-',dir='/private/tmp') as scratch:
        root=Path(scratch);home=root/'home';workspace=root/'workspace'
        for d in (home,workspace,home/'tmp',home/'config',home/'cache',home/'data'):d.mkdir(mode=0o700)
        try:
            provider=h.LocalProvider();provider.RequestHandlerClass=p.handler_class(h)
            provider.projections=[];provider.mode='disabled';provider.request_limit=0;provider.raw_dir=None
            listener=threading.Thread(target=provider.serve_forever,daemon=True);listener.start()
            config=f'''model="{p.BASE_MODEL}"
model_provider="local_fixture"
approval_policy="never"
sandbox_mode="read-only"
web_search="disabled"
[features]
shell_tool=false
unified_exec=false
apps=false
[model_providers.local_fixture]
name="U02b no-inference diagnostic"
base_url="http://127.0.0.1:{provider.server_address[1]}/v1"
wire_api="responses"
requires_openai_auth=false
supports_websockets=false
request_max_retries=0
stream_max_retries=0
[[hooks.UserPromptSubmit]]
hooks=[{{type="command",command={json.dumps(COMMAND)},timeout=2}}]
'''
            (home/'config.toml').write_text(config)
            (raw/'inert-candidate-config.toml').write_text(config)
            process=subprocess.Popen([str(binary),'app-server','--listen','stdio://'],
                stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,cwd=workspace,
                env=h.isolated_env(binary,home),start_new_session=True)
            client=h.AppServer(process);deadline=time.monotonic()+10
            def response(request_id):
                while True:
                    message=client.receive(deadline)
                    if 'method' in message and 'id' in message:raise h.FixtureError('unexpected native server request')
                    if message.get('id')==request_id:
                        if 'error' in message:raise h.FixtureError('native protocol request rejected')
                        return message['result']
            client.send({'id':1,'method':'initialize','params':{
                'clientInfo':{'name':'u02b-discovery','version':'1.0.0'},'capabilities':{'experimentalApi':True}}})
            response(1);client.send({'method':'initialized'})
            client.send({'id':2,'method':'hooks/list','params':{'cwds':[str(workspace)]}})
            listed=response(2)
            (raw/'hooks-list.json').write_text(json.dumps(listed,indent=2))
            result['hook_discovery']=project_hooks(listed)
            # Discovery never implies trust. Stop here; no guessed state or turn request.
        finally:
            if client is not None:
                result['process_exit_code']=client.stop()
                result['owned_process_group_gone']=p.wait_group_gone(client.process.pid,h.CLEANUP_TIMEOUT_SECONDS,result)
                (raw/'process-group.json').write_text(json.dumps({'pgid':client.process.pid,
                    'observations':{k:v for k,v in result.items() if k.startswith('group_')}}))
            if provider is not None:
                if listener is not None:provider.shutdown()
                provider.server_close()
                if listener is not None:listener.join(h.CLEANUP_TIMEOUT_SECONDS)
                result['provider_accepted_connections']=provider.accepted_connections
                result['provider_request_count']=provider.request_count
                result['provider_threads_stopped']=not listener.is_alive() and not any(t.is_alive() for t in provider.handler_threads)
    result['temporary_state_removed']=not root.exists()
    result['diagnostic_complete']=bool(result.get('owned_process_group_gone')
        and result.get('provider_threads_stopped') and result['temporary_state_removed']
        and result['provider_accepted_connections']==0 and result['provider_request_count']==0
        and result.get('hook_discovery',{}).get('candidate_count')==1
        and result['hook_discovery']['error_count']==0 and result['hook_discovery']['warning_count']==0)
    result['u02b_complete']=False
    args.output.write_text(json.dumps(result,indent=2,sort_keys=True)+'\n')
    print(json.dumps({k:result[k] for k in ('diagnostic_complete','u02b_complete','provider_request_count','hook_discovery')}))
    return 0 if result['diagnostic_complete'] else 1

if __name__=='__main__':raise SystemExit(main())
