"""Exact 24-cell subscription host entry. No API billing, purchase or fixture sender."""
import argparse,base64,copy,hashlib,json,os,re,stat,sys,types
from pathlib import Path

BUDGET=48000000
TRANSPORT_DIAGNOSTICS_SHA256='b4e6bfc6bc140a524135daf9c43d6dc7f26050825cfd2563d2faf8d184a388e1'
CONSENT_DOCUMENT_SHA256='46f9d3e88855da9314d1a803d78a4c02af7d38bc7ea6159adc90896e89035fa3'
PINS={
 'relay-controller-results-v2.py':'3781df3c0a1c64b717dd46cdbf4dcedf25bcfbbd2251b4f1602db6bfea3e408a',
 'relay-controller-session.py':'9c5809752b2f2940c62fbdaaa4cadad4a4649a0fb1b8581b0caf4e7561081cc5',
 'subscription_gateway.py':'97f2c8bdc55f067848f12cffb5a41086aaf5adec3ed8cb36567f6540b2975f5e',
 'relay-controller-offline.py':'c2943ce88c95b7e9aeefaf5ea827b7e9b2b0780737a84b8fe9c2bd5bbd945828',
 'native-catalog-adapter/native_tool_responses.py':'6e2268fca17bb2bf0f241cf959fe229cf50c19ea7f44b85b67de359c24b80ffe',
 'qualification-inputs/tool_responses.py':'7a2db0fbadfaf95c72029a234d289dfa8dc124170bb1afc4dd590963efdaf755',
 'native-catalog-adapter/expected-native-catalog.json':'9550e472a5b090c76b35bc1db5e2e5a30d6d6d3cc41b38da826acb790713182f'}

def canonical(value):return json.dumps(value,sort_keys=True,separators=(',',':'),ensure_ascii=False,allow_nan=False).encode()
def sha(value):return hashlib.sha256(value).hexdigest()
def hexhash(value):return isinstance(value,str) and re.fullmatch('[0-9a-f]{64}',value) is not None
def strict(raw):
    def pairs(items):
        result={}
        for key,value in items:
            if key in result:raise ValueError('duplicate_key')
            result[key]=value
        return result
    def invalid(_):raise ValueError('nonfinite_json')
    return json.loads(raw,object_pairs_hook=pairs,parse_constant=invalid)

def read_file(path,*,private=False,maximum=4*1024*1024):
    path=Path(path)
    fd=os.open(path,os.O_RDONLY|os.O_NOFOLLOW)
    with os.fdopen(fd,'rb') as source:
        meta=os.fstat(source.fileno())
        if (not stat.S_ISREG(meta.st_mode) or meta.st_uid!=os.getuid() or meta.st_nlink!=1
                or meta.st_size>maximum or private and stat.S_IMODE(meta.st_mode)!=0o600):
            raise ValueError('owned_bounded_file_required')
        value=source.read(maximum+1)
    if len(value)>maximum:raise ValueError('file_bound')
    return value

def verified_sources(root):
    root=Path(root)
    if not root.is_absolute() or root.is_symlink() or not root.is_dir():raise ValueError('qualified_controller_root_required')
    sources={}
    for name,digest in PINS.items():
        value=read_file(root/name)
        if sha(value)!=digest:raise ValueError('qualified_source_changed')
        sources[name]=value
    return root,sources

def load_manager(root,sources):
    path=root/'relay-controller-results-v2.py';namespace={'__name__':'pinned_live_results','__file__':str(path)}
    exec(compile(sources[path.name],str(path),'exec'),namespace)
    return types.SimpleNamespace(**namespace)

def diagnostic_source():
    raw=read_file(Path(__file__).with_name('subscription-transport-diagnostics.py'))
    if sha(raw)!=TRANSPORT_DIAGNOSTICS_SHA256:raise ValueError('transport_diagnostic_source_changed')
    return raw

