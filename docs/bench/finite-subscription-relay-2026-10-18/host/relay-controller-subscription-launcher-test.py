"""Synthetic local tests. No credential read and no provider request."""
import contextlib,copy,importlib.util,io,json,os,pathlib,shutil,tempfile,unittest
from unittest.mock import patch

ROOT=pathlib.Path(__file__).resolve().parent
QUALIFIED=pathlib.Path('/Users/florianbruniaux/.codex/worktrees/f12a-strict-memory/agent-router/docs/bench/finite-subscription-relay-2026-10-18/controller')
spec=importlib.util.spec_from_file_location('live_launcher',ROOT/'relay-controller-subscription-launcher.py')
launcher=importlib.util.module_from_spec(spec);spec.loader.exec_module(launcher)

class LauncherTests(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory(prefix='relay-controller-launcher-test-',dir='/private/tmp');self.addCleanup(self.tmp.cleanup)
        self.root=pathlib.Path(self.tmp.name);self.directory=self.root/'results';self.directory.mkdir(mode=0o700)
        self.consent=self.root/'human-authorization.json';self.consent.write_bytes(b'{"synthetic":true,"budget":48000000}\n');self.consent.chmod(0o600)
        self.consent_patch=patch.object(launcher,'CONSENT_DOCUMENT_SHA256',launcher.sha(self.consent.read_bytes()));self.consent_patch.start();self.addCleanup(self.consent_patch.stop)
        self.spec=self.root/'spec.json';self.value={'schema_version':1,'controller_root':str(QUALIFIED),'results_directory':str(self.directory),'account_sha256':'b'*64,'budget_microcredits':48000000,'human_authorization':str(self.consent),'runtime_recipe_sha256':'c'*64}
        self.write_spec()
    def write_spec(self):
        self.spec.write_text(json.dumps(self.value));self.spec.chmod(0o600)
    def reviewed(self):return launcher.review(self.spec)
    def open_manager(self):
        p=launcher.prepare(self.spec)
        return p.manager.ResultsManager(p.directory,p.manifest,p.template,budget_microcredits=48000000,configuration=p.configuration,verified_configuration_sha256=p.configuration_sha256,evidence_mode='subscription',launch_approval_digest=p.inner_approval_digest)
    def binding(self,aid=None):
        r=self.reviewed()
        return {'assignment_id':aid or r['assignments'][0]['assignment_id'],'run_id':'synthetic-run-1','attempt':1,'approval_digest':'d'*64,'configuration_sha256':r['configuration_sha256'],'account_sha256':'b'*64,'controller_sha256':r['session_controller_sha256'],'channel_instance_sha256':'e'*64,'nonce':'f'*64}
    def test_review_is_read_only_and_binds_all_24_budget_account_code_and_destination(self):
        with patch.object(launcher,'audit_account',side_effect=AssertionError('auth_forbidden')):
            r=self.reviewed()
        self.assertEqual(len(r['assignments']),24);self.assertEqual(r['budget_microcredits'],48000000)
        self.assertFalse(r['execution_authorized']);self.assertEqual(list(self.directory.iterdir()),[])
        self.assertEqual(r['configuration']['account_sha256'],'b'*64)
        self.assertEqual(r['configuration']['observed_tokens_per_trial'],6000)
        self.assertEqual(r['configuration']['wall_seconds_per_trial'],60)
        self.assertTrue(r['parent_sha256'])
    def test_unapproved_or_wrong_budget_never_initializes_results(self):
        with self.assertRaises(ValueError):launcher.next_trial(self.spec,'0'*64)
        self.assertEqual(list(self.directory.iterdir()),[])
        for budget in [None,True,25000000,49000000]:
            self.value['budget_microcredits']=budget;self.write_spec()
            with self.assertRaises(ValueError):self.reviewed()
            self.assertEqual(list(self.directory.iterdir()),[])
    def test_next_initializes_only_the_approved_single_ledger_and_fixed_first_cell(self):
        r=self.reviewed();n=launcher.next_trial(self.spec,r['approval_digest'])
        self.assertEqual(n['next_assignment']['assignment_id'],'t02-Lx-r1-H0')
        self.assertEqual(n['observation']['known_cost_microcredits'],0)
        state=json.loads((self.directory/'results.json').read_text())
        self.assertEqual(state['budget_approved']['budget_microcredits'],48000000)
        self.assertEqual(state['model_calls_started'],0);self.assertEqual(len(state['rows']),24)
        self.assertTrue(all(row['status']=='not_run' for row in state['rows']))
    def test_altered_source_or_account_or_destination_invalidates_approval(self):
        approved=self.reviewed()['approval_digest'];self.value['account_sha256']='c'*64;self.write_spec()
        with self.assertRaises(ValueError):launcher.next_trial(self.spec,approved)
        self.assertEqual(list(self.directory.iterdir()),[])
    def test_changed_transport_parent_invalidates_the_same_budget_approval(self):
        tools=self.root/'tools';tools.mkdir()
        for name in ['relay-controller-subscription-launcher.py','relay-controller-subscription-parent.py']:shutil.copyfile(ROOT/name,tools/name)
        spec=importlib.util.spec_from_file_location('copied_parent_approval',tools/'relay-controller-subscription-launcher.py');module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
        with patch.object(module,'CONSENT_DOCUMENT_SHA256',launcher.CONSENT_DOCUMENT_SHA256):
            approved=module.review(self.spec)['approval_digest']
            file=tools/'relay-controller-subscription-parent.py';file.write_bytes(file.read_bytes()+b'\n# synthetic changed transport source\n')
            with self.assertRaises(ValueError):module.next_trial(self.spec,approved)
        self.assertEqual(list(self.directory.iterdir()),[])
        self.value['account_sha256']='b'*64;self.write_spec();r=self.reviewed();old=self.directory.with_name('old-results');self.directory.rename(old);self.directory.mkdir(mode=0o700)
        with self.assertRaises(ValueError):launcher.next_trial(self.spec,r['approval_digest'])
        self.assertEqual(list(self.directory.iterdir()),[])
    def test_unknown_and_incomplete_previous_attempt_block_next_after_reopen(self):
        r=self.reviewed();launcher.next_trial(self.spec,r['approval_digest'])
        with self.open_manager() as m:m.begin_trial('t02-Lx-r1-H0','synthetic-run-1','d'*64)
        n=launcher.next_trial(self.spec,r['approval_digest']);self.assertIsNone(n['next_assignment'])
        self.assertTrue(n['observation']['usage_unknown']);self.assertFalse(n['observation']['budget_observation_allows_next'])
    def complete(self,manager,assignment,index,*,tokens=600,cleanup=True):
        p=launcher.prepare(self.spec);session=p.manager.load_reviewed_session(launcher.PINS['relay-controller-session.py']);gateway=session.load_pinned('subscription_gateway.py')
        body={'model':assignment['model'],'reasoning':{'effort':assignment['effort']},'stream':True,'store':False,
              'input':[{'role':'user','content':'SYNTHETIC OFFLINE TEST'}],
              'tools':json.loads((QUALIFIED/'native-catalog-adapter/expected-native-catalog.json').read_text())}
        raw=json.dumps(body,ensure_ascii=False,separators=(',',':')).encode();digest=launcher.sha(raw)
        response={'id':'synthetic_response_'+str(index),'model':assignment['model'],'status':'completed','output':[],
                  'usage':{'input_tokens':tokens-200,'output_tokens':200,'total_tokens':tokens,'input_tokens_details':{'cached_tokens':0},'output_tokens_details':{'reasoning_tokens':0}}}
        wire=b'data: '+json.dumps({'type':'response.completed','response':response}).encode()+b'\n\n'
        observer=manager.common['PipeObserver'](assignment['assignment_id'],assignment['model'],assignment['effort'],started_at=0,clock=lambda:0)
        gw=gateway.Gateway({'model':assignment['model'],'effort':assignment['effort']},lambda raw,remaining:(200,'text/event-stream',iter((wire,))),clock=lambda:0)
        observer.begin(1,digest)
        frames=list(gw.handle({'schema_version':1,'id':1,'body':body}));observer.feed(b''.join(launcher.canonical(f)+b'\n' for f in frames))
        aid=assignment['assignment_id'];run='synthetic_run_'+str(index);approval=launcher.sha(str(index).encode())
        manager.begin_trial(aid,run,approval);manager.before_request(aid,1,digest);manager.before_transport(aid,1);manager.ingest(aid,observer.receipt())
        proof={'run_id':run,'approval_digest':approval,'proof_sha256':'c'*64,'writers_excluded':True} if cleanup else None
        manager.finish_trial(aid,'completed',proof)
    def test_all_24_cells_advance_in_order_across_manager_closes_without_retries(self):
        reviewed=self.reviewed();approval=reviewed['approval_digest']
        for index,assignment in enumerate(reviewed['assignments']):
            n=launcher.next_trial(self.spec,approval);self.assertEqual(n['next_assignment'],assignment)
            with self.open_manager() as manager:self.complete(manager,assignment,index)
        final=launcher.next_trial(self.spec,approval);self.assertIsNone(final['next_assignment']);self.assertEqual(final['statuses']['completed'],24)
        self.assertEqual(final['observation']['known_model_responses'],24)
    def test_observed_token_limit_blocks_campaign_after_completed_trial(self):
        r=self.reviewed();launcher.next_trial(self.spec,r['approval_digest'])
        with self.open_manager() as manager:self.complete(manager,r['assignments'][0],0,tokens=6000)
        n=launcher.next_trial(self.spec,r['approval_digest']);self.assertIsNone(n['next_assignment']);self.assertEqual(n['observation']['stop_reason'],'observed_token_limit')
    def test_missing_cleanup_blocks_campaign_and_keeps_known_consumption(self):
        r=self.reviewed();launcher.next_trial(self.spec,r['approval_digest'])
        with self.open_manager() as manager:self.complete(manager,r['assignments'][0],0,cleanup=False)
        n=launcher.next_trial(self.spec,r['approval_digest']);self.assertIsNone(n['next_assignment']);self.assertGreater(n['observation']['known_cost_microcredits'],0)
        self.assertEqual(n['observation']['stop_reason'],'cleanup_unconfirmed')
    def test_deadline_stop_persists_and_never_advances_after_manager_reopen(self):
        r=self.reviewed();launcher.next_trial(self.spec,r['approval_digest'])
        with self.open_manager() as manager:
            manager.begin_trial('t02-Lx-r1-H0','synthetic-run-1','d'*64);manager.mark_unknown('t02-Lx-r1-H0','transport_deadline')
        n=launcher.next_trial(self.spec,r['approval_digest']);self.assertIsNone(n['next_assignment']);self.assertTrue(n['observation']['usage_unknown'])
    def test_serve_refuses_wrong_binding_before_dispatch(self):
        r=self.reviewed();binding=self.binding(r['assignments'][1]['assignment_id'])
        with patch.object(launcher,'dispatch_subscription',side_effect=AssertionError('must_not_dispatch')):
            with self.assertRaises(ValueError):launcher.serve(self.spec,r['approval_digest'],binding,0,1)
    def test_audit_never_exports_secret_headers_or_private_exception_values(self):
        fake=type('Gateway',(),{'host_auth':staticmethod(lambda expected:{'Authorization':'Bearer SYNTHETIC_SECRET_SENTINEL','ChatGPT-Account-ID':'synthetic-account'})})
        expected=launcher.sha(b'synthetic-account')
        with patch.object(launcher,'load_gateway_for_audit',return_value=fake):
            report=launcher.audit_account(QUALIFIED,expected)
        self.assertEqual(report['account_sha256'],expected);self.assertNotIn('SYNTHETIC_SECRET',json.dumps(report));self.assertNotIn('synthetic-account',json.dumps(report))

if __name__=='__main__':unittest.main()
