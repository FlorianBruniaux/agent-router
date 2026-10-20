"""Authored broker validation only. Never provider execution or Runtime authority.

The native SDK receives and sends its unmodified catalogue. This adapter pins
that raw catalogue, then projects only its finite-tool map for the old script's
validation. It never emits a resource-wrapper call or changes tool arguments.
"""
import hashlib
import json
import re
from tool_responses import ToolScript as FiniteToolScript

EXPECTED_CATALOG_SHA256 = 'f83cb0cae17d9b72a5bd2637f6db80fc2f5f6ef5eeeb61d5dd8bc6a869c7de85'
DISCOVERY_NAMES = frozenset(('list_mcp_resources', 'list_mcp_resource_templates',
                             'read_mcp_resource'))


def validate_native_catalog(request):
    tools = request.get('tools') if isinstance(request, dict) else None
    if (not isinstance(tools, list) or len(tools) != 4
            or not all(isinstance(t, dict) and isinstance(t.get('name'), str)
                       for t in tools)):
        raise ValueError('native_catalog_shape')
    encoded = json.dumps(sorted(tools, key=lambda t: t['name']), sort_keys=True,
                         separators=(',', ':'), ensure_ascii=False).encode()
    if hashlib.sha256(encoded).hexdigest() != EXPECTED_CATALOG_SHA256:
        raise ValueError('native_catalog_unreviewed')
    inputs = request.get('input')
    if (not isinstance(inputs, list) or not all(isinstance(i, dict) for i in inputs)
            or any(i.get('type') == 'additional_tools' and i.get('tools')
                   for i in inputs)):
        raise ValueError('native_additional_tools')
    return EXPECTED_CATALOG_SHA256


class ToolScript(FiniteToolScript):
    """Same frozen authored steps/results, exactly pinned native catalogue."""
    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.observed_native_catalogs = []

    def respond(self, request):
        digest = validate_native_catalog(request)
        projection = dict(request)
        projection['tools'] = [t for t in request['tools']
                               if t['name'] not in DISCOVERY_NAMES]
        projection['input'] = [project_native_output(i) for i in request['input']]
        response = super().respond(projection)
        self.observed_native_catalogs.append(digest)
        return response


def project_native_output(item):
    """Validate the SDK's exact successful two-part envelope, not error text."""
    if item.get('type') != 'function_call_output':
        return item
    output = item.get('output')
    if (not isinstance(output, list) or len(output) != 2
            or not all(isinstance(p, dict) and set(p) == {'type', 'text'}
                       and p['type'] == 'input_text' and isinstance(p['text'], str)
                       for p in output)
            or not re.fullmatch(r'Wall time: [0-9]{1,6}\.[0-9]{4} seconds\nOutput:',
                                output[0]['text'])
            or len(output[1]['text'].encode()) > 262144):
        raise ValueError('native_result_envelope')

    def unique(pairs):
        result = {}
        for key, value in pairs:
            if key in result:
                raise ValueError('native_result_duplicate_key')
            result[key] = value
        return result

    def invalid_constant(_):
        raise ValueError('native_result_constant')

    value = json.loads(output[1]['text'], object_pairs_hook=unique,
                       parse_constant=invalid_constant)
    if (not isinstance(value, dict) or set(value) != {
            'elapsed_ms', 'exit_code', 'stderr', 'stdout', 'termination', 'truncated'}
            or type(value['elapsed_ms']) is not int or not 0 <= value['elapsed_ms'] <= 60000
            or type(value['exit_code']) is not int or not 0 <= value['exit_code'] <= 255
            or value['termination'] != 'exited' or value['truncated'] is not False
            or not isinstance(value['stderr'], str) or not isinstance(value['stdout'], str)):
        raise ValueError('native_result_value')
    projected = dict(item)
    projected['output'] = json.dumps(value)
    return projected
