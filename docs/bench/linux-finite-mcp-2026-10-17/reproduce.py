#!/usr/bin/env python3
"""Replay MCP worker components offline. No native SDK, account or model."""
import argparse
import hashlib
import io
import json
from pathlib import Path
import subprocess
import tarfile
import time
import uuid

ROOT=Path(__file__).resolve().parents[3]
HERE=Path(__file__).resolve().parent
IMAGE='sha256:a808cb44a27ee036c8f0d7bd8aeb6150ecc3fe83f8dac3cfbfcf4df19a88f71b'
CACHE='ar-pr176-native-build-20261015'
BASE='33dc663f7c7b8ce0311056fc02ecfc3f5d0b5dd3'
FILES=[
 'crates/router-runtime/src/finite_client.rs','crates/router-runtime/src/finite_client/linux_backend.rs',
 'crates/router-runtime/src/finite_client/runtime_binding.rs','crates/router-runtime/src/finite_client/write_workspace.rs',
 'crates/router-runtime/src/finite_tools.rs','crates/router-runtime/src/finite_tools/mcp_stdio.rs',
 'crates/router-runtime/src/finite_tools/policy.rs','crates/router-runtime/src/finite_tools/exec.rs',
 'crates/router-runtime/src/finite_tools/transport.rs','crates/router-runtime/src/finite_tools/session.rs',
 'crates/router-runtime/src/finite_tools/linux_tests.rs','crates/router-runtime/src/lib.rs',
 'crates/router-runtime/src/bin/finite-codex-client.rs','crates/router-runtime/support/linux-finite-guard.c',
 'crates/router-runtime/tests/runtime.rs','docs/bench/linux-finite-tools-guard-2026-10-16/feedback.py',
 'docs/bench/linux-finite-mcp-2026-10-17/bridge-watchdog.py']


