import tempfile
import unittest
from pathlib import Path

import codex_appserver_local as fixture


class ProviderAddressTests(unittest.TestCase):
    def test_accepts_loopback_and_rejects_non_loopback_bind(self):
        fixture.require_loopback("127.0.0.1")
        fixture.require_loopback("::1")
        with self.assertRaisesRegex(ValueError, "loopback"):
            fixture.require_loopback("0.0.0.0")

    def test_provider_url_must_use_loopback_http(self):
        self.assertTrue(fixture.is_loopback_url("http://127.0.0.1:1234/v1"))
        self.assertTrue(fixture.is_loopback_url("http://localhost:1234/v1"))
        self.assertFalse(fixture.is_loopback_url("https://127.0.0.1:1234/v1"))
        self.assertFalse(fixture.is_loopback_url("http://example.com/v1"))


class EvidenceTests(unittest.TestCase):
    def test_matches_only_correlated_function_call_output(self):
        matching = {"input": [{"type": "function_call_output", "call_id": "call_fixture_1",
                               "output": "LOCAL_FIXTURE_CALLBACK_ACCEPTED"}]}
        other = {"input": [{"type": "function_call_output", "call_id": "other",
                            "output": "LOCAL_FIXTURE_CALLBACK_ACCEPTED"}]}
        self.assertTrue(fixture.has_matching_output(matching, "call_fixture_1"))
        self.assertFalse(fixture.has_matching_output(other, "call_fixture_1"))

    def test_rejects_matching_call_id_with_wrong_callback_content(self):
        request = {"input": [{"type": "function_call_output", "call_id": "call_fixture_1",
                              "output": ""}]}
        self.assertFalse(fixture.has_matching_output(request, "call_fixture_1"))

    def test_zero_is_a_valid_jsonrpc_callback_id(self):
        callback = {"jsonrpc_id": 0, "thread_id": "thread", "turn_id": "turn",
                    "call_id": "call_fixture_1", "tool_name": "router_probe_tool"}
        self.assertTrue(fixture.valid_callback(callback))

    def test_callback_must_match_the_current_thread_and_turn(self):
        callback = {"jsonrpc_id": 0, "thread_id": "thread", "turn_id": "turn",
                    "call_id": "call_fixture_1", "tool_name": "router_probe_tool"}
        self.assertTrue(fixture.callback_matches(callback, "thread", "turn"))
        self.assertFalse(fixture.callback_matches(callback, "other-thread", "turn"))
        self.assertFalse(fixture.callback_matches(callback, "thread", "other-turn"))

    def test_dynamic_tool_arguments_require_exact_synthetic_nonce(self):
        self.assertTrue(fixture.valid_tool_arguments({"nonce": fixture.NONCE}))
        self.assertTrue(fixture.valid_tool_arguments('{"nonce":"' + fixture.NONCE + '"}'))
        self.assertFalse(fixture.valid_tool_arguments("{"))
        self.assertFalse(fixture.valid_tool_arguments({"nonce": "wrong"}))
        self.assertFalse(fixture.valid_tool_arguments({"nonce": fixture.NONCE, "extra": True}))

    def test_matching_output_must_be_in_request_two_and_total_requests_must_equal_two(self):
        request = {"input": [{"type": "function_call_output", "call_id": "call_fixture_1",
                              "output": "LOCAL_FIXTURE_CALLBACK_ACCEPTED"}]}
        self.assertTrue(fixture.provider_request_matches_call_output(2, request))
        self.assertFalse(fixture.provider_request_matches_call_output(3, request))
        self.assertTrue(fixture.exact_two_request_roundtrip(2, True))
        self.assertFalse(fixture.exact_two_request_roundtrip(3, True))
        self.assertFalse(fixture.exact_two_request_roundtrip(2, False))

    def test_rejects_executable_that_does_not_report_codex_version(self):
        with tempfile.TemporaryDirectory() as directory:
            candidate = Path(directory) / "codex"
            candidate.write_text("#!/bin/sh\nprintf 'not codex\\n'\n", encoding="utf-8")
            candidate.chmod(0o700)
            with self.assertRaisesRegex(fixture.FixtureError, "did not identify as codex-cli"):
                fixture.verify_codex(candidate.resolve())

    def test_stops_reading_oversized_codex_version_output(self):
        with tempfile.TemporaryDirectory() as directory:
            candidate = Path(directory) / "codex"
            candidate.write_text("#!/bin/sh\n/usr/bin/head -c 10000000 /dev/zero\n", encoding="utf-8")
            candidate.chmod(0o700)
            started = __import__("time").monotonic()
            with self.assertRaisesRegex(fixture.FixtureError, "output limit"):
                fixture.verify_codex(candidate.resolve())
            self.assertLess(__import__("time").monotonic() - started, 3)


if __name__ == "__main__":
    unittest.main()
