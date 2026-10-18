"""Bounded host-side subscription relay. Importing this file never sends a request.

No Runtime authority. Only the approved controller may supply frames over its
private pipe. Credentials and outbound networking never enter the worker.
"""
import base64
import hashlib
import http.client
import json
import pathlib
import os
import re
import stat
import sys
import time

ROOT = pathlib.Path(__file__).resolve().parent
sys.path[:0] = [str(ROOT / 'native-catalog-adapter'), str(ROOT / 'qualification-inputs')]
from native_tool_responses import validate_native_catalog

MAX_BODY = 262144
MAX_STREAM = 8 * 1024 * 1024
MAX_CHUNK = 65536
RATES = {'gpt-6-luna': (2500, 250, 12500), 'gpt-6-astra': (250000, 25000, 1250000)}
BODY_KEYS = frozenset(('client_metadata', 'include', 'input', 'instructions', 'model',
                       'parallel_tool_calls', 'prompt_cache_key', 'reasoning', 'store',
                       'stream', 'tool_choice', 'tools', 'service_tier'))


def strict_json(raw):
    def unique(pairs):
        result = {}
        for key, value in pairs:
            if key in result: raise ValueError('duplicate_key')
            result[key] = value
        return result
    def invalid(_): raise ValueError('invalid_constant')
    return json.loads(raw, object_pairs_hook=unique, parse_constant=invalid)


def observed_usage(response, model):
    actual = response.get('model')
    if not isinstance(actual, str) or not (actual == model or actual.startswith(model + '-')):
        raise ValueError('response_model_unverified')
    if response.get('status') != 'completed': raise ValueError('response_incomplete')
    usage = response.get('usage')
    if not isinstance(usage, dict): raise ValueError('usage_unknown')
    inp, out, total = (usage.get(k) for k in ('input_tokens', 'output_tokens', 'total_tokens'))
    details = usage.get('input_tokens_details')
    cached = details.get('cached_tokens') if isinstance(details, dict) else None
    if (any(type(v) is not int or not 0 <= v <= 1_000_000_000 for v in (inp, out, total, cached))
            or cached > inp or inp + out != total or total == 0):
        raise ValueError('usage_unknown')
    if usage.get('cache_write_input_tokens', 0) != 0:
        raise ValueError('cache_write_pricing_unknown')
    reason = usage.get('output_tokens_details', {}).get('reasoning_tokens')
    if reason is not None and (type(reason) is not int or not 0 <= reason <= out):
        raise ValueError('usage_incoherent')
    return {'input_tokens': inp, 'cached_input_tokens': cached, 'output_tokens': out,
            'reasoning_output_tokens': reason, 'total_tokens': total}


