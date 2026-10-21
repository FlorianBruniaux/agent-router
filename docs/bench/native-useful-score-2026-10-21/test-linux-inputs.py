"""Refuse altered exports in a prepared 8 GiB Linux cell, without native replay."""
import argparse
import copy,hashlib,json,pathlib,shutil,subprocess
parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--input',type=pathlib.Path,required=True);parser.add_argument('--output',type=pathlib.Path,required=True);args=parser.parse_args()
root=args.input;out=args.output;out.mkdir(mode=0o700)
rows=[]
for name in ['changed_candidate','changed_source','changed_patch','unknown_cleanup','retained_admission','wrong_packet','changed_mode','malformed_mode','extra_file']:
 d=out/name;shutil.copytree(root,d);r=json.loads((d/'runtime-result.json').read_text())
 if name=='changed_candidate':(d/'lib.rs').write_bytes(b'// changed\n')
 elif name=='changed_source':(d/'source-lib.rs').write_bytes(b'// changed\n')
 elif name=='changed_patch':(d/'change.patch').write_bytes(b'wrong patch\n')
 elif name=='unknown_cleanup':r['cleanup']='unknown'
 elif name=='retained_admission':r['admission']['granted']=1
 elif name=='wrong_packet':r['packet_sha256']='0'*64
 elif name in ('changed_mode','malformed_mode'):
  patch=(d/'change.patch').read_bytes().replace(b'diff --git a/lib.rs b/lib.rs\n',b'diff --git a/lib.rs b/lib.rs\nold mode 100644\nnew mode 100755\n').replace(b' 100644\n',b'\n')
  if name=='changed_mode': patch=patch.replace(b'old mode\n',b'old mode 100644\n')
  (d/'change.patch').write_bytes(patch);r['result']['run']['artifact_sha256']=hashlib.sha256(patch).hexdigest()
 elif name=='extra_file':
  patch=(d/'change.patch').read_bytes()+b'diff --git a/extra b/extra\nnew file mode 100644\nindex 0000000..7898192\n--- /dev/null\n+++ b/extra\n@@ -0,0 +1 @@\n+a\n'
  (d/'change.patch').write_bytes(patch);r['result']['run']['artifact_sha256']=hashlib.sha256(patch).hexdigest()
 (d/'runtime-result.json').write_text(json.dumps(r))
 result=subprocess.run(['python3','-B',str(pathlib.Path(__file__).resolve().with_name('verify.py')),'--input',str(d),'--output',str(d/'verdict.json')],capture_output=True,timeout=30)
 assert result.returncode!=0 and not (d/'verdict.json').exists(),name
 rows.append({'case':name,'refused':True,'reason':result.stderr.decode().splitlines()[-1]})
print(json.dumps(rows,indent=2))
