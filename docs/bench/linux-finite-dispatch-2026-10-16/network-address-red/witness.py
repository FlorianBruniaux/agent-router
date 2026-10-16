"""Exact guard, local sockets only. Never starts Codex or a model."""
import hashlib,json,os,pathlib,socket,subprocess,tempfile

guard=pathlib.Path('/diag/guard')
assert hashlib.sha256(guard.read_bytes()).hexdigest()=='3bf55ab3a0e35946225f17937c14f9e18a7efbe7d17b6476dc5cc41f5ed79934'
with tempfile.TemporaryDirectory(prefix='guard-ip-',dir='/tmp') as tmp:
 root=pathlib.Path(tmp);home=root/'home';workspace=root/'workspace'
 (home/'tmp').mkdir(parents=True,mode=0o700);workspace.mkdir(mode=0o700)
 reads=[]
 for i in range(5):
  p=root/f'asset-{i}';p.mkdir(mode=0o700);reads.append(str(p))
 broker=socket.socket();broker.bind(('127.0.0.1',0));broker.listen(4);port=broker.getsockname()[1]
 foreign=socket.socket();foreign.bind(('127.0.0.2',port));foreign.listen(4)
 other=socket.socket();other.bind(('127.0.0.1',0));other.listen(4)
 child='''import json,socket
rows=[]
for label,ip,port in json.loads(__import__('sys').argv[1]):
 s=socket.socket();s.settimeout(1)
 try:s.connect((ip,port));rows.append({'label':label,'address':ip,'port':port,'connected':True,'errno':None})
 except OSError as e:rows.append({'label':label,'address':ip,'port':port,'connected':False,'errno':e.errno})
 finally:s.close()
print(json.dumps(rows))
'''
 endpoints=[['owned_broker','127.0.0.1',port],['foreign_same_port','127.0.0.2',port],['other_port','127.0.0.1',other.getsockname()[1]]]
 args=[str(guard),'--restrict-v1',str(port),str(home),str(workspace),'0','5',*reads,'--','/usr/bin/python3','-B','-c',child,json.dumps(endpoints)]
 result=subprocess.run(args,env={'PATH':'/usr/bin:/bin','HOME':str(home)},capture_output=True,timeout=5)
 report={'scope':'exact guard component; no bwrap/native model/full Runtime replay','guard_sha256':hashlib.sha256(guard.read_bytes()).hexdigest(),'model_calls_started':0,'exit_code':result.returncode,'stderr':result.stderr.decode(),'external_network_used':False}
 if result.returncode==0:
  rows=json.loads(result.stdout);report['connections']=rows
  assert rows[0]['connected'] and not rows[2]['connected'],rows
  report['status']='FAIL_BROKER_ADDRESS_ISOLATION_CONFIRMED' if rows[1]['connected'] else 'FOREIGN_ADDRESS_REFUSED'
 else:report['status']='BLOCKED_COMPONENT_TEST'
 print(json.dumps(report,indent=2))
 broker.close();foreign.close();other.close()
