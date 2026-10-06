"""Offline tests only: no native Codex, real provider, or trust-state writes."""
import copy
import contextlib
import io
from types import SimpleNamespace
from unittest import mock
import importlib.util
import json
import os
import socket
import http.client
import tomllib
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

try:
    import failure_probe as p
except ImportError:
    p = None


def hook_input(case='block'):
    prompts = {'block': 'U07:BLOCK', 'exit1': 'U07:EXIT1', 'malformed': 'U07:MALFORMED',
        'timeout': 'U07:TIMEOUT', 'signal': 'U07:SIGNAL', 'exit2_reason': 'U07:EXIT2_REASON',
        'exit2_empty': 'U07:EXIT2_EMPTY', 'disabled': 'U07:DISABLED'}
    return {'cwd': '/private/tmp/u07-workspace', 'hook_event_name': 'UserPromptSubmit',
        'model': 'gpt-6.1-sol', 'permission_mode': 'default', 'prompt': prompts[case],
        'session_id': 'thread', 'transcript_path': None, 'turn_id': 'turn'}


def evidence(case='block', status='blocked', requests=0):
    run = {'id': '/private/tmp/private-hook:run', 'eventName': 'userPromptSubmit',
        'handlerType': 'command', 'executionMode': 'sync', 'status': 'running'}
    rpc = [
        {'id': 3, 'result': {'thread': {'id': 'thread'}}},
        {'id': 4, 'result': {'turn': {'id': 'turn', 'status': 'inProgress'}}},
        {'method': 'turn/started', 'params': {'threadId': 'thread', 'turn': {'id': 'turn', 'status': 'inProgress'}}},
        {'method': 'hook/started', 'params': {'threadId': 'thread', 'turnId': 'turn', 'run': run}},
        {'method': 'hook/completed', 'params': {'threadId': 'thread', 'turnId': 'turn', 'run': {**run, 'status': status}}},
        {'method': 'turn/completed', 'params': {'threadId': 'thread', 'turn': {'id': 'turn', 'status': 'failed'}}}]
    journal = [{'case': case, 'session_id': 'thread', 'turn_id': 'turn', 'event': event} for event in ('entered', 'action')]
    return {'rpc': rpc, 'journal': journal, 'http': [{'prompt_case': case, 'rejected': True}] * requests,
        'provider_request_count': requests, 'provider_accepted_connections': requests, 'provider_rejected_connections': 0,
        'trust_verified': True, 'output_complete': True, 'exit_code': 0,
        'owned_process_group_gone': True, 'hook_processes_gone': True, 'provider_stopped': True, 'case': case}