def run(args,**kwargs):return subprocess.run(args,check=True,**kwargs)


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args();output=args.output.resolve()
    if output==ROOT or ROOT in output.parents:parser.error('output must be outside the checkout')
    output.mkdir(parents=True,exist_ok=False)
    run(['docker','image','inspect',IMAGE],stdout=subprocess.DEVNULL)
    run(['docker','volume','inspect',CACHE],stdout=subprocess.DEVNULL)
    files={p:(ROOT/p).read_bytes() for p in FILES}
    assert hashlib.sha256(files['crates/router-runtime/support/linux-finite-guard.c']).hexdigest()=='ba75d2f7ec03bed21eaf2137ba5d800bcaccbd1c3340f969dd8ebe4cd25ec487'
    assert hashlib.sha256(files['docs/bench/linux-finite-tools-guard-2026-10-16/feedback.py']).hexdigest()=='8a62ffb5eb9c59318bd9b274d0234f11def6eeb2dafb99e5163e305f3c8fd0ca'
    with tarfile.open(output/'public-overlay.tar','w') as archive:
        manifest=[]
        for path,data in files.items():
            assert len(data)<1000000
            item=tarfile.TarInfo(path);item.size=len(data);item.mode=0o644;item.mtime=time.time()
            archive.addfile(item,io.BytesIO(data))
            manifest.append({'path':path,'bytes':len(data),'sha256':hashlib.sha256(data).hexdigest()})
    (output/'source-overlay.json').write_text(json.dumps(manifest,indent=2)+'\n')
    # Verify cached Rust/manifests against the published dependency, then use
    # the explicit overlay for changed/new source and one stale cached test.
    expected={}
    for row in subprocess.check_output(['rtk','proxy','git','ls-tree','-rz',BASE],cwd=ROOT).split(b'\0'):
        if not row:continue
        spec,path=row.split(b'\t',1);path=path.decode();_mode,kind,blob=spec.decode().split()
        if kind=='blob' and ((path.startswith(('crates/','xtask/')) and (path.endswith('.rs') or path.endswith('/Cargo.toml'))) or path in ('Cargo.toml','Cargo.lock')):
            expected[path]=blob
    for path,data in files.items():
        if path.endswith('.rs'):expected[path]=hashlib.sha1(b'blob '+str(len(data)).encode()+b'\0'+data).hexdigest()
    name='finite-mcp-'+uuid.uuid4().hex;label='agent-router.task='+name
    command=['docker','run','-d','--label',label,'--network=none','--read-only','--cap-drop=ALL',
        '--security-opt=no-new-privileges','--security-opt=seccomp=unconfined','--security-opt=systempaths=unconfined',
        '--memory=8589934592','--memory-swap=8589934592','--pids-limit=256','--cpus=1','--user=65534:65534',
        '--tmpfs=/tmp:rw,exec,nosuid,nodev,size=536870912,mode=1777','--tmpfs=/task:rw,exec,nosuid,nodev,size=2147483648,mode=1777',
        '--mount','type=volume,src='+CACHE+',dst=/work',IMAGE,'sleep','infinity']
    identity=subprocess.check_output(command,text=True).strip()
    def docker(*args,**kwargs):return run(['docker','exec',*args,identity,*kwargs.pop('command')],**kwargs)
    def cargo(argv,log,json_output=False):
        with (output/log).open('wb') as stdout,(output/(log+'.stderr')).open('wb') as stderr:
            docker('-w','/task/mcp-source','-e','RUSTUP_TOOLCHAIN=1.85.1','-e','CARGO_HOME=/work/cargo','-e','CARGO_TARGET_DIR=/work/source/target','-e','CARGO_BUILD_JOBS=1',command=['cargo',*argv],stdout=stdout,stderr=stderr)
    try:
        info=json.loads(subprocess.check_output(['docker','inspect',identity]))[0];h=info['HostConfig']
        assert info['Id']==identity and info['Config']['Labels']['agent-router.task']==name
        assert h['Memory']==h['MemorySwap']==8589934592 and h['NetworkMode']=='none' and h['ReadonlyRootfs'] and not h['Privileged']
        assert h['CapDrop']==['ALL'] and info['Config']['User']=='65534:65534' and h['MaskedPaths']==h['ReadonlyPaths']==[]
        assert all(m['Type']=='volume' and m['Name']==CACHE for m in info['Mounts'])
        (output/'container.json').write_text(json.dumps({'id':identity,'image':info['Image'],'host_config':h,'mounts':info['Mounts']},indent=2)+'\n')
        docker(command=['cp','-a','/work/source-finite-20261016','/task/mcp-source'])
        with (output/'public-overlay.tar').open('rb') as packet:docker('-i',command=['tar','-xf','-','-C','/task/mcp-source'],stdin=packet)
        verify="import hashlib,json,pathlib,sys; rows=json.load(sys.stdin); bad=[]; root=pathlib.Path('/task/mcp-source');\nfor p,want in rows.items():\n f=root/p; b=f.read_bytes(); actual=hashlib.sha1(b'blob '+str(len(b)).encode()+b'\\0'+b).hexdigest()\n if actual!=want:bad.append(p)\nprint(json.dumps({'checked':len(rows),'mismatches':bad}));sys.exit(bool(bad))"
        with (output/'compiled-source-check.json').open('w') as log:docker('-i',command=['python3','-c',verify],input=json.dumps(expected),text=True,stdout=log)
        cargo(['build','-p','router-runtime','--bin','finite-codex-client','--release','--offline','--locked'],'client-build.log')
        cargo(['test','-p','router-runtime','--lib','--release','--offline','--locked','--no-run','--message-format=json'],'test-build.jsonl')
        executables=[]
        for line in (output/'test-build.jsonl').read_text().splitlines():
            try:value=json.loads(line)
            except ValueError:continue
            if value.get('reason')=='compiler-artifact' and value.get('target',{}).get('name')=='router_runtime' and value.get('executable'):executables.append(value['executable'])
        assert len(executables)==1
        prepare=r'''set -eu
mkdir -m 700 /task/programs /task/programs/compiler /task/qualification
cp /usr/bin/bwrap /task/programs/namespace
cp /work/source/target/release/router-sandbox-helper /task/programs/helper
cp /work/source/target/release/finite-codex-client /task/programs/client
cc -O2 -Wall -Wextra -Werror /task/mcp-source/crates/router-runtime/support/linux-finite-guard.c -o /task/programs/guard
cat > /task/programs/compiler/rustc <<'WRAPPER'
#!/usr/bin/python3
import os,sys
real='/usr/local/rustup/toolchains/1.85.1-aarch64-unknown-linux-gnu/bin/rustc'
os.execv(real,[real,'-C','linker=/usr/bin/aarch64-linux-gnu-gcc-12',*sys.argv[1:]])
WRAPPER
chmod 500 /task/programs/namespace /task/programs/helper /task/programs/client /task/programs/guard /task/programs/compiler/rustc
sha256sum /task/programs/namespace /task/programs/helper /task/programs/client /task/programs/guard /task/programs/compiler/rustc /usr/local/rustup/toolchains/1.85.1-aarch64-unknown-linux-gnu/bin/rustc /usr/bin/aarch64-linux-gnu-gcc-12
cat /sys/fs/cgroup/memory.max /sys/fs/cgroup/memory.swap.max
'''
        with (output/'programs-and-limits.log').open('w') as log:docker('-i',command=['sh'],input=prepare,text=True,stdout=log,stderr=subprocess.STDOUT)
        assert 'ff5add553bea12a45c18ebe6c67d3ed637052b869a1ca2fc5d74a6a2e8c5cce6' in (output/'programs-and-limits.log').read_text()
        assert 'd1cc1e8658059c6a5040bdd27d9cc8e37a49fc4923a0243ae070c1bcc96a80d4' in (output/'programs-and-limits.log').read_text()
        outer=['/usr/bin/bwrap','--unshare-all','--cap-drop','ALL','--ro-bind','/usr','/usr','--symlink','usr/bin','/bin','--symlink','usr/lib','/lib','--symlink','usr/lib64','/lib64','--proc','/proc','--dev','/dev','--size','536870912','--tmpfs','/tmp','--ro-bind','/work/source/target','/work/source/target','--bind','/task','/task','--size','16777216','--tmpfs','/mcp-scratch','--chmod','700','/mcp-scratch','--size','1048576','--tmpfs','/router-tools','--chmod','700','/router-tools','--chdir','/task','--']
        with (output/'mcp-tests.log').open('w') as log:docker('-e','AR_MCP_ROOT=/task/qualification','-e','AR_MCP_SCRATCH=/mcp-scratch','-e','AR_MCP_PROGRAMS=/task/programs','-e','AR_MCP_CLIENT=/task/programs/client',command=[*outer,executables[0],'finite_tools::','--include-ignored','--nocapture','--test-threads=1'],stdout=log,stderr=subprocess.STDOUT)
        with (output/'bridge-watchdog.log').open('w') as log:docker(command=[*outer,'/usr/bin/python3','-B','/task/mcp-source/docs/bench/linux-finite-mcp-2026-10-17/bridge-watchdog.py','/task/programs/client'],stdout=log,stderr=subprocess.STDOUT)
        (output/'result.json').write_text(json.dumps({'verdict':'PASS_LOCAL_COMPONENT_ONLY','models_started':0,'native_sdk_started':0,'memory_bytes':8589934592,'swap_bytes':0,'scratch_bytes':16777216,'product_tools_admission':False,'sealed_trials_started':0},indent=2)+'\n')
    finally:
        info=json.loads(subprocess.check_output(['docker','inspect',identity]))[0]
        if info['Id']!=identity or info['Config']['Labels']['agent-router.task']!=name:raise RuntimeError('container identity changed; retained')
        run(['docker','rm','--force',identity])
    print('Evidence: '+str(output))


if __name__=='__main__':main()
