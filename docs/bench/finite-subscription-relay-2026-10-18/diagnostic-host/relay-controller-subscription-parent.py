"""Single real campaign parent. Production JSONL only, no coded sender or probe mode."""
import argparse,fcntl,hashlib,importlib.util,json,os,select,stat,subprocess,sys,threading,time
from pathlib import Path

ROOT=Path(__file__).resolve().parent
_spec=importlib.util.spec_from_file_location('subscription_host_entry',ROOT/'relay-controller-subscription-launcher.py')
host=importlib.util.module_from_spec(_spec);_spec.loader.exec_module(host)
PACKETS_SHA='2d4d225e528fff5ba402e7265140521f212d7b4a3443c3dd62f1db00f9e0281a'

def read_control(fd,timeout):
    deadline=time.monotonic()+timeout;raw=bytearray()
    while True:
        remaining=deadline-time.monotonic()
        if remaining<=0:raise TimeoutError('runtime_preparation_deadline')
        if not select.select([fd],[],[],min(.1,remaining))[0]:continue
        try:part=os.read(fd,1)
        except BlockingIOError:continue
        if not part:raise EOFError('runtime_preparation_eof')
        raw.extend(part)
        if len(raw)>65536:raise ValueError('runtime_control_bound')
        if part==b'\n':
            value=host.strict(bytes(raw))
            if not isinstance(value,dict):raise ValueError('runtime_control_object_required')
            return value

def write_control(fd,value,timeout=10):
    raw=host.canonical(value)+b'\n'
    if len(raw)>300000:raise ValueError('runtime_write_bound')
    deadline=time.monotonic()+timeout;position=0
    while position<len(raw):
        remaining=deadline-time.monotonic()
        if remaining<=0:raise TimeoutError('runtime_control_write_deadline')
        if not select.select([], [fd],[],min(.1,remaining))[1]:continue
        try:count=os.write(fd,raw[position:position+8192])
        except BlockingIOError:continue
        if count<=0:raise OSError('runtime_control_write')
        position+=count

def expected_prepare(p,assignment,packet):
    return {'schema_version':1,'kind':'runtime_prepare',
        **{key:assignment[key] for key in ('assignment_id','model','effort','condition','acceptance_sha256')},
        'packet_sha256':packet['packet_sha256'],'configuration_sha256':p.configuration_sha256,
        'account_sha256':p.configuration['account_sha256'],'controller_sha256':host.PINS['relay-controller-session.py'],
        'deadline_ms':60000,'observed_tokens':6000,'observed_campaign_stop_microcredits':48000000}

def validate_prepare(value,p,assignment,packet):
    if host.canonical(value)!=host.canonical(expected_prepare(p,assignment,packet)):
        raise ValueError('runtime_prepare_identity_or_limits')

def runtime_approval(binding,packet_sha256):
    return {'schema_version':1,'kind':'approve_runtime','binding':binding,'packet_sha256':packet_sha256}

def validate_binding_envelope(envelope,p,assignment):
    if (not isinstance(envelope,dict) or set(envelope)!={'schema_version','kind','binding'}
            or type(envelope['schema_version']) is not int or envelope['schema_version']!=1
            or envelope['kind']!='runtime_binding'):raise ValueError('runtime_binding_envelope')
    binding=envelope['binding'];host.validate_binding(p,assignment,binding);return binding

