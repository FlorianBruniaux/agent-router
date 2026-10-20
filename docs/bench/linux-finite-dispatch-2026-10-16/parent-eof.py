#!/usr/bin/env python3
"""Explicit local qualification only. Never deletes retained private state."""
import json,os,pathlib,signal,sqlite3,subprocess,time
root=pathlib.Path(os.environ['AR_FINITE_ROOT'])
assert root.is_absolute() and not root.exists()
repo=pathlib.Path(__file__).resolve().parents[3]
name='finite_client::linux_backend::e2e_tests::q01_parent_eof_controller'
built=subprocess.run(['cargo','test','--release','-p','router-runtime','--lib','--no-run','--offline','--locked','--message-format=json'],cwd=repo,stdout=subprocess.PIPE,stderr=subprocess.PIPE,check=True)
artifacts=[json.loads(line) for line in built.stdout.decode().splitlines()]
bins=[pathlib.Path(a['executable']) for a in artifacts if a.get('reason')=='compiler-artifact' and a.get('executable') and a['target']['name']=='router_runtime' and a['profile']['test']]
assert len(bins)==1,bins
binary=bins[0]
env={**os.environ,'AR_FINITE_ROOT':str(root)}
log=root.with_name(root.name+'-controller.log').open('wb')
process=subprocess.Popen([str(binary),'--ignored','--exact',name,'--nocapture','--test-threads=1'],env=env,stdout=log,stderr=subprocess.STDOUT)
try:
 deadline=time.monotonic()+180
 while not (root/'provider.json').exists():
  assert process.poll() is None,'controller exited before positive request'
  assert time.monotonic()<deadline,'provider request deadline'
  time.sleep(.05)
 observed=json.loads((root/'provider.json').read_text());assert observed['model_calls_started']==0
 db=sqlite3.connect(f'file:{root}/runtime/runtime.sqlite3?mode=ro',uri=True)
 lease=json.loads(db.execute('SELECT record FROM attempt_leases WHERE run_id=? AND attempt=1',('q01',)).fetchone()[0]);db.close()
 assert lease['namespace_required'] and lease['host_state_pending']
 lease_path=root/'runtime/leases'/lease['name']
 assert lease_path.read_bytes()!=b'router-runtime-lease-v1:namespace-clean\n','task must be live before EOF'
 pins=json.loads((root/'pins.json').read_text());assert pathlib.Path(pins['manifest']['private_state']).is_dir()
 # The root PID is the exact Popen handle, not a name/pattern match.
 assert pathlib.Path(f'/proc/{process.pid}/exe').resolve()==binary.resolve()
 process.send_signal(signal.SIGTERM);process.wait(timeout=10);log.close()
 assert process.returncode==-signal.SIGTERM
 deadline=time.monotonic()+10
 while lease_path.read_bytes()!=b'router-runtime-lease-v1:namespace-clean\n':
  assert time.monotonic()<deadline,'namespace helper did not resolve parent EOF'
  time.sleep(.02)
 pathlib.Path(root/'parent-eof.json').write_text(json.dumps({'controller_pid':process.pid,'controller_signal':'SIGTERM','positive_request_observed':observed,'namespace_lease_marker':'namespace-clean','namespace_required':lease['namespace_required'],'host_state_pending':lease['host_state_pending'],'model_calls_started':0},indent=2)+'\n')
 result=subprocess.run([str(binary),'--ignored','--exact','finite_client::linux_backend::e2e_tests::q01_parent_eof_recovery_retains_unknown_private_cleanup','--nocapture','--test-threads=1'],env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=30)
 (root/'recovery.log').write_bytes(result.stdout);assert result.returncode==0,result.stdout.decode()[-2000:]
 print((root/'eof-recovery.json').read_text())
finally:
 if process.poll() is None:
  process.terminate();process.wait(timeout=10)
 log.close()
