"""Parent protocol tests: synthetic descriptors only, no model/authentication."""
import contextlib,importlib.util,json,pathlib,unittest,tempfile,subprocess,sys
from unittest.mock import patch

ROOT=pathlib.Path(__file__).resolve().parent
spec=importlib.util.spec_from_file_location('subscription_parent',ROOT/'relay-controller-subscription-parent.py')
parent=importlib.util.module_from_spec(spec);spec.loader.exec_module(parent)

class ParentTests(unittest.TestCase):
    def test_runtime_binary_is_checked_before_launch_and_wrong_bytes_refuse(self):
        import subprocess
        recipe={'container_id':'a'*64,'runtime_executable_path':'/task/programs/real-campaign','runtime_executable_sha256':'b'*64}
        with patch.object(parent.subprocess,'run',return_value=subprocess.CompletedProcess([],0,stdout=b'0'*64+b'  /task/programs/real-campaign\n')) as probe:
            with self.assertRaises(ValueError):parent.verify_runtime_binary(recipe)
            self.assertEqual(probe.call_args.args[0],['docker','exec','a'*64,'sha256sum','/task/programs/real-campaign'])
    def test_bounded_private_line_preserves_next_line_without_prefetch(self):
        import os
        read,write=os.pipe()
        try:
            os.write(write,b'{"first":1}\n{"next":2}\n')
            self.assertEqual(parent.read_control(read,1),{'first':1})
            self.assertEqual(parent.read_control(read,1),{'next':2})
        finally:os.close(read);os.close(write)
    def test_missing_newline_or_oversized_control_fails_closed(self):
        import os
        for raw in [b'{"incomplete":1}',b'x'*65537]:
            read,write=os.pipe()
            try:
                # Bounded worker needed because a whole oversized pipe write blocks.
                import threading
                def supply():
                    try:
                        offset=0
                        while offset<len(raw):offset+=os.write(write,raw[offset:])
                    except OSError:pass
                    finally:os.close(write)
                worker=threading.Thread(target=supply,daemon=True);worker.start()
                with self.assertRaises((ValueError,EOFError)):parent.read_control(read,2)
            finally:os.close(read)
            worker.join(2);self.assertFalse(worker.is_alive())
    def test_approval_has_exact_full_binding_and_packet_digest(self):
        binding={'synthetic_binding':True};value=parent.runtime_approval(binding,'a'*64)
        self.assertEqual(value,{'schema_version':1,'kind':'approve_runtime','binding':binding,'packet_sha256':'a'*64})
    def test_prepare_requires_exact_identity_and_original_limits(self):
        p=type('Plan',(),{'configuration_sha256':'c'*64,'configuration':{'account_sha256':'b'*64}})()
        assignment={'assignment_id':'synthetic_cell','model':'gpt-6-luna','effort':'medium','condition':'H0','acceptance_sha256':'a'*64}
        packet={'packet_sha256':'d'*64}
        value=parent.expected_prepare(p,assignment,packet);parent.validate_prepare(value,p,assignment,packet)
        for change in [{'model':'gpt-6-astra'},{'deadline_ms':60001},{'observed_tokens':True},{'packet_sha256':'e'*64},{'extra':1}]:
            with self.subTest(change=change),self.assertRaises(ValueError):parent.validate_prepare(dict(value,**change),p,assignment,packet)
    def test_raw_stdout_or_qualification_prefix_is_not_control_protocol(self):
        import os
        read,write=os.pipe()
        try:
            os.write(write,b'AR_RELAY_PREPARE {}\n')
            with self.assertRaises(ValueError):parent.read_control(read,1)
        finally:os.close(read);os.close(write)
    def test_binding_envelope_requires_integer_schema_and_exact_keys(self):
        good={'schema_version':1,'kind':'runtime_binding','binding':{}}
        with patch.object(parent.host,'validate_binding') as validate:
            self.assertEqual(parent.validate_binding_envelope(good,None,None),{})
            for change in [{'schema_version':True},{'kind':'runtime_ready'},{'extra':1}]:
                with self.subTest(change=change),self.assertRaises(ValueError):parent.validate_binding_envelope(dict(good,**change),None,None)
            self.assertEqual(validate.call_count,1)