class Gateway:
    def __init__(self, assignment, send, *, clock=time.monotonic):
        if (assignment.get('model') not in RATES
                or assignment.get('effort') not in ('medium', 'xhigh')):
            raise ValueError('assignment')
        self.assignment, self.send, self.clock = dict(assignment), send, clock
        self.deadline = clock() + 60
        self.next_id, self.stopped = 1, False
        self.totals = {'input_tokens': 0, 'cached_input_tokens': 0, 'output_tokens': 0,
                       'total_tokens': 0}
        self.response_ids = set()
        self.nano_credits = 0

    def handle(self, frame):
        identifier = frame.get('id') if isinstance(frame, dict) else None
        terminal = {'schema_version': 1, 'id': identifier, 'kind': 'terminal',
                    'complete': False, 'usage': None, 'provider_request_started': False,
                    'unit': 'credit_equivalents_not_subscription_debit_or_usd',
                    'provider_billing_stop_proven': False}
        response, chunks = None, None
        try:
            if self.stopped: raise ValueError('gateway_stopped')
            if (not isinstance(frame, dict) or set(frame) != {'schema_version', 'id', 'body'}
                    or type(frame['schema_version']) is not int or frame['schema_version'] != 1
                    or type(identifier) is not int or identifier != self.next_id or identifier > 64):
                raise ValueError('frame_identity')
            remaining = self.deadline - self.clock()
            if remaining <= 0: raise ValueError('trial_deadline')
            body = frame['body']
            if (not isinstance(body, dict) or set(body) - BODY_KEYS
                    or body.get('model') != self.assignment['model']
                    or not isinstance(body.get('reasoning'), dict)
                    or body['reasoning'].get('effort') != self.assignment['effort']
                    or body.get('stream') is not True or body.get('store') is not False
                    or body.get('service_tier') not in (None, 'default')
                    or 'previous_response_id' in body):
                raise ValueError('request_identity')
            validate_native_catalog(body)
            raw = json.dumps(body, ensure_ascii=False, separators=(',', ':')).encode()
            if len(raw) > MAX_BODY: raise ValueError('body_bound')
            terminal['request_body_sha256'] = hashlib.sha256(raw).hexdigest()
            terminal['provider_request_started'] = True
            status, content_type, chunks = self.send(raw, remaining)
            terminal['upstream_http_status'] = status
            if status != 200: raise ValueError('upstream_http_status')
            if content_type.split(';')[0].strip().lower() != 'text/event-stream':
                raise ValueError('upstream_content_type')
            received, pending, completions = 0, b'', 0
            for chunk in chunks:
                if self.clock() >= self.deadline: raise ValueError('trial_deadline')
                if not isinstance(chunk, bytes): raise ValueError('upstream_chunk_type')
                received += len(chunk)
                if received > MAX_STREAM: raise ValueError('stream_bound')
                pending = (pending + chunk).replace(b'\r\n', b'\n')
                while b'\n\n' in pending:
                    block, pending = pending.split(b'\n\n', 1)
                    if len(block) > MAX_BODY: raise ValueError('event_bound')
                    data = b'\n'.join(line[5:].lstrip(b' ') for line in block.split(b'\n')
                                      if line.startswith(b'data:'))
                    if not data or data == b'[DONE]': continue
                    event = strict_json(data)
                    if not isinstance(event, dict): raise ValueError('event_shape')
                    if completions: raise ValueError('event_after_completion')
                    if event.get('type') in ('error', 'response.failed', 'response.incomplete'):
                        raise ValueError('upstream_failed')
                    if event.get('type') == 'response.completed':
                        completions += 1
                        if completions != 1: raise ValueError('duplicate_completion')
                        response = event.get('response')
                        if not isinstance(response, dict): raise ValueError('response_shape')
                if len(pending) > MAX_BODY: raise ValueError('event_bound')
                for offset in range(0, len(chunk), MAX_CHUNK):
                    yield {'schema_version': 1, 'id': identifier, 'kind': 'chunk',
                           'data': base64.b64encode(chunk[offset:offset + MAX_CHUNK]).decode()}
            if pending.strip() or completions != 1: raise ValueError('upstream_incomplete')
            rid = response.get('id')
            if not isinstance(rid, str) or not 1 <= len(rid) <= 256 or rid in self.response_ids:
                raise ValueError('response_id')
            usage = observed_usage(response, self.assignment['model'])
            self.response_ids.add(rid)
            for key in self.totals: self.totals[key] += usage[key]
            inp_rate, cache_rate, out_rate = RATES[self.assignment['model']]
            self.nano_credits += ((usage['input_tokens'] - usage['cached_input_tokens']) * inp_rate
                                 + usage['cached_input_tokens'] * cache_rate
                                 + usage['output_tokens'] * out_rate)
            terminal.update({'complete': True, 'usage': usage,
                             'response_id': rid, 'observed_response_model': response['model'],
                             'cumulative_usage': dict(self.totals),
                             'cumulative_equivalent_microcredits': (self.nano_credits + 999) // 1000,
                             'stop_reason': None})
            self.next_id += 1
            if self.totals['total_tokens'] >= 6000 or self.next_id > 64:
                terminal['stop_reason'] = 'observed_token_limit' if self.next_id <= 64 else 'response_count_limit'
                self.stopped = True
        except (ValueError, TypeError, KeyError, OSError, RecursionError, AttributeError):
            self.stopped = True
            terminal['stop_reason'] = 'unknown_or_refused_stop_campaign'
        finally:
            if chunks is not None and callable(getattr(chunks, 'close', None)):
                try: chunks.close()
                except OSError:
                    self.stopped = True
                    terminal.update({'complete': False, 'usage': None,
                                     'stop_reason': 'upstream_cleanup_unknown_stop_campaign'})
        yield terminal


def subscription_sender(auth_provider, *, connection_factory=http.client.HTTPSConnection):
    """Trusted host transport; child frames cannot select URL, auth, headers or retries."""
    def send(raw, remaining):
        headers = auth_provider()
        if (not isinstance(headers, dict) or set(headers) != {'Authorization', 'ChatGPT-Account-ID'}
                or not all(isinstance(v, str) and 1 <= len(v) <= 32768
                           and not any(c in v for c in '\r\n\x00') for v in headers.values())):
            raise ValueError('subscription_auth_headers')
        headers = {**headers, 'Content-Type': 'application/json', 'Accept': 'text/event-stream',
                   'originator': 'codex_cli_rs', 'User-Agent': 'codex-cli/0.159.2'}
        deadline = time.monotonic() + remaining
        connection = connection_factory('chatgpt.com', timeout=max(0.001, remaining))
        try:
            connection.request('POST', '/backend-api/codex/responses', raw, headers)
            response = connection.getresponse()
            status, content_type = response.status, response.getheader('Content-Type', '')
            if status != 200:
                connection.close()
                return status, content_type, iter(())
        except (OSError, http.client.HTTPException):
            connection.close()
            raise OSError('subscription_transport') from None

        def chunks():
            try:
                while True:
                    seconds = deadline - time.monotonic()
                    if seconds <= 0: raise OSError('subscription_deadline')
                    if connection.sock is not None: connection.sock.settimeout(seconds)
                    chunk = response.read1(MAX_CHUNK)
                    if not chunk: break
                    yield chunk
            except (OSError, http.client.HTTPException):
                raise OSError('subscription_stream') from None
            finally:
                response.close()
                connection.close()
        return status, content_type, chunks()
    return send


