import contextlib
import hashlib
import http.client
import io
import json
import os
import signal
import socket
import subprocess
import sys
import tempfile
import threading
import time
from unittest import mock
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

    def test_callback_is_validated_before_any_response_is_sent(self):
        callback = {"jsonrpc_id": 0, "thread_id": "thread", "turn_id": "turn",
                    "call_id": "call_fixture_1", "tool_name": "router_probe_tool",
                    "arguments_valid": True}

        class RecordingClient:
            def __init__(self):
                self.messages = []

            def send(self, message):
                self.messages.append(message)

        accepted = RecordingClient()
        self.assertTrue(fixture.respond_to_tool_callback(
            accepted, callback, {"nonce": fixture.NONCE}, "thread", "turn",
            callback_consumed=False))
        self.assertEqual(accepted.messages, [{
            "id": 0, "result": {
                "contentItems": [{"type": "inputText", "text": "LOCAL_FIXTURE_CALLBACK_ACCEPTED"}],
                "success": True,
            },
        }])

        invalid_callbacks = []
        for field, value in (("jsonrpc_id", True), ("thread_id", "other-thread"),
                             ("turn_id", "other-turn"), ("call_id", "other-call"),
                             ("tool_name", "unapproved_tool")):
            invalid = dict(callback)
            invalid[field] = value
            invalid_callbacks.append((invalid, {"nonce": fixture.NONCE}, False))
        invalid_callbacks.append((dict(callback), {"nonce": "wrong"}, False))
        invalid_callbacks.append((dict(callback), {"nonce": fixture.NONCE}, True))
        for invalid, arguments, consumed in invalid_callbacks:
            with self.subTest(callback=invalid, consumed=consumed):
                refused = RecordingClient()
                with self.assertRaisesRegex(fixture.FixtureError, "callback refused"):
                    fixture.respond_to_tool_callback(
                        refused, invalid, arguments, "thread", "turn",
                        callback_consumed=consumed)
                self.assertEqual(refused.messages, [])

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


def assert_descendant_stopped(test, group_pid, child_pid):
    try:
        os.killpg(group_pid, 0)
    except ProcessLookupError:
        return
    test.assertEqual(sys.platform, "linux", "descendant process group still exists")
    # This fixture creates exactly one descendant. Linux may retain its zombie
    # until PID 1 reaps it, even after SIGKILL and both pipe readers have exited.
    try:
        stat = Path(f"/proc/{child_pid}/stat").read_text(encoding="utf-8")
    except (FileNotFoundError, ProcessLookupError):
        return  # Reaped before open (ENOENT) or during read (ESRCH).
    fields = stat.rsplit(")", 1)[1].split()
    test.assertEqual(int(fields[2]), group_pid, "unexpected descendant process group")
    test.assertEqual(fields[0], "Z", "descendant is still live")


class DescendantStateTests(unittest.TestCase):
    def check_linux_stat(self, state, group=100):
        # Linux comm may contain spaces and closing parentheses.
        stat = f"101 (fixture ) child) {state} 1 {group} 100 0"
        with mock.patch.object(sys, "platform", "linux"), \
                mock.patch.object(os, "killpg") as killpg, \
                mock.patch.object(Path, "read_text", autospec=True, return_value=stat) as read_stat:
            assert_descendant_stopped(self, 100, 101)
        killpg.assert_called_once_with(100, 0)
        read_stat.assert_called_once_with(Path("/proc/101/stat"), encoding="utf-8")

    def test_linux_zombie_from_expected_group_is_stopped(self):
        self.check_linux_stat("Z")

    def test_linux_live_or_stopped_descendant_is_not_accepted(self):
        for state in ("R", "S", "D", "T"):
            with self.subTest(state=state), self.assertRaises(AssertionError):
                self.check_linux_stat(state)

    def test_linux_zombie_from_another_group_is_not_accepted(self):
        with self.assertRaises(AssertionError):
            self.check_linux_stat("Z", group=999)

    def test_linux_reaped_between_group_probe_and_stat_is_stopped(self):
        # /proc may disappear before open (ENOENT) or during read (ESRCH).
        for error in (FileNotFoundError, ProcessLookupError):
            with self.subTest(error=error.__name__), \
                    mock.patch.object(sys, "platform", "linux"), \
                    mock.patch.object(os, "killpg"), \
                    mock.patch.object(Path, "read_text", side_effect=error):
                assert_descendant_stopped(self, 100, 101)

    def test_absent_group_needs_no_proc_observation(self):
        with mock.patch.object(os, "killpg", side_effect=ProcessLookupError), \
                mock.patch.object(Path, "read_text") as read_stat:
            assert_descendant_stopped(self, 100, 101)
        read_stat.assert_not_called()

    def test_unreadable_proc_state_does_not_count_as_stopped(self):
        with mock.patch.object(sys, "platform", "linux"), \
                mock.patch.object(os, "killpg"), \
                mock.patch.object(Path, "read_text", side_effect=PermissionError), \
                self.assertRaises(PermissionError):
            assert_descendant_stopped(self, 100, 101)

    def test_nonlinux_still_requires_absent_group(self):
        with mock.patch.object(sys, "platform", "darwin"), \
                mock.patch.object(os, "killpg"), \
                mock.patch.object(Path, "read_text") as read_stat, \
                self.assertRaises(AssertionError):
            assert_descendant_stopped(self, 100, 101)
        read_stat.assert_not_called()


