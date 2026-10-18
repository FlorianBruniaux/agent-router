"""Host-side composition over an already-owned private Runtime channel.

No CLI, auth lookup, process launch, campaign creation or Runtime authority issuer.
The trusted parent must retain provenance of these descriptors to its pinned Runtime
process. An arbitrary descriptor or a JSON binding is not that provenance.
"""
import copy
import hashlib
import json
import math
import os
from pathlib import Path
import re
import select
import socket
import stat
import sys
import time
import types

ROOT=Path(__file__).resolve().parent
PINS={'subscription_gateway.py':'97f2c8bdc55f067848f12cffb5a41086aaf5adec3ed8cb36567f6540b2975f5e',
      'relay-controller-offline.py':'c2943ce88c95b7e9aeefaf5ea827b7e9b2b0780737a84b8fe9c2bd5bbd945828'}
BINDING_KEYS={'assignment_id','run_id','attempt','approval_digest','configuration_sha256',
              'account_sha256','controller_sha256','channel_instance_sha256','nonce'}
MAX_LINE=300000


def source_sha256():return hashlib.sha256(Path(__file__).read_bytes()).hexdigest()


def load_pinned(name):
    source=(ROOT/name).read_bytes()
    if hashlib.sha256(source).hexdigest()!=PINS[name]:raise ValueError('frozen_component_changed')
    namespace={'__file__':str(ROOT/name),'__name__':'pinned_'+name.replace('.py','').replace('-','_')}
    exec(compile(source,str(ROOT/name),'exec'),namespace)
    return types.SimpleNamespace(**namespace)


def canonical(value):return json.dumps(value,sort_keys=True,separators=(',',':'),ensure_ascii=False,allow_nan=False).encode()


class PrivateRuntimeIO:
    """Consume private inherited descriptors, never open paths or remote endpoints.

    borrow=True exists for tests/embedding; the parent retains closure responsibility.
    Descriptor type/UID checks do not authenticate the parent. Its retained Runtime
    owner must supply that provenance; no field received from a worker can supply it.
    """
    def __init__(self,read_fd,write_fd,*,timeout=60,borrow=False,clock=time.monotonic):
        if type(timeout) not in (int,float) or not math.isfinite(timeout) or not 0<timeout<=60:
            raise ValueError('bounded_timeout_required')
        for fd in {read_fd,write_fd}:
            if type(fd) is not int or fd<0:raise ValueError('private_descriptor_required')
            meta=os.fstat(fd)
            if meta.st_uid!=os.getuid():raise ValueError('foreign_descriptor')
            if stat.S_ISSOCK(meta.st_mode):
                with socket.socket(fileno=os.dup(fd)) as sock:
                    if sock.family!=socket.AF_UNIX or sock.type!=socket.SOCK_STREAM:
                        raise ValueError('private_unix_stream_required')
            elif stat.S_ISFIFO(meta.st_mode):
                anonymous=(meta.st_nlink==0 or sys.platform=='linux' and
                           re.fullmatch(r'pipe:\[\d+\]',os.readlink('/proc/self/fd/'+str(fd))) is not None)
                if not anonymous:raise ValueError('anonymous_pipe_required')
            else:raise ValueError('private_descriptor_required')
        self.read_fd,self.write_fd=read_fd,write_fd
        self.timeout,self.borrow,self.clock=timeout,borrow,clock
        self.deadline=clock()+timeout;self.buffer=b'';self.closed=False
        for fd in {read_fd,write_fd}:os.set_blocking(fd,False)
        self.parser=load_pinned('subscription_gateway.py').strict_json

    def window(self):self.deadline=self.clock()+self.timeout

    def wait(self,writing=False):
        while True:
            remaining=self.deadline-self.clock()
            if remaining<=0:raise TimeoutError('private_channel_deadline')
            readable,writable,_=select.select([] if writing else [self.read_fd],
                                               [self.write_fd] if writing else [],[],min(.1,remaining))
            if readable or writable:return

    def read(self):
        while True:
            if b'\n' in self.buffer:
                line,self.buffer=self.buffer.split(b'\n',1)
                if len(line)+1>MAX_LINE:raise ValueError('private_frame_bound')
                return self.parser(line)
            if len(self.buffer)>=MAX_LINE:raise ValueError('private_frame_bound')
            self.wait()
            try:chunk=os.read(self.read_fd,min(65536,MAX_LINE-len(self.buffer)))
            except BlockingIOError:continue
            if not chunk:raise EOFError('private_channel_eof')
            self.buffer+=chunk

    def write(self,value):
        raw=canonical(value)+b'\n'
        if len(raw)>MAX_LINE:raise ValueError('private_frame_bound')
        position=0
        while position<len(raw):
            self.wait(True)
            try:count=os.write(self.write_fd,raw[position:])
            except BlockingIOError:continue
            if count<=0:raise OSError('private_channel_write')
            position+=count

    def close(self):
        if self.closed:return
        self.closed=True
        if not self.borrow:
            for fd in {self.read_fd,self.write_fd}:
                try:os.close(fd)
                except OSError:pass


