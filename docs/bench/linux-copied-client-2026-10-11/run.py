#!/usr/bin/env python3
"""Explicit isolated diagnostic replay, Python 3.11+. No pulls, model calls or host mounts."""
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

IMAGE = 'sha256:528257d48c1da0dcecc2e725d1ae34498d60c965f1241e39cd6a85a8859bdf84'
HERE = Path(__file__).resolve().parent
CASES = ('normal', 'deadline', 'eof', 'helper-kill')
MAX_OUTPUT = 131072
BOUND = 8589934592


def docker(*args, timeout=15):
    return subprocess.run(['docker', *args], capture_output=True, timeout=timeout, check=True)


def settings():
    return ['--pull=never', '--network=none', '--read-only', '--cap-drop=ALL',
            '--security-opt=no-new-privileges', '--memory=8589934592', '--memory-swap=8589934592',
            '--pids-limit=256', '--cpus=1']


def observe(volume):
    # Independent disposable observer sees only this task's heartbeat volume, read-only.
    return int(docker('run', '--rm', *settings(), '--user=65534:65534',
        '--mount', 'type=volume,source=' + volume + ',target=/evidence,readonly',
        '--entrypoint=/usr/local/bin/python3', IMAGE, '-I', '-c',
        'from pathlib import Path;print(Path("/evidence/heartbeat").stat().st_size)').stdout)


def validate(row):
    assert row['diagnostic_only'] is True and row['f12a'] == 'NO_GO'
    assert row['case'] in CASES and row['address_space_bytes'] == BOUND
    assert row['protected_identities_after'] == 'unchanged'
    assert row['controller_still_owns_copies'] is True
    assert row['namespace_cleanup'] == 'PENDING_EXTERNAL_OWNER'
    assert row['native_rpc'] == 'initialize-and-thread-start-only' and row['provider_turns'] == 0
    assert row['broker_connections'] == 3 and row['other_listener_connections'] == 1
    assert row['canary']['memory'] == '8GiB' and row['canary']['protected_assets'] == 6
    assert row['canary']['broker'] == 'exact-port'
    assert len(row['copy_hashes']) == 6 and all(isinstance(x, str) and len(x) == 64 and all(c in '0123456789abcdef' for c in x) for x in row['copy_hashes'])
    expected = {'normal': 0, 'deadline': 124, 'eof': 125, 'helper-kill': None}[row['case']]
    if expected is None:
        assert row['helper_exit_code'] is None
    else:
        assert type(row['helper_exit_code']) is int and row['helper_exit_code'] == expected
    assert row['lease'] == 'router-runtime-lease-v1:' + ('active' if row['case'] == 'helper-kill' else 'group-clean')


def validate_complete(record):
    validate(record['provisional'])
    assert record['stop_reason'] is None and record['diagnostic_only'] is True and record['f12a'] == 'NO_GO'
    assert record['image'] == IMAGE and record['model_calls'] == 0
    assert record['memory_bytes'] == BOUND and record['namespace_exit_code'] == 137
    assert record['namespace_stopped'] is True and record['container_absent'] is True and record['volume_removed'] is True
    before, after = record['heartbeat_before_teardown'], record['heartbeat_after_teardown']
    assert len(before) == 2 and len(after) == 3 and all(type(x) is int for x in before + after)
    assert 0 < before[0] < before[1] <= after[0] and len(set(after)) == 1


