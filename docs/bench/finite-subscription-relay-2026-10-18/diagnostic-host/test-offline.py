"""Replay legacy host tests plus diagnostic regressions with all networking denied."""
import ast,hashlib,importlib.util,json,os,pathlib,platform,re,shutil,subprocess,sys,tempfile
ROOT=pathlib.Path(__file__).resolve().parent
FROZEN=ROOT.parent/'host'
spec=importlib.util.spec_from_file_location('historical_offline_runner',FROZEN/'test-offline.py')
frozen=importlib.util.module_from_spec(spec);spec.loader.exec_module(frozen)

def main():
 historical=json.loads((FROZEN/'recovery.json').read_text())['files']
 assert all(hashlib.sha256((FROZEN/r['path']).read_bytes()).hexdigest()==r['sha256'] for r in historical),'historical_snapshot_changed'
 controller=ROOT.parent/'controller';packets=ROOT.parents[3]/'crates/router-runtime/examples/campaign/packets.json'
 assert hashlib.sha256(packets.read_bytes()).hexdigest()=='2d4d225e528fff5ba402e7265140521f212d7b4a3443c3dd62f1db00f9e0281a','public_packets_changed'
 tree=ast.parse((ROOT/'relay-controller-subscription-launcher.py').read_text())
 pins=next(ast.literal_eval(n.value) for n in tree.body if isinstance(n,ast.Assign) and any(isinstance(t,ast.Name) and t.id=='PINS' for t in n.targets))
 assert len(pins)==7 and all(hashlib.sha256((controller/name).read_bytes()).hexdigest()==h for name,h in pins.items()),'qualified_controller_changed'
 rows=[]
 with tempfile.TemporaryDirectory(prefix='ar-diagnostic-host-offline-') as tmp:
  scratch=pathlib.Path(tmp).resolve()
  for name in ['relay-controller-subscription-launcher.py','relay-controller-subscription-parent.py','subscription-transport-diagnostics.py','test_transport_diagnostics.py','test_diagnostic_host.py']:shutil.copyfile(ROOT/name,scratch/name)
  for _,name,_ in frozen.SUITES:
   text=(FROZEN/name).read_text()
   if name!=frozen.SUITES[2][1]:
    text=frozen.replace_once(text,frozen.HISTORICAL_CONTROLLER,str(controller));text=frozen.replace_once(text,"dir='/private/tmp'",'dir='+repr(str(scratch)))
    if name==frozen.SUITES[1][1]:text=frozen.replace_once(text,frozen.HISTORICAL_PACKETS,str(packets))
    # Coded test transports ignore the observer's production connection factory.
    text=text.replace('def factory(auth_loader):','def factory(auth_loader, **kwargs):')
    text=text.replace("['relay-controller-subscription-launcher.py','relay-controller-subscription-parent.py']","['relay-controller-subscription-launcher.py','relay-controller-subscription-parent.py','subscription-transport-diagnostics.py']")
   else:text=text.replace('def factory(auth_loader):','def factory(auth_loader, **kwargs):')
   (scratch/name).write_text(text)
  (scratch/'sitecustomize.py').write_text(frozen.GUARD);(scratch/'guard-probe.py').write_text(frozen.PROBE)
  env=dict(os.environ,PYTHONPATH=str(scratch),AR_OFFLINE_SCRATCH=str(scratch),AR_DIAGNOSTIC_CONTROLLER=str(controller),PYTHONDONTWRITEBYTECODE='1')
  suites=[('guard','guard-probe.py',None),*frozen.SUITES,('transport','test_transport_diagnostics.py',20),('diagnostic_host','test_diagnostic_host.py',3)]
  for label,name,count in suites:
   p=subprocess.run([sys.executable,'-B',str(scratch/name)],cwd=scratch,env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True,timeout=120)
   found=re.search(r'^Ran (\d+) tests? in ',p.stdout,re.MULTILINE);number=int(found[1]) if found else None
   passed=p.returncode==0 and (number==count if count is not None else json.loads(p.stdout)==['network','credentials','registry','process'])
   rows.append({'suite':label,'passed':passed,'tests':number,'exit_code':p.returncode})
   if not passed:print(p.stdout,file=sys.stderr)
 report={'python_version':platform.python_version(),'qualified_controller_pins_match':7,'all_passed':all(r['passed'] for r in rows),'tests':sum(r['tests'] or 0 for r in rows),'suites':rows,'models_started':0,'provider_requests':0,'credentials_accessed':False,'live_registry_accessed':False,'historical_hashes_unchanged':all(hashlib.sha256((FROZEN/r['path']).read_bytes()).hexdigest()==r['sha256'] for r in historical)}
 print(json.dumps(report,indent=2));return 0 if report['all_passed'] and report['historical_hashes_unchanged'] else 1

if __name__=='__main__':raise SystemExit(main())
