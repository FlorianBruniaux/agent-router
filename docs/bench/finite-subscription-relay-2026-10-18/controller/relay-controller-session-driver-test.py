"""Driver qualification only: no native process, model or external transport."""
import importlib.util,json,os
from pathlib import Path
import socket,tempfile,threading,unittest
ROOT=Path(__file__).resolve().parent

def load(name,file):
 s=importlib.util.spec_from_file_location(name,ROOT/file);m=importlib.util.module_from_spec(s);s.loader.exec_module(m);return m
fixtures=load('driver_fixtures','relay-controller-results-test.py')
session=load('driver_session','relay-controller-session.py')
driver=None
if (ROOT/'relay-controller-session-driver.py').exists():driver=load('driver','relay-controller-session-driver.py')

class DriverTests(unittest.TestCase):
 def setUp(self):
  self.assertIsNotNone(driver,'Missing offline-only Linux qualification driver')
  self.tmp=tempfile.TemporaryDirectory(prefix='relay-controller-driver-test-');self.addCleanup(self.tmp.cleanup)
  self.root=Path(self.tmp.name)
  (self.root/'manifest.json').write_bytes(fixtures.MANIFEST);(self.root/'template.json').write_bytes(fixtures.TEMPLATE)
  expected={'exit_code':0,'termination':'exited','truncated':False}
  script={'model':'gpt-6-luna','effort':'xhigh','marker':'R2-LOCAL-TOOLS-V1','steps':[
   {'tool':'mcp__finite__read_file','arguments':{'path':'statement.md'},'expected':expected},
   {'tool':'mcp__finite__write_file','arguments':{'path':'lib.rs','content':'pub fn fixture() {}\n','create_new':False},'expected':expected},
   {'tool':'mcp__finite__read_file','arguments':{'path':'lib.rs'},'expected':expected}]}
  (self.root/'script.json').write_text(json.dumps(script))
  self.descriptor={'mode':'offline_qualification','manifest':'manifest.json','template':'template.json',
                   'script':'script.json','binding':'binding.json','prepared':'prepared.json',
                   'summary':'summary.json','budget_microcredits':200000000,'account_sha256':'b'*64}
  (self.root/'descriptor.json').write_text(json.dumps(self.descriptor))
 def prepared(self):
  p=driver.prepare(self.root/'descriptor.json')
  self.addCleanup(lambda:__import__('shutil').rmtree(p['results_directory']))
  return p
 def test_prepare_has_only_temporary_results_and_no_execution_authority(self):
  p=self.prepared()
  self.assertTrue(Path(p['results_directory']).name.startswith('relay-controller-results-test-'))
  self.assertFalse(p['execution_authorized']);self.assertEqual(p['provider_requests'],0)
  self.assertTrue((Path(p['results_directory'])/'results.json').exists())
  self.assertEqual(p['controller_sha256'],session.source_sha256())
 def test_no_budget_wrong_mode_or_external_path_refuses_before_preparation(self):
  for change in ({'budget_microcredits':None},{'mode':'live'},{'manifest':'../secret'},{'script':'https://example.invalid'}):
   d={**self.descriptor,**change};(self.root/'descriptor.json').write_text(json.dumps(d))
   with self.assertRaises(ValueError):driver.prepare(self.root/'descriptor.json')
   self.assertFalse((self.root/'prepared.json').exists())
 def test_unknown_second_response_keeps_only_confirmed_prefix_then_cleanup(self):
  self.run_loop(unknown=True)

 def test_diagnostic_only_renders_allowlisted_static_codes_and_error_class(self):
  result=driver.safe_diagnostic(ValueError('public_leaf_required'))
  self.assertEqual(result['code'],'public_leaf_required')
  self.assertEqual(result['error_class'],'ValueError')
  private=driver.safe_diagnostic(ValueError('synthetic_secret_request_body'))
  self.assertEqual(private['code'],'details_redacted')
  self.assertNotIn('synthetic_secret_request_body',json.dumps(private))
 def test_runtime_public_descriptor_schema_prepares_without_helpers_or_secret_paths(self):
  d={'qualification_only':True,'assignment_id':'t02-Lx-r1-H0','model':'gpt-6-luna','effort':'xhigh',
     'marker':'R2-LOCAL-TOOLS-V1','output_file':'lib.rs','output_content':'pub fn fixture() {}\n',
     'account_sha256':'b'*64,'budget_microcredits':200000000,'evidence_directory':str(self.root)}
  (self.root/'descriptor.json').write_text(json.dumps(d));p=self.prepared()
  self.assertEqual(p['configuration']['observed_campaign_budget_microcredits'],200000000)
  self.assertEqual(p['configuration']['account_sha256'],'b'*64)
  self.assertTrue((self.root/'host-prepared.json').exists())
  self.assertFalse(p['execution_authorized'])
 def test_three_tool_rounds_then_final_persist_four_synthetic_receipts(self):
  self.run_loop()

 def run_loop(self,unknown=False):
  p=self.prepared()
  binding={'assignment_id':fixtures.AID,'run_id':fixtures.RUN,'attempt':1,'approval_digest':fixtures.APPROVAL,
           'configuration_sha256':p['configuration_sha256'],'account_sha256':'b'*64,
           'controller_sha256':p['controller_sha256'],'channel_instance_sha256':'d'*64,'nonce':'e'*64}
  (self.root/'binding.json').write_text(json.dumps(binding))
  left,right=socket.socketpair();self.addCleanup(left.close);self.addCleanup(right.close)
  failures=[]
  def run():
   try:driver.run_qualification(left.fileno(),left.fileno(),self.root/'descriptor.json',borrow=True,
                                qualification_case='unknown_usage_second_response' if unknown else 'normal')
   except Exception as e:failures.append(e)
  t=threading.Thread(target=run,daemon=True);t.start()
  def stop():
   try:right.shutdown(socket.SHUT_RDWR)
   except OSError:pass
   t.join(2)
  self.addCleanup(stop)
  def put(v):right.sendall(json.dumps(v).encode()+b'\n')
  def get():
   right.settimeout(4);raw=b''
   while not raw.endswith(b'\n'):raw+=right.recv(1)
   return json.loads(raw)
  put({'schema_version':1,'kind':'runtime_ready','binding':binding});self.assertEqual(get()['kind'],'controller_ready')
  body=fixtures.fixture.request();body['reasoning']['effort']='xhigh'
  body['input']=[x for x in body['input'] if x.get('role') in ('user','developer','system')]
  for i in range(1,3 if unknown else 5):
   if i>1:
    value={'elapsed_ms':1,'exit_code':0,'stderr':'','stdout':'fixture','termination':'exited','truncated':False}
    body['input'].append({'type':'function_call_output','call_id':'call_r2_local_'+str(i-1),
     'output':[{'type':'input_text','text':'Wall time: 0.0001 seconds\nOutput:'},{'type':'input_text','text':json.dumps(value)}]})
   put({'schema_version':1,'id':i,'body':body})
   while get()['kind']!='terminal':pass
   barrier=get();self.assertEqual(barrier['kind'],'controller_request_finished')
   self.assertEqual(barrier['continue'],not(unknown and i==2),(i,barrier,[type(e).__name__ for e in failures]))
  put({'schema_version':1,'kind':'runtime_cleanup','binding':binding,'cleanup':'namespace_terminated',
       'native_status':'interrupted' if unknown else 'completed','proof_sha256':'c'*64})
  self.assertEqual(get()['kind'],'controller_finished');t.join(4);self.assertFalse(t.is_alive());self.assertEqual(failures,[])
  summary=json.loads((self.root/'summary.json').read_text())
  self.assertEqual(summary['coded_responses'],2 if unknown else 4);self.assertEqual(summary['observed_tool_results'],1 if unknown else 3)
  self.assertTrue(summary['synthetic_usage']);self.assertEqual(summary['provider_requests'],0)
  state=json.loads((Path(p['results_directory'])/'results.json').read_text())
  self.assertEqual(len(state['rows'][0]['relay_results']['receipts']),1 if unknown else 4)
  self.assertEqual(state['rows'][0]['usage']['total_tokens'],600 if unknown else 2400)
  if unknown:
   self.assertEqual(state['rows'][0]['cost_microcredits'],3275)
   self.assertTrue(state['rows'][0]['relay_results']['usage_unknown'])
   self.assertFalse(summary['observation']['budget_observation_allows_next'])
  self.assertFalse(state['execution_authorized'])

if __name__=='__main__':unittest.main()