def run_case(case, archive, output):
    output.mkdir(mode=0o700)
    nonce = uuid.uuid4().hex
    volume = 'ar-f12a-heartbeat-' + nonce
    name = 'ar-f12a-copied-' + nonce
    cid = None
    proc = None
    record = {'case': case, 'image': IMAGE, 'memory_bytes': BOUND, 'model_calls': 0,
              'diagnostic_only': True, 'f12a': 'NO_GO', 'stop_reason': None}
    started = time.monotonic()
    # New uniquely labelled Docker volume, never a host directory or existing volume.
    created_volume = docker('volume', 'create', '--label', 'agent-router.diagnostic=' + nonce, volume).stdout.decode().strip()
    assert created_volume == volume
    try:
        docker('run', '--rm', *settings(), '--user=0:0', '--mount', 'type=volume,source=' + volume + ',target=/evidence',
               '--entrypoint=/usr/local/bin/python3', IMAGE, '-I', '-c',
               'from pathlib import Path;p=Path("/evidence/heartbeat");p.write_bytes(b"heartbeat-v1\\n");p.chmod(0o666)')
        argv = ['create', '--name', name, '--label', 'agent-router.diagnostic=' + nonce, *settings(),
            '--user=65534:65534', '--tmpfs=/work:rw,exec,nosuid,nodev,size=1073741824,mode=1777',
            '--mount', 'type=volume,source=' + volume + ',target=/evidence',
            '--entrypoint=/bin/sh', '-i', IMAGE, '-c',
            'set -eu; tar -xz -C /work; exec /usr/local/bin/python3 -I /work/inside.py ' + case]
        cid = docker(*argv).stdout.decode().strip()
        assert len(cid) == 64 and all(x in '0123456789abcdef' for x in cid)
        record['container_id'], record['command'] = cid, ['docker', *argv]
        inspect = json.loads(docker('inspect', cid).stdout)[0]
        assert inspect['Config']['Labels']['agent-router.diagnostic'] == nonce
        assert inspect['HostConfig']['Memory'] == BOUND and inspect['HostConfig']['MemorySwap'] == BOUND
        assert inspect['HostConfig']['NetworkMode'] == 'none' and inspect['HostConfig']['ReadonlyRootfs'] is True
        with archive.open('rb') as src, (output/'stdout.json').open('wb') as out, (output/'stderr.log').open('wb') as err:
            proc = subprocess.Popen(['docker', 'start', '-a', '-i', cid], stdin=src, stdout=out, stderr=err)
            while True:
                if any((output/x).stat().st_size > MAX_OUTPUT for x in ('stdout.json', 'stderr.log')):
                    raise AssertionError('capture output exceeded budget')
                if time.monotonic() - started >= 60:
                    raise AssertionError('capture deadline')
                if proc.poll() is not None:
                    raise AssertionError('namespace exited before the provisional report')
                data = (output/'stdout.json').read_bytes()
                if data.endswith(b'\n'):
                    row = json.loads(data); validate(row); break
                time.sleep(.05)
            record['provisional'] = row
            # Demonstrate real continued activity AFTER group completion / helper death.
            before = [observe(volume), observe(volume)]
            assert before[1] > before[0], 'detached writer not independently observed alive'
            record['heartbeat_before_teardown'] = before
            docker('kill', '--signal=KILL', cid)
            proc.wait(timeout=5)
            after = [observe(volume), observe(volume), observe(volume)]
            assert len(set(after)) == 1 and after[0] >= before[1], 'detached writer continued after namespace teardown'
            record['heartbeat_after_teardown'] = after
        stopped = json.loads(docker('inspect', cid).stdout)[0]
        assert stopped['State']['Running'] is False and stopped['State']['ExitCode'] == 137
        record['namespace_stopped'] = True
        record['namespace_exit_code'] = 137
    except BaseException as exc:
        record['stop_reason'] = type(exc).__name__ + ': ' + str(exc)
        raise
    finally:
        if cid:
            subprocess.run(['docker', 'rm', '-f', cid], capture_output=True, timeout=10)
        if proc is not None and proc.poll() is None:
            proc.kill(); proc.wait(timeout=5)
        listing = docker('ps', '-a', '--filter', 'label=agent-router.diagnostic=' + nonce, '--format', '{{.ID}}')
        record['container_absent'] = not listing.stdout.strip()
        # The volume is retained until every observer/worker is gone, then only this owned volume is removed.
        removed = subprocess.run(['docker', 'volume', 'rm', volume], capture_output=True, timeout=10)
        volumes = docker('volume', 'ls', '--filter', 'label=agent-router.diagnostic=' + nonce, '--format', '{{.Name}}')
        record['volume_removed'] = removed.returncode == 0 and not volumes.stdout.strip()
        record['elapsed_seconds'] = time.monotonic() - started
        for filename in ('stdout.json', 'stderr.log'):
            p = output/filename
            if p.exists(): record[filename + '_sha256'] = hashlib.sha256(p.read_bytes()).hexdigest()
        (output/'receipt.json').write_text(json.dumps(record, indent=2) + '\n')
    validate_complete(record)
    return record


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binaries', required=True, type=Path)
    parser.add_argument('--native', required=True, type=Path)
    parser.add_argument('--output-dir', required=True, type=Path)
    parser.add_argument('--case', choices=CASES, action='append')
    args = parser.parse_args()
    assert shutil.which('docker')
    args.output_dir.mkdir(mode=0o700, exist_ok=False)
    image = json.loads(docker('image', 'inspect', IMAGE).stdout)[0]
    assert image['Id'] == IMAGE and image['Os'] == 'linux' and image['Architecture'] == 'arm64'
    entries = {name: (args.binaries/name).read_bytes() for name in ('client','controller','helper','wrapper')}
    entries['native'] = args.native.read_bytes()
    inside_bytes = (HERE/'inside.py').read_bytes()
    assert len(entries['native']) < 256*1024**2 and hashlib.sha256(entries['native']).hexdigest() == '113aa5d5952a6fd3950a88323219747d0c91978ebee948509a540c8a460e59a3'
    assert all(0 < len(b) < 256*1024**2 and b[:4] == b'\x7fELF' for b in entries.values())
    archive = args.output_dir/'input.tar.gz'
    with tarfile.open(archive, 'w:gz') as tar:
        for name, data in entries.items():
            entry=tarfile.TarInfo('input/'+name);entry.mode=0o500;entry.size=len(data);tar.addfile(entry,io.BytesIO(data))
        data=inside_bytes;entry=tarfile.TarInfo('inside.py');entry.mode=0o400;entry.size=len(data);tar.addfile(entry,io.BytesIO(data))
    rows = [run_case(case,archive,args.output_dir/case) for case in (args.case or CASES)]
    assert (HERE/'inside.py').read_bytes() == inside_bytes
    assert args.native.read_bytes() == entries['native']
    assert all((args.binaries/name).read_bytes() == entries[name] for name in ('client','controller','helper','wrapper'))
    (args.output_dir/'summary.json').write_text(json.dumps({'diagnostic_only':True,'f12a':'NO_GO','binaries_sha256':{name:hashlib.sha256(data).hexdigest() for name,data in entries.items()},'inside_sha256':hashlib.sha256(inside_bytes).hexdigest(),'cases':rows},indent=2)+'\n')
    print(json.dumps({'status':'passed','cases':len(rows),'f12a':'NO_GO','evidence':str(args.output_dir)}))


if __name__ == '__main__':
    main()
