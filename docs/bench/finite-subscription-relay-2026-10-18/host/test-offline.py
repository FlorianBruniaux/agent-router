"""Replay the 26 historical tests in disposable, offline fixtures."""
import ast
import hashlib
import json
import os
import platform
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parent
HISTORICAL_CONTROLLER = '/Users/florianbruniaux/.codex/worktrees/f12a-strict-memory/agent-router/docs/bench/finite-subscription-relay-2026-10-18/controller'
HISTORICAL_PACKETS = '/private/tmp/ar-real-campaign-packets-20261018.json'
SUITES = [('launcher', 'relay-controller-subscription-launcher-test.py', 12),
          ('parent', 'relay-controller-subscription-parent-test.py', 10),
          ('initial_timeout', 'relay-controller-subscription-timeout-test.py', 4)]
# Loaded by each test interpreter, including the simulated Runtime children.
GUARD = r'''
import os, sys
_scratch = os.path.realpath(os.environ['AR_OFFLINE_SCRATCH'])
def _inside(path):
    return os.path.commonpath((_scratch, os.path.realpath(os.fsdecode(path)))) == _scratch

def _audit(event, args):
    if event.startswith('socket.') or event in ('os.system', 'os.exec', 'os.fork'):
        raise PermissionError('offline_network_or_process_forbidden')
    if event in ('subprocess.Popen', 'os.posix_spawn'):
        executable, argv = args[:2]
        if (os.path.realpath(executable) != os.path.realpath(sys.executable)
                or not isinstance(argv, (list, tuple)) or len(argv) < 3
                or argv[1] != '-B' or not _inside(argv[2])):
            raise PermissionError('offline_only_fixture_python_processes')
    if event == 'open' and not isinstance(args[0], int):
        path, mode, flags = args
        name = os.path.basename(os.fsdecode(path))
        if name == 'auth.json' or name in ('results.json', 'parent.lock') and not _inside(path):
            raise PermissionError('offline_auth_or_live_registry_forbidden')
        if flags & (os.O_WRONLY | os.O_RDWR | os.O_CREAT | os.O_TRUNC) and not _inside(path):
            raise PermissionError('offline_write_outside_fixture')
    if event in ('os.remove', 'os.rmdir', 'os.mkdir') and not _inside(args[0]):
        raise PermissionError('offline_mutation_outside_fixture')
    if event in ('os.rename', 'os.link', 'os.symlink') and not all(_inside(p) for p in args[:2]):
        raise PermissionError('offline_mutation_outside_fixture')
sys.addaudithook(_audit)
'''
PROBE = r'''
import json, os, pathlib, socket, subprocess
checks = []
for name, operation in [
    ('network', lambda: socket.socket()),
    ('credentials', lambda: open(pathlib.Path(os.environ['AR_OFFLINE_SCRATCH'])/'auth.json')),
    ('registry', lambda: open('/__ar_offline_forbidden__/results.json')),
    ('process', lambda: subprocess.run(['docker', 'version'])),
]:
    try: operation()
    except PermissionError: checks.append(name)
    else: raise AssertionError('offline_guard_failed_' + name)
assert checks == ['network', 'credentials', 'registry', 'process']
print(json.dumps(checks))
'''

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def replace_once(text, old, new):
    if text.count(old) != 1:
        raise ValueError('historical_test_preimage_changed')
    return text.replace(old, new)

def main():
    recovery = json.loads((ROOT/'recovery.json').read_text())
    expected = {row['path']: row['sha256'] for row in recovery['files']}
    if len(expected) != 5 or any(digest(ROOT/name) != h for name, h in expected.items()):
        raise ValueError('historical_source_hash_changed')
    controller = ROOT.parent/'controller'
    packets = ROOT.parents[3]/'crates/router-runtime/examples/campaign/packets.json'
    tree = ast.parse((ROOT/'relay-controller-subscription-launcher.py').read_text())
    pins = next(ast.literal_eval(n.value) for n in tree.body if isinstance(n, ast.Assign)
                and any(isinstance(t, ast.Name) and t.id == 'PINS' for t in n.targets))
    if any(digest(controller/name) != h for name, h in pins.items()):
        raise ValueError('qualified_controller_hash_changed')
    if digest(packets) != '2d4d225e528fff5ba402e7265140521f212d7b4a3443c3dd62f1db00f9e0281a':
        raise ValueError('public_packets_hash_changed')
    results = []
    with tempfile.TemporaryDirectory(prefix='ar-subscription-host-offline-') as tmp:
        scratch = Path(tmp).resolve()
        for name in expected:
            shutil.copyfile(ROOT/name, scratch/name)
        # Rebind historical test locations only in disposable test copies.
        for name in (SUITES[0][1], SUITES[1][1]):
            text = (scratch/name).read_text()
            text = replace_once(text, HISTORICAL_CONTROLLER, str(controller))
            text = replace_once(text, "dir='/private/tmp'", 'dir=' + repr(str(scratch)))
            if name == SUITES[1][1]:
                text = replace_once(text, HISTORICAL_PACKETS, str(packets))
            (scratch/name).write_text(text)
        (scratch/'sitecustomize.py').write_text(GUARD)
        (scratch/'guard-probe.py').write_text(PROBE)
        env = os.environ.copy()
        env.update(PYTHONPATH=str(scratch), AR_OFFLINE_SCRATCH=str(scratch), PYTHONDONTWRITEBYTECODE='1')
        def run(name):
            return subprocess.run([sys.executable, '-B', str(scratch/name)], cwd=scratch,
                                  env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                  text=True, timeout=120)
        guard = run('guard-probe.py')
        if guard.returncode or json.loads(guard.stdout) != ['network', 'credentials', 'registry', 'process']:
            raise RuntimeError('offline_guard_probe_failed: ' + guard.stdout)
        for label, name, count in SUITES:
            result = run(name)
            found = re.search(r'^Ran (\d+) tests? in ', result.stdout, re.MULTILINE)
            passed = result.returncode == 0 and found is not None and int(found[1]) == count
            results.append({'suite': label, 'tests': int(found[1]) if found else None,
                            'expected_tests': count, 'exit_code': result.returncode, 'passed': passed})
            if not passed:
                print(result.stdout, file=sys.stderr)
        fixture_processes_only = True
    unchanged = all(digest(ROOT/name) == h for name, h in expected.items())
    report = {'scope': 'offline replay of recovered historical host delivery',
              'python_version': platform.python_version(), 'suites': results,
              'tests_run': sum(r['tests'] or 0 for r in results),
              'all_passed': all(r['passed'] for r in results), 'five_historical_hashes_match': unchanged,
              'qualified_controller_pins_match': len(pins), 'public_packets_pin_matches': True,
              'guard_probe_passed': True, 'fixture_python_processes_only': fixture_processes_only,
              'provider_requests': 0, 'credentials_accessed': False, 'live_registry_accessed': False,
              'historical_sources_modified': not unchanged,
              'test_only_changes': 'fixture paths rebound only in disposable copies'}
    print(json.dumps(report, indent=2))
    return 0 if report['all_passed'] and unchanged else 1

if __name__ == '__main__':
    raise SystemExit(main())
