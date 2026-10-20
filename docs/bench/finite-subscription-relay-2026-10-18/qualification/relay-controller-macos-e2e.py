"""Fixed offline Mac host to Linux Runtime qualification. No live sender or auth."""
import argparse,hashlib,importlib.util,json,os,pathlib,queue,re,socket,subprocess,threading,time

ROOT=pathlib.Path(__file__).resolve().parent
CAPSULE=ROOT.parent/'controller'
LIMIT=300000

def strict(raw):
    def unique(pairs):
        value={}
        for key,item in pairs:
            if key in value:raise ValueError('duplicate_key')
            value[key]=item
        return value
    def invalid(_):raise ValueError('invalid_constant')
    return json.loads(raw,object_pairs_hook=unique,parse_constant=invalid)

class ControlParser:
    def __init__(self):self.stage=0;self.log_lines=0;self.frames=0;self.bytes=0;self.digest=hashlib.sha256()
    def accept(self,line):
        if len(line)>LIMIT:raise ValueError('line_bound')
        for prefix,kind,stage in [(b'AR_RELAY_PREPARE ', 'prepare',0),(b'AR_RELAY_BINDING ','binding',1),(b'AR_RELAY_V1 ','frame',2)]:
            if not line.startswith(prefix):continue
            if self.stage!=stage:raise ValueError('protocol_order')
            value=strict(line[len(prefix):])
            if not isinstance(value,dict):raise ValueError('object_required')
            if kind!='frame':self.stage+=1
            else:
                self.frames+=1;self.bytes+=len(line);self.digest.update(line)
                if self.bytes>16*1024*1024:raise ValueError('qualification_stream_bound')
            return kind,value
        self.log_lines+=1
        return None

def public_descriptor(root):
    return {'qualification_only':True,'assignment_id':'t02-Lx-r1-H0','model':'gpt-6-luna','effort':'xhigh',
            'marker':'R2-LOCAL-TOOLS-V1','output_file':'lib.rs','output_content':'',
            'account_sha256':'b'*64,'budget_microcredits':200000000,'evidence_directory':str(root)}

def relocate_descriptor(remote,root):
    expected=public_descriptor(root)
    if not isinstance(remote,dict) or set(remote)!=set(expected):raise ValueError('public_descriptor_keys')
    if any(remote[k]!=v for k,v in expected.items() if k not in ('output_content','evidence_directory')):raise ValueError('fixed_qualification_only')
    if not isinstance(remote['output_content'],str) or len(remote['output_content'].encode())>65536:raise ValueError('public_source_bound')
    return dict(remote,evidence_directory=str(root))

def load_host():
    spec=importlib.util.spec_from_file_location('cross_host_qualification',CAPSULE/'relay-controller-session-driver.py')
    value=importlib.util.module_from_spec(spec);spec.loader.exec_module(value);return value

def resolve_argv(argv,configuration_sha,controller_sha):
    resolved=[s.replace('@CONFIG_SHA@',configuration_sha).replace('@CONTROLLER_SHA@',controller_sha) for s in argv]
    if any(re.search(r'@[A-Z_]+@',s) for s in resolved):raise ValueError('unresolved_recipe')
    return resolved

