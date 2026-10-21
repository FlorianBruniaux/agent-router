"""Real frozen Gateway plus simulated HTTPS boundary; no sockets or credentials."""
import importlib.util,json,os,ssl,stat,tempfile,unittest
from pathlib import Path
ROOT=Path(__file__).resolve().parent
_spec=importlib.util.spec_from_file_location('transport_diagnostics',ROOT/'subscription-transport-diagnostics.py')
diag=importlib.util.module_from_spec(_spec);_spec.loader.exec_module(diag)
CONTROLLER=Path(os.environ['AR_DIAGNOSTIC_CONTROLLER'])
_spec=importlib.util.spec_from_file_location('frozen_gateway',CONTROLLER/'subscription_gateway.py')
gateway=importlib.util.module_from_spec(_spec);_spec.loader.exec_module(gateway)
SECRET='PRIVATE_SENTINEL_DO_NOT_RETAIN'

def completed(*,usage=True):
 r={'id':'fixture_response','model':'gpt-6-luna','status':'completed','output':[{'text':SECRET}]}
 if usage:r['usage']={'input_tokens':400,'output_tokens':200,'total_tokens':600,'input_tokens_details':{'cached_tokens':0},'output_tokens_details':{'reasoning_tokens':0}}
 return b'data: '+json.dumps({'type':'response.completed','response':r}).encode()+b'\n\n'

class FakeResponse:
 def __init__(self,status,body,content_type='application/json',error=None):
  self.status=status;self.body=body;self.content_type=content_type;self.closed=False;self.reads=[];self.error=error
 def getheader(self,name,default=''):
  return self.content_type if name=='Content-Type' else default
 def read1(self,limit):
  self.reads.append(limit)
  if self.error:raise self.error
  b,self.body=self.body[:limit],self.body[limit:];return b
 def close(self):self.closed=True

class FakeConnection:
 def __init__(self,response,request_error=None):self.response=response;self.request_error=request_error;self.sock=None;self.closed=False;self.requests=[]
 def request(self,*args):
  self.requests.append(args)
  if self.request_error:raise self.request_error
 def getresponse(self):return self.response
 def close(self):self.closed=True

