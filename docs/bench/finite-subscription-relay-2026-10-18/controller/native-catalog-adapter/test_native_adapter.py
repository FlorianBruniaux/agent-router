import copy
import json
import pathlib
import sys
import unittest

ROOT = pathlib.Path(__file__).resolve().parent
EXPORT = ROOT.parent / 'qualification-inputs'
sys.path.insert(0, str(EXPORT))
from native_tool_responses import ToolScript, project_native_output, validate_native_catalog
from tool_responses import ToolScript as FrozenToolScript


def request():
    return {'model': 'gpt-6-luna', 'reasoning': {'effort': 'medium'}, 'stream': True,
            'store': False, 'tools': json.loads((ROOT / 'expected-native-catalog.json').read_text()),
            'input': [{'role': 'user', 'content': 'R2-LOCAL-TOOLS-V1'}]}


def envelope(index, value):
    return {'type': 'function_call_output', 'call_id': f'call_r2_local_{index}',
            'output': [{'type': 'input_text', 'text': 'Wall time: 0.0010 seconds\nOutput:'},
                       {'type': 'input_text', 'text': json.dumps(value)}]}


class AdapterTests(unittest.TestCase):
    def test_exact_catalogue_and_frozen_refusal(self):
        r = request()
        before = copy.deepcopy(r)
        validate_native_catalog(r)
        self.assertEqual(r, before)
        step = {'tool': 'mcp__finite__read_file', 'arguments': {'path': 'lib.rs'}, 'expected': {}}
        with self.assertRaises(ValueError):
            FrozenToolScript([step]).respond(r)

    def test_catalogue_refusals(self):
        for name in ('extra', 'duplicate', 'missing', 'unknown_field', 'wrapper_schema',
                     'finite_schema', 'namespace', 'additional'):
            with self.subTest(name=name):
                r = request()
                if name == 'extra': r['tools'].append({'name': 'view_image', 'type': 'function'})
                if name == 'duplicate': r['tools'].append(copy.deepcopy(r['tools'][0]))
                if name == 'missing': r['tools'].pop(0)
                if name == 'unknown_field': r['tools'][0]['extra'] = True
                if name == 'wrapper_schema': r['tools'][0]['parameters']['additionalProperties'] = True
                finite = next(t for t in r['tools'] if t['name'] == 'mcp__finite')
                if name == 'finite_schema': finite['tools'][0]['parameters']['additionalProperties'] = True
                if name == 'namespace': finite['name'] = 'mcp__private'
                if name == 'additional': r['input'].append({'type': 'additional_tools', 'tools': [{}]})
                with self.assertRaises(ValueError): validate_native_catalog(r)

    def test_synthetic_read_envelope_without_mutation(self):
        witness = json.loads(pathlib.Path(str(ROOT.parent/'relay-controller-native-request.json')).read_text())
        item = next(i for i in witness['input'] if i.get('type') == 'function_call_output')
        before = copy.deepcopy(item)
        projected = project_native_output(item)
        self.assertEqual(item, before)
        value = json.loads(projected['output'])
        self.assertEqual(value['exit_code'], 0)
        self.assertIn('t02', value['stdout'])

    def test_envelope_refusals(self):
        value = {'stdout': 'actual', 'stderr': '', 'exit_code': 0, 'elapsed_ms': 0,
                 'truncated': False, 'termination': 'exited'}
        for name in ('approval_error', 'truncated', 'duplicate_part', 'missing_part', 'prefix',
                     'json_duplicate', 'nan', 'bool_exit', 'termination', 'extra_field'):
            with self.subTest(name=name):
                item = envelope(1, value)
                if name == 'approval_error': item['output'][1]['text'] = 'MCP tool call requires approval, but approval policy is never'
                if name == 'duplicate_part': item['output'].append(copy.deepcopy(item['output'][1]))
                if name == 'missing_part': item['output'].pop(0)
                if name == 'prefix': item['output'][0]['text'] += '\nError'
                if name == 'json_duplicate': item['output'][1]['text'] = item['output'][1]['text'][:-1] + ',"exit_code":0}'
                if name == 'nan': item['output'][1]['text'] = item['output'][1]['text'].replace('"elapsed_ms": 0', '"elapsed_ms": NaN')
                if name in ('truncated', 'bool_exit', 'termination', 'extra_field'):
                    v = dict(value)
                    if name == 'truncated': v['truncated'] = True
                    if name == 'bool_exit': v['exit_code'] = False
                    if name == 'termination': v['termination'] = 'timeout'
                    if name == 'extra_field': v['isError'] = True
                    item['output'][1]['text'] = json.dumps(v)
                with self.assertRaises(ValueError): project_native_output(item)

    def test_six_frozen_dialogues_and_wrong_values(self):
        for path in sorted(EXPORT.glob('*/controller/tool-script.json')):
            with self.subTest(packet=path.parent.parent.name):
                d = json.loads(path.read_text())
                s = ToolScript(d['steps'], model=d['model'], effort=d['effort'], marker=d['marker'])
                r = request()
                r['input'][0]['content'] = d['marker']
                for index, step in enumerate(d['steps'], 1):
                    before = copy.deepcopy(r)
                    s.respond(r)
                    self.assertEqual(r, before)
                    value = {'stdout': '', 'stderr': '', 'exit_code': 0, 'elapsed_ms': 0,
                             'truncated': False, 'termination': 'exited', **step['expected']}
                    if step.get('stdout_contains'): value['stdout'] = step['stdout_contains']
                    r['input'].append(envelope(index, value))
                s.respond(r)
                self.assertEqual(s.observed_tool_results, len(d['steps']))
        s = ToolScript([{'tool': 'mcp__finite__read_file', 'arguments': {'path': 'lib.rs'},
                         'expected': {'stdout': 'correct'}}])
        r = request()
        s.respond(r)
        r['input'].append(envelope(1, {'stdout': 'false', 'stderr': '', 'exit_code': 0,
                                     'elapsed_ms': 0, 'truncated': False, 'termination': 'exited'}))
        with self.assertRaisesRegex(ValueError, 'tool_result_mismatch'): s.respond(r)


if __name__ == '__main__':
    unittest.main()