def validate_auth(value, expected_account_sha256, *, now=None):
    """Validate local native credential selection; server still validates the token."""
    if now is None: now = time.time()
    if (not isinstance(value, dict) or value.get('auth_mode') != 'chatgpt'
            or value.get('OPENAI_API_KEY') or not isinstance(value.get('tokens'), dict)):
        raise ValueError('subscription_auth_only')
    account = value['tokens'].get('account_id')
    token = value['tokens'].get('access_token')
    if (not isinstance(account, str) or not re.fullmatch(r'[A-Za-z0-9_-]{1,128}', account)
            or hashlib.sha256(account.encode()).hexdigest() != expected_account_sha256
            or not isinstance(token, str) or not 1 <= len(token) <= 32768
            or any(c in token for c in '\r\n\x00') or len(token.split('.')) != 3):
        raise ValueError('subscription_auth_identity')
    payload = token.split('.')[1]
    claims = strict_json(base64.urlsafe_b64decode(payload + '=' * (-len(payload) % 4)))
    expiry = claims.get('exp') if isinstance(claims, dict) else None
    if type(expiry) not in (int, float) or expiry < now + 60:
        raise ValueError('subscription_auth_expired')
    return {'Authorization': 'Bearer ' + token, 'ChatGPT-Account-ID': account}


def host_auth(expected_account_sha256):
    """Read only the native host credential file. Never export it or refresh it."""
    fd = os.open('/Users/florianbruniaux/.codex/auth.json', os.O_RDONLY | os.O_NOFOLLOW)
    with os.fdopen(fd, 'rb') as stream:
        metadata = os.fstat(stream.fileno())
        if (not stat.S_ISREG(metadata.st_mode) or metadata.st_uid != os.getuid()
                or stat.S_IMODE(metadata.st_mode) != 0o600 or metadata.st_nlink != 1
                or metadata.st_size > 65536):
            raise ValueError('subscription_auth_file')
        raw = stream.read(65537)
        if len(raw) > 65536: raise ValueError('subscription_auth_file')
    return validate_auth(strict_json(raw), expected_account_sha256)


def drive(source, target, make_gateway):
    """Private controller JSONL pipe. The controller also owns its idle deadline."""
    gateway = None
    while True:
        line = source.readline(300001)
        if not line: return 0
        try:
            if len(line) > 300000 or not line.endswith(b'\n'):
                raise ValueError('pipe_frame_bound')
            frame = strict_json(line)
        except (ValueError, TypeError, RecursionError):
            row = {'schema_version': 1, 'id': None, 'kind': 'terminal', 'complete': False,
                   'usage': None, 'provider_request_started': False,
                   'stop_reason': 'pipe_frame_refused_stop_campaign'}
            target.write(json.dumps(row).encode() + b'\n')
            target.flush()
            return 1
        if gateway is None: gateway = make_gateway()
        terminal = None
        for row in gateway.handle(frame):
            target.write(json.dumps(row, separators=(',', ':')).encode() + b'\n')
            target.flush()
            if row['kind'] == 'terminal': terminal = row
        if terminal is None or not terminal['complete']: return 1
        if gateway.stopped: return 0


def main():
    import argparse
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--assignment', required=True)
    args = parser.parse_args()
    intent_bytes = (ROOT / 'launch-intent.json').read_bytes()
    if hashlib.sha256(intent_bytes).hexdigest() != 'fc31607f68d55a1631f06e545e3fea3bfa546d1e8c81be06a1545532fa9a0450':
        raise ValueError('sealed_launch_intent_changed')
    intent = strict_json(intent_bytes)
    matches = [r for r in intent['unmodified_experiment']['assignments']
               if r['assignment_id'] == args.assignment]
    if len(matches) != 1: raise ValueError('assignment_unknown')
    metadata = strict_json((ROOT / 'host-auth-metadata.json').read_bytes())
    expected = metadata.get('account_id_sha256')
    if (metadata.get('matches_current_app_account') is not True
            or not isinstance(expected, str) or not re.fullmatch(r'[0-9a-f]{64}', expected)):
        raise ValueError('subscription_account_unverified')
    send = subscription_sender(lambda: host_auth(expected))
    return drive(sys.stdin.buffer, sys.stdout.buffer, lambda: Gateway(matches[0], send))


if __name__ == '__main__':
    try:
        raise SystemExit(main())
    except (ValueError, KeyError, TypeError, OSError, RecursionError):
        # Never render credential, response-body or exception values.
        sys.stderr.write('subscription relay refused before completion\n')
        raise SystemExit(1)
