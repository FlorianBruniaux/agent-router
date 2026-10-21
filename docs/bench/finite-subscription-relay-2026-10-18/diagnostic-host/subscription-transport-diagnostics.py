"""Host-only, bounded diagnostics around the unchanged pinned HTTPS sender.

No retry, accounting receipt, launch authority or exception/body logging.
"""
import http.client
import json
import math
import os
from pathlib import Path
import re
import ssl
import stat
import time

ERROR_CODES=frozenset(('invalid_api_key','authentication_error','invalid_request_error',
 'model_not_found','unsupported_value','unsupported_parameter','rate_limit_exceeded',
 'insufficient_quota','server_error','context_length_exceeded'))
SSE_KINDS={'error':'error','response.failed':'failed','response.incomplete':'incomplete','response.completed':'completed'}
MAX_ERROR=16384
MAX_EVENT=262144

def error_kind(error,phase):
 if phase=='auth':return 'auth_refused'
 if isinstance(error,ssl.SSLCertVerificationError):return 'tls_certificate'
 if isinstance(error,ssl.SSLError):return 'tls'
 if isinstance(error,TimeoutError):return 'timeout'
 if isinstance(error,http.client.HTTPException):return 'http_protocol'
 if isinstance(error,OSError):return 'io'
 return 'invalid_transport_state'

def strict(raw):
 def unique(items):
  out={}
  for k,v in items:
   if k in out:raise ValueError('duplicate_key')
   out[k]=v
  return out
 def invalid(_):raise ValueError('nonfinite')
 return json.loads(raw,object_pairs_hook=unique,parse_constant=invalid)

class Recorder:
 def __init__(self,assignment,path):
  if not isinstance(assignment,str) or re.fullmatch(r't(?:02|05|08)-(?:Lm|Lx|Am|Ax)-r1-H[01]',assignment) is None:raise ValueError('sealed_diagnostic_assignment')
  self.path=Path(path)
  if not self.path.is_absolute() or self.path.parent.is_symlink():raise ValueError('private_diagnostic_directory')
  parent=self.path.parent.stat()
  if not stat.S_ISDIR(parent.st_mode) or parent.st_uid!=os.getuid() or stat.S_IMODE(parent.st_mode)!=0o700:raise ValueError('private_diagnostic_directory')
  self.assignment=assignment;self.fd=None;self.count=0;self.closed=False;self.current={};self.pending=b'';self.model=None;self.gateway=None
 def start(self,raw,gateway):
  if self.closed or self.count>=64:raise ValueError('diagnostic_record_bound_or_closed')
  self.ensure_file()
  self.current={'phase':'auth','failure':None,'http_status':None,'content_kind':None,'upstream_error_code':None,'sse_terminal':None,'response_usage':None,'response_model_match':None}
  self.pending=b'';self.gateway=gateway;self.model=gateway.strict_json(raw).get('model')
 def failure(self,error):self.current['failure']=error_kind(error,self.current.get('phase'))
 def error_code(self,value):
  if not isinstance(value,dict):return
  error=value.get('error')
  if not isinstance(error,dict) and isinstance(value.get('response'),dict):error=value['response'].get('error')
  code=error.get('code') if isinstance(error,dict) else None
  if isinstance(code,str) and code in ERROR_CODES:self.current['upstream_error_code']=code
 def feed(self,chunk):
  if self.current.get('sse_terminal')=='evidence_bound':return
  self.pending=(self.pending+chunk).replace(b'\r\n',b'\n')
  if len(self.pending)>MAX_EVENT+65536:
   self.pending=b'';self.current['sse_terminal']='evidence_bound';return
  while b'\n\n' in self.pending:
   block,self.pending=self.pending.split(b'\n\n',1)
   if len(block)>MAX_EVENT:self.current['sse_terminal']='evidence_bound';self.pending=b'';return
   raw=b'\n'.join(x[5:].lstrip(b' ') for x in block.split(b'\n') if x.startswith(b'data:'))
   if not raw or raw==b'[DONE]':continue
   try:event=strict(raw)
   except (ValueError,RecursionError):self.current['sse_terminal']='invalid_event';continue
   if not isinstance(event,dict):continue
   event_type=event.get('type')
   kind=SSE_KINDS.get(event_type) if isinstance(event_type,str) else None
   if kind:
    self.current['sse_terminal']=kind;self.error_code(event)
    if kind=='completed':
     response=event.get('response');actual=response.get('model') if isinstance(response,dict) else None
     self.current['response_model_match']='invalid' if not isinstance(actual,str) else 'exact' if actual==self.model else 'variant' if isinstance(self.model,str) and actual.startswith(self.model+'-') else 'other'
     try:self.gateway.observed_usage(event.get('response'),self.model)
     except (ValueError,TypeError,AttributeError,KeyError):self.current['response_usage']='invalid'
     else:self.current['response_usage']='valid'
  if len(self.pending)>MAX_EVENT:self.pending=b'';self.current['sse_terminal']='evidence_bound'
 def ensure_file(self):
  if self.fd is None:
   self.fd=os.open(self.path,os.O_WRONLY|os.O_CREAT|os.O_EXCL|os.O_NOFOLLOW,0o600)
   meta=os.fstat(self.fd)
   if not stat.S_ISREG(meta.st_mode) or meta.st_uid!=os.getuid() or meta.st_nlink!=1 or stat.S_IMODE(meta.st_mode)!=0o600:
    self.close();raise ValueError('private_diagnostic_file')
 def terminal(self,frame):
  if self.closed or self.count>=64 or type(frame.get('id')) is not int or frame['id']!=self.count+1:raise ValueError('diagnostic_record_bound_or_order')
  row={'schema_version':1,'assignment_id':self.assignment,'request_id':frame['id'],
   'phase':self.current.get('phase','not_observed'),'failure':self.current.get('failure'),
   'http_status':self.current.get('http_status'),'content_kind':self.current.get('content_kind'),'upstream_error_code':self.current.get('upstream_error_code'),
   'sse_terminal':self.current.get('sse_terminal'),'response_usage':self.current.get('response_usage'),
   'response_model_match':self.current.get('response_model_match'),
   'gateway_complete':frame.get('complete') is True,'accounting_authority':False}
  raw=json.dumps(row,sort_keys=True,separators=(',',':'),allow_nan=False).encode()+b'\n'
  if len(raw)>1024:raise ValueError('diagnostic_record_bound')
  self.ensure_file()
  offset=0
  while offset<len(raw):
   n=os.write(self.fd,raw[offset:])
   if n<=0:raise OSError('diagnostic_write_failed')
   offset+=n
  os.fsync(self.fd);self.count+=1;self.current={};self.pending=b'';self.model=None;self.gateway=None
 def close(self):
  if self.fd is not None:os.close(self.fd);self.fd=None
  self.closed=True;self.pending=b''

