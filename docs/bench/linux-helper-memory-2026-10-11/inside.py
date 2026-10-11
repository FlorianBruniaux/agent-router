"""Closed, inert Linux workload. This cannot grant native execution authority."""
import hashlib
import json
import mmap
import os
from pathlib import Path
import resource
import selectors
import struct
import subprocess
import sys
import time

BOUND = 8589934592
HELPER = '/work/router-sandbox-helper'
SELF = '/work/inside.py'
LIMITS = ['--cpu-seconds', '300', '--address-space-bytes', str(BOUND),
          '--max-open-files', '256', '--max-processes', '256',
          '--max-file-bytes', '16777216']
ENV = {'PATH': '/usr/bin:/bin'}


def limits():
    return list(resource.getrlimit(resource.RLIMIT_AS))


def memory_oracle():
    assert limits() == [BOUND, BOUND]
    assert resource.getrlimit(resource.RLIMIT_CORE) == (0, 0)
    with mmap.mmap(-1, 16 * 1024**2, flags=mmap.MAP_PRIVATE | mmap.MAP_ANONYMOUS, prot=0):
        pass
    small = bytearray(4 * 1024**2)
    small[-1] = 1
    del small
    try:
        mmap.mmap(-1, 9 * 1024**3, flags=mmap.MAP_PRIVATE | mmap.MAP_ANONYMOUS, prot=0)
    except OSError as exc:
        assert exc.errno == 12
    else:
        raise AssertionError('oversized mapping accepted')
    try:
        bytearray(9 * 1024**3)
    except MemoryError:
        pass
    else:
        raise AssertionError('oversized allocation accepted')
    for ceiling in (BOUND + 1024**2, resource.RLIM_INFINITY):
        try:
            resource.setrlimit(resource.RLIMIT_AS, (ceiling, ceiling))
        except (ValueError, PermissionError):
            pass
        else:
            raise AssertionError('hard bound raised or removed')
    return {'soft_hard': limits(), 'small_allocation_bytes': 4 * 1024**2,
            'large_mapping_errno': 12, 'large_allocation_refused': True,
            'raise_and_remove_refused': True}


def child(role):
    row = {'pid': os.getpid(), 'soft_hard': limits()}
    assert row['soft_hard'] == [BOUND, BOUND]
    if role == 'memory':
        row['memory'] = memory_oracle()
        read_fd, write_fd = os.pipe()
        pid = os.fork()
        if pid == 0:
            os.close(read_fd)
            os.write(write_fd, json.dumps(memory_oracle()).encode())
            os.close(write_fd)
            os._exit(0)
        os.close(write_fd)
        row['fork'] = json.loads(os.read(read_fd, 4096))
        os.close(read_fd)
        assert os.waitpid(pid, 0) == (pid, 0)
        result = subprocess.run([sys.executable, '-I', SELF, '--child', 'leaf'],
                                capture_output=True, text=True, env=ENV, timeout=5)
        assert result.returncode == 0 and not result.stderr
        row['exec'] = json.loads(result.stdout)
    elif role == 'leaf':
        row['memory'] = memory_oracle()
    elif role == 'live':
        descendant = subprocess.Popen([sys.executable, '-I', '-c', 'import time;time.sleep(20)'], env=ENV)
        row['descendant'] = descendant.pid
        print(json.dumps(row), flush=True)
        time.sleep(20)
        return
    else:
        raise AssertionError('unknown child role')
    print(json.dumps(row), flush=True)
    if role == 'memory' and os.environ.get('WAIT_RELEASE') == '1':
        end = time.monotonic() + 5
        while not Path('/work/normal-release').exists():
            assert time.monotonic() < end, 'controller did not release worker'
            time.sleep(.01)


def ready_line(pipe):
    data = bytearray()
    end = time.monotonic() + 5
    with selectors.DefaultSelector() as selector:
        selector.register(pipe, selectors.EVENT_READ)
        while not data.endswith(b'\n'):
            remaining = end - time.monotonic()
            assert remaining > 0 and selector.select(remaining), 'worker did not become ready'
            chunk = os.read(pipe.fileno(), 8192 - len(data))
            assert chunk, 'worker closed output before readiness'
            data.extend(chunk)
            assert len(data) < 8192, 'oversized readiness row'
    return json.loads(data)


def reap_orphans():
    results = []
    end = time.monotonic() + 2
    while time.monotonic() < end:
        try:
            pid, status = os.waitpid(-1, os.WNOHANG)
        except ChildProcessError:
            return results
        if pid:
            results.append({'pid': pid, 'status': status})
        else:
            time.sleep(.01)
    raise AssertionError('adopted child not reaped')


