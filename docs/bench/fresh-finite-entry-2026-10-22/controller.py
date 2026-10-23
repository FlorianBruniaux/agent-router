"""Single authored parent for the fresh product CLI, in an isolated Linux cell.

No provider/account imports, no routing logic, no fabricated Runtime receipts.
Every request is received from the product. Never run this against an account.
"""
import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import selectors
import signal
import sqlite3
import stat
import subprocess
import sys
import tempfile
import time
import traceback

HERE = Path(__file__).resolve().parent
EVALUATOR = HERE / 'evaluation-repo/crates/router-runtime/examples/campaign/t02-v2/verify.py'
sys.path.insert(0, str(HERE / 'inputs'))
from native_tool_responses import ToolScript, validate_native_catalog
from tool_responses import _response, _catalog

LIMIT = 1048576
sha = lambda b: hashlib.sha256(b).hexdigest()


def unique(pairs):
    result = {}
    for k, v in pairs:
        if k in result:
            raise ValueError('duplicate key')
        result[k] = v
    return result


def loads(raw):
    return json.loads(raw, object_pairs_hook=unique,
                      parse_constant=lambda _: (_ for _ in ()).throw(ValueError('constant')))


def write_new(path, data):
    with Path(path).open('xb') as f:
        f.write(data)


def save(path, data):
    write_new(path, (json.dumps(data, indent=2) + '\n').encode())


def denied_output(output):
    # Error content stays separate from the successful tool-result adapter.
    if isinstance(output, list) and len(output) == 2:
        first, second = output
        if (set(first) == {'type', 'text'} and first['type'] == 'input_text'
                and re.fullmatch(r'Wall time: [0-9]{1,6}\.[0-9]{4} seconds\nOutput:', first['text'])
                and set(second) == {'type', 'text'} and second['type'] == 'input_text'):
            output = second['text']
    return output in ('Tool request refused.', 'Error: Tool request refused.')


class DeniedThenScript:
    """An actual unapproved read, then the unchanged authored correction loop."""
    def __init__(self, config):
        self.script = ToolScript(config['steps'])
        self.initial = True
        self.denials_seen = 0

    @property
    def observed_tool_results(self):
        return self.script.observed_tool_results

    def respond(self, request):
        validate_native_catalog(request)
        if self.initial:
            # Reuse the exact native/identity/marker validation without consuming
            # the real successful script. The rejected read has a distinct ID.
            copy.deepcopy(self.script).respond(request)
            finite = {**request, 'tools': [t for t in request['tools'] if t['name'] == 'mcp__finite']}
            namespace, name = _catalog(finite)['mcp__finite__read_file']
            self.initial = False
            return _response('gpt-6-luna', 0, name, {'path': 'private-canary'}, namespace)
        outputs = [i for i in request['input'] if i.get('type') == 'function_call_output'
                   and i.get('call_id') == 'call_r2_local_0']
        if len(outputs) != 1 or not denied_output(outputs[0].get('output')):
            raise ValueError('unapproved read did not return the exact refusal')
        self.denials_seen += 1
        projected = dict(request)
        projected['input'] = [i for i in request['input'] if i is not outputs[0]]
        return self.script.respond(projected)


def submission():
    # Explicit public inline bytes, not a directory scan of a user's workspace.
    files = {}
    for name in ('AGENTS.md', 'statement.md', 'lib.rs', 'HARNESS.md',
                 'feedback-tests.rs', 'feedback.py'):
        data = (HERE / 'inputs' / name).read_bytes()
        files[name] = {'content': data.decode(), 'sha256': sha(data)}
    return {'schema_version': 1, 'kind': 'submit', 'model': 'gpt-6-luna',
            'effort': 'medium', 'output': 'lib.rs', 'public_files': files,
            'task': {'objective': 'Correct score according to statement.md. R2-LOCAL-TOOLS-V1',
                     'kind': 'implement', 'deliverables': ['lib.rs'], 'scope': ['lib.rs'],
                     'risk': 'low', 'sensitivity': 'public',
                     'budget': {'max_cost_microusd': 10000, 'max_elapsed_ms': 60000,
                                'max_tokens': 6000},
                     'allowed_mutations': ['workspace_write', 'network'],
                     'acceptance': ['Only lib.rs changes and matches the public score contract'],
                     'required_capabilities': ['network', 'workspace_write', 'tools']},
            'risk_context': {'schema_version': 1, 'environment': 'local',
                             'reversibility': 'reversible', 'operations': ['workspace_change']}}


