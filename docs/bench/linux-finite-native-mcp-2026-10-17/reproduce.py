#!/usr/bin/env python3
"""Real SDK + MCP against frozen authored packets; no model or account."""
import argparse
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import tarfile
import time
import uuid

ROOT = Path(__file__).resolve().parents[3]
HERE = Path(__file__).resolve().parent
NATIVE = '113aa5d5952a6fd3950a88323219747d0c91978ebee948509a540c8a460e59a3'
INPUTS = '5f72c2221b5feb350ea05410f5496ee31bd924658130bf6ae394699cb0cfdf1b'
ADAPTER = '6e2268fca17bb2bf0f241cf959fe229cf50c19ea7f44b85b67de359c24b80ffe'
EXPECTED = '9550e472a5b090c76b35bc1db5e2e5a30d6d6d3cc41b38da826acb790713182f'
GUARD = 'ba75d2f7ec03bed21eaf2137ba5d800bcaccbd1c3340f969dd8ebe4cd25ec487'


def run(args, **kwargs):
    return subprocess.run(args, check=True, **kwargs)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--inputs', type=Path, required=True, help='frozen six-packet public/controller export')
    parser.add_argument('--adapter', type=Path, required=True, help='frozen authored broker validator, never mounted for worker')
    parser.add_argument('--native', type=Path, required=True, help='public pinned Linux ARM64 Codex executable, never auth.json')
    args = parser.parse_args()
    output = args.output.resolve()
    if output == ROOT or ROOT in output.parents:
        parser.error('new output directory must be outside checkout')
    output.mkdir(parents=True, exist_ok=False)
    spec = importlib.util.spec_from_file_location('components', ROOT/'docs/bench/linux-finite-mcp-2026-10-17/reproduce.py')
    components = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(components)
    run(['docker', 'image', 'inspect', components.IMAGE], stdout=subprocess.DEVNULL)
    run(['docker', 'volume', 'inspect', components.CACHE], stdout=subprocess.DEVNULL)
    names = [*components.FILES, 'crates/router-runtime/src/finite_tools/owner.rs',
             'crates/router-runtime/src/finite_client/native_tools_tests.rs',
             'crates/router-runtime/tests/finite_client.rs',
             'crates/router-runtime/tests/runtime/finite_client_binding.rs']
    files = {p: (ROOT/p).read_bytes() for p in names}
    assert digest(files['crates/router-runtime/support/linux-finite-guard.c']) == GUARD
    manifest_bytes = (args.inputs/'manifest.json').read_bytes()
    assert digest(manifest_bytes) == INPUTS
    manifest = json.loads(manifest_bytes)
    packet_files = {'inputs/manifest.json': manifest_bytes}
    for row in manifest['files']:
        data = (args.inputs/row['path']).read_bytes()
        assert len(data) == row['bytes'] and digest(data) == row['sha256']
        packet_files['inputs/'+row['path']] = data
    for name, expected in [('native_tool_responses.py', ADAPTER), ('expected-native-catalog.json', EXPECTED)]:
        data = (args.adapter/name).read_bytes()
        assert digest(data) == expected
        packet_files['adapter/'+name] = data
    native = args.native.read_bytes()
    assert len(native) <= 268435456 and digest(native) == NATIVE
    packet_files['programs/native'] = native
    for filename, contents in [('source.tar', files), ('authored-controller.tar', packet_files)]:
        with tarfile.open(output/filename, 'w') as archive:
            for name, data in contents.items():
                item = tarfile.TarInfo(name)
                item.size, item.mode, item.mtime = len(data), 0o644, time.time()
                archive.addfile(item, io.BytesIO(data))
    source_manifest = [{'path': p, 'bytes': len(b), 'sha256': digest(b)} for p, b in files.items()]
    (output/'source.json').write_text(json.dumps(source_manifest, indent=2)+'\n')
    expected = {}
    for row in subprocess.check_output(['rtk', 'proxy', 'git', 'ls-tree', '-rz', 'HEAD'], cwd=ROOT).split(b'\0'):
        if not row:
            continue
        spec, path = row.split(b'\t', 1)
        path = path.decode()
        _mode, kind, blob = spec.decode().split()
        if kind == 'blob' and ((path.startswith(('crates/', 'xtask/')) and (path.endswith('.rs') or path.endswith('/Cargo.toml'))) or path in ('Cargo.toml', 'Cargo.lock')):
            expected[path] = blob
    for name, data in files.items():
        if name.endswith('.rs'):
            expected[name] = hashlib.sha1(b'blob '+str(len(data)).encode()+b'\0'+data).hexdigest()
    label = 'finite-native-mcp-'+uuid.uuid4().hex
    identity = subprocess.check_output(['docker', 'run', '-d', '--label', 'agent-router.task='+label,
        '--network=none', '--read-only', '--cap-drop=ALL', '--security-opt=no-new-privileges',
        '--security-opt=seccomp=unconfined', '--security-opt=systempaths=unconfined',
        '--memory=8589934592', '--memory-swap=8589934592', '--pids-limit=256', '--cpus=1', '--user=65534:65534',
        '--tmpfs=/tmp:rw,exec,nosuid,nodev,size=536870912,mode=1777',
        '--tmpfs=/task:rw,exec,nosuid,nodev,size=2147483648,mode=1777',
        '--mount', 'type=volume,src='+components.CACHE+',dst=/work', components.IMAGE, 'sleep', 'infinity'], text=True).strip()
    def docker(*args, **kwargs):
        return run(['docker', 'exec', *args, identity, *kwargs.pop('command')], **kwargs)
    def cargo(args, stdout, stderr):
        with (output/stdout).open('wb') as out, (output/stderr).open('wb') as err:
            docker('-w', '/task/source', '-e', 'RUSTUP_TOOLCHAIN=1.85.1', '-e', 'CARGO_HOME=/work/cargo',
                   '-e', 'CARGO_TARGET_DIR=/work/source/target', '-e', 'CARGO_BUILD_JOBS=1',
                   command=['cargo', *args], stdout=out, stderr=err)
    try:
        info = json.loads(subprocess.check_output(['docker', 'inspect', identity]))[0]
        h = info['HostConfig']
        assert h['Memory'] == h['MemorySwap'] == 8589934592 and h['NetworkMode'] == 'none'
        assert h['ReadonlyRootfs'] and not h['Privileged'] and h['CapDrop'] == ['ALL']
        assert info['Config']['User'] == '65534:65534' and h['MaskedPaths'] == h['ReadonlyPaths'] == []
        assert all(m['Type'] == 'volume' and m['Name'] == components.CACHE for m in info['Mounts'])
        (output/'container.json').write_text(json.dumps({'id': identity, 'image': info['Image'], 'host_config': h, 'mounts': info['Mounts']}, indent=2)+'\n')
        docker(command=['cp', '-a', '/work/source-finite-20261016', '/task/source'])
        with (output/'source.tar').open('rb') as stream:
            docker('-i', command=['tar', '-xf', '-', '-C', '/task/source'], stdin=stream)
        with (output/'authored-controller.tar').open('rb') as stream:
            docker('-i', command=['tar', '-xf', '-', '-C', '/task'], stdin=stream)
        verify = "import hashlib,json,pathlib,sys;rows=json.load(sys.stdin);bad=[];root=pathlib.Path('/task/source');\nfor p,want in rows.items():\n b=(root/p).read_bytes();actual=hashlib.sha1(b'blob '+str(len(b)).encode()+b'\\0'+b).hexdigest()\n if actual!=want:bad.append(p)\nprint(json.dumps({'checked':len(rows),'mismatches':bad}));sys.exit(bool(bad))"
        with (output/'compiled-source-check.json').open('w') as log:
            docker('-i', command=['python3', '-c', verify], input=json.dumps(expected), text=True, stdout=log)
        cargo(['build', '-p', 'router-runtime', '--bin', 'finite-codex-client', '--release', '--offline', '--locked'], 'client-build.log', 'client-build.stderr')
        cargo(['test', '-p', 'router-runtime', '--lib', '--release', '--offline', '--locked', '--no-run', '--message-format=json'], 'test-build.jsonl', 'test-build.stderr')
        records = [json.loads(line) for line in (output/'test-build.jsonl').read_text().splitlines() if line.startswith('{')]
        assert any(r.get('reason') == 'build-finished' and r.get('success') for r in records)
        binaries = [r['executable'] for r in records if r.get('reason') == 'compiler-artifact' and r.get('target', {}).get('name') == 'router_runtime' and r.get('executable')]
        assert len(binaries) == 1
        setup = '''set -eu
mkdir -m 700 /task/programs/compiler /task/component-cases
cp /usr/bin/bwrap /task/programs/namespace
cp /work/source/target/release/router-sandbox-helper /task/programs/helper
cp /work/source/target/release/finite-codex-client /task/programs/client
cc -O2 -Wall -Wextra -Werror /task/source/crates/router-runtime/support/linux-finite-guard.c -o /task/programs/guard
cat > /task/programs/compiler/rustc <<'WRAPPER'
#!/usr/bin/python3
import os,sys
real='/usr/local/rustup/toolchains/1.85.1-aarch64-unknown-linux-gnu/bin/rustc'
os.execv(real,[real,'-C','linker=/usr/bin/aarch64-linux-gnu-gcc-12',*sys.argv[1:]])
WRAPPER
chmod 500 /task/programs/native /task/programs/namespace /task/programs/helper /task/programs/client /task/programs/guard /task/programs/compiler/rustc
sha256sum /task/programs/native /task/programs/namespace /task/programs/helper /task/programs/client /task/programs/guard /task/programs/compiler/rustc
cat /sys/fs/cgroup/memory.max /sys/fs/cgroup/memory.swap.max
cat > /task/loopback.py <<'LOOP'
import socket,fcntl,struct,os,sys
s=socket.socket(socket.AF_INET,socket.SOCK_DGRAM)
r=struct.pack('16sH',b'lo',0)+b'\\x00'*22
flags=struct.unpack_from('H',fcntl.ioctl(s,0x8913,r),16)[0]
fcntl.ioctl(s,0x8914,struct.pack('16sH',b'lo',flags|1)+b'\\x00'*22);s.close()
os.execvp('setpriv',['setpriv','--bounding-set=-all','--inh-caps=-all','--ambient-caps=-all','--no-new-privs',*sys.argv[1:]])
LOOP
'''
        with (output/'programs-and-limits.log').open('w') as log:
            docker('-i', command=['sh'], input=setup, text=True, stdout=log, stderr=subprocess.STDOUT)
        recorded = (output/'programs-and-limits.log').read_text()
        assert 'd1cc1e8658059c6a5040bdd27d9cc8e37a49fc4923a0243ae070c1bcc96a80d4' in recorded
        assert 'ff5add553bea12a45c18ebe6c67d3ed637052b869a1ca2fc5d74a6a2e8c5cce6' in recorded
        assert recorded.endswith('8589934592\n0\n')
        outer = ['/usr/bin/bwrap', '--unshare-all', '--cap-drop', 'ALL', '--ro-bind', '/usr', '/usr', '--symlink', 'usr/bin', '/bin', '--symlink', 'usr/lib', '/lib', '--symlink', 'usr/lib64', '/lib64', '--proc', '/proc', '--dev', '/dev', '--size', '536870912', '--tmpfs', '/tmp', '--ro-bind', '/work/source/target', '/work/source/target', '--bind', '/task', '/task', '--size', '16777216', '--tmpfs', '/mcp-scratch', '--chmod', '700', '/mcp-scratch', '--size', '1048576', '--tmpfs', '/router-tools', '--chmod', '700', '/router-tools', '--chdir', '/task', '--']
        with (output/'components.log').open('w') as log:
            docker('-e', 'AR_MCP_ROOT=/task/component-cases', '-e', 'AR_MCP_SCRATCH=/mcp-scratch', '-e', 'AR_MCP_PROGRAMS=/task/programs', '-e', 'AR_MCP_CLIENT=/task/programs/client', command=[*outer, binaries[0], 'finite_tools::', '--include-ignored', '--nocapture', '--test-threads=1'], stdout=log, stderr=subprocess.STDOUT)
        with (output/'native.log').open('w') as log:
            docker('-e', 'AR_MCP_NATIVE_ROOT=/task/native-cases', '-e', 'AR_MCP_NATIVE_INPUTS=/task/inputs', '-e', 'AR_MCP_NATIVE_ADAPTER=/task/adapter', '-e', 'AR_MCP_PROGRAMS=/task/programs', command=['unshare', '--user', '--map-current-user', '--keep-caps', '--net', '--mount', '--propagation', 'private', 'sh', '-c', 'mount -t sysfs -o ro sysfs /sys && exec python3 -B /task/loopback.py "$@"', 'sh', binaries[0], 'native_sdk_returns_actual_mcp_results_for_six_public_packets', '--ignored', '--nocapture', '--test-threads=1'], stdout=log, stderr=subprocess.STDOUT)
        names = []
        for packet in manifest['packets']:
            name = packet['assignment_id']
            names.extend([name+'/candidate', name+'/evidence', name+'/terminal.json', name+'/boot.json'])
        with (output/'public-results.tar').open('wb') as stream:
            docker(command=['tar', '-cf', '-', '-C', '/task/native-cases', *names], stdout=stream)
        with tarfile.open(output/'public-results.tar') as archive:
            for item in archive.getmembers():
                assert not item.name.startswith('/') and '..' not in Path(item.name).parts
                destination = output/'cases'/item.name
                if item.isdir():
                    destination.mkdir(parents=True, exist_ok=True)
                else:
                    assert item.isfile() and item.size <= 1048576
                    destination.parent.mkdir(parents=True, exist_ok=True)
                    destination.write_bytes(archive.extractfile(item).read())
        (output/'result.json').write_text(json.dumps({'verdict': 'PASS_LOCAL_NATIVE_COMPONENT_ONLY', 'models_started': 0, 'runtime_tools_admission': False, 'sealed_trials_started': 0, 'memory_bytes': 8589934592, 'swap_bytes': 0}, indent=2)+'\n')
    finally:
        current = json.loads(subprocess.check_output(['docker', 'inspect', identity]))[0]
        if current['Id'] != identity or current['Config']['Labels']['agent-router.task'] != label:
            raise RuntimeError('container ownership changed; retained')
        run(['docker', 'rm', '--force', identity])
    print('Evidence: '+str(output))


if __name__ == '__main__':
    main()