def supervise(case):
    lease = Path('/work/lease-' + case)
    lease.write_bytes(b'router-runtime-lease-v1:pending\n')
    lease.chmod(0o600)
    env = dict(ENV, ROUTER_RUNTIME_SUPERVISION='framed-v1', ROUTER_RUNTIME_LEASE=str(lease))
    role = 'memory' if case == 'normal' else 'live'
    if case == 'normal':
        env['WAIT_RELEASE'] = '1'
    # Worker limits apply in its helper; the cleanup supervisor stays outside them.
    argv = [HELPER, *LIMITS, '--supervise-v1',
            sys.executable, '-I', SELF, '--child', role]
    process = subprocess.Popen(argv, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                               stderr=subprocess.PIPE, env=env, start_new_session=True)
    started = time.monotonic()
    header = b'ARLIFE01' + struct.pack('>QQQ', 0, 250 if case == 'deadline' else 5000, 10)
    process.stdin.write(header)
    process.stdin.flush()
    try:
        row = ready_line(process.stdout)
        assert row['soft_hard'] == [BOUND, BOUND]
        supervisor_limits = list(resource.prlimit(process.pid, resource.RLIMIT_AS))
        assert supervisor_limits == limits(), supervisor_limits
        if case == 'normal':
            Path('/work/normal-release').touch()
        if case == 'eof':
            process.stdin.close()
            process.stdin = None
        elif case == 'extra':
            process.stdin.write(b'x')
            process.stdin.flush()
        orphans = []
        end = time.monotonic() + 8
        while process.poll() is None:
            # As namespace PID 1, reap the fixture's adopted child during cleanup.
            # waitpid only accepts our child; no worker-provided PID is signalled.
            if role == 'live' and not orphans:
                assert row['descendant'] not in (process.pid, os.getpid(), row['pid'])
                try:
                    pid, status = os.waitpid(row['descendant'], os.WNOHANG)
                    if pid:
                        orphans.append({'pid': pid, 'status': status})
                except ChildProcessError:
                    pass  # not adopted yet
            assert time.monotonic() < end, 'supervisor did not stop'
            time.sleep(.005)
        if process.stdin is not None:
            process.stdin.close()
            process.stdin = None
        stdout, stderr = process.communicate(timeout=2)
        assert not stdout and not stderr, (case, stdout[:1024], stderr[:1024])
        expected = {'normal': 0, 'deadline': 124, 'eof': 125, 'extra': 125}[case]
        assert process.returncode == expected
        assert lease.read_bytes() == b'router-runtime-lease-v1:group-clean\n'
        orphans.extend(reap_orphans())
        if role == 'live':
            assert any(x['pid'] == row['descendant'] for x in orphans)
            assert not Path('/proc/' + str(row['pid'])).exists()
            assert not Path('/proc/' + str(row['descendant'])).exists()
        return {'case': case, 'exit_code': process.returncode, 'worker': row,
                'supervisor_soft_hard': supervisor_limits, 'lease': 'group-clean',
                'adopted_reaped': orphans, 'elapsed_seconds': time.monotonic() - started}
    finally:
        if process.returncode is None:
            process.kill()  # owned unreaped child only; container teardown handles failure residue
            process.wait(timeout=2)
        for pipe in (process.stdin, process.stdout, process.stderr):
            if pipe is not None:
                pipe.close()


def main():
    assert os.getuid() == 65534 and sys.platform == 'linux'
    baseline = limits()
    assert baseline == [resource.RLIM_INFINITY, resource.RLIM_INFINITY], 'causal control must start uncapped'
    binary_before = hashlib.sha256(Path(HELPER).read_bytes()).hexdigest()
    probe = subprocess.run([HELPER, *LIMITS, '--probe'], capture_output=True, env=ENV, timeout=5)
    assert probe.returncode == 0 and not probe.stderr
    assert probe.stdout == b'router-sandbox-helper-v1:limits-applied:address-space=enforced\n'
    direct = subprocess.run([HELPER, *LIMITS, '--', sys.executable, '-I', SELF, '--child', 'memory'],
                            capture_output=True, env=ENV, timeout=8)
    assert direct.returncode == 0 and not direct.stderr
    direct_row = json.loads(direct.stdout)
    rows = [supervise(case) for case in ('normal', 'deadline', 'eof', 'extra')]
    assert limits() == baseline
    assert hashlib.sha256(Path(HELPER).read_bytes()).hexdigest() == binary_before
    cgroups = {name: Path('/sys/fs/cgroup/' + name).read_text().strip()
               for name in ('memory.max', 'memory.swap.max', 'pids.max')}
    assert cgroups == {'memory.max': str(BOUND), 'memory.swap.max': '0', 'pids.max': '32'}
    print(json.dumps({'diagnostic_only': True, 'f12a': 'NO_GO', 'helper_sha256': binary_before,
                      'requested_address_space_bytes': BOUND, 'controller_limits_before_after': baseline,
                      'probe': 'enforced', 'direct': direct_row, 'supervision': rows,
                      'cgroups': cgroups, 'kernel': os.uname().release, 'architecture': os.uname().machine}))


if __name__ == '__main__':
    if len(sys.argv) == 3 and sys.argv[1] == '--child':
        child(sys.argv[2])
    else:
        assert len(sys.argv) == 1
        main()
