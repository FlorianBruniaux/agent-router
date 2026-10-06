"""Offline behavior tests: native Codex and live providers are never launched."""
import copy
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest

try:
    import probe as p
except ImportError:
    p = None


def route(model='gpt-5.6-terra', effort='medium'):
    return {'schema_version': '1', 'decision_id': 'a' * 64,
        'catalog_version': 'fixture', 'policy_version': 'stable', 'decision': {
        'selected': {'plan': {'preset': 'plan-execute-verify', 'stages': [
            {'kind': 'plan', 'model': 'gpt-5.6-sol', 'effort': 'high', 'skills': []},
            {'kind': 'execute', 'model': model, 'effort': effort, 'skills': ['tdd']},
            {'kind': 'verify', 'model': 'gpt-5.6-terra', 'effort': 'high', 'skills': []}],
            'utility_micros': 1}, 'alternatives': [], 'exclusions': [], 'missing_evidence': []}}}


def messages():
    return [
        {'id': 1, 'result': {}},
        {'id': 2, 'result': {'model': 'gpt-6.1-sol', 'reasoningEffort': 'low',
            'modelProvider': 'local_fixture', 'thread': {'id': 'thread'}}},
        {'id': 3, 'result': {'turn': {'id': 'turn', 'status': 'inProgress'}}},
        {'method': 'turn/started', 'params': {'threadId': 'thread', 'turn': {'id': 'turn', 'status': 'inProgress'}}},
        {'method': 'item/started', 'params': {'threadId': 'thread', 'turnId': 'turn',
            'item': {'id': 'msg_u06', 'type': 'agentMessage', 'text': ''}}},
        {'method': 'item/completed', 'params': {'threadId': 'thread', 'turnId': 'turn',
            'item': {'id': 'msg_u06', 'type': 'agentMessage', 'text': 'U06_NATIVE_ROUTE_APPLIED'}}},
        {'method': 'turn/completed', 'params': {'threadId': 'thread',
            'turn': {'id': 'turn', 'status': 'completed', 'error': None}}}]


