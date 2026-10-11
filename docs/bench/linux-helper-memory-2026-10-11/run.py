#!/usr/bin/env python3
"""Bounded, offline diagnostic for an already-built real Linux helper. Python 3.11+."""
import argparse
import hashlib
import io
import json
from pathlib import Path
import shutil
import subprocess
import tarfile
import time
import uuid

BOUND = 8589934592
IMAGE = 'sha256:528257d48c1da0dcecc2e725d1ae34498d60c965f1241e39cd6a85a8859bdf84'
HERE = Path(__file__).resolve().parent
MAX_LOG = 65536


def sha(data):
    return hashlib.sha256(data).hexdigest()


def command(docker, name):
    return [docker, 'run', '--pull=never', '--rm', '--name', name,
            '--network=none', '--read-only', '--cap-drop=ALL',
            '--security-opt=no-new-privileges', '--user=65534:65534',
            '--memory=' + str(BOUND), '--memory-swap=' + str(BOUND),
            '--pids-limit=32', '--cpus=1',
            '--tmpfs=/work:rw,exec,nosuid,nodev,size=67108864,mode=1777',
            '--entrypoint=/bin/sh', '-i', IMAGE, '-c',
            'set -eu; tar -x -C /work; exec /usr/local/bin/python3 -I /work/inside.py']


def validate(row, receipt):
    """Evidence acceptance, never native authorization. Raise on missing or altered facts."""
    assert receipt['exit_code'] == 0 and receipt['container_absent'] is True
    assert receipt['elapsed_seconds'] < receipt['timeout_seconds'] == 45
    assert row['diagnostic_only'] is True and row['f12a'] == 'NO_GO'
    assert row['helper_sha256'] == receipt['helper_sha256']
    assert row['requested_address_space_bytes'] == BOUND
    assert row['controller_limits_before_after'] == [-1, -1]
    assert row['cgroups'] == {'memory.max': str(BOUND), 'memory.swap.max': '0', 'pids.max': '32'}
    assert row['probe'] == 'enforced'
    rows = row['supervision']
    assert [x['case'] for x in rows] == ['normal', 'deadline', 'eof', 'extra']
    for x, exit_code in zip(rows, (0, 124, 125, 125)):
        assert x['exit_code'] == exit_code and x['lease'] == 'group-clean'
        assert x['worker']['soft_hard'] == [BOUND, BOUND]
        assert x['supervisor_soft_hard'] == [-1, -1]
        assert 0 <= x['elapsed_seconds'] < 10
        if x['case'] != 'normal':
            assert any(y['pid'] == x['worker']['descendant'] for y in x['adopted_reaped'])
    for x in (row['direct'], rows[0]['worker']):
        assert x['soft_hard'] == [BOUND, BOUND]
        for oracle in (x['memory'], x['fork'], x['exec']['memory']):
            assert oracle['soft_hard'] == [BOUND, BOUND]
            assert oracle['small_allocation_bytes'] == 4194304
            assert oracle['large_mapping_errno'] == 12
            assert oracle['large_allocation_refused'] is True
            assert oracle['raise_and_remove_refused'] is True
        assert x['exec']['soft_hard'] == [BOUND, BOUND]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--helper', required=True, type=Path)
    parser.add_argument('--output-dir', required=True, type=Path)
    args = parser.parse_args()
    docker = shutil.which('docker')
    assert docker, 'existing Docker is required; nothing is installed'
    helper = args.helper.read_bytes()
    fixture = (HERE / 'inside.py').read_bytes()
    assert 0 < len(helper) < 16 * 1024**2 and helper[:4] == b'\x7fELF'
    assert 0 < len(fixture) < MAX_LOG
    # Output must be new. Never reuse or remove user directories.
    args.output_dir.mkdir(mode=0o700, parents=False, exist_ok=False)
    inspect = subprocess.run([docker, 'image', 'inspect', IMAGE], capture_output=True, timeout=10, check=True)
    assert len(inspect.stdout) < MAX_LOG
    image = json.loads(inspect.stdout)[0]
    assert image['Id'] == IMAGE and image['Os'] == 'linux' and image['Architecture'] == 'arm64'
    (args.output_dir / 'image.json').write_text(json.dumps({k: image[k] for k in ('Id', 'Os', 'Architecture')}, indent=2) + '\n')
    data = io.BytesIO()
    with tarfile.open(fileobj=data, mode='w') as archive:
        for name, content, mode in [('router-sandbox-helper', helper, 0o500), ('inside.py', fixture, 0o400)]:
            entry = tarfile.TarInfo(name)
            entry.size, entry.mode = len(content), mode
            archive.addfile(entry, io.BytesIO(content))
    name = 'ar-f12a-runtime-' + uuid.uuid4().hex[:16]
    argv = command(docker, name)
    started = time.monotonic()
    input_path = args.output_dir / 'input.tar'
    input_path.write_bytes(data.getvalue())
    stdout_path, stderr_path = args.output_dir / 'stdout.json', args.output_dir / 'stderr.log'
    stopped = None
    with input_path.open('rb') as stdin, stdout_path.open('wb') as stdout, stderr_path.open('wb') as stderr:
        proc = subprocess.Popen(argv, stdin=stdin, stdout=stdout, stderr=stderr)
        try:
            while proc.poll() is None:
                if time.monotonic() - started >= 45:
                    stopped = 'timeout'
                    break
                if any(p.stat().st_size > MAX_LOG for p in (stdout_path, stderr_path)):
                    stopped = 'output-budget'
                    break
                time.sleep(.02)
        finally:
            if proc.poll() is None:
                proc.kill()  # our unreaped Docker CLI child only
            proc.wait(timeout=5)
            # Always remove only this invocation's generated container, including interrupted runs.
            removal = subprocess.run([docker, 'rm', '-f', name], capture_output=True, timeout=10)
    listing = subprocess.run([docker, 'ps', '-a', '--filter', 'name=^' + name + '$', '--format', '{{.Names}}'],
                             capture_output=True, timeout=10)
    receipt = {'command': argv, 'exit_code': proc.returncode, 'stop_reason': stopped,
               'timeout_seconds': 45, 'elapsed_seconds': time.monotonic() - started,
               'helper_sha256': sha(helper), 'fixture_sha256': sha(fixture),
               'input_sha256': sha(data.getvalue()),
               'stdout_sha256': sha(stdout_path.read_bytes()), 'stderr_sha256': sha(stderr_path.read_bytes()),
               'stdout_bytes': stdout_path.stat().st_size, 'stderr_bytes': stderr_path.stat().st_size,
               'cleanup_exit_code': removal.returncode,
               'container_absent': listing.returncode == 0 and listing.stdout.strip() == b''}
    (args.output_dir / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
    assert not stopped and max(receipt['stdout_bytes'], receipt['stderr_bytes']) <= MAX_LOG
    assert args.helper.read_bytes() == helper and (HERE / 'inside.py').read_bytes() == fixture
    assert not stderr_path.read_bytes()
    row = json.loads(stdout_path.read_bytes())
    validate(row, receipt)
    print(json.dumps({'status': 'passed', 'evidence': str(args.output_dir), 'helper_sha256': sha(helper),
                      'f12a': 'NO_GO', 'container_absent': receipt['container_absent']}))


if __name__ == '__main__':
    main()
