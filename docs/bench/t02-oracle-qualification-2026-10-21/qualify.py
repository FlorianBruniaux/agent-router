#!/usr/bin/env python3
"""Finite offline t02 oracle probe. Not a product evaluator or adoption authority."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import resource
import signal
import stat
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parent
ORACLE_SHA256 = 'dcf8247d51256f888681b604b29503304eaa7fca89a9be97c3c6f1373c92b821'
MAX_OUTPUT = 65536
MEMORY = 8589934592
SUMMARY = re.compile(rb'^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out; finished in [^\r\n]+$', re.M)
CONTROLS = {
    'correct_i128': 'correct', 'correct_u128': 'correct',
    'sum_u64': 'known_defect', 'round_each': 'known_defect',
    'omit_bounds': 'known_defect', 'truncate_final': 'known_defect',
}


def digest(data):
    return hashlib.sha256(data).hexdigest()


def classify(returncode, output, expected_count):
    """Only a complete libtest summary can identify a functional outcome."""
    if len(output) > MAX_OUTPUT or expected_count <= 0:
        return 'unknown'
    matches = SUMMARY.findall(output)
    if len(matches) != 1:
        return 'unknown'
    label, *counts = matches[0]
    passed, failed, ignored, measured, filtered = map(int, counts)
    if passed + failed != expected_count or ignored or measured or filtered:
        return 'unknown'
    if returncode == 0 and label == b'ok' and failed == 0:
        return 'passed'
    if returncode == 101 and label == b'FAILED' and failed > 0:
        return 'failed'
    return 'unknown'


def read_input(path, max_bytes=1024 * 1024):
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(fd, 'rb') as stream:
        metadata = os.fstat(stream.fileno())
        if not stat.S_ISREG(metadata.st_mode) or metadata.st_nlink != 1:
            raise ValueError('input must be a regular single-link file')
        data = stream.read(max_bytes + 1)
        if len(data) > max_bytes:
            raise ValueError('input too large')
        return data


def isolation():
    if sys.platform != 'linux':
        raise ValueError('Linux isolation required')
    limits = {name: Path('/sys/fs/cgroup/' + name).read_text().strip()
              for name in ('memory.max', 'memory.swap.max')}
    if limits != {'memory.max': str(MEMORY), 'memory.swap.max': '0'}:
        raise ValueError('expected unchanged 8 GiB and zero swap')
    return limits


def child_limits():
    os.umask(0o077)
    resource.setrlimit(resource.RLIMIT_CPU, (60, 60))
    resource.setrlimit(resource.RLIMIT_AS, (4 * 1024**3, 4 * 1024**3))
    resource.setrlimit(resource.RLIMIT_FSIZE, (16 * 1024**2, 16 * 1024**2))
    resource.setrlimit(resource.RLIMIT_CORE, (0, 0))


def command(argv, cwd):
    with tempfile.TemporaryFile(dir=cwd) as output:
        try:
            proc = subprocess.Popen(argv, cwd=cwd, stdout=output, stderr=subprocess.STDOUT,
                                    stdin=subprocess.DEVNULL, start_new_session=True,
                                    preexec_fn=child_limits, env={'PATH': '/usr/bin:/bin',
                                    'HOME': str(cwd), 'LANG': 'C', 'LC_ALL': 'C'})
        except (OSError, subprocess.SubprocessError):
            return None, b'', 'spawn_failed'
        reason = None
        try:
            proc.wait(timeout=90)
        except subprocess.TimeoutExpired:
            reason = 'timeout'
        finally:
            try:
                os.killpg(proc.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            proc.wait()
        output.seek(0)
        data = output.read(MAX_OUTPUT + 1)
        if len(data) > MAX_OUTPUT:
            reason = 'output_limit'
        return proc.returncode, data, reason


def run_suite(compiler, source, suite, count, root):
    with tempfile.TemporaryDirectory(dir=root) as name:
        work = Path(name)
        (work / 'check.rs').write_bytes(source + b'\n' + suite)
        compiled, log, reason = command([compiler, '--edition=2021', '--test', 'check.rs',
                                        '-o', 'check'], work)
        if compiled != 0 or reason:
            return {'outcome': 'unknown', 'stage': 'compile', 'exit_code': compiled,
                    'reason': reason or 'compile_failed', 'output_sha256': digest(log)}
        exited, log, reason = command([str(work / 'check'), '--test-threads=1'], work)
        outcome = 'unknown' if reason else classify(exited, log, count)
        return {'outcome': outcome, 'stage': 'test', 'exit_code': exited,
                'reason': reason, 'output_sha256': digest(log)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--oracle', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    os.umask(0o077)
    limits = isolation()
    oracle = read_input(args.oracle)
    if digest(oracle) != ORACLE_SHA256:
        raise ValueError('frozen oracle digest mismatch')
    manifest_bytes = read_input(ROOT / 'controls.json')
    manifest = json.loads(manifest_bytes)
    expected_paths = {'public_witnesses.rs'} | {f'controls/{name}.rs' for name in CONTROLS}
    if set(manifest) != expected_paths:
        raise ValueError('unexpected control manifest')
    inputs = {path: read_input(ROOT / path) for path in sorted(expected_paths)}
    if any(digest(data) != manifest[path] for path, data in inputs.items()):
        raise ValueError('control digest mismatch')
    compiler = '/usr/local/rustup/toolchains/1.85.1-aarch64-unknown-linux-gnu/bin/rustc'
    # The local pinned image contains the compiler; never run rustup or download it.
    compiler_hash = digest(read_input(compiler, 64 * 1024**2))
    with tempfile.TemporaryDirectory() as tmp:
        code, version, reason = command([compiler, '--version', '--verbose'], tmp)
        if code != 0 or reason:
            raise ValueError('compiler identity unavailable')
        rows = []
        for name, label in CONTROLS.items():
            source = inputs[f'controls/{name}.rs']
            rows.append({'control': name, 'independent_label': label,
                         'source_sha256': digest(source),
                         'public_witnesses': run_suite(compiler, source, inputs['public_witnesses.rs'], 13, tmp),
                         'frozen_oracle': run_suite(compiler, source, oracle, 4, tmp)})
        failures = [
            run_suite('/nonexistent-compiler-qa174', b'', oracle, 4, tmp),
            run_suite(compiler, b'compile_error!("qa174 infrastructure probe");', oracle, 4, tmp),
        ]
    public_labels_match = all(row['public_witnesses']['outcome'] ==
                              ('passed' if row['independent_label'] == 'correct' else 'failed')
                              for row in rows)
    misses = [row['control'] for row in rows if row['independent_label'] == 'known_defect'
              and row['frozen_oracle']['outcome'] == 'passed']
    false_rejections = [row['control'] for row in rows if row['independent_label'] == 'correct'
                        and row['frozen_oracle']['outcome'] == 'failed']
    complete = public_labels_match and all(row['frozen_oracle']['outcome'] != 'unknown' for row in rows)
    complete = complete and all(failure['outcome'] == 'unknown' for failure in failures)
    verdict = ('unqualified' if misses or false_rejections else 'tested_controls_pass') if complete else 'inconclusive'
    result = {'schema_version': 1, 'task_id': 't02', 'verdict': verdict,
              'oracle_sha256': digest(oracle), 'runner_sha256': digest(read_input(Path(__file__))),
              'manifest_sha256': digest(manifest_bytes), 'input_sha256': manifest,
              'compiler_sha256': compiler_hash, 'compiler_version': version.decode('ascii').strip(),
              'limits': limits, 'child_address_space_bytes': 4 * 1024**3,
              'child_cpu_seconds': 60, 'child_wall_seconds': 90, 'child_output_bytes': MAX_OUTPUT,
              'public_labels_match': public_labels_match, 'controls': rows,
              'missed_defects': misses, 'false_rejections': false_rejections,
              'infrastructure_probes': failures, 'human_acceptance': None,
              'oracle_authenticity': 'supplied_unverified', 'adoption_authorized': False,
              'model_calls': 0, 'registry_access': False}
    fd = os.open(args.output, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    with os.fdopen(fd, 'w') as stream:
        json.dump(result, stream, indent=2, sort_keys=True)
        stream.write('\n')
    print(json.dumps({'verdict': verdict, 'missed_defects': misses}))
    return 0 if complete else 2


if __name__ == '__main__':
    raise SystemExit(main())
