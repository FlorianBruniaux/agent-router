"""Authored local responses. No provider, auth, inference or catalogue adoption."""
import hashlib
import json
import re
from pathlib import Path


def compact(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':'), ensure_ascii=False)


def canonical_catalog(tools):
    if not isinstance(tools, list) or len(tools) != 4:
        raise ValueError('catalog.shape')
    return compact(sorted(tools, key=lambda tool: tool['name'])).encode('utf-8')


def unique(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError('result.duplicate')
        result[key] = value
    return result


class ContextScript:
    def __init__(self, fixtures, expected_catalog_sha256):
        self.root = Path(fixtures)
        self.script = json.loads((self.root / 'script.json').read_text())
        self.expected = expected_catalog_sha256
        self.results = {}
        self.next = 0
        self.raw = '\x1b[31m' * 80 + 'RED é\r\n' + '\x1b[0m'
        self.handle = None

    def respond(self, request):
        result = {'refused': True, 'tool_results': len(self.results)}
        try:
            wire = canonical_catalog(request['tools'])
            result.update(catalog_digest=hashlib.sha256(wire).hexdigest(),
                          catalog_json=wire.decode('utf-8'))
            if (not re.fullmatch('[0-9a-f]{64}', self.expected)
                    or result['catalog_digest'] != self.expected):
                return result
            namespace = [t for t in request['tools'] if t['name'] == 'mcp__finite']
            if (len(namespace) != 1 or namespace[0].get('type') != 'namespace'
                    or sorted(t['name'] for t in namespace[0]['tools']) !=
                    ['exec', 'read_file', 'read_output', 'write_file']
                    or sorted(t['name'] for t in request['tools'] if t['name'] != 'mcp__finite') !=
                    ['list_mcp_resource_templates', 'list_mcp_resources', 'read_mcp_resource']
                    or request['model'] != self.script['model']
                    or self.next > len(self.script['steps'])):
                return result
            inputs = request['input']
            if not isinstance(inputs, list) or any(i.get('type') == 'additional_tools' and i.get('tools') for i in inputs):
                return result
            if self.next == 0 and self.script['marker'] not in compact(inputs):
                return result
            added = 0
            for item in inputs:
                if item.get('type') != 'function_call_output':
                    continue
                call_id = item['call_id']
                match = re.fullmatch(r'call_context_([0-4])', call_id)
                if not match:
                    return result
                index = int(match[1])
                value = self.decode(item['output'])
                if index in self.results:
                    if value != self.results[index]:
                        return result
                    continue
                if index != self.next - 1:
                    return result
                self.validate(index, value)
                self.results[index] = value
                added += 1
            if added != (0 if self.next == 0 else 1):
                return result
            result.update(refused=False, tool_results=len(self.results),
                          sse=self.response(self.next).decode('utf-8'))
            self.next += 1
            return result
        except (AssertionError, ValueError, TypeError, KeyError, IndexError, RecursionError):
            return result

    def decode(self, output):
        if (not isinstance(output, list) or len(output) != 2
                or any(set(part) != {'type', 'text'} or part['type'] != 'input_text'
                       or not isinstance(part['text'], str) for part in output)
                or not re.fullmatch(r'Wall time: [0-9]{1,6}\.[0-9]{4} seconds\nOutput:', output[0]['text'])
                or len(output[1]['text'].encode('utf-8')) > 32768):
            raise ValueError('result.envelope')
        value = json.loads(output[1]['text'], object_pairs_hook=unique,
                           parse_constant=lambda _: (_ for _ in ()).throw(ValueError('result.constant')))
        if (not isinstance(value, dict) or type(value.get('exit_code')) is not int
                or type(value.get('elapsed_ms')) is not int
                or not 0 <= value['elapsed_ms'] <= 60000
                or value.get('truncated') is not False or value.get('termination') != 'exited'
                or not isinstance(value.get('stdout'), str) or not isinstance(value.get('stderr'), str)):
            raise ValueError('result.value')
        return value

    def validate(self, index, value):
        base = {'stdout', 'stderr', 'exit_code', 'termination', 'truncated', 'elapsed_ms'}
        wanted_keys = base | ({'window'} if index == 0 else {'view'} if index == 1 else set())
        if set(value) != wanted_keys:
            raise ValueError('result.fields')
        if index == 0:
            expected = {'source_sha256': hashlib.sha256((self.root / 'source.txt').read_bytes()).hexdigest(),
                        'first_line': 2, 'returned_lines': 1, 'total_lines': 3, 'has_more': True}
            assert value['stdout'] == 'é\r\n' and value['window'] == expected
        elif index == 1:
            assert value['stdout'] == 'RED é\r\n' and value['stderr'] == 'detail\n' and value['exit_code'] == 1
            view = value['view']
            assert set(view) == {'method', 'raw_handle', 'raw_content_bytes', 'shown_content_bytes'}
            assert view['method'] == 'terminal_sgr_v1' and re.fullmatch('[0-9a-f]{64}', view['raw_handle'])
            assert view['raw_content_bytes'] == len(self.raw.encode('utf-8')) + 7
            assert view['shown_content_bytes'] == len('RED é\r\n'.encode('utf-8')) + 7
            self.handle = view['raw_handle']
        elif index == 2:
            assert value['stdout'] == self.raw and value['stderr'] == 'detail\n' and value['exit_code'] == 1
            assert value['elapsed_ms'] == self.results[1]['elapsed_ms']
        elif index == 3:
            assert value['stdout'] == '1'
        else:
            assert value['stdout'] == ''
        if index not in (1, 2):
            assert value['stderr'] == '' and value['exit_code'] == 0

    def response(self, index):
        if index < len(self.script['steps']):
            step = self.script['steps'][index]
            args = dict(step['arguments'])
            if step['name'] == 'read_output':
                assert self.handle is not None
                args = {'handle': self.handle}
            item = {'id': f'fc_context_{index}', 'type': 'function_call', 'status': 'completed',
                    'call_id': f'call_context_{index}', 'namespace': 'mcp__finite',
                    'name': step['name'], 'arguments': compact(args)}
        else:
            item = {'id': 'msg_context_done', 'type': 'message', 'status': 'completed', 'role': 'assistant',
                    'content': [{'type': 'output_text', 'text': 'CONTEXT_AUTHORED_DONE', 'annotations': []}]}
        response = {'id': f'resp_context_{index}', 'object': 'response', 'created_at': 1,
                    'model': self.script['model'], 'status': 'completed', 'output': [item]}
        events = [{'type': 'response.created', 'response': dict(response, status='in_progress', output=[])}]
        if item['type'] == 'function_call':
            events += [
                {'type': 'response.output_item.added', 'output_index': 0, 'item': dict(item, status='in_progress', arguments='')},
                {'type': 'response.function_call_arguments.delta', 'item_id': item['id'], 'output_index': 0, 'delta': item['arguments']},
                {'type': 'response.function_call_arguments.done', 'item_id': item['id'], 'output_index': 0, 'arguments': item['arguments']},
            ]
        else:
            text = item['content'][0]['text']
            events += [
                {'type': 'response.output_item.added', 'output_index': 0, 'item': dict(item, status='in_progress', content=[{'type': 'output_text', 'text': '', 'annotations': []}])},
                {'type': 'response.output_text.delta', 'item_id': item['id'], 'output_index': 0, 'content_index': 0, 'delta': text},
                {'type': 'response.output_text.done', 'item_id': item['id'], 'output_index': 0, 'content_index': 0, 'text': text},
            ]
        events += [{'type': 'response.output_item.done', 'output_index': 0, 'item': item},
                   {'type': 'response.completed', 'response': response}]
        # No usage is fabricated for the authored dialogue.
        return ''.join('event: ' + e['type'] + '\ndata: ' + compact(e) + '\n\n' for e in events).encode('utf-8')


if __name__ == '__main__':
    import sys
    script = ContextScript(sys.argv[1], sys.argv[2])
    for line in sys.stdin:
        print(compact(script.respond(json.loads(line))), flush=True)
