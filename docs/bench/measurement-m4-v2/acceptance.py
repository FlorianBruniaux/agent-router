#!/usr/bin/env python3
"""Run frozen criteria on a candidate checkout; never launches the pilot/host."""
import argparse
import contextlib
import importlib.util
import io
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


def module(path):
    spec = importlib.util.spec_from_file_location('candidate', path)
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


def report_case(repo, case):
    pilot = module(repo / 'docs/bench/campaign-next/pilot/run_pilot.py')
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        instance = pilot.Pilot.__new__(pilot.Pilot)  # Skip all initialization/host discovery.
        instance.results = root
        instance.runs_dir = root / 'runs'
        instance.pricing = {'version': 'synthetic'}
        instance._manifest = {'arms': ['E'], 'mode': 'fake', 'manifest_sha256': 'synthetic', 'host_config': 'isolated'}
        for name, cost, invalid in [('accepted', case['valid_cost'], []), ('invalid', case['invalid_cost'], ['infrastructure_failure'])]:
            row = {'provider': 'fake', 'arm': 'E', 'task': name, 'rep': 1,
                   'invalid': invalid, 'accepted': bool(case['accepted']) and not invalid,
                   'cost_list_microusd': None if cost is None else cost * 1000000,
                   'cost_product_microusd': None if cost is None else cost * 2000000,
                   'host_cost_usd': cost, 'time_to_acceptance_ms': 1,
                   'tokens': {k: 0 for k in ('input_tokens', 'cached_input_tokens', 'cache_write_input_tokens', 'output_tokens', 'reasoning_output_tokens')},
                   'phases': [], 'agent_verdict': 'PASS'}
            target = instance.runs_dir / name
            target.mkdir(parents=True)
            (target / 'result.json').write_text(json.dumps(row))
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
            instance.report()
        row = json.loads((root / 'report.json').read_text())['per_arm']['fake']['E']
        expected = None if case['invalid_cost'] is None else case['valid_cost'] + case['invalid_cost']
        ratio = expected if case['accepted'] else None
        return (row['cost_list_usd_total'] == expected
                and row['cost_product_usd_total'] == (None if expected is None else expected * 2)
                and row['cost_per_accepted_list_usd'] == ratio
                and row['cost_per_accepted_product_usd'] == (None if ratio is None else ratio * 2)
                and row['cost_unknown_runs'] == int(case['invalid_cost'] is None)
                and row['accepted'] == case['accepted'] and row['valid'] == 1 and row['invalid'] == 1), row


def cli_case(repo, kind):
    script = repo / 'docs/bench/calibration-v2/validate_manifest.py'
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        shutil.copytree(script.parent / 'fixtures/synthetic', root, dirs_exist_ok=True)
        path = root / 'manifest.json'
        if kind == 'invalid':
            path.write_text('{')
        elif kind == 'template':
            path.write_bytes((script.parent / 'manifest.template.json').read_bytes())
        plain = subprocess.run([sys.executable, str(script), str(path)], capture_output=True, text=True, timeout=15)
        for args in ([str(path), '--compact'], ['--compact', str(path)]):
            compact = subprocess.run([sys.executable, str(script), *args], capture_output=True, text=True, timeout=15)
            if (compact.stderr or compact.returncode != plain.returncode or len(compact.stdout.splitlines()) != 1
                    or not compact.stdout.endswith('\n') or json.loads(compact.stdout) != json.loads(plain.stdout)):
                return False
        return len(plain.stdout.splitlines()) > 1


def hash_case(repo, bad):
    script = repo / 'docs/bench/calibration-v2/validate_manifest.py'
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        shutil.copytree(script.parent / 'fixtures/synthetic', root, dirs_exist_ok=True)
        path = root / 'manifest.json'
        value = json.loads(path.read_text())
        value['input_hashes'] = bad
        path.write_text(json.dumps(value))
        result = subprocess.run([sys.executable, str(script), str(path)], capture_output=True, text=True, timeout=15)
        report = json.loads(result.stdout)
        return (result.returncode == 2 and not result.stderr and not report['structurally_valid']
                and not report['offline_preparation_ready'] and not report['executable_ready']
                and {'code': 'fields', 'field': 'input_hashes'} in report['errors']
                and not any(e['code'] == 'json' for e in report['errors']))


def check(repo, task, hidden=None):
    cases = json.loads(hidden.read_text()) if hidden else {'t1': [{'accepted': 1, 'valid_cost': 1, 'invalid_cost': 2}], 't2': ['synthetic'], 't3': [None]}
    if task == 't1':
        results = [report_case(repo, case)[0] for case in cases[task]]
    elif task == 't2':
        results = [cli_case(repo, case) for case in cases[task]]
    else:
        results = [hash_case(repo, case) for case in cases[task]]
    return {'task_id': task, 'passed': all(results), 'cases': len(results), 'passed_cases': sum(results),
            'scope': 'local acceptance only; not provider/runtime evidence'}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo', required=True, type=Path)
    parser.add_argument('--task', required=True, choices=['t1', 't2', 't3'])
    parser.add_argument('--hidden', type=Path)
    args = parser.parse_args()
    result = check(args.repo, args.task, args.hidden)
    print(json.dumps(result, sort_keys=True))
    raise SystemExit(0 if result['passed'] else 1)