class Session:
    """Bounded nonblocking parent I/O. No background readers or retry loop."""
    def __init__(self, argv, env, evidence, deadline=120):
        self.evidence = evidence
        self.child = subprocess.Popen(argv, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                      stderr=subprocess.PIPE, env=env, start_new_session=True)
        self.end = time.monotonic() + deadline
        self.selector = selectors.DefaultSelector()
        self.buffers = {'stdout': bytearray(), 'stderr': bytearray()}
        self.pending = bytearray()
        self.raw = {n: (evidence / (n + '.raw')).open('xb') for n in self.buffers}
        self.total = 0
        for name in self.buffers:
            f = getattr(self.child, name)
            os.set_blocking(f.fileno(), False)
            self.selector.register(f, selectors.EVENT_READ, name)
        os.set_blocking(self.child.stdin.fileno(), False)

    def tick(self, end=None):
        left = min(self.end, end or self.end) - time.monotonic()
        if left <= 0:
            raise TimeoutError('parent deadline')
        for key, _ in self.selector.select(min(left, 0.25)):
            data = os.read(key.fileobj.fileno(), 65536)
            if not data:
                self.selector.unregister(key.fileobj)
                continue
            self.total += len(data)
            if self.total > 8388608:
                raise ValueError('capture limit')
            self.raw[key.data].write(data)
            self.buffers[key.data].extend(data)
            if key.data == 'stdout':
                self.pending.extend(data)
                if len(self.pending) > LIMIT and b'\n' not in self.pending[:LIMIT + 1]:
                    raise ValueError('line limit')

    def send(self, value):
        raw = (json.dumps(value, separators=(',', ':')) + '\n').encode()
        if len(raw) > 262144:
            raise ValueError('reply limit')
        fd = self.child.stdin.fileno()
        end = time.monotonic() + 5
        offset = 0
        while offset < len(raw):
            try:
                offset += os.write(fd, raw[offset:])
            except BlockingIOError:
                self.tick(end)

    def line(self):
        while b'\n' not in self.pending:
            if not self.selector.get_map():
                return None
            self.tick()
        raw, _, rest = self.pending.partition(b'\n')
        self.pending = bytearray(rest)
        if len(raw) > LIMIT:
            raise ValueError('line limit')
        return loads(raw)

    def finish(self):
        self.child.stdin.close()
        while self.selector.get_map():
            self.tick()
        code = self.child.wait(timeout=max(0.1, self.end - time.monotonic()))
        if self.pending:
            raise ValueError('unterminated stdout')
        return code

    def close(self):
        # Kill process group while the leader is still waitable if possible.
        if self.child.returncode is None:
            try:
                os.killpg(self.child.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            self.child.wait(timeout=5)
        for f in self.raw.values():
            f.close()
        self.selector.close()
        for name in ('stdin', 'stdout', 'stderr'):
            f = getattr(self.child, name)
            if not f.closed:
                f.close()


def independent_readback(root):
    db = root / 'runtime/runtime.sqlite3'
    if not db.exists():
        return {'runtime_exists': False}
    st = db.lstat()
    assert stat.S_ISREG(st.st_mode) and st.st_nlink == 1
    with sqlite3.connect(db.as_uri() + '?mode=ro', uri=True) as c:
        summaries = c.execute('SELECT summary FROM runs WHERE run_id=?', ('entry',)).fetchmany(2)
        events = c.execute('SELECT sequence,kind,state FROM events WHERE run_id=? ORDER BY sequence', ('entry',)).fetchmany(257)
        assert len(summaries) <= 1 and len(events) <= 256
        artifacts = c.execute('SELECT count(*) FROM artifacts WHERE run_id=?', ('entry',)).fetchone()[0]
        attempts = c.execute('SELECT attempt,status FROM run_attempts WHERE run_id=? ORDER BY attempt', ('entry',)).fetchmany(3)
    admission_db = root / 'admission/admission.sqlite3'
    with sqlite3.connect(admission_db.as_uri() + '?mode=ro', uri=True) as c:
        admission = dict(c.execute('SELECT state,count(*) FROM tickets GROUP BY state').fetchmany(4))
    patch = root / 'runtime/artifacts/entry.patch'
    return {'runtime_exists': True, 'summary': loads(summaries[0][0]) if summaries else None,
            'events': events, 'artifact_count': artifacts, 'attempts': attempts,
            'admission_counts': admission,
            'persisted_artifact_sha256': sha(patch.read_bytes()) if patch.is_file() else None,
            'source_files': {n: sha((root / 'candidate' / n).read_bytes())
                             for n in submission()['public_files']}}


def retain_probe(root, evidence):
    # Preserve before interpreting, including on a failed or interrupted case.
    for stream in ('stdout', 'stderr'):
        source = root / ('discovery/probe.' + stream)
        target = evidence / ('native-discovery.' + stream)
        if not source.exists() or target.exists():
            continue
        meta = source.lstat()
        assert stat.S_ISREG(meta.st_mode) and meta.st_nlink == 1 and meta.st_uid == os.getuid()
        assert meta.st_size <= 65536
        write_new(target, source.read_bytes())


def acceptance(report, source, evidence):
    # Evaluate only this already reviewed, authored fixture inside the cell.
    assert sha(EVALUATOR.read_bytes()) == sha((HERE / 'inputs/verify.py').read_bytes())
    spec = importlib.util.spec_from_file_location('t02v2', EVALUATOR)
    verifier = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(verifier)
    corrected = loads((HERE / 'inputs/tool-script.json').read_bytes())['steps'][2]['arguments']['content'].encode()
    output = report['output']
    assert output['name'] == 'lib.rs' and output['content'].encode() == corrected
    assert report['human_acceptance'] is None and report['source_unchanged'] is True
    patch = report['artifact']['patch'].encode()
    assert report['artifact']['sha256'] == sha(patch)
    checks = (HERE / 'inputs/cases.rs').read_bytes()
    names = [s.decode() for s in re.findall(rb'fn (t02_v2_[a-z0-9_]+)\(', checks)]
    assert len(names) == len(set(names)) == 18
    with tempfile.TemporaryDirectory(prefix='entry-acceptance-', dir='/task') as temporary:
        work = Path(temporary)
        env = verifier.environment(work)
        def run(args):
            return verifier.run(args, work, env).stdout
        run(['git', 'init', '-q'])
        (work / 'lib.rs').write_bytes(source)
        run(['git', 'add', '--', 'lib.rs'])
        tree = run(['git', 'write-tree']).decode().strip()
        (work / 'change.patch').write_bytes(patch)
        run(['git', 'apply', '--cached', '--check', 'change.patch'])
        run(['git', 'apply', '--cached', 'change.patch'])
        entries = run(['git', 'ls-files', '--stage']).decode().splitlines()
        assert len(entries) == 1 and entries[0].startswith('100644 ') and entries[0].endswith('\tlib.rs')
        assert run(['git', 'show', ':lib.rs']) == corrected
        assert run(['git', 'diff', '--cached', '--no-ext-diff', '--no-color', tree, '--']) == patch
        results = {name: verifier.evaluate_source(data, checks, names, work)
                   for name, data in [('source', source), ('candidate', corrected)]}
    assert results['source']['outcome'] == 'failed'
    assert results['candidate']['outcome'] == 'passed'
    save(evidence / 'public-acceptance.json', {
        'source_sha256': sha(source), 'candidate_sha256': sha(corrected),
        'artifact_sha256': sha(patch), 'cases_sha256': sha(checks),
        'verifier_sha256': sha((HERE / 'inputs/verify.py').read_bytes()),
        'results': results, 'human_acceptance': None, 'model_calls': 0,
        'not_a_benchmark_trial': True, 'runtime_verdict_modified': False})


def main(case):
    os.umask(0o077)
    assert os.getuid() == 65534
    assert Path('/sys/fs/cgroup/memory.max').read_text().strip() == '8589934592'
    assert Path('/sys/fs/cgroup/memory.swap.max').read_text().strip() == '0'
    assert case in ('positive', 'wrong_digest', 'eof_runtime', 'stale_session', 'stale_config', 'stale_source',
                    'disabled', 'wrong_scope', 'incompatible', 'existing_root')
    evidence = Path('/task/evidence') / case
    evidence.mkdir(mode=0o700)
    root = Path('/task/entries') / case
    config = loads((HERE / 'config-template.json').read_bytes())
    config['root'] = str(root)
    task = submission()
    if case == 'disabled':
        config['enabled'] = False
    if case == 'wrong_scope':
        task['task']['scope'] = ['statement.md']
    if case == 'incompatible':
        task['model'] = 'unapproved'
    if case == 'existing_root':
        root.mkdir(mode=0o700)
        write_new(root / 'sentinel', b'keep')
    config_path = evidence / 'config.json'
    save(config_path, config)
    save(evidence / 'config-preimage.json', config)
    raw = (json.dumps(task, separators=(',', ':')) + '\n').encode()
    write_new(evidence / 'submission.jsonl', raw)
    digest = sha(raw[:-1])
    env = {'HOME': '/task/home', 'TMPDIR': '/tmp', 'PATH': '/usr/bin:/bin',
           'LANG': 'C', 'LC_ALL': 'C', 'GIT_CONFIG_NOSYSTEM': '1',
           'GIT_CONFIG_GLOBAL': '/dev/null', 'GIT_TERMINAL_PROMPT': '0'}
    argv = ['/task/programs/agent-router', 'finite-entry', '--config', str(config_path), '--json']
    save(evidence / 'argv.json', {'argv': argv, 'environment': env})
    session = Session(argv, env, evidence)
    frames, phases, requests = [], [], []
    script_config = loads((HERE / 'inputs/tool-script.json').read_bytes())
    script = DeniedThenScript(script_config)
    failure = None
    failure_traceback = None
    code = None
    identity = None
    report = None
    try:
        session.send(task)
        for sequence in range(32):
            frame = session.line()
            if frame is None:
                break
            frames.append(frame)
            assert frame['schema_version'] == 1 and frame['sequence'] == sequence
            assert frame['submission_sha256'] == digest
            sid = frame['session_id']
            assert isinstance(sid, str) and re.fullmatch('[0-9a-f]{64}', sid)
            if identity is None:
                identity = sid
            assert sid == identity
            kind = frame['kind']
            if kind == 'entry.accepted':
                assert sequence == 0 and frame['worker_attempts'] == 0
                assert frame['task']['risk'] == 'medium'
                assert frame['model'] == task['model'] and frame['effort'] == task['effort']
            elif kind == 'entry.route':
                assert not requests and phases == ['diagnostic']
                selected = frame['receipt']['selected']['candidate']
                assert selected['model'] == task['model'] and selected['effort'] == task['effort']
            elif kind == 'entry.diagnostic_complete':
                assert phases == ['diagnostic'] and not requests and frame['worker_attempts'] == 0
            elif kind == 'entry.dispatch':
                assert case == 'positive' and phases == ['diagnostic', 'runtime'] and not requests
                assert frame['model'] == task['model'] and frame['effort'] == task['effort']
            elif kind == 'entry.approval_required':
                phase = frame['phase']
                assert phase == ('diagnostic' if not phases else 'runtime')
                assert not requests
                phases.append(phase)
                if case == 'eof_runtime' and phase == 'runtime':
                    session.child.stdin.close()
                    continue
                reply = {k: frame[k] for k in ('schema_version', 'session_id', 'submission_sha256', 'phase', 'approval_digest')}
                reply['kind'] = 'approve'
                if phase == 'runtime' and case == 'wrong_digest':
                    reply['approval_digest'] = '0' * 64
                if phase == 'runtime' and case == 'stale_session':
                    reply['session_id'] = '0' * 64
                if phase == 'runtime' and case == 'stale_config':
                    changed = copy.deepcopy(config)
                    changed['enabled'] = False
                    config_path.write_text(json.dumps(changed))
                    save(evidence / 'mutation.json', {'kind': 'disabled_before_approval',
                         'before_sha256': sha((evidence / 'config-preimage.json').read_bytes()),
                         'after_sha256': sha(config_path.read_bytes())})
                if phase == 'runtime' and case == 'stale_source':
                    tamper = b'public tamper control\n'
                    (root / 'candidate/lib.rs').write_bytes(tamper)
                    save(evidence / 'mutation.json', {'kind': 'public_source_changed_before_approval',
                         'before_sha256': task['public_files']['lib.rs']['sha256'],
                         'after_sha256': sha(tamper)})
                session.send(reply)
            elif kind == 'entry.request':
                assert phases == ['diagnostic', 'runtime']
                assert case == 'positive'
                assert frame['request_id'] == len(requests) + 1
                request = frame['request']
                requests.append(request)
                save(evidence / ('request-%02d.json' % len(requests)), request)
                response = script.respond(request).decode()
                write_new(evidence / ('response-%02d.sse' % len(requests)), response.encode())
                session.send({'schema_version': 1, 'kind': 'response', 'session_id': sid,
                              'submission_sha256': digest, 'request_id': frame['request_id'], 'sse': response})
            elif kind == 'entry.result':
                assert report is None
                report = frame['report']
            else:
                raise ValueError('unknown event')
        else:
            raise ValueError('frame limit')
        code = session.finish()
        readback = independent_readback(root)
        save(evidence / 'independent-readback.json', readback)
        retain_probe(root, evidence)
        probe = root / 'discovery/probe.stdout'
        if case in ('positive', 'wrong_digest', 'eof_runtime', 'stale_session', 'stale_config', 'stale_source'):
            observations = [loads(line) for line in probe.read_bytes().splitlines()]
            assert len(observations) == 3
            assert observations[0]['linux_finite_probe'] == 'v2'
            assert observations[0]['memory_bytes'] == 8589934592
            assert observations[0]['workspace_write'] is False
            assert observations[1] == {'finite_client_probe': 'v1', 'native_version': 'codex-cli 0.159.2'}
            assert observations[2] == {'finite_tools_probe': 'v1', 'nonce': observations[0]['nonce'],
                                       'direct_write_refused': True, 'calls': 6}
            save(evidence / 'native-discovery-observations.json', observations)
        if case == 'positive':
            assert code == 3 and report is not None
            assert [f['kind'] for f in frames] == [
                'entry.accepted', 'entry.approval_required', 'entry.diagnostic_complete',
                'entry.route', 'entry.approval_required', 'entry.dispatch',
                *(['entry.request'] * (len(script_config['steps']) + 2)), 'entry.result']
            assert len(requests) == len(script_config['steps']) + 2
            assert script.observed_tool_results == len(script_config['steps'])
            assert script.denials_seen == len(script_config['steps']) + 1
            assert readback['summary'] == report['summary']
            assert readback['summary']['state'] == 'verifying'
            assert readback['summary']['attempts'] == 1 and len(readback['attempts']) == 1
            assert report['runtime_error'] is False and report['broker_error'] is False
            assert report['cleanup'] == 'namespace_terminated'
            assert report['admission']['granted'] == 0 and report['admission']['released'] == 1
            assert readback['admission_counts'] == {'released': 1}
            assert readback['persisted_artifact_sha256'] == report['artifact']['sha256']
            for name, file in task['public_files'].items():
                assert readback['source_files'][name] == file['sha256']
            acceptance(report, task['public_files']['lib.rs']['content'].encode(), evidence)
        else:
            assert code == 2 and not requests and report is None
            expected_errors = {'disabled': 'finite_entry.disabled',
                               'wrong_scope': 'finite_entry.task',
                               'incompatible': 'finite_entry.selection',
                               'existing_root': 'finite_entry.fresh_required'}
            error = loads(bytes(session.buffers['stderr']))
            if case in expected_errors:
                assert not frames and error['error'] == expected_errors[case]
            else:
                assert error['error'] == 'finite_entry.refused'
                assert phases == ['diagnostic', 'runtime']
                assert [f['kind'] for f in frames] == [
                    'entry.accepted', 'entry.approval_required', 'entry.diagnostic_complete',
                    'entry.route', 'entry.approval_required']
                assert readback['summary']['state'] == 'awaiting_approval'
            if case in ('disabled', 'wrong_scope', 'incompatible'):
                assert not root.exists()
            if case == 'existing_root':
                assert (root / 'sentinel').read_bytes() == b'keep'
                assert len(list(root.iterdir())) == 1
            if case == 'stale_config':
                assert loads(config_path.read_bytes())['enabled'] is False
            if case == 'stale_source':
                assert (root / 'candidate/lib.rs').read_bytes() == b'public tamper control\n'
            if readback.get('summary'):
                # Diagnostics may have run, but no worker attempt can exist.
                assert readback['summary']['attempts'] == 0 and readback['attempts'] == []
    except Exception as error:
        failure = type(error).__name__ + ': ' + str(error)
        failure_traceback = traceback.format_exc()
    finally:
        session.close()
        retain_probe(root, evidence)
        if not (evidence / 'independent-readback.json').exists():
            save(evidence / 'independent-readback.json', independent_readback(root))
        save(evidence / 'frames.json', frames)
        save(evidence / 'parent-result.json', {'case': case, 'status': 'passed' if failure is None else 'failed',
             'error': failure, 'error_traceback': failure_traceback, 'exit_code': code,
             'child_exit_code_after_stop': session.child.returncode, 'session_id': identity,
             'phases': phases, 'worker_requests': len(requests),
             'observed_tool_results': script.observed_tool_results,
             'distinct_permission_denials_observed': 1 if script.denials_seen else 0,
             'permission_denial_history_appearances': script.denials_seen,
             'model_calls': 0, 'provider': 'authored_local_sse', 'not_a_benchmark_trial': True})
    if failure is not None:
        raise RuntimeError(failure)


if __name__ == '__main__':
    main(sys.argv[1])
