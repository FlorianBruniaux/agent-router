"""Separate synthetic qualification. Never imports or invokes a live sender."""
import pathlib,json,subprocess as sp,importlib.util,hashlib,sys,tempfile,copy,select
import argparse,re
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument("assignment")
parser.add_argument("--output",type=pathlib.Path,required=True,help="Private prepared candidate directory with id and program-pins.json")
parser.add_argument("--recipe",type=pathlib.Path,required=True,help="Verified argv recipe for this isolated Linux container")
parser.add_argument("--useful-score",action="store_true",help="Replay the authored t02 correction with RED/GREEN feedback; external acceptance remains separate")
args=parser.parse_args()
if args.useful_score and args.assignment!='t02-Lm-r1-H1':
 parser.error('--useful-score requires the sealed t02-Lm-r1-H1 packet')
OUT=args.output
HOST=pathlib.Path(__file__).resolve().parent.parent/'finite-subscription-relay-2026-10-18/controller'
def load(n,f):
 spec=importlib.util.spec_from_file_location(n,HOST/f);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);return m
fixture=load('campaign_test_fixture','relay-controller-results-test.py')
session=load('campaign_offline_session','relay-controller-session.py')
driver=load('campaign_offline_driver','relay-controller-session-driver.py')
sys.path[:0]=[str(HOST/'native-catalog-adapter'),str(HOST/'qualification-inputs')]
from native_tool_responses import ToolScript
PACKETS=json.loads((pathlib.Path(__file__).resolve().parents[3]/'crates/router-runtime/examples/campaign/packets.json').read_text())['assignments']
CASES=['t02-Lx-r1-H0','t02-Lm-r1-H1','t08-Am-r1-H0','t08-Ax-r1-H1','t05-Lm-r1-H0','t05-Lx-r1-H1']
def prefix(manager,aid):
 count=0
 for row in copy.deepcopy(manager.snapshot()['rows']):
  if row['assignment_id']==aid:break
  rid='resp_'+row['assignment_id'];body=fixture.fixture.request();body['model']=row['model'];body['reasoning']['effort']=row['effort']
  request_hash=hashlib.sha256(json.dumps(body,ensure_ascii=False,separators=(',',':')).encode()).hexdigest()
  event=json.loads(fixture.fixture.wire(rid).split(b'data: ',1)[1]);event['response']['model']=row['model'];wire=b'data: '+json.dumps(event).encode()+b'\n\n'
  g=fixture.fixture.gateway.Gateway({'model':row['model'],'effort':row['effort']},lambda raw,remaining:(200,'text/event-stream',iter((wire,))),clock=lambda:0)
  observer=fixture.pipe.PipeObserver(row['assignment_id'],row['model'],row['effort'],started_at=0,clock=lambda:0)
  observer.begin(1,request_hash);observer.feed(fixture.fixture.encoded(list(g.handle({'schema_version':1,'id':1,'body':body}))))
  run='synthetic-prefix-'+str(count);manager.begin_trial(row['assignment_id'],run,fixture.APPROVAL);manager.before_request(row['assignment_id'],1,request_hash);manager.ingest(row['assignment_id'],observer.receipt());manager.finish_trial(row['assignment_id'],'completed',{**fixture.cleanup(),'run_id':run});count+=1
 return count

def argv(container):
 shell=json.loads(args.recipe.read_text())['argv']
 fixed=shell[shell.index('unshare'):];fixed=fixed[:fixed.index('sh',fixed.index('-c')+2)+1]
 return ['docker','exec','-i',container,*fixed,'/work/source/target/release/examples/finite-campaign','--run']
def first_line(proc):
 if not select.select([proc.stdout],[],[],300)[0]:raise TimeoutError('runtime preparation deadline')
 raw=proc.stdout.readline(16385)
 if not raw.endswith(b'\n') or len(raw)>16384:raise ValueError('runtime control bound')
 return fixture.pipe.strict_json(raw)
def extract(c,path,bound):
 program='import os,sys,stat; p=sys.argv[1]; n=int(sys.argv[2]); f=os.open(p,os.O_RDONLY|os.O_NOFOLLOW|os.O_NONBLOCK); m=os.fstat(f); assert stat.S_ISREG(m.st_mode) and m.st_nlink==1 and m.st_uid==os.getuid() and m.st_size<=n; b=os.read(f,n+1);assert len(b)==m.st_size;sys.stdout.buffer.write(b)'
 return sp.check_output(['docker','exec',c,'python3','-B','-c',program,path,str(bound)])