class SessionController:
    def __init__(self,channel,manager,expected_binding,send):
        self.io,self.manager=channel,manager
        self.gateway_module=load_pinned('subscription_gateway.py')
        self.observer_module=load_pinned('relay-controller-offline.py')
        if not isinstance(channel,PrivateRuntimeIO) or not callable(send):raise ValueError('owned_channel_required')
        if (not isinstance(expected_binding,dict) or set(expected_binding)!=BINDING_KEYS
                or type(expected_binding['attempt']) is not int or expected_binding['attempt']!=1
                or not isinstance(expected_binding['run_id'],str)
                or not re.fullmatch('[A-Za-z0-9_-]{1,128}',expected_binding['run_id'])
                or any(not isinstance(expected_binding[k],str) or not re.fullmatch('[0-9a-f]{64}',expected_binding[k])
                       for k in BINDING_KEYS-{'assignment_id','run_id','attempt'})
                or expected_binding['configuration_sha256']!=manager.config_sha
                or expected_binding['account_sha256']!=manager.config['account_sha256']
                or expected_binding['controller_sha256']!=source_sha256()):
            raise ValueError('expected_runtime_binding')
        state=manager.snapshot()
        self.row=next((r for r in state['rows'] if r['assignment_id']==expected_binding['assignment_id']),None)
        if self.row is None:raise ValueError('sealed_assignment_required')
        self.binding=copy.deepcopy(expected_binding);self.send=send
        self.started=False;self.stopped=False;self.finished=False

    def control(self,kind,**fields):
        return {'schema_version':1,'kind':kind,'binding':copy.deepcopy(self.binding),**fields}

    def bound_control(self,frame,kind,extra=()):
        return (isinstance(frame,dict) and set(frame)=={'schema_version','kind','binding'}|set(extra)
                and type(frame['schema_version']) is int and frame['schema_version']==1
                and frame['kind']==kind and canonical(frame['binding'])==canonical(self.binding))

    def unknown(self,reason):
        self.stopped=True
        try:self.manager.mark_unknown(self.row['assignment_id'],reason)
        except (ValueError,OSError):pass

    def request(self,frame):
        if (self.stopped or not isinstance(frame,dict) or set(frame)!={'schema_version','id','body'}
                or type(frame['schema_version']) is not int or frame['schema_version']!=1
                or type(frame['id']) is not int or not isinstance(frame['body'],dict)):
            raise ValueError('runtime_request_shape')
        body=json.dumps(frame['body'],ensure_ascii=False,separators=(',',':')).encode()
        if len(body)>262144:raise ValueError('request_body_bound')
        identifier=frame['id'];request_hash=hashlib.sha256(body).hexdigest()
        self.manager.before_request(self.row['assignment_id'],identifier,request_hash)
        self.observer.begin(identifier,request_hash)
        terminal=None
        for output in self.gateway.handle(frame):
            if output['kind']=='chunk':
                self.observer.feed(canonical(output)+b'\n')
                self.io.write(output)
            else:
                terminal=output
                try:
                    if output.get('complete') is True and output.get('observed_response_model')!=self.row['model']:
                        raise ValueError('unapproved_model_variant')
                    self.observer.feed(canonical(output)+b'\n')
                    self.manager.ingest(self.row['assignment_id'],self.observer.receipt())
                except (ValueError,OSError):self.unknown('terminal_observation_unknown')
                self.io.write(output)
        if terminal is None:raise ValueError('missing_gateway_terminal')
        state=self.manager.observation()
        self.stopped=self.stopped or state['stop_reason'] is not None
        # This barrier is control-plane only: Rust waits before closing worker HTTP.
        self.io.write(self.control('controller_request_finished',id=identifier,
                                  **{'continue':not self.stopped,'reason':state['stop_reason']}))

    def cleanup(self,frame):
        if not self.bound_control(frame,'runtime_cleanup',{'cleanup','native_status','proof_sha256'}):
            raise ValueError('runtime_cleanup_binding')
        if frame['cleanup'] not in ('namespace_terminated','unknown') or frame['native_status'] not in ('completed','failed','interrupted'):
            raise ValueError('runtime_cleanup_state')
        proof=None
        if frame['cleanup']=='namespace_terminated':
            if not isinstance(frame['proof_sha256'],str) or not re.fullmatch('[0-9a-f]{64}',frame['proof_sha256']):
                raise ValueError('runtime_cleanup_proof')
            proof={'run_id':self.binding['run_id'],'approval_digest':self.binding['approval_digest'],
                   'proof_sha256':frame['proof_sha256'],'writers_excluded':True}
        elif frame['proof_sha256'] is not None:raise ValueError('unknown_cleanup_has_no_proof')
        self.manager.finish_trial(self.row['assignment_id'],frame['native_status'],proof)
        observation=self.manager.observation()
        self.io.write(self.control('controller_finished',execution_authorized=False,observation=observation))
        self.finished=True
        return observation

    def run(self):
        try:
            ready=self.io.read()
            if not self.bound_control(ready,'runtime_ready'):raise ValueError('runtime_ready_binding')
            self.manager.begin_trial(self.row['assignment_id'],self.binding['run_id'],self.binding['approval_digest'])
            self.started=True;self.io.window()
            self.gateway=self.gateway_module.Gateway(self.row,self.send,clock=self.io.clock)
            self.gateway.deadline=self.io.deadline
            self.observer=self.observer_module.PipeObserver(self.row['assignment_id'],self.row['model'],self.row['effort'],
                started_at=self.io.clock(),clock=self.io.clock)
            self.io.write(self.control('controller_ready'))
            while True:
                frame=self.io.read()
                if isinstance(frame,dict) and frame.get('kind')=='runtime_cleanup':return self.cleanup(frame)
                try:
                    self.request(frame)
                except (ValueError,OSError,EOFError,TimeoutError):
                    self.unknown('request_pipeline_unknown')
                    self.io.write(self.control('controller_stop',reason='request_pipeline_unknown'))
                    # No new request may enter after a stop. Keep only the proof lane.
                    return self.cleanup(self.io.read())
        except (ValueError,OSError,EOFError,TimeoutError):
            if self.started:
                self.unknown('private_session_unknown')
                try:self.io.write(self.control('controller_stop',reason='private_session_unknown'))
                except (ValueError,OSError,TimeoutError):pass
            raise
        finally:self.io.close()