class ObservedResponse:
 def __init__(self,response,recorder):self.response=response;self.recorder=recorder;self.status=response.status
 def getheader(self,*args):return self.response.getheader(*args)
 def read1(self,limit):
  self.recorder.current['phase']='stream'
  try:chunk=self.response.read1(limit)
  except (OSError,http.client.HTTPException) as error:self.recorder.failure(error);raise
  self.recorder.feed(chunk);return chunk
 def close(self):
  try:self.response.close()
  except OSError as error:
   self.recorder.current['phase']='response_cleanup';self.recorder.failure(error);raise

class ObservedConnection:
 def __init__(self,connection,recorder,deadline):self.connection=connection;self.recorder=recorder;self.deadline=deadline
 @property
 def sock(self):return self.connection.sock
 def request(self,*args):
  self.recorder.current['phase']='request'
  try:return self.connection.request(*args)
  except (OSError,http.client.HTTPException) as error:self.recorder.failure(error);raise
 def getresponse(self):
  self.recorder.current['phase']='headers'
  try:response=self.connection.getresponse()
  except (OSError,http.client.HTTPException) as error:self.recorder.failure(error);raise
  status=response.status
  media=response.getheader('Content-Type','')
  media=media.split(';',1)[0].strip().lower() if isinstance(media,str) else ''
  self.recorder.current['content_kind']='sse' if media=='text/event-stream' else 'json' if media=='application/json' else 'other' if media else 'missing'
  if type(status) is int and 100<=status<=599:self.recorder.current['http_status']=status
  if status!=200:
   self.recorder.current['phase']='http_error';body=bytearray()
   try:
    while len(body)<=MAX_ERROR:
     remaining=self.deadline-time.monotonic()
     if remaining<=0:raise TimeoutError('diagnostic_deadline')
     if self.sock is not None:self.sock.settimeout(remaining)
     part=response.read1(MAX_ERROR+1-len(body))
     if not part:break
     body.extend(part)
     if len(body)>MAX_ERROR:break
    if len(body)<=MAX_ERROR:
     try:self.recorder.error_code(strict(bytes(body)))
     except (ValueError,RecursionError):pass
   except (OSError,http.client.HTTPException) as error:self.recorder.failure(error)
   finally:response.close()
  return ObservedResponse(response,self.recorder)
 def close(self):return self.connection.close()

def sender(gateway,auth_provider,recorder,*,connection_factory=http.client.HTTPSConnection):
 """Reuse the fixed pinned destination, headers, request bytes and no-retry sender."""
 deadline=[None]
 def auth():
  recorder.current['phase']='auth'
  try:return auth_provider()
  except (ValueError,OSError) as error:recorder.failure(error);raise
 def connection(host,timeout):
  recorder.current['phase']='connection_create'
  try:return ObservedConnection(connection_factory(host,timeout=timeout),recorder,deadline[0])
  except (OSError,http.client.HTTPException) as error:recorder.failure(error);raise
 fixed=gateway.subscription_sender(auth,connection_factory=connection)
 def send(raw,remaining):
  if type(remaining) not in (int,float) or not math.isfinite(remaining) or remaining<=0:raise ValueError('diagnostic_remaining_deadline')
  deadline[0]=time.monotonic()+remaining
  recorder.start(raw,gateway)
  try:return fixed(raw,remaining)
  except (ValueError,OSError,http.client.HTTPException) as error:
   if recorder.current.get('failure') is None:recorder.failure(error)
   raise
 return send
