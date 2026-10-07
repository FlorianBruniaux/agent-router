"""Private controller-cancellation extension; leaves reviewed F03a artifacts intact."""
from __future__ import annotations
import argparse
import errno
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import socket
import time


def qualify(e):
    try:
        n=e['native']; rpc=e['rpc'];sent=e['sent']
        if n['failure'] is not None or n['process_exit_code'] != 0: return False
        if any(n.get(k) is not True for k in ('cleanup_complete','process_group_gone','output_complete','turn_correlated')): return False
        if any(type(n.get(k)) is not int or n[k] != 0 for k in ('broker_http_requests','provider_http_requests','broker_receives','provider_receives','reserved')): return False
        if n['decisions'] != [] or n.get('provider_body_hashes') != [] or n.get('broker_body_hashes') != []: return False
        if n.get('turn_status')!='interrupted' or n.get('final_output_observed') is not False: return False
        if n.get('provider_rejected_connections')!=0 or n.get('broker_rejected_connections')!=0: return False
        if e.get('native_launch_count')!=1: return False
        checks=e.get('listener_checks')
        if not isinstance(checks,list) or len(checks)!=2: return False
        if any(not isinstance(x,dict) or x.get('fd_closed') is not True or x.get('connect_errno')!=errno.ECONNREFUSED for x in checks): return False
        if any(e.get(k) is not True for k in ('provider_handlers_stopped','broker_handlers_stopped','listeners_closed','pin_unchanged')): return False
        if e['provider_accepted_connections'] != 0 or e['provider_rejected_connections'] != 0: return False
        def one(rows):
            if len(rows)!=1: raise ValueError('missing or duplicated evidence')
            return rows[0]
        thread=one([m for m in rpc if m.get('id')==2])['result']['thread']['id']
        turn=one([m for m in rpc if m.get('id')==3])['result']['turn']['id']
        starts=[m for m in sent if m['message'].get('method')=='turn/start']
        if one(starts)['message']['params']['threadId']!=thread: return False
        interrupt=one([m for m in sent if m['message'].get('method')=='turn/interrupt'])
        if interrupt['message']!={'id':9001,'method':'turn/interrupt','params':{'threadId':thread,'turnId':turn}}: return False
        boundary=interrupt['after_received']
        if type(boundary) is not int or not 1<=boundary<=len(rpc): return False
        wait=rpc[boundary-1]
        if wait.get('method')!='error': return False
        params=wait['params']
        if params.get('willRetry') is not True or params.get('threadId')!=thread or params.get('turnId')!=turn: return False
        ack=one([m for m in rpc if m.get('id')==9001])
        if 'error' in ack or not isinstance(ack.get('result'),dict) or rpc.index(ack)<boundary: return False
        end=one([m for m in rpc if m.get('method')=='turn/completed'])
        if rpc.index(end)<boundary: return False
        return (end['params']['threadId']==thread and end['params']['turn']['id']==turn
            and end['params']['turn']['status']=='interrupted')
    except (KeyError,TypeError,ValueError,IndexError):
        return False


def load(name,path):
    spec=importlib.util.spec_from_file_location(name,path)
    module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
    return module


def make_interrupt(message, thread, turn):
    if not all(isinstance(value,str) and 0<len(value)<=256 for value in (thread,turn)): return None
    params=message.get('params') if isinstance(message,dict) else None
    if not isinstance(params,dict) or message.get('method')!='error': return None
    if params.get('willRetry') is not True or params.get('threadId')!=thread or params.get('turnId')!=turn: return None
    return {'id':9001,'method':'turn/interrupt','params':{'threadId':thread,'turnId':turn}}


