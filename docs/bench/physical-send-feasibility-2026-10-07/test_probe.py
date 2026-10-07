import concurrent.futures
import unittest
from probe import Gate

class GateTests(unittest.TestCase):
    def test_deny_first_consumes_no_reservation(self):
        gate = Gate(0)
        self.assertFalse(gate.admit())
        self.assertEqual(gate.reserved, 0)

    def test_retry_needs_another_reservation(self):
        gate = Gate(1)
        self.assertTrue(gate.admit())
        self.assertFalse(gate.admit())
        self.assertEqual(gate.reserved, 1)

    def test_disabled_or_failed_guard_denies(self):
        for mode in ('disabled', 'failed'):
            with self.subTest(mode=mode):
                gate = Gate(2, mode)
                self.assertFalse(gate.admit())
                self.assertEqual(gate.reserved, 0)

    def test_concurrent_ceiling_is_atomic(self):
        gate = Gate(1)
        with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
            self.assertEqual(sum(pool.map(lambda _: gate.admit(), range(16))), 1)
        self.assertEqual(gate.reserved, 1)


# Socket tests use the same broker and independently counted provider as native runs.
class HttpBoundaryTests(unittest.TestCase):
    def setUp(self):
        import tempfile
        from pathlib import Path
        import probe
        self.probe = probe
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        repo = Path(probe.__file__).resolve().parents[3]
        self.h = probe.load('test_f03a_helpers', repo / 'docs/bench/egress-admission/codex_appserver_local.py')
        self.prior = probe.load('test_f03a_prior', repo / 'docs/bench/native-integration-u06/probe.py')
        self.provider, self.pt = probe.start_server(self.h, probe.handler(self.h, self.prior.final_sse, False), self.root / 'provider')
        self.provider.retry = False
        self.broker, self.bt = probe.start_server(self.h, probe.handler(self.h, self.prior.final_sse, True), self.root / 'broker')
        self.broker.gate = probe.Gate(1)
        self.broker.upstream_port = self.provider.server_address[1]

    def tearDown(self):
        for server, thread in ((self.broker, self.bt), (self.provider, self.pt)):
            server.shutdown(); server.server_close(); thread.join(timeout=3)
            self.assertFalse(thread.is_alive())
        self.temp.cleanup()

    def request(self, method='POST', path='/v1/responses', body=None, headers=None):
        import http.client
        import json
        c = http.client.HTTPConnection('127.0.0.1', self.broker.server_address[1], timeout=2)
        try:
            c.request(method, path, json.dumps({'model': self.probe.MODEL}) if body is None else body, headers or {})
            r = c.getresponse(); r.read(self.probe.LIMIT + 1)
            return r.status
        finally:
            c.close()

    def test_second_http_request_is_denied_before_provider(self):
        self.assertEqual(self.request(), 200)
        self.assertEqual(self.request(), 403)
        self.assertEqual(self.provider.request_count, 1)
        self.assertEqual(len(self.provider.bodies), 1)
        self.assertEqual(self.broker.gate.reserved, 1)

    def test_guard_off_and_failure_never_reach_provider(self):
        for mode in ('disabled', 'failed'):
            self.broker.gate = self.probe.Gate(1, mode)
            self.assertEqual(self.request(), 403)
        self.assertEqual(self.provider.request_count, 0)
        self.assertEqual(self.provider.accepted_connections, 0)

    def test_wrong_model_path_method_and_oversize_refused(self):
        self.assertEqual(self.request(body='{"model":"other"}'), 400)
        self.assertEqual(self.request(path='/other'), 400)
        self.assertEqual(self.request(method='CONNECT'), 501)
        self.assertEqual(self.request(body='', headers={'Content-Length': str(self.probe.LIMIT + 1)}), 400)
        self.assertEqual(self.provider.request_count, 0)
        self.assertEqual(self.broker.gate.reserved, 0)

    def test_lost_upstream_keeps_reservation(self):
        self.provider.shutdown(); self.provider.server_close(); self.pt.join(timeout=3)
        self.assertEqual(self.request(), 502)
        self.assertEqual(self.request(), 403)
        self.assertEqual(self.broker.gate.reserved, 1)

class VerdictTests(unittest.TestCase):
    def capture(self, name):
        import json
        from pathlib import Path
        cases = json.loads((Path(__file__).with_name('results.json')).read_text())['cases']
        return next(case for case in cases if case['case'] == name)

    def test_denial_with_independent_provider_http_is_not_zero_send(self):
        from probe import case_verdict
        result = self.capture('deny_first')
        self.assertTrue(case_verdict(result))
        result['provider_http_requests'] = 1
        self.assertFalse(case_verdict(result))

    def test_extra_broker_http_request_is_rejected(self):
        from probe import case_verdict
        result = self.capture('allow')
        result['broker_http_requests'] = 2
        self.assertFalse(case_verdict(result))

    def test_missing_body_hash_cannot_substantiate_a_received_request(self):
        from probe import case_verdict
        result = self.capture('deny_first')
        result['broker_body_hashes'] = []
        self.assertFalse(case_verdict(result))

    def test_all_frozen_native_outcomes_are_preserved(self):
        from probe import case_verdict
        for name in ('allow', 'deny_first', 'retry_allow', 'retry_deny', 'guard_disabled', 'guard_failed'):
            with self.subTest(name=name):
                self.assertTrue(case_verdict(self.capture(name)))
        self.assertFalse(case_verdict(self.capture('broker_absent')))

if __name__ == '__main__':
    unittest.main()