def load_diagnostics():
    path=Path(__file__).with_name('subscription-transport-diagnostics.py')
    namespace={'__name__':'reviewed_transport_diagnostics','__file__':str(path)}
    exec(compile(diagnostic_source(),str(path),'exec'),namespace)
    return types.SimpleNamespace(**namespace)

def prepare(spec_path):
    diagnostic_source()
    spec_path=Path(spec_path);raw=read_file(spec_path,private=True,maximum=65536);value=strict(raw)
    keys={'schema_version','controller_root','results_directory','account_sha256','budget_microcredits','human_authorization','runtime_recipe_sha256'}
    if (not isinstance(value,dict) or set(value)!=keys or type(value['schema_version']) is not int or value['schema_version']!=1
            or type(value['budget_microcredits']) is not int or value['budget_microcredits']!=BUDGET
            or not hexhash(value['account_sha256']) or not hexhash(value['runtime_recipe_sha256'])):raise ValueError('exact_24_budget_spec_required')
    if sha(read_file(value['human_authorization'],private=True,maximum=65536))!=CONSENT_DOCUMENT_SHA256:
        raise ValueError('current_human_authorization_required')
    root,sources=verified_sources(value['controller_root']);manager=load_manager(root,sources)
    manifest=read_file(root/'sealed-inputs/manifest-24-preparation.json');template=read_file(root/'sealed-inputs/results.template.json')
    if sha(manifest)!=manager.MANIFEST_SHA or sha(template)!=manager.TEMPLATE_SHA:raise ValueError('sealed_matrix_changed')
    assignments=strict(manifest)['assignments'];manager.controller()['budget_check'](assignments)
    configuration={'profile':'codex_subscription_relay','manifest_sha256':manager.MANIFEST_SHA,'template_sha256':manager.TEMPLATE_SHA,
        'controller_sha256':manager.CONTROLLER_SHA,'gateway_sha256':manager.GATEWAY_SHA,'account_sha256':value['account_sha256'],
        'observed_campaign_budget_microcredits':BUDGET,'observed_tokens_per_trial':6000,'wall_seconds_per_trial':60}
    directory=Path(value['results_directory'])
    if not directory.is_absolute():raise ValueError('absolute_private_destination_required')
    inner=manager.review_launch(directory,configuration,BUDGET)
    material={'schema_version':1,'action':'run_exact_subscription_campaign_24','spec_sha256':sha(raw),
        'configuration':configuration,'configuration_sha256':sha(canonical(configuration)),
        'manager_launch_review':inner,'human_authorization_sha256':CONSENT_DOCUMENT_SHA256,
        'runtime_recipe_sha256':value['runtime_recipe_sha256'],'qualified_source_pins':PINS,
        'transport_diagnostics_sha256':TRANSPORT_DIAGNOSTICS_SHA256,
        'parent_sha256':sha(Path(__file__).read_bytes()),
        'transport_parent_sha256':sha(read_file(Path(__file__).with_name('relay-controller-subscription-parent.py')))}
    return types.SimpleNamespace(spec_path=spec_path,spec=value,manager=manager,root=root,manifest=manifest,template=template,
        directory=directory,assignments=assignments,configuration=configuration,configuration_sha256=material['configuration_sha256'],
        inner_approval_digest=inner['approval_digest'],review_material=material,approval_digest=sha(canonical(material)))

def review(spec_path):
    p=prepare(spec_path)
    return {'schema_version':1,'action':'review_only_no_approval','approval_digest':p.approval_digest,
        'review':p.review_material,'configuration':p.configuration,'configuration_sha256':p.configuration_sha256,
        'parent_sha256':p.review_material['parent_sha256'],'session_controller_sha256':PINS['relay-controller-session.py'],
        'assignments':copy.deepcopy(p.assignments),'budget_microcredits':BUDGET,'model_calls_started':0,
        'execution_authorized':False,'hard_provider_cap':False}