def main(aid):
 assert aid in CASES
 c=(OUT/'id').read_text();info=json.loads(sp.check_output(['docker','inspect',c]))[0]
 assert info['Config']['Labels']['agent-router.task']=='finite-campaign-offline-20261018'
 assert info['HostConfig']['Memory']==info['HostConfig']['MemorySwap']==8589934592 and info['HostConfig']['NetworkMode']=='none'
 p=next(p for p in PACKETS if p['assignment_id']==aid)
 out=pathlib.Path(tempfile.mkdtemp(prefix=aid+'-',dir=OUT));out.chmod(0o700)
 config=fixture.configuration(48000000);cfg=fixture.digest(config)
 root=pathlib.Path(tempfile.mkdtemp(prefix='relay-controller-results-test-'))
 manager=fixture.manager.ResultsManager(root,fixture.MANIFEST,fixture.TEMPLATE,budget_microcredits=48000000,configuration=config,verified_configuration_sha256=cfg)
 skipped=prefix(manager,aid)
 desc={'schema_version':1,'assignment_id':aid,'configuration_sha256':cfg,'account_sha256':config['account_sha256'],'controller_sha256':session.source_sha256(),'observed_campaign_stop_microcredits':48000000,'deadline_ms':60000,'observed_tokens':6000,'root':'/task/campaign/'+aid,'programs':json.loads((OUT/'program-pins.json').read_text())}
 code=p['files'].get(p['output'],'{"findings":[]}\n')
 if p['output'].endswith('.rs'):code+='\n// Public authored transport probe.\n'
 if p['output']=='tests.rs' and p['condition']=='H1':code+='#[test] fn authored_transport_probe() { assert_eq!(1, 2); }\n'
 expected={'exit_code':0,'termination':'exited','truncated':False}
 steps=[{'tool':'mcp__finite__read_file','arguments':{'path':'statement.md'},'expected':expected}, {'tool':'mcp__finite__write_file','arguments':{'path':p['output'],'content':code,'create_new':p['create_new']},'expected':expected}, {'tool':'mcp__finite__read_file','arguments':{'path':p['output']},'expected':expected}]
 if p['condition']=='H1':
  steps.append({'tool':'mcp__finite__exec','arguments':{'argv':['python3','-B','feedback.py']},'expected':{**expected,'exit_code':0 if aid.startswith('t05') else 1}})
 if args.useful_score:
  authored=pathlib.Path(__file__).resolve().parent.parent/'linux-finite-dispatch-2026-10-16/inputs/t02/controller/control-output.json'
  code=json.loads(authored.read_text())['lib.rs']
  steps[1]['arguments']['content']=code
  steps[-1]['expected']['exit_code']=0
  steps.insert(1,{'tool':'mcp__finite__exec','arguments':{'argv':['python3','-B','feedback.py']},'expected':{**expected,'exit_code':1}})
 script=ToolScript(steps,model=p['model'],effort=p['effort'],marker='Read statement.md and AGENTS.md. Follow HARNESS.md if present.')
 calls=[]
 def send(raw,remaining):
  request=fixture.pipe.strict_json(raw)
  calls.append({'sha256':hashlib.sha256(raw).hexdigest(),'model':request.get('model'),'effort':request.get('reasoning',{}).get('effort')})
  wire=driver.add_synthetic_usage(script.respond(request))
  return 200,'text/event-stream',iter((wire,))
 error=None;observation=None;binding=None;report=None;feedback=None
 stderr=(out/'runtime.stderr').open('wb');proc=sp.Popen(argv(c),stdin=sp.PIPE,stdout=sp.PIPE,stderr=stderr,bufsize=0)
 try:
  proc.stdin.write(session.canonical(desc)+b'\n');proc.stdin.flush()
  prepare=first_line(proc);assert prepare['kind']=='runtime_prepare'
  for k in ('assignment_id','model','effort','condition','packet_sha256','acceptance_sha256'):assert prepare[k]==p[k],k
  for k in ('configuration_sha256','account_sha256','controller_sha256','deadline_ms','observed_tokens','observed_campaign_stop_microcredits'):assert prepare[k]==desc[k],k
  control=first_line(proc);assert control['kind']=='runtime_binding';binding=control['binding']
  assert binding['assignment_id']==aid and binding['run_id']==aid and binding['attempt']==1
  for k in ('configuration_sha256','account_sha256','controller_sha256'):assert binding[k]==desc[k],k
  ack={'schema_version':1,'kind':'approve_runtime','binding':binding,'packet_sha256':p['packet_sha256']}
  proc.stdin.write(session.canonical(ack)+b'\n');proc.stdin.flush()
  channel=session.PrivateRuntimeIO(proc.stdout.fileno(),proc.stdin.fileno(),borrow=True)
  # Preparation can rehash retained programs before runtime_ready. This initial
  # wait grants no trial/provider authority; Session resets to timeout=60 at ready.
  assert channel.timeout==60
  channel.deadline=channel.clock()+300
  observation=session.SessionController(channel,manager,binding,send).run()
  proc.stdin.close();rc=proc.wait(timeout=30)
  report_bytes=extract(c,desc['root']+'/runtime-result.json',4*1024*1024);report=json.loads(report_bytes);(out/'runtime-result.json').write_bytes(report_bytes)
  assert rc==0 and report['source_unchanged'] is True and report['cleanup']=='namespace_terminated' and report['export_failed'] is False
  assert report['admission']['granted']==0 and report['admission']['released']==1
  sub=report['submission'];assert sub['relative']==p['output'] and sub['bytes']<=32768
  actual=extract(c,desc['root']+'/submission/'+p['output'],32768);assert actual==code.encode() and hashlib.sha256(actual).hexdigest()==sub['sha256'];(out/p['output']).write_bytes(actual)
  assert len(calls)==len(steps)+1 and script.observed_tool_results==len(steps)
  assert all(x['model']==p['model'] and x['effort']==p['effort'] for x in calls)
  if p['condition']=='H1':
   artifact=report['result']['output_artifacts']['stdout']
   raw=extract(c,artifact['path'],65536)
   assert len(raw)==artifact['bytes'] and hashlib.sha256(raw).hexdigest()==artifact['sha256']
   text=raw.decode();runs=re.findall(r'running ([0-9]+) tests?',text)
   if aid.startswith('t05'):assert 'JSON shape valid; defect accuracy and explanation quality are not checked.' in text
   elif args.useful_score:assert runs==['4','4'] and 'test result: FAILED' in text and 'test result: ok. 4 passed; 0 failed' in text
   else:assert runs==(['4'] if aid.startswith('t02') else ['1']) and 'test result: FAILED' in text
   feedback={'native_stdout_sha256':artifact['sha256'],'rust_tests_executed':sum(map(int,runs)),'expected_feedback_failure':not aid.startswith('t05'),'json_shape_valid':aid.startswith('t05')}
   (out/'native-stdout.bin').write_bytes(raw)
  if args.useful_score:
   (out/'source-lib.rs').write_text(p['files']['lib.rs'])
   patch=extract(c,desc['root']+'/runtime/artifacts/'+aid+'.patch',65536)
   assert hashlib.sha256(patch).hexdigest()==report['result']['run']['artifact_sha256']
   (out/'change.patch').write_bytes(patch)
 except Exception as exc:
  error=type(exc).__name__+': '+str(exc)[:200]
  try:proc.stdin.close()
  except Exception:pass
  try:proc.wait(timeout=20)
  except sp.TimeoutExpired:pass
 finally:
  stderr.close();manager.close()
  summary={'assignment_id':aid,'passed':error is None,'failure':error,'model_calls_started':0,'actual_provider_requests':0,'auth_read':False,'only_current_cell_executed_natively':True,'synthetic_prefix_cells':skipped,'separate_fixture_ledger':str(root),'packet_sha256':p['packet_sha256'],'binding':binding,'calls':calls,'observed_tool_results':script.observed_tool_results,'feedback_evidence':feedback,'observation':observation,'runtime_exit_code':proc.poll()}
  summary['qualification_mode']='useful-score' if args.useful_score else 'transport'
  (out/'summary.json').write_text(json.dumps(summary,indent=2)+'\n');print(json.dumps({'assignment_id':aid,'passed':summary['passed'],'failure':error,'calls':len(calls),'models_started':0}),flush=True)
 if error:raise SystemExit(1)
if __name__=='__main__':main(args.assignment)