def run(recipe_path,output):
    host=load_host();output=pathlib.Path(output);output.mkdir(mode=0o700)
    descriptor=output/'descriptor.json';host.save_new(descriptor,public_descriptor(output))
    prepared=host.prepare(descriptor)
    recipe=strict(host.read_public(pathlib.Path(recipe_path)))
    if recipe.get('ready') is not True:raise ValueError('recipe_not_ready')
    argv=recipe['argv']
    if not isinstance(argv,list) or not argv or pathlib.Path(argv[0]).name!='docker' or argv[1]!='exec' or '-i' not in argv:raise ValueError('fixed_docker_exec_required')
    argv=resolve_argv(argv,prepared['configuration_sha256'],prepared['controller_sha256'])
    # Only a dedicated, previously pinned offline container is accepted.
    container=recipe.get('container_id',recipe.get('container'))
    if not isinstance(container,str) or container not in argv:raise ValueError('container_pin_required')
    inspection=strict(subprocess.check_output(['docker','inspect',container],timeout=10))[0]
    if inspection['Id']!=container or inspection['Config']['Labels'].get('agent-router.task')!='approved-subscription-offline-20261018':raise ValueError('container_pin_changed')
    left,right=socket.socketpair();left.settimeout(310);right.settimeout(310)
    process=subprocess.Popen(argv,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,bufsize=0)
    controls=queue.Queue(maxsize=3);errors=[];parser=ControlParser();stderr_hash=hashlib.sha256();stderr_bytes=[0]
    def receive():
        try:
            while True:
                line=process.stdout.readline(LIMIT+1)
                if not line:break
                item=parser.accept(line)
                if item is None:continue
                kind,value=item
                if kind=='frame':right.sendall(host.results.canonical(value)+b'\n')
                else:controls.put(item,timeout=1)
        except Exception as error:errors.append(type(error).__name__)
        finally:
            try:right.shutdown(socket.SHUT_WR)
            except OSError:pass
    def transmit():
        try:
            while True:
                part=right.recv(8192)
                if not part:break
                offset=0
                while offset<len(part):
                    written=process.stdin.write(part[offset:])
                    if not written:raise ValueError('short_write')
                    offset+=written
                process.stdin.flush()
        except Exception as error:errors.append(type(error).__name__)
        finally:process.stdin.close()
    def drain_error():
        while True:
            part=process.stderr.read(8192)
            if not part:break
            stderr_hash.update(part);stderr_bytes[0]+=len(part)
    threads=[threading.Thread(target=f,daemon=True) for f in (receive,transmit,drain_error)]
    for thread in threads:thread.start()
    failure=None;summary=None;binding=None
    try:
        kind,remote=controls.get(timeout=300)
        if kind!='prepare':raise ValueError('prepare_required')
        local=relocate_descriptor(remote,output)
        descriptor.write_bytes(host.results.canonical(local)+b'\n')
        kind,binding=controls.get(timeout=300)
        if kind!='binding' or binding.get('configuration_sha256')!=prepared['configuration_sha256'] or binding.get('controller_sha256')!=prepared['controller_sha256']:raise ValueError('runtime_binding_changed')
        expected=output/'expected-binding.json';host.save_new(expected,binding)
        summary=host.run_qualification(left.fileno(),left.fileno(),descriptor,expected,borrow=True)
    except Exception as error:failure=host.safe_diagnostic(error)
    finally:
        try:left.shutdown(socket.SHUT_RDWR)
        except OSError:pass
        left.close()
        for thread in threads[1:2]:thread.join(3)
        try:code=process.wait(timeout=20)
        except subprocess.TimeoutExpired:
            process.terminate()
            try:code=process.wait(timeout=5)
            except subprocess.TimeoutExpired:process.kill();code=process.wait(timeout=5)
            errors.append('runtime_exit_unconfirmed')
        for thread in threads:thread.join(3)
        right.close()
    report={'schema_version':1,'scope':'Mac SessionController and results to Linux native Runtime; coded SSE only',
            'provider_requests':0,'model_calls_started':0,'execution_authorized':False,'synthetic_usage':True,
            'recipe_sha256':hashlib.sha256(pathlib.Path(recipe_path).read_bytes()).hexdigest(),
            'configuration_sha256':prepared['configuration_sha256'],'controller_sha256':prepared['controller_sha256'],
            'runtime_binding':binding,'runtime_exit_code':code,'safe_failure':failure,'bridge_errors':errors,
            'frames':parser.frames,'frame_bytes':parser.bytes,'frame_sha256':parser.digest.hexdigest(),
            'discarded_log_lines':parser.log_lines,'stderr_bytes':stderr_bytes[0],'stderr_sha256':stderr_hash.hexdigest(),
            'host_summary':summary,'raw_native_payloads_saved':False}
    host.save_new(output/'cross-host-report.json',report)
    if failure or errors or code!=0 or not summary:raise ValueError('cross_host_qualification_failed')
    return report

if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('recipe');parser.add_argument('output')
    args=parser.parse_args()
    try:
        result=run(args.recipe,args.output)
        print(json.dumps({'passed':True,'provider_requests':0,'report':str(pathlib.Path(args.output)/'cross-host-report.json')}))
    except Exception as error:
        print(json.dumps({'passed':False,'safe_diagnostic':type(error).__name__}))
        raise SystemExit(1)
