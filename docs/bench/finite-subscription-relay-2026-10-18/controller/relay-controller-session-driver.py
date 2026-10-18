"""Public-data-only Runtime qualification driver. No live transport entry point."""
import argparse
import ast
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import select
import stat
import sys
import tempfile

ROOT=Path(__file__).resolve().parent

def load(name,file):
    spec=importlib.util.spec_from_file_location(name,ROOT/file)
    value=importlib.util.module_from_spec(spec);spec.loader.exec_module(value);return value

session=load('qualification_host_session','relay-controller-session.py')
results=load('qualification_results','relay-controller-results.py')


def read_public(path):
    fd=os.open(path,os.O_RDONLY|os.O_NOFOLLOW)
    try:
        metadata=os.fstat(fd)
        if not stat.S_ISREG(metadata.st_mode) or metadata.st_nlink!=1 or metadata.st_uid!=os.getuid() or metadata.st_size>524288:
            raise ValueError('bounded_public_file_required')
        with os.fdopen(fd,'rb',closefd=False) as stream:return stream.read(524289)
    finally:os.close(fd)


def descriptor(path):
    path=Path(path);root=path.parent
    d=results.controller()['strict_json'](read_public(path))
    if isinstance(d,dict) and d.get('qualification_only') is True:
        keys={'qualification_only','assignment_id','model','effort','marker','output_file','output_content',
              'account_sha256','budget_microcredits','evidence_directory'}
        if (set(d)!=keys or d['assignment_id']!='t02-Lx-r1-H0' or d['model']!='gpt-6-luna'
                or d['effort']!='xhigh' or d['marker']!='R2-LOCAL-TOOLS-V1' or d['output_file']!='lib.rs'
                or not isinstance(d['output_content'],str) or len(d['output_content'].encode())>65536
                or type(d['budget_microcredits']) is not int or d['budget_microcredits']<=0
                or not results.hexhash(d['account_sha256'])):raise ValueError('offline_descriptor_required')
        root=Path(d['evidence_directory'])
        if root.is_symlink() or root.resolve()!=path.parent.resolve() or stat.S_IMODE(root.stat().st_mode)!=0o700:
            raise ValueError('private_public_fixture_directory_required')
        expected={'exit_code':0,'termination':'exited','truncated':False}
        script={'model':d['model'],'effort':d['effort'],'marker':d['marker'],'steps':[
            {'tool':'mcp__finite__read_file','arguments':{'path':'statement.md'},'expected':expected},
            {'tool':'mcp__finite__write_file','arguments':{'path':'lib.rs','content':d['output_content'],'create_new':False},'expected':expected},
            {'tool':'mcp__finite__read_file','arguments':{'path':'lib.rs'},'expected':expected}]}
        return root,{'mode':'offline_qualification','prepared':'host-prepared.json','summary':'host-summary.json',
                     'binding':'expected-binding.json','budget_microcredits':d['budget_microcredits'],
                     'account_sha256':d['account_sha256'],'script_value':script}
    keys={'mode','manifest','template','script','binding','prepared','summary','budget_microcredits','account_sha256'}
    if (not isinstance(d,dict) or set(d)!=keys or d['mode']!='offline_qualification'
            or type(d['budget_microcredits']) is not int or d['budget_microcredits']<=0
            or not results.hexhash(d['account_sha256'])):raise ValueError('offline_descriptor_required')
    for key in ('manifest','template','script','binding','prepared','summary'):
        if (not isinstance(d[key],str) or not re.fullmatch(r'[A-Za-z0-9_-]{1,100}\.json',d[key])
                or 'auth' in d[key].lower()):raise ValueError('public_leaf_required')
    if len({d[k] for k in ('manifest','template','script','binding','prepared','summary')})!=6:
        raise ValueError('distinct_public_files_required')
    return root,d


def sealed_inputs(root,d):
    if 'script_value' in d:
        value=results.controller()['strict_json'](read_public(ROOT/'relay-controller-session-fixtures.json'))
        return value['manifest_utf8'].encode(),value['template_utf8'].encode()
    return read_public(root/d['manifest']),read_public(root/d['template'])