class TransportDiagnosticsTests(unittest.TestCase):
 def setUp(self):
  self.tmp=tempfile.TemporaryDirectory(prefix='transport-diagnostics-',dir=os.environ.get('AR_OFFLINE_SCRATCH'));self.addCleanup(self.tmp.cleanup)
  self.path=Path(self.tmp.name)/'transport-diagnostics.jsonl';self.record=diag.Recorder('t02-Lm-r1-H0',self.path);self.addCleanup(self.record.close)
  self.body=json.loads((CONTROLLER/'relay-controller-native-request.json').read_text());self.body['model']='gpt-6-luna';self.body['reasoning']={'effort':'medium'}
 def auth(self):return {'Authorization':'Bearer '+SECRET,'ChatGPT-Account-ID':SECRET}
 def run_gateway(self,response=None,*,error=None,auth=None):
  self.connection=FakeConnection(response,request_error=error);calls=[]
  def factory(host,timeout):calls.append(host);self.assertEqual(host,'chatgpt.com');return self.connection
  send=diag.sender(gateway,auth or self.auth,self.record,connection_factory=factory)
  g=gateway.Gateway({'model':'gpt-6-luna','effort':'medium'},send)
  frames=list(g.handle({'schema_version':1,'id':1,'body':self.body}));self.record.terminal(frames[-1]);self.record.close()
  self.assertNotIn(SECRET,self.path.read_text());self.assertLessEqual(self.path.stat().st_size,65536)
  return g,frames,json.loads(self.path.read_text()),calls
 def test_http_rejection_keeps_status_and_allowlisted_code_without_body_or_auth(self):
  response=FakeResponse(401,json.dumps({'error':{'code':'invalid_api_key','type':'authentication_error','message':SECRET,'param':SECRET}}).encode())
  g,frames,row,calls=self.run_gateway(response)
  self.assertEqual(row.get('http_status'),401);self.assertEqual(row.get('upstream_error_code'),'invalid_api_key');self.assertEqual(row.get('phase'),'http_error')
  self.assertFalse(row.get('gateway_complete'));self.assertFalse(frames[-1]['complete']);self.assertIsNone(frames[-1]['usage'])
  list(g.handle({'schema_version':1,'id':2,'body':self.body}));self.assertEqual(calls,['chatgpt.com']);self.assertTrue(response.closed);self.assertTrue(self.connection.closed)
 def test_tls_verification_failure_has_safe_category_without_exception_text(self):
  _,frames,row,calls=self.run_gateway(error=ssl.SSLCertVerificationError(1,SECRET))
  self.assertEqual(row.get('failure'),'tls_certificate');self.assertEqual(row.get('phase'),'request');self.assertIsNone(row.get('http_status'));self.assertFalse(frames[-1]['complete']);self.assertEqual(len(calls),1)
 def test_auth_failure_never_creates_connection_and_is_not_reclassified_as_zero_usage(self):
  def auth():raise ValueError('subscription_auth_expired')
  _,frames,row,calls=self.run_gateway(auth=auth)
  self.assertEqual(calls,[]);self.assertEqual(row.get('failure'),'auth_refused');self.assertEqual(row.get('phase'),'auth');self.assertFalse(row.get('gateway_complete'));self.assertIsNone(frames[-1]['usage'])
 def test_completed_stream_bytes_and_consumption_are_unchanged(self):
  import base64
  wire=completed();_,frames,row,_=self.run_gateway(FakeResponse(200,wire,'text/event-stream'))
  self.assertEqual(b''.join(base64.b64decode(f['data']) for f in frames if f['kind']=='chunk'),wire)
  self.assertEqual(frames[-1]['usage']['total_tokens'],600);self.assertTrue(row.get('gateway_complete'));self.assertEqual(row.get('sse_terminal'),'completed');self.assertEqual(row.get('response_usage'),'valid')
 def test_missing_usage_remains_unknown_with_diagnostic_evidence(self):
  _,frames,row,_=self.run_gateway(FakeResponse(200,completed(usage=False),'text/event-stream'))
  self.assertFalse(frames[-1]['complete']);self.assertIsNone(frames[-1]['usage']);self.assertEqual(row.get('sse_terminal'),'completed');self.assertEqual(row.get('response_usage'),'invalid')
 def test_failed_sse_event_keeps_allowlisted_code_and_never_outputs_free_text(self):
  wire=b'data: '+json.dumps({'type':'response.failed','response':{'error':{'code':'rate_limit_exceeded','message':SECRET}}}).encode()+b'\n\n'
  _,frames,row,_=self.run_gateway(FakeResponse(200,wire,'text/event-stream'))
  self.assertFalse(frames[-1]['complete']);self.assertEqual(row.get('sse_terminal'),'failed');self.assertEqual(row.get('upstream_error_code'),'rate_limit_exceeded')
 def test_stream_error_is_categorized_and_closes_both_handles(self):
  response=FakeResponse(200,b'','text/event-stream',OSError(SECRET));_,frames,row,_=self.run_gateway(response)
  self.assertEqual(row.get('phase'),'stream');self.assertEqual(row.get('failure'),'io');self.assertFalse(frames[-1]['complete']);self.assertTrue(response.closed);self.assertTrue(self.connection.closed)
 def test_error_body_read_does_not_reset_the_request_deadline(self):
  from unittest.mock import patch
  now=[0];response=FakeResponse(400,b'{"error":{"code":"invalid_api_key"}}');connection=FakeConnection(response)
  def factory(host,timeout):now[0]=6;return connection
  with patch.object(diag.time,'monotonic',side_effect=lambda:now[0]):
   send=diag.sender(gateway,self.auth,self.record,connection_factory=factory)
   status,_,_=send(json.dumps(self.body).encode(),5)
  self.record.terminal({'kind':'terminal','id':1,'complete':False});self.record.close()
  row=json.loads(self.path.read_text());self.assertEqual(status,400);self.assertEqual(response.reads,[]);self.assertEqual(row.get('failure'),'timeout')
 def test_timeout_is_not_a_retry_or_an_increased_deadline(self):
  _,frames,row,calls=self.run_gateway(error=TimeoutError(SECRET))
  self.assertEqual(row.get('failure'),'timeout');self.assertFalse(frames[-1]['complete']);self.assertEqual(calls,['chatgpt.com'])
 def test_unknown_error_code_and_oversized_body_are_not_persisted(self):
  response=FakeResponse(400,json.dumps({'error':{'code':SECRET,'message':SECRET}}).encode()+b'x'*20000)
  _,_,row,_=self.run_gateway(response)
  self.assertIsNone(row.get('upstream_error_code'));self.assertEqual(row.get('http_status'),400);self.assertLessEqual(sum(response.reads),16385)
 def test_existing_diagnostic_refuses_before_auth_or_transport(self):
  self.path.write_text(SECRET);auth_calls=[];connection_calls=[]
  def auth():auth_calls.append(True);return self.auth()
  def connection(*args,**kwargs):connection_calls.append(True);raise AssertionError('must_not_connect')
  send=diag.sender(gateway,auth,self.record,connection_factory=connection)
  with self.assertRaises(FileExistsError):send(json.dumps(self.body).encode(),60)
  self.assertEqual(auth_calls,[]);self.assertEqual(connection_calls,[]);self.assertEqual(self.path.read_text(),SECRET)
 def test_response_model_variant_is_labelled_without_persisting_model_text(self):
  wire=completed().replace(b'gpt-6-luna',b'gpt-6-luna-synthetic-variant')
  _,frames,row,_=self.run_gateway(FakeResponse(200,wire,'text/event-stream'))
  self.assertTrue(frames[-1]['complete']);self.assertEqual(row.get('response_model_match'),'variant')
  self.assertNotIn('synthetic-variant',self.path.read_text())
 def test_private_file_cannot_be_reused_or_follow_a_symlink(self):
  self.path.write_text(SECRET)
  with self.assertRaises(FileExistsError):self.record.terminal({'kind':'terminal','id':1,'complete':False})
  self.assertEqual(self.path.read_text(),SECRET)
  other=self.path.with_name('symlink.jsonl');other.symlink_to(self.path);r=diag.Recorder('t02-Lm-r1-H0',other);self.addCleanup(r.close)
  with self.assertRaises(FileExistsError):r.terminal({'kind':'terminal','id':1,'complete':False})
  self.assertEqual(self.path.read_text(),SECRET)
 def test_record_limit_and_permissions_are_enforced(self):
  for n in range(1,65):self.record.terminal({'kind':'terminal','id':n,'complete':False})
  self.assertEqual(stat.S_IMODE(self.path.stat().st_mode),0o600);self.assertEqual(len(self.path.read_text().splitlines()),64)
  with self.assertRaises(ValueError):self.record.terminal({'kind':'terminal','id':65,'complete':False})
 def test_wrong_content_type_keeps_only_a_safe_media_kind(self):
  _,frames,row,_=self.run_gateway(FakeResponse(200,SECRET.encode(),'text/html; private='+SECRET))
  self.assertFalse(frames[-1]['complete']);self.assertEqual(row.get('content_kind'),'other');self.assertIsNone(frames[-1]['usage'])
 def test_split_sse_completion_has_the_same_valid_usage_diagnostic(self):
  wire=completed();response=FakeResponse(200,wire,'text/event-stream');read=response.read1
  response.read1=lambda limit:read(min(limit,17))
  _,frames,row,_=self.run_gateway(response)
  self.assertTrue(frames[-1]['complete']);self.assertEqual(row.get('response_usage'),'valid')
 def test_unrecognized_structured_event_type_does_not_change_valid_stream(self):
  wire=b'data: {"type":[]}\n\n'+completed()
  _,frames,row,_=self.run_gateway(FakeResponse(200,wire,'text/event-stream'))
  self.assertTrue(frames[-1]['complete']);self.assertEqual(frames[-1]['usage']['total_tokens'],600);self.assertEqual(row.get('response_usage'),'valid')
 def test_pretransport_refusal_does_not_reuse_previous_request_diagnostics(self):
  connection=FakeConnection(FakeResponse(200,completed(),'text/event-stream'))
  send=diag.sender(gateway,self.auth,self.record,connection_factory=lambda *args,**kwargs:connection)
  g=gateway.Gateway({'model':'gpt-6-luna','effort':'medium'},send)
  first=list(g.handle({'schema_version':1,'id':1,'body':self.body}))[-1];self.record.terminal(first)
  bad=dict(self.body,model='gpt-6-astra')
  second=list(g.handle({'schema_version':1,'id':2,'body':bad}))[-1];self.record.terminal(second);self.record.close()
  rows=[json.loads(line) for line in self.path.read_text().splitlines()]
  self.assertTrue(first['complete']);self.assertFalse(second['provider_request_started']);self.assertEqual(len(connection.requests),1)
  self.assertEqual(rows[1]['phase'],'not_observed');self.assertIsNone(rows[1]['http_status']);self.assertIsNone(rows[1]['response_usage'])
 def test_cleanup_failure_stays_unknown_and_has_a_cleanup_stage(self):
  response=FakeResponse(200,completed(),'text/event-stream')
  def close():raise OSError(SECRET)
  response.close=close
  _,frames,row,_=self.run_gateway(response)
  self.assertFalse(frames[-1]['complete']);self.assertIsNone(frames[-1]['usage']);self.assertEqual(row.get('phase'),'response_cleanup');self.assertEqual(row.get('failure'),'io')
 def test_incomplete_sse_and_unknown_error_fields_never_become_accounting_receipts(self):
  _,frames,row,_=self.run_gateway(FakeResponse(200,b'data: {"type":"response.incomplete","error":{"code":"'+SECRET.encode()+b'"}}\n\n','text/event-stream'))
  self.assertEqual(row.get('sse_terminal'),'incomplete');self.assertIsNone(row.get('upstream_error_code'));self.assertFalse(frames[-1]['complete']);self.assertIsNone(frames[-1]['usage'])

if __name__=='__main__':unittest.main()