def check_recipe(p,recipe_path):
    raw=host.read_file(recipe_path,private=True,maximum=4*1024*1024)
    if host.sha(raw)!=p.spec['runtime_recipe_sha256']:raise ValueError('reviewed_runtime_recipe_changed')
    recipe=host.strict(raw)
    keys={'schema_version','kind','ready','container_id','runtime_executable_path','runtime_executable_sha256','runtime_source_manifest_sha256','packet_manifest','browser_email_sha256','cells'}
    if (not isinstance(recipe,dict) or set(recipe)!=keys or type(recipe['schema_version']) is not int or recipe['schema_version']!=1
            or recipe['kind']!='real_subscription_campaign24' or recipe['ready'] is not True
            or not host.hexhash(recipe['container_id']) or not isinstance(recipe['runtime_executable_path'],str)
            or not recipe['runtime_executable_path'].startswith(('/task/','/work/source/target/'))
            or '..' in Path(recipe['runtime_executable_path']).parts
            or not host.hexhash(recipe['runtime_executable_sha256']) or not host.hexhash(recipe['runtime_source_manifest_sha256'])
            or not host.hexhash(recipe['browser_email_sha256']) or not isinstance(recipe['cells'],list) or len(recipe['cells'])!=24):
        raise ValueError('real_runtime_recipe_required')
    packet_bytes=host.read_file(recipe['packet_manifest'])
    if host.sha(packet_bytes)!=PACKETS_SHA:raise ValueError('sealed_public_packet_manifest_changed')
    packets=host.strict(packet_bytes)['assignments']
    if len(packets)!=24:raise ValueError('24_public_packets_required')
    common_argv=None
    for assignment,cell,packet in zip(p.assignments,recipe['cells'],packets):
        if (not isinstance(cell,dict) or set(cell)!={'assignment_id','argv','request'}
                or cell['assignment_id']!=assignment['assignment_id'] or packet['assignment_id']!=assignment['assignment_id']
                or any(packet[k]!=assignment[k] for k in ('model','effort','condition','acceptance_sha256'))):
            raise ValueError('runtime_recipe_cell_order')
        argv=cell['argv']
        if (not isinstance(argv,list) or len(argv)<4 or any(not isinstance(a,str) or not a or len(a)>65536 or '\x00' in a for a in argv)
                or Path(argv[0]).name!='docker' or argv[1]!='exec' or '-i' not in argv
                or recipe['container_id'] not in argv or recipe['runtime_executable_path'] not in argv
                or any(a in ('--ignored','--nocapture') or 'run_qualification' in a or '/deps/router_runtime-' in a for a in argv)):
            raise ValueError('production_owned_docker_exec_required')
        if common_argv is not None and common_argv!=argv:raise ValueError('one_fixed_runtime_program_required')
        common_argv=argv
        request=cell['request'];expected={'schema_version':1,'assignment_id':assignment['assignment_id'],
            'configuration_sha256':p.configuration_sha256,'account_sha256':p.configuration['account_sha256'],
            'controller_sha256':host.PINS['relay-controller-session.py'],'observed_campaign_stop_microcredits':48000000,
            'deadline_ms':60000,'observed_tokens':6000}
        if (not isinstance(request,dict) or set(request)!=set(expected)|{'root','programs'}
                or any(host.canonical(request[k])!=host.canonical(v) for k,v in expected.items())
                or request['root']!='/task/campaign/'+assignment['assignment_id']
                or not isinstance(request['programs'],dict) or len(request['programs'])!=6):
            raise ValueError('runtime_launch_request_identity')
    return recipe,packets

def verify_runtime_binary(recipe):
    result=subprocess.run(['docker','exec',recipe['container_id'],'sha256sum',recipe['runtime_executable_path']],
        stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,check=True,timeout=15)
    expected=recipe['runtime_executable_sha256'].encode()+b'  '+recipe['runtime_executable_path'].encode()+b'\n'
    if result.stdout!=expected:raise ValueError('runtime_executable_changed')

def save_private(path,value):
    fd=os.open(path,os.O_WRONLY|os.O_CREAT|os.O_EXCL|os.O_NOFOLLOW,0o600)
    with os.fdopen(fd,'wb') as out:out.write(host.canonical(value)+b'\n');out.flush();os.fsync(out.fileno())