class ParentIntegrationTests(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory(prefix='subscription-parent-offline-',dir='/private/tmp');self.addCleanup(self.tmp.cleanup)
        self.root=pathlib.Path(self.tmp.name);self.ledger=self.root/'ledger';self.ledger.mkdir(mode=0o700)
        self.consent=self.root/'synthetic-consent.json';self.consent.write_text('{"synthetic":true}');self.consent.chmod(0o600)
        self.patch_consent=patch.object(parent.host,'CONSENT_DOCUMENT_SHA256',parent.host.sha(self.consent.read_bytes()));self.patch_consent.start();self.addCleanup(self.patch_consent.stop)
        self.qualified=pathlib.Path('/Users/florianbruniaux/.codex/worktrees/f12a-strict-memory/agent-router/docs/bench/finite-subscription-relay-2026-10-18/controller')
        self.packet_path=pathlib.Path('/private/tmp/ar-real-campaign-packets-20261018.json')
        self.packets=json.loads(self.packet_path.read_text())['assignments']
        self.spec=self.root/'spec.json';self.recipe_path=self.root/'recipe.json'
        self.spec_value={'schema_version':1,'controller_root':str(self.qualified),'results_directory':str(self.ledger),'account_sha256':'b'*64,'budget_microcredits':48000000,'human_authorization':str(self.consent),'runtime_recipe_sha256':'c'*64}
        self.write_private(self.spec,self.spec_value);p=parent.host.prepare(self.spec)
        self.recipe={'schema_version':1,'kind':'real_subscription_campaign24','ready':True,'container_id':'a'*64,
            'runtime_executable_path':'/task/programs/real-campaign','runtime_executable_sha256':'d'*64,
            'runtime_source_manifest_sha256':'e'*64,'packet_manifest':str(self.packet_path),'browser_email_sha256':'f'*64,'cells':[]}
        for assignment in p.assignments:
            aid=assignment['assignment_id']
            request={'schema_version':1,'assignment_id':aid,'configuration_sha256':p.configuration_sha256,
                'account_sha256':'b'*64,'controller_sha256':parent.host.PINS['relay-controller-session.py'],
                'observed_campaign_stop_microcredits':48000000,'deadline_ms':60000,'observed_tokens':6000,
                'root':'/task/campaign/'+aid,'programs':{str(i):{'synthetic':True} for i in range(6)}}
            self.recipe['cells'].append({'assignment_id':aid,'argv':['docker','exec','-i','a'*64,'/task/programs/real-campaign'],'request':request})
        self.write_private(self.recipe_path,self.recipe);self.spec_value['runtime_recipe_sha256']=parent.host.sha(self.recipe_path.read_bytes());self.write_private(self.spec,self.spec_value)
        self.approval=parent.host.review(self.spec)['approval_digest'];self.fixture=self.root/'fake-runtime.py'
        self.fixture.write_text('''import sys,json,pathlib,hashlib
def get():
 s=sys.stdin.buffer.readline()
 if not s:raise SystemExit(4)
 return json.loads(s)
def put(v):print(json.dumps(v),flush=True)
r=get();aid=r['assignment_id'];packets=json.loads(pathlib.Path(sys.argv[1]).read_text())['assignments'];p=next(v for v in packets if v['assignment_id']==aid)
prepare={'schema_version':1,'kind':'runtime_prepare',**{k:p[k] for k in ('assignment_id','model','effort','condition','acceptance_sha256','packet_sha256')},**{k:r[k] for k in ('configuration_sha256','account_sha256','controller_sha256','deadline_ms','observed_tokens','observed_campaign_stop_microcredits')}}
if sys.argv[3]=='bad_prepare':prepare['packet_sha256']='0'*64
put(prepare)
h=hashlib.sha256(aid.encode()).hexdigest()
b={'assignment_id':aid,'run_id':'synthetic_'+aid,'attempt':1,'approval_digest':h,'configuration_sha256':r['configuration_sha256'],'account_sha256':r['account_sha256'],'controller_sha256':r['controller_sha256'],'channel_instance_sha256':h,'nonce':h}
put({'schema_version':1,'kind':'runtime_binding','binding':b})
approval=get();assert approval=={'schema_version':1,'kind':'approve_runtime','binding':b,'packet_sha256':p['packet_sha256']}
put({'schema_version':1,'kind':'runtime_ready','binding':b});assert get()['kind']=='controller_ready'
if sys.argv[3]=='early_eof':raise SystemExit(6)
catalog=json.loads(pathlib.Path(sys.argv[2]).read_text())
body={'model':p['model'],'reasoning':{'effort':p['effort']},'store':False,'stream':True,'tools':catalog,'input':[{'role':'user','content':'SYNTHETIC '+aid}]}
put({'schema_version':1,'id':1,'body':body})
while get()['kind']!='terminal':pass
assert get()['kind']=='controller_request_finished'
put({'schema_version':1,'kind':'runtime_cleanup','binding':b,'cleanup':'namespace_terminated','native_status':'completed','proof_sha256':h})
assert get()['kind']=='controller_finished'
assert sys.stdin.buffer.read()==b''
''')
        self.sender_calls=0;self.children=0;self.auth_reads=0
    @staticmethod
    def write_private(path,value):path.write_bytes(parent.host.canonical(value)+b'\n');path.chmod(0o600)
    @contextlib.contextmanager
    def coded_environment(self,mode='normal'):
        original_load=parent.host.load_manager;real_popen=subprocess.Popen
        def test_load(root,sources):
            manager=original_load(root,sources);original_session=manager.load_reviewed_session
            def test_session(expected):
                session=original_session(expected);original_pin=session.load_pinned
                def pin(name):
                    module=original_pin(name)
                    if name=='subscription_gateway.py':
                        def no_auth(*args):self.auth_reads+=1;raise AssertionError('credential_read_forbidden')
                        module.host_auth=no_auth
                        def factory(auth_loader):
                            def send(raw,remaining):
                                self.sender_calls+=1;body=parent.host.strict(raw)
                                response={'id':'synthetic_'+parent.host.sha(raw)[:32],'model':body['model'],'status':'completed','output':[],
                                    'usage':{'input_tokens':400,'output_tokens':200,'total_tokens':600,'input_tokens_details':{'cached_tokens':0},'output_tokens_details':{'reasoning_tokens':0}}}
                                wire=b'data: '+json.dumps({'type':'response.completed','response':response}).encode()+b'\n\n'
                                return 200,'text/event-stream',iter((wire,))
                            return send
                        module.subscription_sender=factory
                    return module
                session.load_pinned=pin;return session
            manager.run_subscription.__globals__['load_reviewed_session']=test_session
            manager.load_reviewed_session=test_session
            return manager
        def spawn(argv,**kwargs):
            self.children+=1
            return real_popen([sys.executable,'-B',str(self.fixture),str(self.packet_path),str(self.qualified/'native-catalog-adapter/expected-native-catalog.json'),mode],**kwargs)
        with patch.object(parent.host,'load_manager',side_effect=test_load),patch.object(parent.host,'audit_account',return_value={'same_browser_email':True}),patch.object(parent,'verify_runtime_binary'),patch.object(parent.subprocess,'Popen',side_effect=spawn):yield
    def test_all_24_real_parent_protocol_rounds_use_one_ledger_with_coded_transport_only(self):
        with self.coded_environment():result=parent.run_campaign(self.spec,self.approval,self.recipe_path)
        self.assertEqual(result['statuses']['completed'],24);self.assertEqual(self.children,24);self.assertEqual(self.sender_calls,24);self.assertEqual(self.auth_reads,0)
        references=list(self.root.glob('*-runtime-reference.json'));self.assertEqual(len(references),24)
        self.assertTrue(all(json.loads(p.read_text())['output_export_allowed'] for p in references))
    def test_wrong_prepare_never_acknowledges_or_sends_and_blocks_campaign_durably(self):
        with self.coded_environment('bad_prepare'):
            with self.assertRaises(ValueError):parent.run_campaign(self.spec,self.approval,self.recipe_path)
        self.assertEqual(self.children,1);self.assertEqual(self.sender_calls,0);self.assertEqual(self.auth_reads,0)
        self.assertIsNone(parent.host.next_trial(self.spec,self.approval)['next_assignment']);self.assertEqual(list(self.root.glob('*-runtime-reference.json')),[])
    def test_eof_after_ready_never_retries_and_never_exports_output(self):
        with self.coded_environment('early_eof'):
            with self.assertRaises(ValueError):parent.run_campaign(self.spec,self.approval,self.recipe_path)
        self.assertEqual(self.children,1);self.assertEqual(self.sender_calls,0);self.assertEqual(self.auth_reads,0)
        result=parent.host.next_trial(self.spec,self.approval);self.assertIsNone(result['next_assignment']);self.assertTrue(result['observation']['usage_unknown'])
        self.assertEqual(list(self.root.glob('*-runtime-reference.json')),[])

if __name__=='__main__':unittest.main()
