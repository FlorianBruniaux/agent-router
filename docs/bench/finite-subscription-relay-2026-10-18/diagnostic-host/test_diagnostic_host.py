"""Diagnostic host approval and private-pipe regressions, synthetic ledger only."""
import importlib.util,json,os,pathlib,shutil,unittest
from unittest.mock import patch
ROOT=pathlib.Path(__file__).resolve().parent

def load(name,path):
 spec=importlib.util.spec_from_file_location(name,path);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);return m
fixtures=load('diagnostic_parent_fixtures',ROOT/'relay-controller-subscription-parent-test.py')
transport=load('diagnostic_transport_fixtures',ROOT/'test_transport_diagnostics.py')
parent=fixtures.parent

class DiagnosticHostTests(unittest.TestCase):
 def setUp(self):
  self.fixture=fixtures.ParentIntegrationTests('test_all_24_real_parent_protocol_rounds_use_one_ledger_with_coded_transport_only')
  self.fixture.setUp();self.addCleanup(self.fixture.doCleanups)
 def test_diagnostic_code_changes_invalidate_review_without_initializing_a_ledger(self):
  review=parent.host.review(self.fixture.spec)
  self.assertIsInstance(review['review'].get('transport_diagnostics_sha256'),str)
  tools=self.fixture.root/'changed-tools';tools.mkdir()
  for name in ('relay-controller-subscription-launcher.py','relay-controller-subscription-parent.py','subscription-transport-diagnostics.py'):shutil.copyfile(ROOT/name,tools/name)
  other=load('changed_diagnostic_host',tools/'relay-controller-subscription-launcher.py')
  with patch.object(other,'CONSENT_DOCUMENT_SHA256',parent.host.CONSENT_DOCUMENT_SHA256):
   approved=other.review(self.fixture.spec)['approval_digest']
   source=tools/'subscription-transport-diagnostics.py';source.write_bytes(source.read_bytes()+b'\n# tampered fixture\n')
   with self.assertRaises(ValueError):other.next_trial(self.fixture.spec,approved)
  self.assertEqual(list(self.fixture.ledger.iterdir()),[])
 def test_http_failure_diagnostic_survives_cleanup_and_23_trials_remain_not_run(self):
  self.assertTrue(callable(getattr(parent.host,'load_diagnostics',None)))
  original=parent.host.load_diagnostics;connections=[]
  def observed_module():
   module=original();original_sender=module.sender
   def factory(gateway,auth,recorder):
    # Restore the real pinned transport below the test-only coded environment.
    gateway.subscription_sender=transport.gateway.subscription_sender
    def connection(host,timeout):
     c=transport.FakeConnection(transport.FakeResponse(401,b'{"error":{"code":"invalid_api_key","message":"PRIVATE_SENTINEL_DO_NOT_RETAIN"}}'));connections.append(c);return c
    return original_sender(gateway,lambda:{'Authorization':'Bearer SYNTHETIC','ChatGPT-Account-ID':'SYNTHETIC'},recorder,connection_factory=connection)
   module.sender=factory;return module
  with self.fixture.coded_environment(),patch.object(parent.host,'load_diagnostics',side_effect=observed_module):
   result=parent.run_campaign(self.fixture.spec,self.fixture.approval,self.fixture.recipe_path)
  self.assertTrue(result['observation']['usage_unknown']);self.assertEqual(result['statuses']['not_run'],23);self.assertEqual(result['statuses']['stopped'],1)
  self.assertEqual(self.fixture.children,1);self.assertEqual(self.fixture.auth_reads,0);self.assertEqual(sum(len(c.requests) for c in connections),1)
  path=self.fixture.root/'t02-Lx-r1-H0-transport-diagnostic.jsonl';row=json.loads(path.read_text());self.assertEqual(row['http_status'],401);self.assertFalse(row['accounting_authority'])
  self.assertNotIn('PRIVATE_SENTINEL',path.read_text())
  reference=json.loads((self.fixture.root/'t02-Lx-r1-H0-runtime-reference.json').read_text());self.assertFalse(reference['output_export_allowed'])
  self.assertIsNone(parent.host.next_trial(self.fixture.spec,self.fixture.approval)['next_assignment'])
 def test_bad_prepare_produces_no_transport_diagnostic_or_provider_attempt(self):
  with self.fixture.coded_environment('bad_prepare'):
   with self.assertRaises(ValueError):parent.run_campaign(self.fixture.spec,self.fixture.approval,self.fixture.recipe_path)
  self.assertEqual(list(self.fixture.root.glob('*-transport-diagnostic.jsonl')),[]);self.assertEqual(self.fixture.sender_calls,0);self.assertEqual(self.fixture.children,1)

if __name__=='__main__':unittest.main()