def run_cell(p,approval,assignment,cell,packet,recipe):
    # The operator must have verified the exact Runtime executable/source pins in
    # the reviewed recipe. These retained process/FD objects supply provenance;
    # neither PREPARE JSON nor an arbitrary pipe is an attestation by itself.
    verify_runtime_binary(recipe)
    child=subprocess.Popen(cell['argv'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,bufsize=0)
    stderr_digest=hashlib.sha256();stderr_size=[0]
    def drain():
        while True:
            part=child.stderr.read(8192)
            if not part:break
            stderr_digest.update(part);stderr_size[0]+=len(part)
    reader=threading.Thread(target=drain,daemon=True);reader.start()
    binding=None;failure=None;observation=None;stage='initial_request'
    try:
        write_control(child.stdin.fileno(),cell['request'])
        stage='runtime_prepare';prepare=read_control(child.stdout.fileno(),300)
        stage='prepare_validation';validate_prepare(prepare,p,assignment,packet)
        stage='runtime_binding'
        envelope=read_control(child.stdout.fileno(),30)
        stage='binding_validation';binding=validate_binding_envelope(envelope,p,assignment)
        write_control(child.stdin.fileno(),runtime_approval(binding,packet['packet_sha256']))
        stage='subscription_session';observation=host.serve(p.spec_path,approval,binding,child.stdout.fileno(),child.stdin.fileno())
        stage='runtime_exit'
    except BaseException as error:failure=type(error).__name__
    finally:
        child.stdin.close()
        try:code=child.wait(timeout=20)
        except subprocess.TimeoutExpired:
            child.terminate()
            try:code=child.wait(timeout=5)
            except subprocess.TimeoutExpired:child.kill();code=child.wait(timeout=5)
            failure='RuntimeExitUnconfirmed'
        reader.join(3)
        if reader.is_alive():failure='RuntimeStderrUnclosed'
        child.stdout.close();child.stderr.close()
    if failure or code!=0 or observation is None:
        host.stop_campaign(p.spec_path,approval,'runtime_parent_failed')
        raise ValueError('real_runtime_trial_failed_stage_'+stage+'_type_'+str(failure)+'_exit_'+str(code))
    # Root exports/evaluates only the authorized captured output, and only after
    # reconciling this ledger cleanup proof with the private Runtime result.
    with p.manager.ResultsManager(p.directory,p.manifest,p.template,**host.manager_args(p)) as manager:
        row=manager.row(assignment['assignment_id']);cleanup=row[p.manager.META]['cleanup_confirmation']
        export_allowed=(row['status']=='completed' and cleanup is not None and row[p.manager.META]['native_status']=='completed'
                        and not row[p.manager.META]['usage_unknown'])
    return {'assignment_id':assignment['assignment_id'],'packet_sha256':packet['packet_sha256'],
        'runtime_root':cell['request']['root'],'runtime_result_reference':cell['request']['root']+'/runtime-result.json',
        'declared_output':packet['output'] if export_allowed else None,'output_export_allowed':export_allowed,
        'cleanup_confirmation':cleanup,'runtime_binding':binding,'observation':observation,
        'stderr_sha256':stderr_digest.hexdigest(),'stderr_bytes':stderr_size[0],
        'quality_accepted':None,'actual_subscription_debit':None}

def run_campaign(spec_path,approval,recipe_path):
    p=host.approved(spec_path,approval);recipe,packets=check_recipe(p,recipe_path)
    # Local audit only, no identity or credentials are exported in this result.
    audit=host.audit_account(p.root,p.spec['account_sha256'],recipe['browser_email_sha256'])
    if audit['same_browser_email'] is not True:raise ValueError('current_account_binding_required')
    lockfd=os.open(p.directory/'parent.lock',os.O_CREAT|os.O_RDWR|os.O_NOFOLLOW,0o600)
    try:
        meta=os.fstat(lockfd)
        if not stat.S_ISREG(meta.st_mode) or meta.st_uid!=os.getuid() or meta.st_nlink!=1 or stat.S_IMODE(meta.st_mode)!=0o600:
            raise ValueError('private_parent_lock_required')
        fcntl.flock(lockfd,fcntl.LOCK_EX|fcntl.LOCK_NB)
        while True:
            status=host.next_trial(spec_path,approval);assignment=status['next_assignment']
            if assignment is None:return status
            index=next(i for i,a in enumerate(p.assignments) if a['assignment_id']==assignment['assignment_id'])
            try:
                reference=run_cell(p,approval,assignment,recipe['cells'][index],packets[index],recipe)
                save_private(p.directory.parent/(assignment['assignment_id']+'-runtime-reference.json'),reference)
            except BaseException:
                host.stop_campaign(spec_path,approval,'runtime_parent_failed');raise
    finally:os.close(lockfd)

def main(argv=None):
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('action',choices=['check-recipe','run']);parser.add_argument('spec');parser.add_argument('recipe');parser.add_argument('--approval-digest');args=parser.parse_args(argv)
    try:
        if args.action=='check-recipe':
            p=host.prepare(args.spec);check_recipe(p,args.recipe)
            result={'runtime_recipe_valid':True,'assignments':24,'execution_authorized':False,'provider_requests':0}
        else:result=run_campaign(args.spec,args.approval_digest,args.recipe)
        sys.stdout.buffer.write(host.canonical(result)+b'\n');sys.stdout.buffer.flush();return 0
    except BaseException as error:
        sys.stderr.write('subscription parent refused: '+type(error).__name__+'\n');return 1

if __name__=='__main__':raise SystemExit(main())