def save_new(path,value):
    fd=os.open(path,os.O_WRONLY|os.O_CREAT|os.O_EXCL|os.O_NOFOLLOW,0o600)
    with os.fdopen(fd,'wb') as stream:
        stream.write(results.canonical(value)+b'\n');stream.flush();os.fsync(stream.fileno())


def prepare(path):
    root,d=descriptor(path)
    if (root/d['prepared']).exists():raise ValueError('already_prepared')
    manifest,template=sealed_inputs(root,d)
    config={'profile':'codex_subscription_relay','manifest_sha256':hashlib.sha256(manifest).hexdigest(),
            'template_sha256':hashlib.sha256(template).hexdigest(),'controller_sha256':results.CONTROLLER_SHA,
            'gateway_sha256':results.GATEWAY_SHA,'account_sha256':d['account_sha256'],
            'observed_campaign_budget_microcredits':d['budget_microcredits'],
            'observed_tokens_per_trial':6000,'wall_seconds_per_trial':60}
    configuration_sha=results.sha(results.canonical(config))
    directory=Path(tempfile.mkdtemp(prefix='relay-controller-results-test-'))
    with results.ResultsManager(directory,manifest,template,budget_microcredits=d['budget_microcredits'],
            configuration=config,verified_configuration_sha256=configuration_sha):pass
    prepared={'mode':'offline_qualification','configuration':config,'configuration_sha256':configuration_sha,
              'controller_sha256':session.source_sha256(),'results_directory':str(directory),
              'execution_authorized':False,'provider_requests':0}
    save_new(root/d['prepared'],prepared)
    return prepared


def coded_script(value):
    if (not isinstance(value,dict) or set(value)!={'model','effort','marker','steps'}
            or value['model']!='gpt-6-luna' or value['effort']!='xhigh' or value['marker']!='R2-LOCAL-TOOLS-V1'
            or not isinstance(value['steps'],list) or len(value['steps'])!=3):raise ValueError('coded_script_only')
    for step,tool,path in zip(value['steps'],('mcp__finite__read_file','mcp__finite__write_file','mcp__finite__read_file'),('statement.md','lib.rs','lib.rs')):
        if (not isinstance(step,dict) or step.get('tool')!=tool or not isinstance(step.get('arguments'),dict)
                or step['arguments'].get('path')!=path):raise ValueError('coded_three_tools_only')
    write=value['steps'][1]['arguments']
    if (write.get('create_new') is not False or not isinstance(write.get('content'),str)
            or len(write['content'].encode())>65536):raise ValueError('coded_write_bound')
    sys.path[:0]=[str(ROOT/'native-catalog-adapter'),str(ROOT/'qualification-inputs')]
    from native_tool_responses import ToolScript
    return ToolScript(value['steps'],model=value['model'],effort=value['effort'],marker=value['marker'])


def add_synthetic_usage(wire,*,omit=False):
    blocks=[]
    for block in wire.split(b'\n\n'):
        if not block:continue
        lines=[]
        for line in block.split(b'\n'):
            if line.startswith(b'data: '):
                event=json.loads(line[6:])
                if event.get('type')=='response.completed' and not omit:
                    event['response']['usage']={'input_tokens':400,'input_tokens_details':{'cached_tokens':100},
                        'output_tokens':200,'output_tokens_details':{'reasoning_tokens':50},'total_tokens':600}
                line=b'data: '+json.dumps(event).encode()
            lines.append(line)
        blocks.append(b'\n'.join(lines))
    return b'\n\n'.join(blocks)+b'\n\n'