def approved(spec_path,digest):
    p=prepare(spec_path)
    if not hexhash(digest) or digest!=p.approval_digest:raise ValueError('exact_parent_approval_required')
    return p

def manager_args(p):
    return {'budget_microcredits':BUDGET,'configuration':p.configuration,'verified_configuration_sha256':p.configuration_sha256,
        'evidence_mode':'subscription','launch_approval_digest':p.inner_approval_digest}

def next_from_manager(manager):
    observation=manager.observation();state=manager.snapshot()
    row=next((r for r in state['rows'] if r['status']=='not_run'),None) if observation['budget_observation_allows_next'] else None
    assignment=next((a for a in manager.manifest['assignments'] if row and a['assignment_id']==row['assignment_id']),None)
    return {'schema_version':1,'next_assignment':copy.deepcopy(assignment),'observation':observation,
        'statuses':{s:sum(r['status']==s for r in state['rows']) for s in ('not_run','in_progress','completed','failed','stopped')},
        'execution_authorized':False}

def next_trial(spec_path,digest):
    p=approved(spec_path,digest)
    with p.manager.ResultsManager(p.directory,p.manifest,p.template,**manager_args(p)) as manager:return next_from_manager(manager)

def dispatch_subscription(p,read_fd,write_fd,binding):
    """Actual subscription entry from parent-owned FDs, without class-identity drift.

    Keep channel and controller in the SAME reviewed Session namespace. The frozen
    v2 convenience entry reloads Session after its caller constructed a channel;
    independently executed Python classes then fail its strict isinstance guard.
    This adapter preserves that guard and every v2 approval/accounting gate.
    """
    with p.manager.ResultsManager(p.directory,p.manifest,p.template,**manager_args(p)) as manager:
        session=p.manager.load_reviewed_session(PINS['relay-controller-session.py'])
        diagnostics=load_diagnostics()
        recorder=diagnostics.Recorder(binding['assignment_id'],p.directory.parent/(binding['assignment_id']+'-transport-diagnostic.jsonl'))
        class DiagnosticIO(session.PrivateRuntimeIO):
            def write(self,frame):
                if frame.get('kind')=='terminal':recorder.terminal(frame)
                return super().write(frame)
        channel=None
        try:
            channel=DiagnosticIO(read_fd,write_fd,borrow=True)
            # Preparation is separate; READY restores the unchanged 60s window.
            channel.deadline=channel.clock()+300
            gateway=session.load_pinned('subscription_gateway.py')
            approved_account_sha256=manager.config['account_sha256']
            sender=diagnostics.sender(gateway,lambda:gateway.host_auth(approved_account_sha256),recorder)
            wrapped=p.manager.tracked_sender(manager,binding['assignment_id'],sender)
            return session.SessionController(channel,manager,binding,wrapped).run()
        finally:
            if channel is not None:channel.close()
            recorder.close()

def serve(spec_path,digest,binding,read_fd,write_fd):
    """FD provenance belongs to the trusted parent retaining the actual Runtime peer.

    These JSON values and FIFO metadata alone never attest a Runtime process.
    No worker may receive the spec, approval, peer or host credentials.
    """
    p=approved(spec_path,digest)
    with p.manager.ResultsManager(p.directory,p.manifest,p.template,**manager_args(p)) as manager:
        selected=next_from_manager(manager)['next_assignment']
    validate_binding(p,selected,binding)
    return dispatch_subscription(p,read_fd,write_fd,binding)

def validate_binding(p,selected,binding):
    session=p.manager.load_reviewed_session(PINS['relay-controller-session.py'])
    if (selected is None or not isinstance(binding,dict) or set(binding)!=session.BINDING_KEYS
            or binding['assignment_id']!=selected['assignment_id'] or binding['configuration_sha256']!=p.configuration_sha256
            or binding['account_sha256']!=p.configuration['account_sha256']
            or binding['controller_sha256']!=PINS['relay-controller-session.py']
            or type(binding['attempt']) is not int or binding['attempt']!=1
            or not isinstance(binding['run_id'],str) or not re.fullmatch('[A-Za-z0-9_-]{1,128}',binding['run_id'])
            or any(not hexhash(binding[k]) for k in ('approval_digest','configuration_sha256','account_sha256','controller_sha256','channel_instance_sha256','nonce'))):
        raise ValueError('next_runtime_binding_required')