class FailureProbeTests(unittest.TestCase):
    def setUp(self):
        self.assertIsNotNone(p, 'failure hook diagnostic does not exist yet')

    def test_fixed_prompt_dispatch_rejects_unbound_or_malformed_inputs(self):
        self.assertEqual(p.parse_hook_input(json.dumps(hook_input()).encode())['case'], 'block')
        for field, value in [('session_id', ''), ('turn_id', ''), ('hook_event_name', 'Stop'),
                ('prompt', 'arbitrary command'), ('prompt', {'case': 'block'})]:
            value_in = hook_input(); value_in[field] = value
            with self.subTest(field=field), self.assertRaises(p.ProbeError):
                p.parse_hook_input(json.dumps(value_in).encode())
        for raw in [b'[]', b'{', b' ' * (65536 + 1)]:
            with self.assertRaises(p.ProbeError): p.parse_hook_input(raw)

    def test_hook_program_has_distinct_exit_json_stderr_signal_and_timeout_modes(self):
        with tempfile.TemporaryDirectory() as directory:
            journal = Path(directory) / 'journal.jsonl'
            for case, code, stdout, stderr in [
                ('block', 0, b'{"decision":"block","reason":"U07_BLOCK_CONTROL"}\n', b''),
                ('exit1', 1, b'', b'U07_EXIT1\n'),
                ('malformed', 0, b'{not-json\n', b''),
                ('exit2_reason', 2, b'', b'U07_EXIT2_BLOCK_REASON\n'),
                ('exit2_empty', 2, b'', b''),
                ('signal', -15, b'', b'')]:
                result = subprocess.run([sys.executable, '-B', str(Path(p.__file__)), 'hook', '--journal', str(journal)],
                    input=json.dumps(hook_input(case)).encode(), capture_output=True, timeout=5)
                self.assertEqual((result.returncode, result.stdout, result.stderr), (code, stdout, stderr), case)
            result = subprocess.run([sys.executable, '-B', str(Path(p.__file__)), 'hook', '--journal', str(journal)],
                input=json.dumps(hook_input('timeout')).encode(), capture_output=True, timeout=5)
            self.assertEqual(result.returncode, 0)
            self.assertEqual(result.stdout, b'{"decision":"block","reason":"U07_TIMEOUT_RETURNED_LATE"}\n')
            rows = [json.loads(line) for line in journal.read_text().splitlines()]
            self.assertEqual(len(rows), 15)
            self.assertEqual(rows[-1]['event'], 'late_return')
            self.assertTrue(all(row['session_id'] == 'thread' and row['turn_id'] == 'turn' for row in rows))

    def test_zero_parent_requests_requires_correlated_hook_and_terminal_parent(self):
        self.assertEqual(p.qualify(evidence())['verdict'], 'BLOCK_OBSERVED')
        for mutation in ['lost-hook', 'lost-turn', 'wrong-thread', 'wrong-run', 'duplicate-hook', 'wrong-journal', 'failed-hook']:
            value = evidence()
            if mutation == 'lost-hook': value['rpc'].pop(4)
            if mutation == 'lost-turn': value['rpc'].pop()
            if mutation == 'wrong-thread': value['rpc'][4]['params']['threadId'] = 'other'
            if mutation == 'wrong-run': value['rpc'][4]['params']['run']['id'] = 'other'
            if mutation == 'duplicate-hook': value['rpc'].append(copy.deepcopy(value['rpc'][4]))
            if mutation == 'wrong-journal': value['journal'][0]['turn_id'] = 'other'
            if mutation == 'failed-hook': value['rpc'][4]['params']['run']['status'] = 'failed'
            with self.subTest(mutation=mutation): self.assertEqual(p.qualify(value)['verdict'], 'UNKNOWN')

    def test_correlated_parent_http_after_failure_is_unsafe_even_if_it_is_rejected(self):
        result = p.qualify(evidence('exit1', 'failed', 2))
        self.assertEqual(result['verdict'], 'PARENT_FALLTHROUGH_OBSERVED')
        self.assertFalse(result['safe_failure_refusal'])
        self.assertEqual(result['parent_http_requests'], 2)
        self.assertNotIn('/private/tmp/', json.dumps(result))
        value = evidence('exit1', 'failed', 1); value['http'][0]['prompt_case'] = 'unrelated'
        self.assertEqual(p.qualify(value)['verdict'], 'UNKNOWN')

    def test_incomplete_cleanup_or_nonzero_exit_never_qualifies(self):
        for field, value in [('output_complete', False), ('provider_stopped', False),
                ('owned_process_group_gone', None), ('exit_code', 2), ('exit_code', False), ('trust_verified', False)]:
            changed = evidence(); changed[field] = value
            self.assertEqual(p.qualify(changed)['verdict'], 'UNKNOWN')

    def test_disabled_control_requires_no_hook_and_rejected_parent_contact(self):
        value = evidence('disabled', requests=2)
        value['rpc'] = [m for m in value['rpc'] if not m.get('method', '').startswith('hook/')]
        value['journal'] = []
        self.assertEqual(p.qualify(value)['verdict'], 'DISABLED_PARENT_CONTACT_OBSERVED')
        value['http'] = []; value['provider_request_count'] = 0; value['provider_accepted_connections'] = 0
        self.assertEqual(p.qualify(value)['verdict'], 'UNKNOWN')

    def test_native_trust_requires_exact_command_hash_enabled_state_and_native_written_hash(self):
        command = '/fixed/hook command'; digest = 'sha256:' + 'a' * 64
        listed = {'data': [{'errors': [], 'warnings': [], 'hooks': [{'eventName': 'userPromptSubmit',
            'handlerType': 'command', 'command': command, 'async': False, 'timeoutSec': 1,
            'key': 'fixed-key', 'currentHash': digest, 'trustStatus': 'trusted', 'enabled': True}]}]}
        state = {'fixed-key': {'trusted_hash': digest}}
        self.assertEqual(p.require_trust(listed, state, command, enabled=True), digest)
        for field, value in [('command', 'different'), ('currentHash', 'sha256:' + 'b' * 64),
                ('enabled', False), ('trustStatus', 'untrusted'), ('async', True)]:
            changed = copy.deepcopy(listed); changed['data'][0]['hooks'][0][field] = value
            with self.assertRaises(p.ProbeError): p.require_trust(changed, state, command, enabled=True)
        with self.assertRaises(p.ProbeError): p.require_trust(listed, {}, command, enabled=True)


    def test_config_allows_only_exact_native_ui_changes(self):
        baseline = {'model': 'gpt-6.1-sol', 'hooks': {'UserPromptSubmit': [{'hooks': [{'command': 'fixed'}]}]}}
        config = copy.deepcopy(baseline)
        key = '/private/tmp/example/home/config.toml:user_prompt_submit:0:0'
        config['hooks']['state'] = {key: {'trusted_hash': 'sha256:' + 'a' * 64, 'enabled': True}}
        config['projects'] = {'/private/tmp/example/workspace': {'trust_level': 'trusted'}}
        config['tui'] = {'screen_reader_detection_done': True, 'model_availability_nux': {'gpt-6.1-sol': 2}}
        self.assertEqual(p.validate_config(baseline, config, key, '/private/tmp/example/workspace', True), config['hooks']['state'])
        for field, value in [('model', 'other'), ('mcp_servers', {'evil': {}}), ('tui', {'unknown': True})]:
            changed = copy.deepcopy(config); changed[field] = value
            with self.assertRaises(p.ProbeError): p.validate_config(baseline, changed, key, '/private/tmp/example/workspace', True)
        with self.assertRaises(p.ProbeError): p.validate_config(baseline, config, key, '/private/tmp/example/workspace', False)

    def test_timeout_late_block_is_not_safe_failure_refusal(self):
        for mode in ('timeout', 'exit2_reason'):
            self.assertFalse(p.qualify(evidence(mode))['safe_failure_refusal'])

    def test_additional_correlation_counter_and_journal_mutations(self):
        mutations = [
            lambda e: e['rpc'][2]['params']['turn'].update(id='wrong'),
            lambda e: e['rpc'][-1]['params']['turn'].update(id='wrong'),
            lambda e: e['rpc'].append(e['rpc'][0]),
            lambda e: e['rpc'].pop(1),
            lambda e: e['journal'].append(e['journal'][0]),
            lambda e: e['journal'][1].update(case='other'),
            lambda e: e['rpc'][4]['params']['run'].update(executionMode='async'),
            lambda e: e.update(provider_accepted_connections=1),
            lambda e: e.update(provider_request_count=1),
            lambda e: e.update(provider_rejected_connections=1),
            lambda e: e.pop('provider_rejected_connections'),
            lambda e: [e['rpc'][i]['params']['run'].update(id='') for i in (3, 4)],
            lambda e: e['rpc'].insert(5, e['rpc'].pop(2)),
            lambda e: e['rpc'].append(e['rpc'].pop(4))]
        for mutate in mutations:
            value = evidence(); mutate(value)
            self.assertEqual(p.qualify(value)['verdict'], 'UNKNOWN')

    def test_rejecting_provider_records_http_and_cleans_before_rebind(self):
        repo = Path(os.environ['U07_REPO_ROOT'])
        h, _ = p.helpers(repo)
        provider, listener = p.start_provider(h, 0)
        port = provider.server_address[1]
        try:
            body = json.dumps({'input': [{'role': 'user', 'content': [{'type': 'input_text', 'text': 'U07:EXIT1'}]}]})
            connection = http.client.HTTPConnection('127.0.0.1', port, timeout=2)
            connection.request('POST', '/v1/responses', body, {'Content-Type': 'application/json'})
            response = connection.getresponse()
            self.assertEqual(response.status, 503)
            self.assertIn(b'U07_FIXED_REFUSAL', response.read()); connection.close()
            self.assertEqual(provider.projections, [{'prompt_case': 'exit1', 'rejected': True}])
            with self.assertRaises(OSError): p.start_provider(h, port)
        finally:
            self.assertTrue(p.stop_provider(h, provider, listener))
        provider, listener = p.start_provider(h, port)
        self.assertTrue(p.stop_provider(h, provider, listener))

    def test_prepare_never_writes_trust_and_pins_exact_script(self):
        h, _ = p.helpers(Path(os.environ['U07_REPO_ROOT']))
        with tempfile.TemporaryDirectory(dir='/private/tmp') as directory:
            state = Path(directory) / 'session'
            args = SimpleNamespace(state=state, repo_root=Path(os.environ['U07_REPO_ROOT']), binary=Path(sys.executable))
            provider = SimpleNamespace(server_address=('127.0.0.1', 12345), accepted_connections=0, request_count=0)
            with mock.patch.object(p, 'start_provider', return_value=(provider, None)), \
                    mock.patch.object(p, 'stop_provider', return_value=True), \
                    mock.patch.object(p.threading.Event, 'wait', return_value=True), contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(p.prepare(args, h), 0)
            config = tomllib.loads((state / 'home/config.toml').read_text())
            self.assertNotIn('state', config['hooks'])
            self.assertEqual((state / 'hook.py').read_bytes(), Path(p.__file__).read_bytes())
            launch = json.loads((state / 'ui-launch.json').read_text())
            self.assertEqual(launch['env']['HOME'], str(state / 'home'))
            self.assertNotIn('ANTHROPIC_API_KEY', launch['env'])
            self.assertTrue(json.loads((state / 'prepare-cleanup.json').read_text())['provider_stopped'])

    def test_scripted_appserver_roundtrip_cleanup_and_exit2_refusal(self):
        h, prior = p.helpers(Path(os.environ['U07_REPO_ROOT']))
        for exit_code, drift, malformed in ((0, False, False), (2, False, False), (0, True, False), (0, False, True)):
            with self.subTest(exit_code=exit_code, drift=drift, malformed=malformed), tempfile.TemporaryDirectory(dir='/private/tmp') as directory:
                state = Path(directory); (state / 'home').mkdir(); (state / 'workspace').mkdir()
                (state / 'home/config.toml').write_bytes(b'fixture')
                binary = state / 'scripted-appserver'
                binary.write_text('#!' + sys.executable + '\n' + '''import json,sys,os
from pathlib import Path
state=Path(os.environ['HOME']).parent
key=str(state/'home/config.toml')+':user_prompt_submit:0:0'
def send(m): print(json.dumps(m),flush=True)
for line in sys.stdin:
 m=json.loads(line); i=m.get('id')
 if i==1: send({'id':i,'result':{}})
 if i==2:
  hook={'eventName':'userPromptSubmit','handlerType':'command','command':'fixed','async':False,'timeoutSec':1,'key':key,'currentHash':'sha256:'+'a'*64,'trustStatus':'trusted','enabled':True,'source':'user','sourcePath':str(state/'home/config.toml'),'matcher':None,'pluginId':None,'isManaged':False}
  send({'id':i,'result':{'data':[{'errors':[],'warnings':[],'hooks':[hook]}]}})
 if i==3: send({'id':i,'result':{'thread':{'id':'thread'}}})
 if i==4:
  assert m['params']['input']==[{'type':'text','text':'U07:BLOCK'}]
  send({'id':i,'result':{'turn':{'id':'turn','status':'inProgress'}}})
  send({'method':'turn/started','params':{'threadId':'thread','turn':{'id':'turn','status':'inProgress'}}})
  run={'id':'private-hook-run','eventName':'userPromptSubmit','handlerType':'command','executionMode':'sync','status':'running'}
  send({'method':'hook/started','params':{'threadId':'thread','turnId':'turn','run':run}})
  with (state/'hook.jsonl').open('a') as journal:
   for e in ('entered','action'): journal.write(json.dumps({'event':e,'case':'block','session_id':'thread','turn_id':'turn','pid':os.getpid(),'pgid':os.getpgrp()})+'\\n')
  run['status']='blocked'
  send({'method':'hook/completed','params':{'threadId':'thread','turnId':'turn','run':run}})
  send({'method':'turn/completed','params':{'threadId':'thread','turn':{'id':'turn','status':'failed'}}})
''' + '\nsys.exit(' + str(exit_code) + ')\n')
                if malformed:
                    binary.write_text(binary.read_text().replace("  send({'method':'turn/completed'", "  print('{invalid-rpc',flush=True)\n  send({'method':'turn/completed'"))
                binary.chmod(0o700)
                args = SimpleNamespace(case='block', state=state, binary=binary)
                key = str(state / 'home/config.toml') + ':user_prompt_submit:0:0'
                info = {'port': 12345, 'command': 'fixed', 'pins': {str(state / 'hook.py'): 'a'*64}}
                states = {key: {'trusted_hash': 'sha256:' + 'a'*64, 'enabled': True}}
                provider = SimpleNamespace(projections=[], request_count=0, accepted_connections=0, rejected_connections=0)
                images = (info, states, key, b'fixture')
                with mock.patch.object(p, 'check_images', side_effect=[images, p.ProbeError('postrun source drift') if drift else images]), \
                        mock.patch.object(p, 'start_provider', return_value=(provider, None)), \
                        mock.patch.object(p, 'stop_provider', return_value=True), contextlib.redirect_stdout(io.StringIO()):
                    code = p.run_case(args, h, prior)
                report = json.loads((state / 'case-block/report.json').read_text())
                self.assertEqual(code, 0 if exit_code == 0 and not drift and not malformed else 2)
                self.assertEqual(report['verdict'], 'BLOCK_OBSERVED' if exit_code == 0 and not drift and not malformed else 'UNKNOWN')
                self.assertFalse(report['u07_complete'])
                private = json.loads((state / 'case-block/private-evidence.json').read_text())
                self.assertTrue(private['owned_process_group_gone']); self.assertTrue(private['output_complete'])


    def test_initial_native_trust_may_omit_enabled_but_disable_must_be_explicit(self):
        baseline = {'hooks': {'UserPromptSubmit': []}}
        config = {'hooks': {'UserPromptSubmit': [], 'state': {'key': {'trusted_hash': 'sha256:' + 'a'*64}}},
            'projects': {'workspace': {'trust_level': 'trusted'}}}
        self.assertEqual(p.validate_config(baseline, config, 'key', 'workspace', True), config['hooks']['state'])
        with self.assertRaises(p.ProbeError): p.validate_config(baseline, config, 'key', 'workspace', False)

    def test_temp_boundary_rejects_traversal_and_symlink_escape(self):
        with self.assertRaises(p.ProbeError): p.private_state(Path('/private/tmp/../../outside'))
        with tempfile.TemporaryDirectory(dir='/private/tmp') as directory:
            link = Path(directory) / 'link'; link.symlink_to('/Users')
            with self.assertRaises(p.ProbeError): p.private_state(link / 'escaped')
            self.assertEqual(p.private_state(Path(directory) / 'new'), Path(directory) / 'new')

    def test_fatal_protocol_loss_never_qualifies_after_cleanup(self):
        value = evidence(); value['failure'] = 'FixtureError'
        self.assertEqual(p.qualify(value)['verdict'], 'UNKNOWN')

    def test_missing_hook_process_cleanup_is_unknown(self):
        value = evidence(); value['hook_processes_gone'] = False
        self.assertEqual(p.qualify(value)['verdict'], 'UNKNOWN')



if __name__ == '__main__':
    unittest.main()
