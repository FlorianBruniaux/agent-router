#!/usr/bin/env python3
"""Check current offline M3 harness tests against their sealed historical inputs.

No pilot, host or provider launch. Current report behavior has its own campaign
suite; solved tasks are not reintroduced as eligible work by this fixture check.
"""
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import re
import subprocess
import sys
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parents[1]
SCOPES = ('docs/bench/calibration-v2', 'docs/bench/campaign-next/pilot',
          'tests/fixtures/evaluation/accepted-changes-v1.json')
MAX_ARCHIVE = 16 * 1024 * 1024


def prepare(root, destination):
    candidate = json.loads((root / 'docs/bench/measurement-m123/m3/candidate.json').read_text())
    revision = candidate['source_commit']
    if not isinstance(revision, str) or not re.fullmatch('[0-9a-f]{40}', revision):
        raise ValueError('baseline_revision')
    archive = destination.parent / 'baseline.tar'
    subprocess.run(['git', '-C', str(root), 'archive', '--format=tar',
                    '--output=' + str(archive), revision, '--', *SCOPES],
                   check=True, capture_output=True, timeout=10)
    if archive.stat().st_size > MAX_ARCHIVE:
        raise ValueError('baseline_size')
    destination.mkdir(mode=0o700)
    with tarfile.open(fileobj=io.BytesIO(archive.read_bytes()), mode='r:') as tar:
        members = tar.getmembers()
        if len(members) > 512 or sum(m.size for m in members) > MAX_ARCHIVE:
            raise ValueError('baseline_size')
        for member in members:
            path = PurePosixPath(member.name)
            if path.is_absolute() or '..' in path.parts or not (member.isfile() or member.isdir()):
                raise ValueError('baseline_path')
            target = destination.joinpath(*path.parts)
            if member.isdir():
                target.mkdir(parents=True, exist_ok=True, mode=0o700)
            else:
                target.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
                stream = tar.extractfile(member)
                if stream is None:
                    raise ValueError('baseline_file')
                with target.open('xb') as output:
                    output.write(stream.read(MAX_ARCHIVE + 1))
                target.chmod(0o600)
    for name, expected in candidate['repository_inputs'].items():
        path = PurePosixPath(name)
        if path.is_absolute() or '..' in path.parts:
            raise ValueError('baseline_input_path')
        actual = hashlib.sha256(destination.joinpath(*path.parts).read_bytes()).hexdigest()
        if actual != expected:
            raise ValueError('baseline_hash')
    return revision


def main():
    try:
        with tempfile.TemporaryDirectory(prefix='ar-m3-baseline-') as directory:
            baseline = Path(directory) / 'source'
            revision = prepare(ROOT, baseline)
            print('M3 historical fixture source: ' + revision, flush=True)
            env = dict(os.environ, M3_REPO=str(baseline), PYTHONDONTWRITEBYTECODE='1')
            return subprocess.run([sys.executable, '-B', '-m', 'unittest', 'discover', '-s',
                'docs/bench/measurement-m123/m3/tests', '-p', 'test_*.py', '-v'],
                cwd=ROOT, env=env, timeout=120).returncode
    except (OSError, ValueError, KeyError, subprocess.SubprocessError, tarfile.TarError):
        print('M3 baseline unavailable or mismatched', file=sys.stderr)
        return 2


if __name__ == '__main__':
    raise SystemExit(main())
