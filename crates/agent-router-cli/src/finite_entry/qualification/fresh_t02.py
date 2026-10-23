"""Fixed fresh-entry qualification, without campaign identity translation."""
import hashlib
import stat
import types
import json
import os
from pathlib import Path
import re
import tempfile

SOURCE = '13518cf6578d789587a93e2fd904c937e16e7d38f5f02dddf5527915b4063871'
CANDIDATE = '10b6d5b17fb59a200f52a97876ec2f112e186886dae4ed4e352e2e0950eb7896'
PATCH = '9371af9ffc9f7a79e3bbd24d6787505fdf2353f21587b272005faf950440bb9c'
COMPILER = '1cdb3b2cbd8c79f44e0c8ff4a5c50cce33395c8c7767eb3a51f4f17a350f8d97'
VERIFIER = '4b85be2f53dc6027ee5edc8a062ab01401c4c20b3e23b916b9e17bb14733b4d1'
CASES = 'a0c07b43f7ec44d87e5b7b28b8f0e98f6efdda1e0f997167e794f0cc451aaebd'
sha = lambda data: hashlib.sha256(data).hexdigest()


def require(condition, reason):
    if not condition:
        raise ValueError(reason)


def unique(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, 'duplicate key')
        result[key] = value
    return result


def loads(raw):
    def invalid(_):
        raise ValueError('nonfinite number')
    return json.loads(raw, object_pairs_hook=unique, parse_constant=invalid)


def bind(report, submission, source, candidate, patch, compiler_sha256):
    require((sha(source), sha(candidate), sha(patch), compiler_sha256) ==
            (SOURCE, CANDIDATE, PATCH, COMPILER), 'unreviewed fixture bytes')
    require(report['schema_version'] == 1 and report['run_id'] == 'entry', 'fresh report identity')
    require(re.fullmatch('[0-9a-f]{64}', report['session_id']) is not None, 'session identity')
    require(report['submission_sha256'] == sha(submission), 'submission binding')
    task = loads(submission)
    require(task['output'] == 'lib.rs', 'output scope')
    require(task['public_files']['lib.rs'] == {'content': source.decode(), 'sha256': SOURCE}, 'source binding')
    for file in task['public_files'].values():
        require(sha(file['content'].encode()) == file['sha256'], 'public file binding')
    result, summary = report['result'], report['summary']
    for run in (result['run'], summary):
        require(run['run_id'] == 'entry' and run['state'] == 'verifying' and
                run['artifact_sha256'] == PATCH, 'run binding')
    require(summary['attempts'] == 1 and re.fullmatch('[0-9a-f]{64}', summary['receipt_id']), 'attempt or route')
    require(result['memory_enforcement'] == 'enforced', 'memory enforcement')
    require(result['transport']['exit_code'] == 0 and result['transport']['termination'] == 'exited', 'transport')
    require(report['runtime_error'] is False and report['broker_error'] is False, 'failed execution')
    require(report['source_unchanged'] is True and report['cleanup'] == 'namespace_terminated', 'source or cleanup')
    require(report['admission']['granted'] == 0 and report['admission']['released'] == 1, 'admission release')
    require(report['artifact'] == {'patch': patch.decode(), 'sha256': PATCH}, 'captured patch')
    require(report['output'] == {'name': 'lib.rs', 'content': candidate.decode()}, 'candidate binding')
    require(report['human_acceptance'] is None and report['independent_verification_performed'] is False, 'pre-verification report')
    return {'session_id': report['session_id'], 'submission_sha256': sha(submission),
            'run_id': 'entry', 'receipt_id': summary['receipt_id'], 'source_sha256': SOURCE,
            'candidate_sha256': CANDIDATE, 'artifact_sha256': PATCH, 'compiler_sha256': COMPILER}


