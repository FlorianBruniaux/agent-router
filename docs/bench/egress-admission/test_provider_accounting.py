"""The local provider records connection/request metadata, never request bodies."""
import http.client
import json
from pathlib import Path
import socket
import threading
import time
import unittest

import claude_local as driver


class ProviderAccounting(unittest.TestCase):
    def setUp(self):
        self.workspace = Path("/private/tmp")
        self.provider = driver.Provider("inventory", self.workspace)
        self.thread = threading.Thread(target=self.provider.serve_forever, daemon=True)
        self.thread.start()

    def tearDown(self):
        self.provider.shutdown()
        self.provider.server_close()
        self.thread.join(timeout=2)

    def _wait_connections(self, count):
        deadline = time.monotonic() + 2
        while time.monotonic() < deadline:
            if self.provider.observation()["accepted_tcp_connections"] >= count:
                return
            time.sleep(0.01)
        self.fail(f"listener accepted fewer than {count} TCP connections")

    def _raw_request(self, payload):
        with socket.create_connection(("127.0.0.1", self.provider.server_port), timeout=2) as client:
            client.sendall(payload)
            client.shutdown(socket.SHUT_WR)
            client.settimeout(2)
            chunks = []
            while True:
                chunk = client.recv(4096)
                if not chunk:
                    return b"".join(chunks)
                chunks.append(chunk)

    def test_raw_tcp_and_unsupported_method_are_observed_without_body(self):
        self.assertEqual(driver.Handler._safe_path("/healthz/ready?token=synthetic-secret"),
                         "/healthz/<other>")
        self.assertEqual(driver.Handler._safe_path("http://[invalid"), "<other>")
        self._raw_request(b"")
        response = self._raw_request(b"GET /v1/messages?token=synthetic-secret HTTP/1.1\r\nHost: localhost\r\n\r\n")
        self.assertIn(b"501", response)
        root_response = self._raw_request(b"HEAD /?private=synthetic-secret HTTP/1.1\r\nHost: localhost\r\n\r\n")
        self.assertIn(b"501", root_response)
        self._wait_connections(3)

        observed = self.provider.observation()
        self.assertEqual(observed["accepted_tcp_connections"], 3)
        self.assertEqual(len(observed["http_response_events"]), 2)
        self.assertEqual(observed["http_response_events"][0], {
            "method": "GET", "path": "/v1/messages", "status": 501,
            "content_length_bytes": None, "outcome": "http_error"})
        self.assertEqual(observed["http_response_events"][1]["method"], "HEAD")
        self.assertEqual(observed["http_response_events"][1]["path"], "<root>")
        self.assertNotIn("synthetic-secret", json.dumps(observed))

    def test_malformed_http_is_counted_even_when_method_and_path_do_not_parse(self):
        response = self._raw_request(b"NOT HTTP\r\n\r\n")
        self.assertIn(b"400", response)
        self._wait_connections(1)

        event = self.provider.observation()["http_response_events"][0]
        self.assertEqual(event["status"], 400)
        self.assertEqual(event["outcome"], "http_error")
        self.assertNotIn("NOT HTTP", json.dumps(event))

    def test_counter_reset_refuses_an_active_connection(self):
        client = socket.create_connection(("127.0.0.1", self.provider.server_port), timeout=2)
        try:
            self._wait_connections(1)
            with self.assertRaisesRegex(RuntimeError, "connection is active"):
                self.provider.reset_observation()
        finally:
            client.close()
        self.assertTrue(self.provider.wait_until_settled())
        self.provider.reset_observation()
        self.assertEqual(self.provider.observation()["accepted_tcp_connections"], 0)

    def test_invalid_and_oversized_posts_have_outcomes_without_body_capture(self):
        malformed = self._raw_request(
            b"POST /v1/messages HTTP/1.1\r\nHost: localhost\r\nContent-Length: 4\r\n\r\nnope")
        invalid_length = self._raw_request(
            b"POST /v1/messages HTTP/1.1\r\nHost: localhost\r\nContent-Length: private\r\n\r\nx")
        oversized = self._raw_request(
            b"POST /v1/messages HTTP/1.1\r\nHost: localhost\r\nContent-Length: 1048577\r\n\r\nx")
        self.assertIn(b"400", malformed)
        self.assertIn(b"400", invalid_length)
        self.assertIn(b"413", oversized)
        self._wait_connections(3)

        events = self.provider.observation()["http_response_events"]
        self.assertEqual([(event["status"], event["outcome"]) for event in events],
                         [(400, "invalid_json"), (400, "invalid_content_length"),
                          (413, "body_too_large")])
        self.assertTrue(all("nope" not in json.dumps(event) for event in events))
        self.assertTrue(all("private" not in json.dumps(event) for event in events))
        self.assertEqual(self.provider.requests, [])

    def test_event_and_request_records_have_a_fixed_memory_ceiling(self):
        for index in range(driver.MAX_PROVIDER_RECORDS + 3):
            self.provider.record_http_response({"method": "<other>", "path": "<other>",
                                                "status": 400, "content_length_bytes": None,
                                                "outcome": "http_error"})
            self.provider.record_provider_request({"path": "<other>"})

        observed = self.provider.observation()
        self.assertEqual(observed["http_response_events_total"], driver.MAX_PROVIDER_RECORDS + 3)
        self.assertEqual(len(observed["http_response_events"]), driver.MAX_PROVIDER_RECORDS)
        self.assertEqual(observed["http_response_events_dropped"], 3)
        self.assertEqual(observed["valid_json_posts"], driver.MAX_PROVIDER_RECORDS + 3)
        self.assertEqual(len(self.provider.requests), driver.MAX_PROVIDER_RECORDS)
        self.assertEqual(observed["valid_json_post_records_dropped"], 3)

    def test_socket_preflight_requires_clean_allowed_and_denied_controls(self):
        valid = {"exit_code": 0, "termination": "exit",
                 "stdout": "41001 allowed\n41002 denied 1\n"}
        self.assertEqual(driver.validate_socket_control(valid, 41001, 41002), {
            "clean_exit": True, "provider_allowed": True, "other_port_denied": True})
        self.assertEqual(driver.validate_socket_control({**valid, "exit_code": 1}, 41001, 41002), {
            "clean_exit": False, "provider_allowed": False, "other_port_denied": False})
        self.assertEqual(driver.validate_socket_control({**valid, "stdout": "41001 allowed\n41002 allowed\n"},
                                                        41001, 41002), {
            "clean_exit": True, "provider_allowed": True, "other_port_denied": False})

    def test_observation_snapshots_keep_counters_and_records_atomic(self):
        def record_many():
            for index in range(1000):
                self.provider.record_provider_request({"sequence": index})

        writer = threading.Thread(target=record_many)
        writer.start()
        while writer.is_alive():
            observed = self.provider.observation()
            self.assertEqual(observed["valid_json_posts"],
                             len(observed["valid_json_post_records"]) +
                             observed["valid_json_post_records_dropped"])
        writer.join()
        snapshot = self.provider.observation()
        self.assertEqual(snapshot["valid_json_posts"], 1000)
        self.assertEqual(len(snapshot["valid_json_post_records"]), driver.MAX_PROVIDER_RECORDS)
        self.provider.record_provider_request({"sequence": 1000})
        self.assertEqual(snapshot["valid_json_posts"], 1000)
        self.assertEqual(len(snapshot["valid_json_post_records"]), driver.MAX_PROVIDER_RECORDS)

    def test_unsettled_observation_is_rejected_before_snapshot(self):
        class UnsettledProvider:
            def wait_until_settled(self, timeout):
                return False

            def observation(self):
                raise AssertionError("unsettled provider must not be snapshotted")

        with self.assertRaisesRegex(RuntimeError, "did not settle"):
            driver.settled_observation(UnsettledProvider())

    def test_valid_post_retains_only_bounded_metadata_and_reset_excludes_preflight(self):
        self._raw_request(b"GET /preflight HTTP/1.1\r\nHost: localhost\r\n\r\n")
        self._wait_connections(1)
        self.assertTrue(self.provider.wait_until_settled())
        self.provider.reset_observation()

        marker = "SYNTHETIC_PRIVATE_PROMPT_BODY"
        body = json.dumps({"model": "fixture-model", "messages": [{"role": "user", "content": marker}],
                           "tools": [{"name": "fixture_tool"}]}).encode()
        connection = http.client.HTTPConnection("127.0.0.1", self.provider.server_port, timeout=2)
        connection.request("POST", "/v1/messages?discard=this-secret", body,
                           {"Content-Type": "application/json", "Content-Length": str(len(body)),
                            "x-api-key": driver.KEY})
        response = connection.getresponse()
        response.read()
        connection.request("GET", "/other?discard=second-secret")
        followup = connection.getresponse()
        self.assertEqual(followup.status, 501)
        followup.read()
        connection.close()
        self._wait_connections(1)

        observed = self.provider.observation()
        self.assertEqual(observed["accepted_tcp_connections"], 1)
        self.assertEqual(len(observed["http_response_events"]), 2)
        self.assertEqual(observed["http_response_events"][0]["outcome"], "valid_json")
        self.assertEqual(observed["http_response_events"][0]["status"], 200)
        self.assertEqual(observed["http_response_events"][1]["content_length_bytes"], None)
        self.assertEqual(observed["http_response_events"][1]["outcome"], "http_error")
        serialized = json.dumps({"observation": observed, "requests": self.provider.requests})
        for secret in [marker, driver.KEY, "discard=this-secret", "discard=second-secret"]:
            self.assertNotIn(secret, serialized)
        self.assertEqual(self.provider.requests[0]["tool_names"], ["fixture_tool"])
        self.assertTrue(self.provider.requests[0]["synthetic_key_received"])
        self.assertNotIn("tool_result_content", self.provider.requests[0])


if __name__ == "__main__":
    unittest.main()