class ProbeTests(unittest.TestCase):
    def setUp(self):
        self.assertIsNotNone(p, 'U06 route consumer and evidence verifier do not exist yet')

    def test_selected_execute_only_and_changed_route_pair_propagates(self):
        first = p.extract_execute(json.dumps(route()).encode())
        self.assertEqual((first['model'], first['effort']), ('gpt-5.6-terra', 'medium'))
        changed = p.extract_execute(json.dumps(route('gpt-5.6-luna', 'high')).encode())
        self.assertEqual((changed['model'], changed['effort']), ('gpt-5.6-luna', 'high'))
        self.assertEqual(first['not_executed_stages'], ['plan', 'verify'])

    def test_no_route_ambiguous_missing_duplicate_and_unsupported_stop(self):
        variants = []
        value = route(); value['decision'] = {'no_route': {'reasons': []}}; variants.append(value)
        value = route(); value['decision']['no_route'] = {}; variants.append(value)
        value = route(); value['schema_version'] = 1; variants.append(value)
        value = route(); value['decision_id'] = ''; variants.append(value)
        value = route(); value['decision']['selected']['plan']['stages'].pop(1); variants.append(value)
        value = route(); value['decision']['selected']['plan']['stages'].append(copy.deepcopy(value['decision']['selected']['plan']['stages'][1])); variants.append(value)
        variants += [route('', 'medium'), route('unsupported', 'medium'), route(effort='ultra'), route(effort={})]
        for value in variants:
            with self.subTest(value=value), self.assertRaises(p.FixtureError):
                p.extract_execute(json.dumps(value).encode())
        for raw in [b'{', b'[]', b'{"schema_version":"1","schema_version":"1"}', b' ' * (1024 * 1024 + 1)]:
            with self.assertRaises(p.FixtureError): p.extract_execute(raw)

    def test_one_decision_can_dispatch_once_even_after_failure(self):
        gate = p.DispatchGate(p.extract_execute(json.dumps(route()).encode()))
        first = gate.consume()
        self.assertEqual(first['model'], 'gpt-5.6-terra')
        with self.assertRaises(p.FixtureError): gate.consume()

    def test_provider_accepts_exact_pair_and_correlated_prompt_only(self):
        selection = p.extract_execute(json.dumps(route()).encode())
        gate = p.ProviderGate(selection['model'], selection['effort'], 'U06 decision a')
        request = {'model': 'gpt-5.6-terra', 'reasoning': {'effort': 'medium'},
            'input': [{'role': 'user', 'content': [{'type': 'input_text', 'text': 'U06 decision a'}]}]}
        self.assertTrue(gate.accept(request))
        self.assertFalse(gate.accept(request))
        self.assertEqual(gate.accepted, 1)
        for field, value in [('model', 'gpt-6.1-sol'), ('reasoning', {'effort': 'high'}), ('input', [])]:
            wrong = copy.deepcopy(request); wrong[field] = value
            self.assertFalse(p.ProviderGate('gpt-5.6-terra', 'medium', 'U06 decision a').accept(wrong))
        wrong = copy.deepcopy(request); wrong['input'][0]['content'][0]['text'] += ' different'
        self.assertFalse(p.ProviderGate('gpt-5.6-terra', 'medium', 'U06 decision a').accept(wrong))

    def test_duplicate_prompt_and_unsolicited_rpc_response_are_not_evidence(self):
        user = {'role': 'user', 'content': [{'type': 'input_text', 'text': 'expected'}]}
        request = {'model': 'gpt-5.6-terra', 'reasoning': {'effort': 'medium'}, 'input': [user, user]}
        self.assertFalse(p.ProviderGate('gpt-5.6-terra', 'medium', 'expected').accept(request))
        self.assertFalse(p.verify_protocol(messages() + [{'id': 87, 'result': {}}]))

    def test_correlated_protocol_passes_and_wrong_identity_never_does(self):
        self.assertTrue(p.verify_protocol(messages()))
        for index, field in [(3, 'threadId'), (4, 'turnId'), (5, 'threadId'), (6, 'threadId')]:
            changed = messages(); changed[index]['params'][field] = 'unrelated'
            self.assertFalse(p.verify_protocol(changed))
        for index in [3, 6]:
            changed = messages(); changed[index]['params']['turn']['id'] = 'unrelated'
            self.assertFalse(p.verify_protocol(changed))
        changed = messages(); changed[5]['params']['item']['id'] = 'different-item'
        self.assertFalse(p.verify_protocol(changed))

    def test_failed_or_missing_output_duplicate_completion_and_callback_refuse(self):
        for change in ['status', 'text', 'missing', 'duplicate', 'callback', 'error', 'duplicate-response']:
            case = messages()
            if change == 'status': case[-1]['params']['turn']['status'] = 'failed'
            if change == 'text': case[-2]['params']['item']['text'] += ' extra'
            if change == 'missing': case.pop(-2)
            if change == 'duplicate': case.append(copy.deepcopy(case[-1]))
            if change == 'callback': case.append({'id': 9, 'method': 'item/tool/call', 'params': {}})
            if change == 'error': case.append({'method': 'error', 'params': {'message': 'bad'}})
            if change == 'duplicate-response': case.append(copy.deepcopy(case[2]))
            with self.subTest(change=change): self.assertFalse(p.verify_protocol(case))

    def test_native_start_refusal_cannot_be_fabricated_as_completion(self):
        case = messages(); case[2] = {'id': 3, 'error': {'code': -32602, 'message': 'unsupported'}}
        self.assertFalse(p.verify_protocol(case))

    def test_case_requires_cleanup_and_provider_evidence_not_success_boolean(self):
        case = {'rpc': messages(), 'failure': None, 'provider_requests': [
            {'model': 'gpt-5.6-terra', 'effort': 'medium', 'prompt_exact': True}],
            'provider_request_count': 1, 'provider_accepted_connections': 1,
            'provider_rejected_requests': 0, 'provider_rejected_connections': 0,
            'callback_count': 0, 'cleanup_complete': True, 'temporary_state_removed': True,
            'owned_process_group_gone': True, 'output_complete': True, 'process_exit_code': 0, 'success': True}
        self.assertTrue(p.case_verdict(case, 'gpt-5.6-terra', 'medium'))
        for field in ['cleanup_complete', 'temporary_state_removed', 'owned_process_group_gone', 'output_complete']:
            changed = copy.deepcopy(case); changed[field] = None
            self.assertFalse(p.case_verdict(changed, 'gpt-5.6-terra', 'medium'))
        changed = copy.deepcopy(case); changed['provider_requests'][0]['effort'] = 'high'
        self.assertFalse(p.case_verdict(changed, 'gpt-5.6-terra', 'medium'))
        changed = copy.deepcopy(case); changed['provider_request_count'] = 2
        self.assertFalse(p.case_verdict(changed, 'gpt-5.6-terra', 'medium'))
        for exit_code in [None, False, -9, 2]:
            changed = copy.deepcopy(case); changed['process_exit_code'] = exit_code
            self.assertFalse(p.case_verdict(changed, 'gpt-5.6-terra', 'medium'))
        changed = copy.deepcopy(case); del changed['process_exit_code']
        self.assertFalse(p.case_verdict(changed, 'gpt-5.6-terra', 'medium'))

    def test_bounded_command_enforces_exit_timeout_and_both_stream_caps(self):
        with tempfile.TemporaryDirectory() as directory:
            cwd = Path(directory)
            env = {'PATH': '/usr/bin:/bin'}
            result = p.bounded_command([sys.executable, '-c', 'print("ok")'], cwd, env, timeout=2, cap=64)
            self.assertEqual(result, b'ok\n')
            for code, timeout in [('raise SystemExit(2)', 2), ('import time; time.sleep(5)', .05),
                ('print("a"*10000)', 2), ('import sys; sys.stderr.write("a"*10000)', 2)]:
                with self.subTest(code=code), self.assertRaises(p.FixtureError):
                    p.bounded_command([sys.executable, '-c', code], cwd, env, timeout=timeout, cap=64)

    def test_sse_uses_requested_model_and_fixed_output_without_tools(self):
        frames = [json.loads(frame.split('data: ', 1)[1]) for frame in
            p.final_sse('gpt-5.6-terra').decode().strip().split('\n\n')]
        response = frames[-1]['response']
        self.assertEqual(response['model'], 'gpt-5.6-terra')
        self.assertEqual(response['output'][0]['content'][0]['text'], 'U06_NATIVE_ROUTE_APPLIED')
        self.assertEqual(response['output'][0]['type'], 'message')

    def test_loading_frozen_helper_never_creates_bytecode_beside_it(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'helper.py'; path.write_text('value = 42\n')
            previous, prefix = sys.dont_write_bytecode, sys.pycache_prefix
            try:
                sys.dont_write_bytecode = False
                sys.pycache_prefix = None
                self.assertEqual(p.load_module('u06_bytecode_test', path).value, 42)
            finally:
                sys.dont_write_bytecode = previous
                sys.pycache_prefix = prefix
            self.assertFalse((path.parent / '__pycache__').exists())

    def test_real_loopback_orchestration_with_scripted_process_and_refusal(self):
        repo = Path(os.environ['AR_U06_REPO_ROOT'])
        h = p.load_module('u06_test_egress', repo / 'docs/bench/egress-admission/codex_appserver_local.py')
        prior = p.load_module('u06_test_prior', repo / 'docs/bench/native-integration-u02/probe_v5.py')
        # A real child process speaks RPC and contacts the actual loopback handler.
        # No Codex binary, inference provider, credentials or global config is used.
        script = '''import json,os,pathlib,re,sys,urllib.request
MODE = __MODE__
def send(value): print(json.dumps(value),flush=True)
for line in sys.stdin:
 m=json.loads(line); method=m.get('method'); params=m.get('params',{})
 if method=='initialize': send({'id':1,'result':{}})
 elif method=='thread/start': send({'id':2,'result':{'model':'gpt-6.1-sol','reasoningEffort':'low','modelProvider':'local_fixture','thread':{'id':'thread'}}})
 elif method=='turn/start':
  if MODE=='refused':
   send({'id':3,'error':{'code':-32602,'message':'unsupported'}});continue
  config=(pathlib.Path(os.environ['HOME'])/'config.toml').read_text()
  url=re.search(r'base_url = "([^"]+)"',config).group(1)+'/responses'
  body={'model':params.get('model','gpt-6.1-sol'),'reasoning':{'effort':params.get('effort','low')},'input':[{'role':'user','content':[{'type':'input_text','text':params['input'][0]['text']}]}]}
  if MODE=='wrong-model': body['model']='unexpected'
  try:
   with urllib.request.urlopen(urllib.request.Request(url,json.dumps(body).encode(),{'Content-Type':'application/json'}),timeout=2) as response: response.read(65536)
  except Exception: pass
  events=__MESSAGES__
  if MODE=='wrong-correlation': events[-2]['params']['threadId']='unrelated'
  if MODE=='duplicate-completion': events.append(events[-1])
  for event in events[2:]: send(event)
  if MODE=='nonzero-exit': raise SystemExit(2)
'''
        with tempfile.TemporaryDirectory(prefix='ar-u06-unit-') as directory:
            root = Path(directory)
            for mode, success in [('success', True), ('disabled', True), ('refused', False),
                    ('wrong-model', False), ('wrong-correlation', False), ('duplicate-completion', False), ('nonzero-exit', False)]:
                with self.subTest(mode=mode):
                    binary = root / ('fake-' + mode)
                    binary.write_text('#!' + sys.executable + '\n' + script.replace('__MODE__', repr(mode)).replace('__MESSAGES__', repr(messages())))
                    binary.chmod(0o700)
                    raw = root / mode; raw.mkdir(mode=0o700)
                    pair = ('gpt-6.1-sol', 'low') if mode == 'disabled' else ('gpt-5.6-terra', 'medium')
                    case = p.run_case(h, prior, binary, *pair, 'fixture-prompt', mode != 'disabled', raw)
                    self.assertEqual(case['success'], success, case)
                    self.assertTrue(case['cleanup_complete'], case)
                    self.assertTrue(case['owned_process_group_gone'], case)
                    if mode == 'refused':
                        self.assertEqual(case['attempt_outcome'], 'refused')
                        self.assertEqual(case['provider_request_count'], 0)
                    if mode == 'wrong-model':
                        self.assertEqual(case['provider_rejected_requests'], 1)


if __name__ == '__main__':
    unittest.main()
