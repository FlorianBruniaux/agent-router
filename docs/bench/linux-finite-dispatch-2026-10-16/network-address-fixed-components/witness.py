"""Local guard checks. No Codex, model, credentials or external request."""
import errno,hashlib,json,pathlib,socket,subprocess,sys,tempfile,threading

mode=sys.argv[1];assert mode in {'positive','extra-interface'}
guard=pathlib.Path('/diag/guard')
with tempfile.TemporaryDirectory(prefix='guard-fixed-',dir='/tmp') as tmp:
 root=pathlib.Path(tmp);home=root/'home';workspace=root/'workspace'
 (home/'tmp').mkdir(parents=True,mode=0o700);workspace.mkdir(mode=0o700)
 reads=[]
 for i in range(5):
  p=root/f'asset-{i}';p.mkdir(mode=0o700);reads.append(str(p))
 broker=socket.socket();broker.bind(('0.0.0.0',0));broker.listen(4);port=broker.getsockname()[1]
 assert broker.getsockopt(socket.SOL_SOCKET,socket.SO_REUSEADDR)==0
 assert broker.getsockopt(socket.SOL_SOCKET,socket.SO_REUSEPORT)==0
 base=[str(guard),'--restrict-v1',str(port),str(home),str(workspace),'0','5',*reads,'--','/usr/bin/python3','-B','-c']
 report={'scope':'corrected guard and exclusive reservation component, not full Runtime/native replay','guard_sha256':hashlib.sha256(guard.read_bytes()).hexdigest(),'model_calls_started':0,'external_network_used':False,'mode':mode}
 if mode=='extra-interface':
  sentinel=home/'tmp/should-not-execute'
  result=subprocess.run([*base,'import pathlib,sys;pathlib.Path(sys.argv[1]).write_text("BAD")',str(sentinel)],env={'PATH':'/usr/bin:/bin','HOME':str(home)},capture_output=True,timeout=5)
  report.update(exit_code=result.returncode,stderr=result.stderr.decode(),child_executed=sentinel.exists())
  assert result.returncode==125 and result.stderr==b'linux-isolation:no-active-interface:0\n' and not sentinel.exists(),report
  report['status']='PASS_EXTRA_INTERFACE_REFUSED_BEFORE_EXEC'
 else:
  conflicts=[]
  for reuseaddr,reuseport in [(False,False),(True,False),(False,True),(True,True)]:
   candidate=socket.socket();candidate.setsockopt(socket.SOL_SOCKET,socket.SO_REUSEADDR,int(reuseaddr));candidate.setsockopt(socket.SOL_SOCKET,socket.SO_REUSEPORT,int(reuseport))
   try:candidate.bind(('127.0.0.2',port));raise AssertionError('foreign listener acquired broker port')
   except OSError as e:assert e.errno==errno.EADDRINUSE;conflicts.append({'reuseaddr':reuseaddr,'reuseport':reuseport,'errno':e.errno})
   finally:candidate.close()
  other=socket.socket();other.bind(('127.0.0.1',0));other.listen(4)
  marker='owned-broker-diagnostic';accepted=[];broker.settimeout(3)
  def serve():
   for _ in range(2):
    try:conn,addr=broker.accept()
    except TimeoutError:return
    accepted.append(addr[0]);conn.sendall(marker.encode());conn.close()
  thread=threading.Thread(target=serve);thread.start()
  child='''import json,socket,sys
rows=[]
for label,ip,port in json.loads(sys.argv[1]):
 s=socket.socket();s.settimeout(1)
 try:
  s.connect((ip,port));data=s.recv(128).decode();rows.append({'label':label,'connected':True,'owned_marker':data==sys.argv[2],'errno':None})
 except OSError as e:rows.append({'label':label,'connected':False,'errno':e.errno})
 finally:s.close()
print(json.dumps(rows))
'''
  endpoints=[['broker_primary','127.0.0.1',port],['broker_alias','127.0.0.2',port],['other_port','127.0.0.1',other.getsockname()[1]],['external_same_port','192.0.2.1',port]]
  result=subprocess.run([*base,child,json.dumps(endpoints),marker],env={'PATH':'/usr/bin:/bin','HOME':str(home)},capture_output=True,timeout=6)
  thread.join(timeout=4);assert not thread.is_alive()
  if result.returncode!=0:
   report.update(status='BLOCKED_POSITIVE_CONTROL',exit_code=result.returncode,stderr=result.stderr.decode(),owned_accepts=len(accepted))
   print(json.dumps(report,indent=2));raise SystemExit(1)
  rows=json.loads(result.stdout);assert all(r['connected'] and r['owned_marker'] for r in rows[:2]);assert not rows[2]['connected'] and rows[2]['errno'] in {errno.EPERM,errno.EACCES};assert not rows[3]['connected'] and rows[3]['errno']==errno.ENETUNREACH;assert len(accepted)==2
  report.update(status='PASS_EXCLUSIVE_LOOPBACK_COMPONENT',foreign_bind_refusals=conflicts,connections=rows,owned_accepts=len(accepted),exit_code=result.returncode,stderr=result.stderr.decode());other.close()
 broker.close();print(json.dumps(report,indent=2))