def run_qualification(read_fd,write_fd,path,binding_path=None,*,borrow=False,qualification_case='normal'):
    if qualification_case not in ('normal','unknown_usage_second_response'):raise ValueError('fixed_qualification_case_only')
    root,d=descriptor(path)
    parser=results.controller()['strict_json']
    prepared=parser(read_public(root/d['prepared']))
    if (prepared.get('mode')!='offline_qualification' or prepared.get('controller_sha256')!=session.source_sha256()
            or prepared.get('execution_authorized') is not False):raise ValueError('qualification_preparation_changed')
    script=coded_script(d['script_value'] if 'script_value' in d else parser(read_public(root/d['script'])))
    binding_file=root/d['binding'] if binding_path is None else Path(binding_path)
    if binding_file.resolve().parent!=root.resolve() or 'auth' in binding_file.name.lower():raise ValueError('public_binding_required')
    binding=parser(read_public(binding_file))
    if binding.get('assignment_id')!='t02-Lx-r1-H0':raise ValueError('first_sealed_cell_only')
    calls=[]
    def send(raw,remaining):
        request=parser(raw);wire=add_synthetic_usage(script.respond(request),omit=qualification_case=='unknown_usage_second_response' and len(calls)==1)
        calls.append(hashlib.sha256(raw).hexdigest())
        return 200,'text/event-stream',iter((wire,))
    # Preparation/canaries may run before the worker; no provider work occurs here.
    if not select.select([read_fd],[],[],300)[0]:raise TimeoutError('qualification_preparation_deadline')
    manifest,template=sealed_inputs(root,d)
    with results.ResultsManager(Path(prepared['results_directory']),manifest,template,
            budget_microcredits=d['budget_microcredits'],configuration=prepared['configuration'],
            verified_configuration_sha256=prepared['configuration_sha256']) as manager:
        io=session.PrivateRuntimeIO(read_fd,write_fd,borrow=borrow)
        controller=session.SessionController(io,manager,binding,send)
        observation=controller.run()
        summary={'mode':'offline_qualification','qualification_case':qualification_case,'coded_responses':len(calls),'observed_tool_results':script.observed_tool_results,
                 'synthetic_usage':True,'provider_requests':0,'model_calls_started':0,'execution_authorized':False,
                 'request_hashes':calls,'results_directory':prepared['results_directory'],'observation':observation}
        save_new(root/d['summary'],summary)
        return summary


def safe_diagnostic(error):
    """Only source-literal error identifiers, never a raw exception or payload."""
    codes=set()
    for relative in ('relay-controller-session-driver.py','relay-controller-session.py',
                     'relay-controller-offline.py','relay-controller-results.py','subscription_gateway.py',
                     'native-catalog-adapter/native_tool_responses.py','qualification-inputs/tool_responses.py'):
        source=(ROOT/relative).read_bytes()
        if len(source)>524288:continue
        for node in ast.walk(ast.parse(source)):
            if (isinstance(node,ast.Raise) and isinstance(node.exc,ast.Call)
                    and isinstance(node.exc.func,ast.Name) and node.exc.func.id in
                    ('ValueError','OSError','TimeoutError','EOFError','RuntimeError') and node.exc.args
                    and isinstance(node.exc.args[0],ast.Constant) and isinstance(node.exc.args[0].value,str)
                    and re.fullmatch('[a-z][a-z0-9_]{0,79}',node.exc.args[0].value)):
                codes.add(node.exc.args[0].value)
    name=type(error).__name__
    if name not in ('ValueError','KeyError','OSError','TimeoutError','EOFError','TypeError','AttributeError','FileNotFoundError','PermissionError'):
        name='Exception'
    message=str(error)
    result={'error_class':name,'code':message if message in codes else 'details_redacted'}
    if isinstance(error,OSError) and type(error.errno) is int:result['os_errno']=error.errno
    return result


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode',choices=['prepare','run','run-unknown']);parser.add_argument('descriptor');parser.add_argument('binding',nargs='?')
    args=parser.parse_args()
    try:
        if args.mode=='prepare':print(json.dumps(prepare(args.descriptor)),flush=True)
        else:run_qualification(0,1,args.descriptor,args.binding,qualification_case='unknown_usage_second_response' if args.mode=='run-unknown' else 'normal')
    except Exception as error:
        print('offline_qualification_refused '+json.dumps(safe_diagnostic(error),sort_keys=True),file=sys.stderr);raise SystemExit(1)