def main():
    parser=argparse.ArgumentParser()
    parser.add_argument('--repo',type=Path,required=True)
    parser.add_argument('--binary',type=Path,required=True)
    parser.add_argument('--state',type=Path,required=True)
    args=parser.parse_args();os.umask(0o077)
    p=load('f03a_frozen',args.repo/'docs/bench/physical-send-feasibility-2026-10-07/probe.py')
    h=load('f03a_helper',args.repo/'docs/bench/egress-admission/codex_appserver_local.py')
    prior=load('f03a_prior',args.repo/'docs/bench/native-integration-u06/probe.py')
    binary=args.binary.resolve(strict=True)
    if h.sha256_file(binary)!=p.PIN: raise ValueError('native pin mismatch')
    args.state.mkdir(mode=0o700,exist_ok=False)
    source_paths=[Path(__file__),Path(__file__).with_name('test_outage.py'),Path(p.__file__),Path(h.__file__),Path(prior.__file__)]
    source_hashes={str(path):h.sha256_file(path) for path in source_paths}
    clients=[];servers=[]
    class TrackedProvider(h.LocalProvider):
        def __init__(self,*a,**kw):
            super().__init__(*a,**kw);servers.append(self)
    class InterruptingClient(h.AppServer):
        def __init__(self,*a,**kw):
            super().__init__(*a,**kw)
            self.sent=[];self.received=[];self.interrupted_at=None
            self.thread_id=None;self.turn_id=None
            clients.append(self)
        def send(self,message):
            super().send(message)
            self.sent.append({'after_received':len(self.received),'message':message})
        def receive(self,deadline):
            if self.interrupted_at is not None: deadline=min(deadline,self.interrupted_at+3)
            message=super().receive(deadline);self.received.append(message)
            if message.get('id')==2: self.thread_id=message.get('result',{}).get('thread',{}).get('id')
            if message.get('id')==3: self.turn_id=message.get('result',{}).get('turn',{}).get('id')
            request=make_interrupt(message,self.thread_id,self.turn_id)
            if self.interrupted_at is None and request is not None:
                self.interrupted_at=time.monotonic()
                self.send(request)
            return message
    h.LocalProvider=TrackedProvider;h.AppServer=InterruptingClient
    started=time.monotonic()
    native=p.run_case(h,prior,binary,'broker_absent',args.state/'broker_absent')
    elapsed=time.monotonic()-started
    rpc=json.loads((args.state/'broker_absent/rpc.json').read_text())
    client=clients[0] if len(clients)==1 else None
    closed=[]
    for server in servers:
        sock=socket.socket();sock.settimeout(1)
        try: code=sock.connect_ex(('127.0.0.1',server.server_address[1]))
        finally:sock.close()
        closed.append({'fd_closed':server.socket.fileno()==-1,'connect_errno':code})
    e={'native':native,'rpc':rpc,'sent':client.sent if client else [],
        'native_launch_count':len(clients),'provider_accepted_connections':servers[0].accepted_connections if servers else None,
        'provider_rejected_connections':servers[0].rejected_connections if servers else None,
        'provider_handlers_stopped':len(servers)==2 and all(not t.is_alive() for t in servers[0].handler_threads),
        'broker_handlers_stopped':len(servers)==2 and all(not t.is_alive() for t in servers[1].handler_threads),
        'listeners_closed':len(closed)==2 and all(x['fd_closed'] and x['connect_errno']==errno.ECONNREFUSED for x in closed),
        'listener_checks':closed,'pin_unchanged':h.sha256_file(binary)==p.PIN,
        'source_hashes_unchanged':all(h.sha256_file(Path(path))==digest for path,digest in source_hashes.items()),
        'source_sha256':source_hashes,'elapsed_seconds':round(elapsed,3),
        'protocol_limit_seconds':15,'post_interrupt_limit_seconds':3,'global_admission':'NO_GO',
        'original_v2_unmodified':True}
    e['candidate_pass']=qualify(e) and e['source_hashes_unchanged'] and len(clients)==1
    (args.state/'private-evidence.json').write_text(json.dumps(e,indent=2)+'\n')
    report={k:v for k,v in e.items() if k not in ('rpc','sent','source_sha256')}
    report['interrupt_requests']=sum(x['message'].get('method')=='turn/interrupt' for x in e['sent'])
    report['interrupt_acknowledged']=any(x.get('id')==9001 and 'result' in x and 'error' not in x for x in rpc)
    report['retry_notifications']=sum(x.get('method')=='error' and x.get('params',{}).get('willRetry') is True for x in rpc)
    (args.state/'report.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps(report,indent=2))
    return 0 if e['candidate_pass'] else 2

if __name__=='__main__':raise SystemExit(main())