def stop_campaign(spec_path,digest,reason):
    if reason not in ('runtime_parent_failed','runtime_preparation_failed','parent_cancelled','parent_unknown_output'):
        raise ValueError('fixed_parent_stop_reason_required')
    p=approved(spec_path,digest)
    with p.manager.ResultsManager(p.directory,p.manifest,p.template,**manager_args(p)) as manager:
        if manager.state[p.manager.META]['stop_reason'] is None:
            manager.state[p.manager.META]['stop_reason']=reason;manager.save()
        return next_from_manager(manager)

def load_gateway_for_audit(root):
    root,sources=verified_sources(root);manager=load_manager(root,sources)
    return manager.load_reviewed_session(PINS['relay-controller-session.py']).load_pinned('subscription_gateway.py')

def audit_account(root,expected_account_sha256,browser_email_sha256=None):
    """Local native validation only. Secret headers stay ephemeral and never leave.

    Only the already-qualified exact native auth path is read; no refresh/network.
    JWT metadata is self-declared, not independent provider authentication evidence.
    """
    if not hexhash(expected_account_sha256) or browser_email_sha256 is not None and not hexhash(browser_email_sha256):
        raise ValueError('account_identity_hash_required')
    gateway=load_gateway_for_audit(root);headers=gateway.host_auth(expected_account_sha256)
    account_sha=sha(headers['ChatGPT-Account-ID'].encode())
    if account_sha!=expected_account_sha256:raise ValueError('native_account_mismatch')
    email_matches=None
    if browser_email_sha256 is not None:
        token=headers['Authorization'].removeprefix('Bearer ');payload=token.split('.')[1]
        claims=strict(base64.urlsafe_b64decode(payload+'='*(-len(payload)%4)))
        email=claims.get('email')
        if not email and isinstance(claims.get('https://api.openai.com/profile'),dict):email=claims['https://api.openai.com/profile'].get('email')
        if isinstance(email,str):email_matches=sha(email.strip().lower().encode())==browser_email_sha256
    return {'auth_mode':'chatgpt','account_sha256':account_sha,'matches_current_app_account':True,
        'api_key_present':False,'subscription_token_present':True,'jwt_declared_expiry_at_least_60_seconds':True,
        'same_browser_email':email_matches,'jwt_claims_are_not_independent_validation':True,'provider_requests':0}

def main(argv=None):
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action',choices=['review','next','serve','audit-account']);parser.add_argument('spec')
    parser.add_argument('--approval-digest');parser.add_argument('--binding-file');parser.add_argument('--read-fd',type=int,default=0);parser.add_argument('--write-fd',type=int,default=1)
    parser.add_argument('--browser-email-sha256');args=parser.parse_args(argv)
    try:
        if args.action=='review':result=review(args.spec)
        elif args.action=='next':result=next_trial(args.spec,args.approval_digest)
        elif args.action=='audit-account':
            p=prepare(args.spec);result=audit_account(p.root,p.spec['account_sha256'],args.browser_email_sha256)
        else:
            if args.binding_file is None:raise ValueError('private_binding_required')
            serve(args.spec,args.approval_digest,strict(read_file(args.binding_file,private=True,maximum=65536)),args.read_fd,args.write_fd)
            return 0
        sys.stdout.buffer.write(canonical(result)+b'\n');sys.stdout.buffer.flush();return 0
    except Exception as error:
        # Never print source payloads, credentials, exception strings or tracebacks.
        sys.stderr.write('subscription host refused: '+type(error).__name__+'\n');return 1

if __name__=='__main__':raise SystemExit(main())
