"""Namespace PID 1 diagnostic owner. All real effects remain within this container."""
import json
import os
from pathlib import Path
import subprocess
import sys
import threading
import time

assert sys.platform == 'linux' and os.getuid() == 65534
assert Path('/sys/fs/cgroup/memory.max').read_text().strip() == '8589934592'
assert Path('/sys/fs/cgroup/memory.swap.max').read_text().strip() == '0'
case = sys.argv[1]
assert case in ('normal', 'deadline', 'eof', 'helper-kill')
env = {'PATH': '/usr/bin:/bin', 'TMPDIR': '/work/tmp', 'AR_LINUX_DIAGNOSTIC': 'namespace-owned-v1', 'AR_LINUX_CASE': case}
Path('/work/tmp').mkdir(mode=0o700)
process = subprocess.Popen(['/work/input/controller', '--ignored', '--exact',
    'finite_client::linux_qualification::namespace_owned_client_os_qualification', '--nocapture', '--test-threads=1'],
    env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
stopping = threading.Event()
last_errors = {}
def preserve_native_errors():
    while not stopping.wait(.01):
        for p in Path('/work/task/store').glob('**/native-errors.private'):
            try:
                data = p.read_bytes()[:65536]
                if data:
                    last_errors[str(p)] = data
            except FileNotFoundError:
                pass
watcher = threading.Thread(target=preserve_native_errors, daemon=True)
watcher.start()
try:
    for line in process.stdout:
        assert len(line) < 65536
        if b'AR_LINUX_REPORT:' in line:
            row = json.loads(line.split(b'AR_LINUX_REPORT:', 1)[1])
            assert row['case'] == case and row['f12a'] == 'NO_GO'
            print(json.dumps(row), flush=True)
            # Keep PID 1, the preparation owner and its copies alive until the
            # outside owner destroys this exact container identity.
            time.sleep(30)
            raise AssertionError('external teardown did not arrive')
        else:
            sys.stderr.buffer.write(line); sys.stderr.flush()
    raise AssertionError('controller ended before the complete provisional report')
finally:
    stopping.set(); watcher.join(timeout=1)
    for data in last_errors.values():
        sys.stderr.buffer.write(data); sys.stderr.flush()
    if process.poll() is None:
        process.kill(); process.wait(timeout=3)
    errors = Path('/work/task/worker-errors')
    if errors.exists():
        data = errors.read_bytes()
        sys.stderr.buffer.write(data[:65536]); sys.stderr.flush()
