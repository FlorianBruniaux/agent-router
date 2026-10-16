#!/usr/bin/env python3
"""Explicit local Linux lab run. No model, registry pull or private host mount."""
import argparse
import hashlib
import io
import json
from pathlib import Path
import subprocess
import tarfile
import uuid

ROOT=Path(__file__).resolve().parents[3]
HERE=Path(__file__).resolve().parent
IMAGE='sha256:a808cb44a27ee036c8f0d7bd8aeb6150ecc3fe83f8dac3cfbfcf4df19a88f71b'
CACHE='ar-pr176-native-build-20261015'
BEFORE='10d84b1466f133ce88a2e75f961bd0ffa32fe89b97a8c71b9b2d07c50ec191da'
AFTER='71cc65477c1badc89a6f87487b5daf75798f63904241ad9d5c4637db8b1143c5'
FEEDBACK='8a62ffb5eb9c59318bd9b274d0234f11def6eeb2dafb99e5163e305f3c8fd0ca'


def run(args,**kwargs):
    return subprocess.run(args,check=True,**kwargs)


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output',type=Path,required=True,help='new evidence directory outside the checkout')
    args=parser.parse_args()
    output=args.output.resolve()
    if output==ROOT or ROOT in output.parents:parser.error('output must be outside the checkout')
    output.mkdir(parents=True,exist_ok=False)
    # The cache and image must already exist; never pull or install anything.
    run(['docker','image','inspect',IMAGE],stdout=subprocess.DEVNULL)
    run(['docker','volume','inspect',CACHE],stdout=subprocess.DEVNULL)
    baseline=subprocess.check_output(['rtk','proxy','git','show','cb6f33b807aebae67e035e83f156b30e02f2c969:crates/router-runtime/support/linux-finite-guard.c'],cwd=ROOT)
    files={'guard-before.c':baseline,'guard-after.c':(ROOT/'crates/router-runtime/support/linux-finite-guard.c').read_bytes(),
           'check.py':(HERE/'check.py').read_bytes(),'probe.py':(HERE/'probe.py').read_bytes(),'feedback.py':(HERE/'feedback.py').read_bytes()}
    for name,expected in [('guard-before.c',BEFORE),('guard-after.c',AFTER),('feedback.py',FEEDBACK)]:
        if hashlib.sha256(files[name]).hexdigest()!=expected:raise RuntimeError('reviewed source preimage changed')
    manifest=[]
    with tarfile.open(output/'public-files.tar','w') as archive:
        for name,data in files.items():
            if len(data)>65536:raise RuntimeError('source bound')
            item=tarfile.TarInfo(name);item.size=len(data);item.mode=0o644;archive.addfile(item,io.BytesIO(data))
            manifest.append({'name':name,'bytes':len(data),'sha256':hashlib.sha256(data).hexdigest()})
    (output/'source-manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
    label='finite-guard-'+uuid.uuid4().hex
    command=['docker','run','-d','--label','agent-router.task='+label,'--network=none','--read-only','--cap-drop=ALL',
        '--security-opt=no-new-privileges','--security-opt=seccomp=unconfined','--security-opt=systempaths=unconfined',
        '--memory=8589934592','--memory-swap=8589934592','--pids-limit=256','--cpus=1','--user=65534:65534',
        '--tmpfs=/tmp:rw,exec,nosuid,nodev,size=536870912,mode=1777','--tmpfs=/task:rw,exec,nosuid,nodev,size=2147483648,mode=1777',
        '--mount','type=volume,src='+CACHE+',dst=/work',IMAGE,'sleep','infinity']
    identity=subprocess.check_output(command,text=True).strip()
    try:
        info=json.loads(subprocess.check_output(['docker','inspect',identity]))[0]
        host=info['HostConfig']
        assert info['Id']==identity and info['Config']['Labels']['agent-router.task']==label
        assert host['Memory']==host['MemorySwap']==8589934592
        assert host['NetworkMode']=='none' and host['ReadonlyRootfs'] and not host['Privileged']
        assert host['CapDrop']==['ALL'] and info['Config']['User']=='65534:65534'
        assert host['MaskedPaths']==host['ReadonlyPaths']==[]
        assert all(m['Type']=='volume' and m['Name']==CACHE for m in info['Mounts'])
        (output/'container.json').write_text(json.dumps({'id':identity,'image':info['Image'],'user':info['Config']['User'],'host_config':host,'mounts':info['Mounts']},indent=2)+'\n')
        with (output/'public-files.tar').open('rb') as packet:
            run(['docker','exec','-i',identity,'sh','-c','mkdir /task/guard-tests; tar -xf - -C /task/guard-tests'],stdin=packet)
        with (output/'canaries.log').open('wb') as log:
            result=subprocess.run(['docker','exec','-w','/task/guard-tests',identity,'sh','-c','cc -Wall -Wextra -Werror -O2 guard-before.c -o guard-before && cc -Wall -Wextra -Werror -O2 guard-after.c -o guard-after && python3 -B check.py'],stdout=log,stderr=subprocess.STDOUT)
        for source,name in [('/task/guard-tests/results.json','results.json')]:
            with (output/name).open('wb') as stream:run(['docker','exec',identity,'cat',source],stdout=stream)
        with (output/'programs-and-limits.log').open('wb') as stream:
            run(['docker','exec',identity,'sh','-c','sha256sum /task/guard-tests/guard-before /task/guard-tests/guard-after /usr/bin/bwrap /usr/bin/aarch64-linux-gnu-gcc-12 /usr/local/rustup/toolchains/1.85.1-aarch64-unknown-linux-gnu/bin/rustc; cat /sys/fs/cgroup/memory.max; cat /sys/fs/cgroup/memory.swap.max'],stdout=stream)
        (output/'exit-code.json').write_text(json.dumps({'exit_code':result.returncode,'models_started':0})+'\n')
        result.check_returncode()
    finally:
        info=json.loads(subprocess.check_output(['docker','inspect',identity]))[0]
        if info['Id']!=identity or info['Config']['Labels']['agent-router.task']!=label:raise RuntimeError('container identity changed; retained')
        run(['docker','rm','--force',identity])
    print('Evidence: '+str(output))


if __name__=='__main__':main()