def read(path, bound):
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    try:
        meta = os.fstat(fd)
        require(stat.S_ISREG(meta.st_mode) and meta.st_nlink == 1 and meta.st_size <= bound, 'input shape or size')
        with os.fdopen(os.dup(fd), 'rb') as stream:
            data = stream.read(bound + 1)
        require(len(data) == meta.st_size, 'changed input')
        return data
    finally:
        os.close(fd)


def load_engine(script):
    code = read(script, 32768)
    require(sha(code) == VERIFIER, 'verifier identity')
    engine = types.ModuleType('fixed_t02_engine')
    engine.__file__ = str(script)
    # Execute the exact checked bytes, never reopen the path after validation.
    exec(compile(code, str(script), 'exec'), engine.__dict__)
    return engine


def main():
    os.umask(0o077)
    require(Path('/sys/fs/cgroup/memory.max').read_text().strip() == '8589934592', '8 GiB required')
    require(Path('/sys/fs/cgroup/memory.swap.max').read_text().strip() == '0', 'zero swap required')
    root = Path(__file__).resolve().parent
    script = root / 'crates/router-runtime/examples/campaign/t02-v2/verify.py'
    engine = load_engine(script)
    raw_report = engine.read(root / 'report.json', 1048576)
    submission = engine.read(root / 'submission.json', 262144)
    source, candidate, patch = [engine.read(root / name, 131072) for name in ('source.rs', 'candidate.rs', 'change.patch')]
    report = loads(raw_report)
    compiler_hash = sha(engine.read(Path(engine.COMPILER), 67108864))
    receipt = bind(report, submission, source, candidate, patch, compiler_hash)
    checks = engine.read(script.parent / 'cases.rs', 32768)
    require(sha(checks) == CASES, 'case identity')
    names = [n.decode() for n in re.findall(rb'fn (t02_v2_[a-z0-9_]+)\(', checks)]
    require(len(names) == len(set(names)) == 18, 'case inventory')
    with tempfile.TemporaryDirectory(prefix='check-', dir=root) as directory:
        work = Path(directory)
        env = engine.environment(work)
        def run(argv):
            return engine.run(argv, work, env).stdout
        run(['git', 'init', '-q'])
        (work / 'lib.rs').write_bytes(source)
        run(['git', 'add', '--', 'lib.rs'])
        tree = run(['git', 'write-tree']).decode().strip()
        (work / 'change.patch').write_bytes(patch)
        run(['git', 'apply', '--cached', '--check', 'change.patch'])
        run(['git', 'apply', '--cached', 'change.patch'])
        entries = run(['git', 'ls-files', '--stage']).decode().splitlines()
        require(len(entries) == 1 and entries[0].startswith('100644 ') and entries[0].endswith('\tlib.rs'), 'scope or mode')
        require(run(['git', 'show', ':lib.rs']) == candidate, 'patch reconstruction')
        require(run(['git', 'diff', '--cached', '--no-ext-diff', '--no-color', tree, '--']) == patch, 'canonical patch')
        outcomes = {label: engine.evaluate_source(data, checks, names, work)
                    for label, data in [('source', source), ('candidate', candidate)]}
    require(outcomes['source']['outcome'] == 'failed' and outcomes['candidate']['outcome'] == 'passed', 'inconclusive evaluation')
    receipt.update({'schema_version': 1, 'scope': 'fresh_t02_v1', 'runtime_report_sha256': sha(raw_report),
                    'adapter_sha256': sha(read(Path(__file__), 32768)), 'verifier_sha256': VERIFIER,
                    'cases_sha256': CASES, 'outcomes': outcomes, 'memory_bytes': 8589934592,
                    'swap_bytes': 0, 'objective': 'unknown', 'human_acceptance': None,
                    'source_publication': False, 'runtime_verdict_modified': False})
    fd = os.open(root / 'receipt.json', os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    with os.fdopen(fd, 'w') as out:
        json.dump(receipt, out, sort_keys=True)
        out.write('\n')
        out.flush()
        os.fsync(out.fileno())


if __name__ == '__main__':
    main()