class LifecycleTests(unittest.TestCase):
    def test_stop_kills_descendant_after_normal_leader_exit_and_joins_readers(self):
        parent = (
            "import json, subprocess, sys; "
            "child = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(30)']); "
            "print(json.dumps({'child_pid': child.pid}), flush=True); sys.stdin.read()"
        )
        process = subprocess.Popen(
            [sys.executable, "-c", parent], stdin=subprocess.PIPE,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True,
        )
        client = fixture.AppServer(process)
        try:
            child_pid = client.receive(time.monotonic() + 3)["child_pid"]
            self.assertEqual(client.stop(), 0)
            self.assertFalse(client.stdout_thread.is_alive(), "descendant still holds stdout")
            self.assertFalse(client.stderr_thread.is_alive(), "descendant still holds stderr")
            assert_descendant_stopped(self, process.pid, child_pid)
        finally:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            process.wait(timeout=3)
            for reader in (client.stdout_thread, client.stderr_thread):
                reader.join(timeout=3)
            for stream in (process.stdin, process.stdout, process.stderr):
                stream.close()


class ProviderBoundsTests(unittest.TestCase):
    def setUp(self):
        self.patches = mock.patch.multiple(
            fixture, create=True, PROVIDER_IO_TIMEOUT_SECONDS=0.2,
            PROVIDER_CONNECTION_SECONDS=0.6, MAX_PROVIDER_HANDLERS=2,
            MAX_PROVIDER_CONNECTIONS=4, MAX_PROVIDER_REQUESTS=2,
        )
        self.patches.start()
        self.provider = fixture.LocalProvider()
        self.thread = threading.Thread(target=self.provider.serve_forever, daemon=True)
        self.thread.start()
        self.sockets = []

    def tearDown(self):
        for connection in self.sockets:
            connection.close()
        self.provider.shutdown()
        self.provider.server_close()
        self.thread.join(timeout=2)
        self.patches.stop()

    def connect(self):
        connection = socket.create_connection(self.provider.server_address, timeout=1)
        self.sockets.append(connection)
        return connection

    def assert_closed(self, connection):
        try:
            closed = connection.recv(4096) == b""
        except ConnectionResetError:
            closed = True
        except socket.timeout:
            closed = False
        self.assertTrue(closed, "provider left the connection open past its bound")

    def post(self, payload):
        connection = http.client.HTTPConnection(*self.provider.server_address, timeout=1)
        try:
            connection.request("POST", "/v1/responses", json.dumps(payload),
                               {"Content-Type": "application/json"})
            response = connection.getresponse()
            response.read()
            return response.status, response.getheader("Connection")
        finally:
            connection.close()

    def test_idle_connection_is_closed(self):
        self.assert_closed(self.connect())

    def test_incomplete_http_body_is_closed_without_accepting_a_request(self):
        connection = self.connect()
        connection.sendall(b"POST /v1/responses HTTP/1.1\r\nHost: localhost\r\n"
                           b"Content-Length: 100\r\n\r\n{")
        self.assert_closed(connection)
        self.assertEqual(self.provider.request_count, 0)

    def test_body_trickle_cannot_extend_absolute_connection_deadline(self):
        connection = self.connect()
        connection.sendall(b"POST /v1/responses HTTP/1.1\r\nHost: localhost\r\n"
                           b"Content-Length: 100\r\n\r\n{")
        started = time.monotonic()
        def trickle():
            for _ in range(16):
                try:
                    connection.sendall(b" ")
                except OSError:
                    return
                time.sleep(0.08)

        sender = threading.Thread(target=trickle)
        sender.start()
        try:
            self.assert_closed(connection)
            self.assertLess(time.monotonic() - started, 1)
        finally:
            connection.close()
            sender.join(timeout=2)

    def test_early_eof_does_not_accept_valid_json_in_a_short_body(self):
        connection = self.connect()
        connection.sendall(b"POST /v1/responses HTTP/1.1\r\nHost: localhost\r\n"
                           b"Content-Length: 100\r\n\r\n{}")
        connection.shutdown(socket.SHUT_WR)
        self.assertIn(b"400", connection.recv(4096))
        self.assertEqual(self.provider.request_count, 0)

    def test_successful_response_closes_keep_alive_and_preserves_telemetry(self):
        status, connection = self.post({"tools": [{"type": "function", "name": fixture.TOOL_NAME}]})
        self.assertEqual(status, 200)
        self.assertEqual(connection, "close")
        status, _ = self.post({"input": [{"type": "function_call_output", "call_id": fixture.CALL_ID,
                                          "output": "LOCAL_FIXTURE_CALLBACK_ACCEPTED"}]})
        self.assertEqual(status, 200)
        self.assertEqual(self.provider.request_count, 2)
        self.assertTrue(self.provider.second_request_matching_output)
        self.assertEqual(self.provider.output_count, 1)
        self.assertEqual(self.provider.function_tools, [fixture.TOOL_NAME])

    def test_excess_concurrent_connections_are_refused(self):
        # Headers stay incomplete, keeping both admitted handlers occupied.
        with mock.patch.object(fixture, "PROVIDER_IO_TIMEOUT_SECONDS", 2, create=True):
            first, second = self.connect(), self.connect()
            first.sendall(b"POST ")
            second.sendall(b"POST ")
            time.sleep(0.05)
            self.assert_closed(self.connect())

    def test_aggregate_connection_limit_does_not_reset_after_handlers_exit(self):
        for _ in range(4):
            connection = self.connect()
            connection.sendall(b"GET / HTTP/1.0\r\n\r\n")
            self.assertIn(b"501", connection.recv(4096))
            connection.close()
            time.sleep(0.02)
        self.assert_closed(self.connect())

    def test_excess_requests_do_not_expand_accepted_request_telemetry(self):
        self.assertEqual(self.post({})[0], 200)
        self.assertEqual(self.post({})[0], 200)
        self.assertEqual(self.post({})[0], 429)
        self.assertEqual(self.provider.request_count, 2)

    def test_server_close_joins_handlers_and_closes_stalled_connections(self):
        with mock.patch.multiple(fixture, PROVIDER_IO_TIMEOUT_SECONDS=10,
                                 PROVIDER_CONNECTION_SECONDS=10):
            connection = self.connect()
            connection.sendall(b"POST ")
            time.sleep(0.05)
            started = time.monotonic()
            self.provider.shutdown()
            self.provider.server_close()
            self.assert_closed(connection)
            self.assertLess(time.monotonic() - started, 1)
            self.assertFalse(any(worker.is_alive() for worker in self.provider.handler_threads))


class ScriptProvenanceTests(unittest.TestCase):
    def test_failure_evidence_automatically_contains_current_script_digest(self):
        expected = hashlib.sha256(Path(fixture.__file__).read_bytes()).hexdigest()
        output = io.StringIO()
        with tempfile.TemporaryDirectory() as directory, contextlib.redirect_stdout(output):
            destination = Path(directory) / "failure.json"
            self.assertEqual(fixture.main(["--timeout", "1", "--output", str(destination)]), 1)
            evidence = json.loads(destination.read_text())
        self.assertEqual(evidence.get("script_sha256"), expected)
        self.assertEqual(json.loads(output.getvalue()), evidence)


if __name__ == "__main__":
    unittest.main()
