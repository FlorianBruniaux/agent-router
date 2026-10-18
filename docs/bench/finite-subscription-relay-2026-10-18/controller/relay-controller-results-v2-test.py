"""Versioned activation gates and honest attempted-request accounting; fixtures only."""
import copy,hashlib,importlib.util,json
from pathlib import Path
import tempfile,unittest
ROOT=Path(__file__).resolve().parent

def load(name,path):
 s=importlib.util.spec_from_file_location(name,path);m=importlib.util.module_from_spec(s);s.loader.exec_module(m);return m
fixtures=load('v2_fixture',ROOT/'relay-controller-results-test.py')
v2=None
if (ROOT/'relay-controller-results-v2.py').exists():v2=load('v2',ROOT/'relay-controller-results-v2.py')

class ActivationTests(unittest.TestCase):
 def setUp(self):
  self.assertIsNotNone(v2,'Missing explicit versioned real-campaign activation gate')
  self.tmp=tempfile.TemporaryDirectory(prefix='relay-controller-v2-approval-test-');self.addCleanup(self.tmp.cleanup)
  self.path=Path(self.tmp.name);self.config=fixtures.configuration()
 def open(self,approval=None,budget=48000000,config=None,mode='subscription'):
  config=self.config if config is None else config
  m=v2.ResultsManager(self.path,fixtures.MANIFEST,fixtures.TEMPLATE,budget_microcredits=budget,
      configuration=config,verified_configuration_sha256=fixtures.digest(config),
      evidence_mode=mode,launch_approval_digest=approval)
  self.addCleanup(m.close);return m
 def review(self):return v2.review_launch(self.path,self.config,48000000)
 def test_budget_and_exact_review_approval_required_before_any_file(self):
  for approval,budget in ((None,48000000),('0'*64,48000000),(None,None)):
   with self.assertRaises(ValueError):self.open(approval,budget)
   self.assertEqual(list(self.path.iterdir()),[])
  review=self.review();self.assertEqual(list(self.path.iterdir()),[])
  m=self.open(review['approval_digest']);state=m.snapshot()
  self.assertEqual(state['relay_results']['evidence_mode'],'subscription')
  self.assertEqual(state['budget_approved']['budget_microcredits'],48000000)
  self.assertEqual(state['budget_approved']['approval_digest'],review['approval_digest'])
  self.assertFalse(state['execution_authorized']);self.assertEqual(state['model_calls_started'],0)
 def test_old_review_does_not_authorize_new_account_budget_or_destination(self):
  review=self.review()
  config={**self.config,'account_sha256':'d'*64}
  with self.assertRaises(ValueError):self.open(review['approval_digest'],config=config)
  self.assertEqual(list(self.path.iterdir()),[])
  with tempfile.TemporaryDirectory(prefix='relay-controller-v2-other-test-') as tmp:
   with self.assertRaises(ValueError):
    v2.ResultsManager(Path(tmp),fixtures.MANIFEST,fixtures.TEMPLATE,budget_microcredits=48000000,
       configuration=self.config,verified_configuration_sha256=fixtures.digest(self.config),
       evidence_mode='subscription',launch_approval_digest=review['approval_digest'])
   self.assertEqual(list(Path(tmp).iterdir()),[])
 def test_possible_started_is_unknown_until_verified_response_and_keeps_known_prefix(self):
  m=self.open(self.review()['approval_digest']);m.begin_trial(fixtures.AID,fixtures.RUN,fixtures.APPROVAL)
  r=fixtures.receipt();m.before_request(fixtures.AID,1,r['request_body_sha256'])
  self.assertEqual(m.snapshot()['model_calls_started'],0)
  m.before_transport(fixtures.AID,1)
  self.assertIsNone(m.snapshot()['model_calls_started'])
  self.assertEqual(m.observation()['possible_started_requests'],1)
  m.ingest(fixtures.AID,r)
  self.assertEqual(m.snapshot()['model_calls_started'],1)
  second=fixtures.receipt(2);m.before_request(fixtures.AID,2,second['request_body_sha256']);m.before_transport(fixtures.AID,2)
  m.mark_unknown(fixtures.AID,'fixture_transport_lost')
  self.assertIsNone(m.snapshot()['model_calls_started'])
  self.assertEqual(m.observation()['known_model_responses'],1)
  self.assertEqual(m.observation()['known_cost_microcredits'],8300)
  self.assertFalse(m.observation()['budget_observation_allows_next'])
 def test_approved_mode_cannot_ingest_a_response_without_transport_marker(self):
  m=self.open(self.review()['approval_digest']);m.begin_trial(fixtures.AID,fixtures.RUN,fixtures.APPROVAL)
  r=fixtures.receipt();m.before_request(fixtures.AID,1,r['request_body_sha256'])
  with self.assertRaises(ValueError):m.ingest(fixtures.AID,r)
  self.assertFalse(m.observation()['budget_observation_allows_next'])
 def test_synthetic_mode_does_not_claim_real_budget_or_real_model_calls(self):
  with tempfile.TemporaryDirectory(prefix='relay-controller-results-test-') as tmp:
   with v2.ResultsManager(Path(tmp),fixtures.MANIFEST,fixtures.TEMPLATE,budget_microcredits=48000000,
          configuration=self.config,verified_configuration_sha256=fixtures.digest(self.config),evidence_mode='synthetic') as m:
    m.begin_trial(fixtures.AID,fixtures.RUN,fixtures.APPROVAL);r=fixtures.receipt()
    m.before_request(fixtures.AID,1,r['request_body_sha256']);m.before_transport(fixtures.AID,1);m.ingest(fixtures.AID,r)
    self.assertEqual(m.snapshot()['model_calls_started'],0);self.assertIsNone(m.snapshot()['budget_approved'])
    self.assertEqual(m.observation()['evidence_mode'],'synthetic')
 def test_tracked_sender_records_possible_start_before_call_and_refuses_retries(self):
  m=self.open(self.review()['approval_digest']);m.begin_trial(fixtures.AID,fixtures.RUN,fixtures.APPROVAL)
  r=fixtures.receipt();m.before_request(fixtures.AID,1,r['request_body_sha256'])
  observations=[]
  def send(raw,remaining):
   observations.append(m.snapshot()['model_calls_started'])
   return 200,'text/event-stream',iter(())
  wrapped=v2.tracked_sender(m,fixtures.AID,send)
  # The real request hash has already been bound by the session.
  body=fixtures.fixture.request();body['reasoning']['effort']='xhigh'
  raw=__import__('json').dumps(body,ensure_ascii=False,separators=(',',':')).encode()
  wrapped(raw,2)
  self.assertEqual(observations,[None])
  with self.assertRaises(ValueError):wrapped(raw,2)
  self.assertEqual(len(observations),1)
 def test_live_entry_with_missing_approval_does_not_read_auth_or_create_state(self):
  with self.assertRaises(ValueError):
   v2.run_subscription(None,self.path,fixtures.MANIFEST,fixtures.TEMPLATE,
     budget_microcredits=None,configuration=self.config,
     verified_configuration_sha256=fixtures.digest(self.config),launch_approval_digest=None,
     expected_binding={})
  self.assertEqual(list(self.path.iterdir()),[])
 def test_session_source_is_checked_before_exec_and_not_reread_by_loader(self):
  from unittest.mock import patch
  original=Path.read_bytes
  def changed(path):
   if path==ROOT/'relay-controller-session.py':return b'raise AssertionError("unreviewed code executed")'
   return original(path)
  expected=hashlib.sha256((ROOT/'relay-controller-session.py').read_bytes()).hexdigest()
  with patch.object(Path,'read_bytes',changed):
   with self.assertRaises(ValueError):v2.load_reviewed_session(expected)
 def test_subscription_sender_uses_approved_account_snapshot_even_if_caller_mutates_config(self):
  from unittest.mock import patch
  import types
  accounts=[];caller_config=copy.deepcopy(self.config)
  approved=self.review()['approval_digest']
  body=fixtures.fixture.request();body['reasoning']['effort']='xhigh'
  raw=json.dumps(body,ensure_ascii=False,separators=(',',':')).encode();r=fixtures.receipt()
  class FakeSession:
   def __init__(self,channel,manager,binding,send):self.manager=manager;self.send=send
   def run(self):
    self.manager.begin_trial(fixtures.AID,fixtures.RUN,fixtures.APPROVAL)
    self.manager.before_request(fixtures.AID,1,r['request_body_sha256'])
    caller_config['account_sha256']='d'*64
    self.send(raw,2)
    return {'fixture_only':True}
  gateway=types.SimpleNamespace(host_auth=lambda account:accounts.append(account),
      subscription_sender=lambda auth:lambda raw,remaining:auth())
  stub=types.SimpleNamespace(load_pinned=lambda name:gateway,SessionController=FakeSession)
  with patch.object(v2,'load_reviewed_session',return_value=stub):
   v2.run_subscription(None,self.path,fixtures.MANIFEST,fixtures.TEMPLATE,budget_microcredits=48000000,
      configuration=caller_config,verified_configuration_sha256=fixtures.digest(self.config),
      launch_approval_digest=approved,expected_binding={'assignment_id':fixtures.AID})
  self.assertEqual(accounts,['a'*64])
 def test_wrong_evidence_mode_refuses_before_any_file(self):
  with self.assertRaises(ValueError):self.open(mode='local_fixture')
  self.assertEqual(list(self.path.iterdir()),[])

# Compose the actual session with v2, using simulated approval and sender only.
if v2 is not None:
 session_fixtures=load('v2_session_fixture',ROOT/'relay-controller-session-test.py')
 class V2SessionTests(session_fixtures.SessionTests):
  def setUp(self):
   super().setUp()
   self.extra=tempfile.TemporaryDirectory(prefix='relay-controller-v2-session-test-')
   self.addCleanup(self.extra.cleanup)
   config=fixtures.configuration();path=Path(self.extra.name)
   review=v2.review_launch(path,config,48000000)
   self.manager=v2.ResultsManager(path,fixtures.MANIFEST,fixtures.TEMPLATE,budget_microcredits=48000000,
      configuration=config,verified_configuration_sha256=fixtures.digest(config),evidence_mode='subscription',
      launch_approval_digest=review['approval_digest'])
   self.addCleanup(self.manager.close)
   self.binding['configuration_sha256']=self.manager.config_sha
  def start(self,send=None,seconds=2):
   wrapped=v2.tracked_sender(self.manager,fixtures.AID,self.send if send is None else send)
   return super().start(send=wrapped,seconds=seconds)
  def test_ready_ack_persists_intent_before_worker_and_forwards_sse_unchanged(self):
   # The inherited behavioral test covers delivery and cumulative accounting.
   self.start();self.ready();self.request_once();self.complete();self.joined()
   self.assertEqual(self.failure,[])
   self.assertEqual(self.manager.snapshot()['model_calls_started'],1)
   self.assertEqual(self.manager.observation()['known_model_responses'],1)
   self.assertEqual(self.manager.observation()['known_cost_microcredits'],8300)
   self.assertFalse(self.manager.snapshot()['execution_authorized'])
  def test_observed_campaign_budget_stops_at_barrier_and_keeps_cleanup_channel(self):
   # Covered against v1 by SessionTests; this subclass instead checks the v2 gate.
   self.start();self.ready();self.request_once()
   self.assertEqual(self.manager.snapshot()['model_calls_started'],1)
   self.complete(native_status='interrupted');self.joined()
   self.assertFalse(self.manager.observation()['budget_observation_allows_next'])

# Replay the unchanged 27 regression tests against the versioned module.
if v2 is not None:
 namespace={'__file__':str(ROOT/'relay-controller-results-test.py'),'__name__':'v2_regressions'}
 source=(ROOT/'relay-controller-results-test.py').read_text().replace("ROOT/'relay-controller-results.py'","ROOT/'relay-controller-results-v2.py'")
 exec(compile(source,namespace['__file__'],'exec'),namespace)
 RegressionTests=namespace['ResultsTests']

if __name__=='__main__':unittest.main()
