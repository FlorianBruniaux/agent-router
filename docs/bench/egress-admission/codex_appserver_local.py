"""Bounded local Codex app-server measurement fixture."""
from __future__ import annotations

import argparse
import hashlib
import http.server
import ipaddress
import json
import os
from pathlib import Path
import queue
import re
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import threading
import time
from typing import Any
from urllib.parse import urlsplit

MODEL = "gpt-5-codex"
TOOL_NAME = "router_probe_tool"
CALL_ID = "call_fixture_1"
NONCE = "AR-CODEX-APP-SERVER-LOCAL-PROBE"
CALLBACK_RESULT = "CALLBACK_RESULT_RETURNED"
MAX_LINE_BYTES = 256 * 1024
MAX_OUTPUT_BYTES = 2 * 1024 * 1024
MAX_REQUEST_BYTES = 1024 * 1024
MAX_TIMEOUT_SECONDS = 120
MAX_VERSION_OUTPUT_BYTES = 4096
VERSION_TIMEOUT_SECONDS = 10
MAX_PROVIDER_HANDLERS = 4
MAX_PROVIDER_CONNECTIONS = 8
MAX_PROVIDER_REQUESTS = 2
PROVIDER_IO_TIMEOUT_SECONDS = 2
PROVIDER_CONNECTION_SECONDS = 5
CLEANUP_TIMEOUT_SECONDS = 3


class FixtureError(RuntimeError):
    pass


def require_loopback(address: str) -> None:
    """Reject a provider listener that could accept non-local connections."""
    try:
        local = ipaddress.ip_address(address).is_loopback
    except ValueError as exc:
        raise ValueError("provider bind address must be loopback") from exc
    if not local:
        raise ValueError("provider bind address must be loopback")


def is_loopback_url(url: str) -> bool:
    parsed = urlsplit(url)
    if parsed.scheme != "http" or not parsed.hostname:
        return False
    try:
        return ipaddress.ip_address(parsed.hostname).is_loopback
    except ValueError:
        return parsed.hostname.lower() == "localhost"


def resolve_codex(binary: str | None) -> tuple[Path, Path]:
    candidate = binary or shutil.which("codex")
    if not candidate:
        raise FixtureError("Codex CLI not found; pass --binary to a verified executable")
    launcher = Path(candidate).expanduser()
    if not launcher.is_absolute():
        found = shutil.which(str(launcher))
        if not found:
            raise FixtureError("configured Codex binary could not be resolved")
        launcher = Path(found)
    try:
        launcher = launcher.absolute()
        verified = launcher.resolve(strict=True)
        info = verified.stat()
    except OSError as exc:
        raise FixtureError("configured Codex binary could not be verified") from exc
    if not verified.is_file() or not os.access(verified, os.X_OK) or info.st_size > 512 * 1024 * 1024:
        raise FixtureError("configured Codex binary is not a verifiable executable file")
    return launcher, verified


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def isolated_env(binary: Path, home: Path) -> dict[str, str]:
    return {
        "HOME": str(home),
        "CODEX_HOME": str(home),
        "XDG_CONFIG_HOME": str(home / "config"),
        "XDG_CACHE_HOME": str(home / "cache"),
        "XDG_DATA_HOME": str(home / "data"),
        "TMPDIR": str(home / "tmp"),
        "PATH": os.pathsep.join([str(binary.parent), "/usr/bin", "/bin", "/usr/sbin", "/sbin"]),
        "OPENAI_API_KEY": "synthetic-local-fixture-key-never-valid",
        "NO_COLOR": "1",
        "RUST_LOG": "error",
    }


def _read_bounded_version_output(process: subprocess.Popen[bytes]) -> bytes:
    output = bytearray()
    overflow = threading.Event()

    def drain() -> None:
        assert process.stdout is not None
        while True:
            remaining = MAX_VERSION_OUTPUT_BYTES + 1 - len(output)
            if remaining <= 0:
                overflow.set()
                return
            chunk = process.stdout.read(min(1024, remaining))
            if not chunk:
                return
            if len(output) + len(chunk) > MAX_VERSION_OUTPUT_BYTES:
                overflow.set()
                return
            output.extend(chunk)

    reader = threading.Thread(target=drain, daemon=True)
    reader.start()
    deadline = time.monotonic() + VERSION_TIMEOUT_SECONDS
    while process.poll() is None:
        if overflow.is_set():
            _kill_group(process)
            reader.join(timeout=1)
            raise FixtureError("Codex CLI version output limit exceeded")
        if time.monotonic() >= deadline:
            _kill_group(process)
            reader.join(timeout=1)
            raise FixtureError("Codex CLI version check timed out")
        time.sleep(0.01)
    reader.join(timeout=1)
    if overflow.is_set():
        _kill_group(process)
        raise FixtureError("Codex CLI version output limit exceeded")
    if reader.is_alive():
        _kill_group(process)
        raise FixtureError("Codex CLI version output reader did not stop")
    if process.returncode != 0:
        raise FixtureError("Codex CLI version could not be verified")
    return bytes(output)


def verify_codex(binary: Path, verified_binary: Path | None = None) -> tuple[str, str]:
    digest = sha256_file(verified_binary or binary.resolve(strict=True))
    try:
        with tempfile.TemporaryDirectory(prefix="ar-codex-version-") as scratch:
            home = Path(scratch)
            (home / "tmp").mkdir(mode=0o700)
            proc = subprocess.Popen(
                [str(binary), "--version"], stdin=subprocess.DEVNULL,
                stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                env=isolated_env(binary, home), cwd=scratch, start_new_session=True,
            )
            try:
                output = _read_bounded_version_output(proc)
            finally:
                if proc.stdout is not None:
                    proc.stdout.close()
    except OSError as exc:
        raise FixtureError("Codex CLI version could not be verified") from exc
    version_line = output.decode("utf-8", "replace").strip()
    match = re.fullmatch(r"codex-cli\s+([0-9]+\.[0-9]+\.[0-9]+)(?:\s+.*)?", version_line)
    if not match:
        raise FixtureError("configured executable did not identify as codex-cli version x.y.z")
    return match.group(1), digest


def has_matching_output(request: dict[str, Any], call_id: str) -> bool:
    items = request.get("input", [])
    return isinstance(items, list) and any(
        isinstance(item, dict)
        and item.get("type") == "function_call_output"
        and item.get("call_id") == call_id
        and item.get("output") == "LOCAL_FIXTURE_CALLBACK_ACCEPTED"
        for item in items
    )


def valid_tool_arguments(arguments: Any) -> bool:
    if isinstance(arguments, str):
        if len(arguments.encode("utf-8", "ignore")) > 4096:
            return False
        try:
            arguments = json.loads(arguments)
        except (ValueError, UnicodeError, RecursionError):
            return False
    return isinstance(arguments, dict) and arguments == {"nonce": NONCE}


def provider_request_matches_call_output(request_index: int, request: dict[str, Any]) -> bool:
    return request_index == 2 and has_matching_output(request, CALL_ID)


def exact_two_request_roundtrip(request_count: int, second_request_matched: bool) -> bool:
    return request_count == 2 and second_request_matched


def valid_callback(callback: Any) -> bool:
    if not isinstance(callback, dict):
        return False
    request_id = callback.get("jsonrpc_id")
    return bool(
        isinstance(request_id, (str, int))
        and not isinstance(request_id, bool)
        and callback.get("tool_name") == TOOL_NAME
        and callback.get("call_id") == CALL_ID
        and callback.get("thread_id")
        and callback.get("turn_id")
    )


def callback_matches(callback: Any, thread_id: str | None, turn_id: str | None) -> bool:
    return (
        valid_callback(callback)
        and callback.get("thread_id") == thread_id
        and callback.get("turn_id") == turn_id
    )


def respond_to_tool_callback(
    client: Any,
    callback: Any,
    arguments: Any,
    thread_id: str | None,
    turn_id: str | None,
    *,
    callback_consumed: bool,
) -> bool:
    if (callback_consumed or not callback_matches(callback, thread_id, turn_id)
            or not valid_tool_arguments(arguments)):
        raise FixtureError("app-server callback refused before response")
    client.send({"id": callback["jsonrpc_id"], "result": {
        "contentItems": [{"type": "inputText", "text": "LOCAL_FIXTURE_CALLBACK_ACCEPTED"}],
        "success": True,
    }})
    return True


def response_sse(index: int, callback_output_seen: bool) -> bytes:
    now = int(time.time())
    if index == 1:
        call = {
            "id": "fc_fixture_1", "type": "function_call", "status": "completed",
            "call_id": CALL_ID, "name": TOOL_NAME,
            "arguments": json.dumps({"nonce": NONCE}, separators=(",", ":")),
        }
        response_id = "resp_fixture_1"
        events = [
            ("response.created", {"id": response_id, "status": "in_progress", "output": []}),
            ("response.output_item.added", {"output_index": 0, "item": {**call, "status": "in_progress", "arguments": ""}}),
            ("response.function_call_arguments.delta", {"item_id": call["id"], "output_index": 0, "delta": call["arguments"]}),
            ("response.function_call_arguments.done", {"item_id": call["id"], "output_index": 0, "arguments": call["arguments"]}),
            ("response.output_item.done", {"output_index": 0, "item": call}),
            ("response.completed", {"response": {"id": response_id, "status": "completed", "output": [call]}}),
        ]
    else:
        text = CALLBACK_RESULT if callback_output_seen else "CALLBACK_RESULT_MISSING"
        item = {
            "id": "msg_fixture_2", "type": "message", "status": "completed", "role": "assistant",
            "content": [{"type": "output_text", "text": text, "annotations": []}],
        }
        response_id = "resp_fixture_2"
        events = [
            ("response.created", {"id": response_id, "status": "in_progress", "output": []}),
            ("response.output_item.added", {"output_index": 0, "item": {**item, "status": "in_progress", "content": [{"type": "output_text", "text": "", "annotations": []}]}}),
            ("response.output_text.delta", {"item_id": item["id"], "output_index": 0, "content_index": 0, "delta": text}),
            ("response.output_text.done", {"item_id": item["id"], "output_index": 0, "content_index": 0, "text": text}),
            ("response.output_item.done", {"output_index": 0, "item": item}),
            ("response.completed", {"response": {"id": response_id, "status": "completed", "output": [item]}}),
        ]
    frames = []
    for name, payload in events:
        event = {"type": name, **payload}
        if name in ("response.created", "response.completed"):
            response = event.pop("response", None)
            if response is None:
                response = {"id": response_id, "status": "in_progress", "output": []}
            response.update({"object": "response", "created_at": now, "model": MODEL,
                             "usage": {"input_tokens": 1, "output_tokens": 1, "total_tokens": 2}})
            event["response"] = response
        frames.append(f"event: {name}\ndata: {json.dumps(event, separators=(',', ':'))}\n\n")
    return "".join(frames).encode()


class LocalProvider(http.server.HTTPServer):
    """Bound both concurrent work and total work for this one-roundtrip fixture."""

    def __init__(self, address: str = "127.0.0.1"):
        require_loopback(address)
        self.lock = threading.Lock()
        self.handler_threads: list[threading.Thread] = []
        self.connections: set[socket.socket] = set()
        self.closing = False
        self.accepted_connections = 0
        self.rejected_connections = 0
        self.rejected_requests = 0
        self.request_count = 0
        self.function_tools: list[str] = []
        self.output_count = 0
        self.matching_output_seen = False
        self.second_request_matching_output = False
        super().__init__((address, 0), ProviderHandler)
        require_loopback(str(self.server_address[0]))

    def process_request(self, request: socket.socket, client_address: Any) -> None:
        with self.lock:
            if (self.closing or len(self.connections) >= MAX_PROVIDER_HANDLERS
                    or self.accepted_connections >= MAX_PROVIDER_CONNECTIONS):
                self.rejected_connections += 1
                self.shutdown_request(request)
                return
            request.settimeout(PROVIDER_IO_TIMEOUT_SECONDS)
            self.accepted_connections += 1
            self.connections.add(request)
            worker = threading.Thread(target=self._handle_connection,
                                      args=(request, client_address), daemon=True)
            self.handler_threads.append(worker)
            try:
                worker.start()
            except RuntimeError:
                self.connections.discard(request)
                self.shutdown_request(request)
                raise FixtureError("provider handler could not start")

    @staticmethod
    def _interrupt_connection(request: socket.socket) -> None:
        # shutdown wakes a handler blocked in either read or write; close alone
        # does not reliably interrupt another thread's socket operation.
        try:
            request.shutdown(socket.SHUT_RDWR)
        except OSError:
            pass

    def _handle_connection(self, request: socket.socket, client_address: Any) -> None:
        deadline = threading.Timer(PROVIDER_CONNECTION_SECONDS, self._interrupt_connection, (request,))
        deadline.daemon = True
        try:
            deadline.start()
            self.finish_request(request, client_address)
        except OSError:
            # Expected on deadline, peer disconnect or shutdown; no raw request logs.
            pass
        finally:
            deadline.cancel()
            if deadline.ident is not None:
                deadline.join(timeout=CLEANUP_TIMEOUT_SECONDS)
            self.shutdown_request(request)
            with self.lock:
                self.connections.discard(request)

    def server_close(self) -> None:
        with self.lock:
            self.closing = True
            for connection in self.connections:
                self._interrupt_connection(connection)
            workers = list(self.handler_threads)
        super().server_close()
        deadline = time.monotonic() + CLEANUP_TIMEOUT_SECONDS
        for worker in workers:
            if worker.ident is not None:
                worker.join(timeout=max(0, deadline - time.monotonic()))
        if any(worker.is_alive() for worker in workers):
            raise FixtureError("provider handlers did not stop")


class ProviderHandler(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *_args: Any) -> None:
        pass

    def do_POST(self) -> None:
        self.close_connection = True
        if urlsplit(self.path).path != "/v1/responses":
            self.send_error(404)
            return
        try:
            length = int(self.headers.get("Content-Length", "0"))
        except ValueError:
            self.send_error(400)
            return
        if length <= 0 or length > MAX_REQUEST_BYTES:
            self.send_error(413)
            return
        try:
            body = self.rfile.read(length)
            if len(body) != length:
                self.send_error(400)
                return
            request = json.loads(body)
        except (ValueError, UnicodeError):
            self.send_error(400)
            return
        if not isinstance(request, dict):
            self.send_error(400)
            return
        with self.server.lock:
            if self.server.request_count >= MAX_PROVIDER_REQUESTS:
                self.server.rejected_requests += 1
                self.send_error(429)
                return
            self.server.request_count += 1
            index = self.server.request_count
            if index == 1 and isinstance(request.get("tools"), list):
                self.server.function_tools = [
                    tool["name"] for tool in request["tools"]
                    if isinstance(tool, dict) and tool.get("type") == "function" and isinstance(tool.get("name"), str)
                ]
            matched = has_matching_output(request, CALL_ID)
            second_request_matched = provider_request_matches_call_output(index, request)
            self.server.output_count += sum(
                isinstance(item, dict) and item.get("type") == "function_call_output"
                for item in request.get("input", [])
            )
            self.server.matching_output_seen |= matched
            if index == 2:
                self.server.second_request_matching_output = second_request_matched
        payload = response_sse(index, second_request_matched)
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Cache-Control", "no-cache")
        self.send_header("Connection", "close")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)
        self.wfile.flush()


class AppServer:
    def __init__(self, process: subprocess.Popen[bytes]):
        self.process = process
        self.lines: queue.Queue[bytes | None] = queue.Queue()
        self.output_size = 0
        self.stderr_size = 0
        self.exceeded = threading.Event()
        self.stdout_thread = threading.Thread(target=self._read_stdout, daemon=True)
        self.stderr_thread = threading.Thread(target=self._read_stderr, daemon=True)
        self.stdout_thread.start()
        self.stderr_thread.start()

    def _read_stdout(self) -> None:
        assert self.process.stdout is not None
        while True:
            line = self.process.stdout.readline(MAX_LINE_BYTES + 1)
            if not line:
                self.lines.put(None)
                return
            self.output_size += len(line)
            if len(line) > MAX_LINE_BYTES or self.output_size > MAX_OUTPUT_BYTES:
                self.exceeded.set()
                self.lines.put(None)
                return
            self.lines.put(line)

    def _read_stderr(self) -> None:
        assert self.process.stderr is not None
        while True:
            chunk = self.process.stderr.read(8192)
            if not chunk:
                return
            self.stderr_size += len(chunk)
            if self.stderr_size > MAX_OUTPUT_BYTES:
                self.exceeded.set()

    def send(self, message: dict[str, Any]) -> None:
        if self.process.poll() is not None or self.process.stdin is None:
            raise FixtureError("app-server exited during protocol exchange")
        encoded = json.dumps(message, separators=(",", ":")).encode() + b"\n"
        if len(encoded) > MAX_LINE_BYTES:
            raise FixtureError("app-server request exceeded the protocol limit")
        try:
            self.process.stdin.write(encoded)
            self.process.stdin.flush()
        except (BrokenPipeError, OSError) as exc:
            raise FixtureError("app-server closed its protocol input") from exc

    def receive(self, deadline: float) -> dict[str, Any]:
        if self.exceeded.is_set():
            raise FixtureError("app-server output limit exceeded")
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise FixtureError("app-server protocol timed out")
        try:
            line = self.lines.get(timeout=remaining)
        except queue.Empty as exc:
            raise FixtureError("app-server protocol timed out") from exc
        if line is None:
            raise FixtureError("app-server exited or exceeded output limits")
        try:
            message = json.loads(line)
        except (ValueError, UnicodeError) as exc:
            raise FixtureError("app-server emitted invalid protocol data") from exc
        if not isinstance(message, dict):
            raise FixtureError("app-server emitted invalid protocol data")
        return message

    def stop(self) -> int | None:
        if self.process.stdin is not None:
            try:
                self.process.stdin.close()
            except OSError:
                pass
        try:
            self.process.wait(timeout=CLEANUP_TIMEOUT_SECONDS)
        except subprocess.TimeoutExpired:
            pass
        # A successful leader exit does not imply its process group is empty.
        _kill_group(self.process)
        deadline = time.monotonic() + CLEANUP_TIMEOUT_SECONDS
        for reader in (self.stdout_thread, self.stderr_thread):
            reader.join(timeout=max(0, deadline - time.monotonic()))
        if self.process.poll() is None or any(
            reader.is_alive() for reader in (self.stdout_thread, self.stderr_thread)
        ):
            raise FixtureError("app-server processes or output readers did not stop")
        # EOF and a reaped leader do not prove that descendants have finished
        # processing SIGKILL. Preserve the same cleanup deadline while observing
        # the whole original group, including descendants which closed pipes.
        _wait_group_stopped(self.process.pid, deadline)
        for stream in (self.process.stdout, self.process.stderr):
            if stream is not None:
                stream.close()

        return self.process.returncode


def _group_stopped(group_pid: int, deadline: float) -> bool:
    try:
        os.killpg(group_pid, 0)
    except ProcessLookupError:
        return True
    except PermissionError:
        return False
    if sys.platform != "linux":
        return False
    # Linux can retain killed orphan zombies until PID 1 reaps them. This
    # fixture requires a fully observable same-user group (no hidden /proc
    # members). Live, stopped or unreadable members never count as cleanup.
    found = False
    try:
        with os.scandir("/proc") as entries:
            for count, entry in enumerate(entries):
                if count >= 65536 or time.monotonic() >= deadline:
                    return False
                if not entry.name.isdecimal():
                    continue
                try:
                    fields = (Path(entry) / "stat").read_text(encoding="utf-8").rsplit(")", 1)[1].split()
                except FileNotFoundError:
                    continue
                if int(fields[2]) == group_pid:
                    found = True
                    if fields[0] != "Z":
                        return False
    except (OSError, ValueError, IndexError, UnicodeError):
        return False
    if found:
        return True
    # All members might have been reaped during the scan. An existing group
    # with no visible member remains unproven (for example, restricted /proc).
    try:
        os.killpg(group_pid, 0)
    except ProcessLookupError:
        return True
    except PermissionError:
        pass
    return False


def _wait_group_stopped(group_pid: int, deadline: float) -> None:
    while not _group_stopped(group_pid, deadline):
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise FixtureError("app-server descendant cleanup could not be verified")
        time.sleep(min(0.01, remaining))


def _kill_group(process: subprocess.Popen[bytes]) -> None:
    try:
        os.killpg(process.pid, signal.SIGKILL)
    except (ProcessLookupError, PermissionError):
        try:
            process.kill()
        except ProcessLookupError:
            pass
    try:
        process.wait(timeout=3)
    except subprocess.TimeoutExpired:
        pass


def _response_for(client: AppServer, request_id: int, deadline: float) -> dict[str, Any]:
    while True:
        message = client.receive(deadline)
        if message.get("id") == request_id:
            if "error" in message or "result" not in message:
                raise FixtureError("app-server rejected a protocol request")
            return message


def run_turn(client: AppServer, workspace: Path, timeout: int) -> dict[str, Any]:
    deadline = time.monotonic() + timeout
    client.send({"id": 1, "method": "initialize", "params": {
        "clientInfo": {"name": "agent-router-local-fixture", "title": "Agent Router local fixture", "version": "1.0.0"},
        "capabilities": {"experimentalApi": True},
    }})
    _response_for(client, 1, deadline)
    client.send({"method": "initialized"})
    client.send({"id": 2, "method": "thread/start", "params": {
        "model": MODEL, "modelProvider": "local_fixture", "cwd": str(workspace),
        "ephemeral": True, "approvalPolicy": "never", "sandbox": "read-only",
        "dynamicTools": [{"type": "function", "name": TOOL_NAME,
            "description": "Return a fixed local fixture result.",
            "inputSchema": {"type": "object", "properties": {"nonce": {"type": "string"}},
                "required": ["nonce"], "additionalProperties": False}}],
    }})
    start = _response_for(client, 2, deadline)
    thread_id = start.get("result", {}).get("thread", {}).get("id")
    if not isinstance(thread_id, str):
        raise FixtureError("app-server did not start an ephemeral thread")
    client.send({"id": 3, "method": "turn/start", "params": {
        "threadId": thread_id,
        "input": [{"type": "text", "text": f"Call {TOOL_NAME} with nonce {NONCE}, then report the result."}],
    }})
    turn_start = _response_for(client, 3, deadline)
    active_turn = turn_start.get("result", {}).get("turn", {})
    active_turn_id = active_turn.get("id") if isinstance(active_turn, dict) else None
    if not isinstance(active_turn_id, str) or not active_turn_id:
        raise FixtureError("app-server did not start a correlated turn")
    callback = None
    callback_response_sent = False
    callback_consumed = False
    completed = None
    while completed is None:
        message = client.receive(deadline)
        if message.get("method") == "item/tool/call":
            params = message.get("params", {})
            arguments = params.get("arguments") if isinstance(params, dict) else None
            callback = {
                "jsonrpc_id": message.get("id"),
                "thread_id": params.get("threadId") if isinstance(params, dict) else None,
                "turn_id": params.get("turnId") if isinstance(params, dict) else None,
                "call_id": params.get("callId") if isinstance(params, dict) else None,
                "tool_name": params.get("tool") if isinstance(params, dict) else None,
                "arguments_valid": valid_tool_arguments(arguments),
            }
            callback_response_sent = respond_to_tool_callback(
                client, callback, arguments, thread_id, active_turn_id,
                callback_consumed=callback_consumed,
            )
            callback_consumed = True
        if message.get("method") == "turn/completed":
            completed = message.get("params", {}).get("turn", {})
    status = completed.get("status") if isinstance(completed, dict) else None
    turn_id = completed.get("id") if isinstance(completed, dict) else None
    final_items = completed.get("items", []) if isinstance(completed, dict) else []
    final_seen = any(
        isinstance(item, dict) and CALLBACK_RESULT in str(item.get("text", ""))
        for item in final_items
    )
    return {"thread_id": thread_id, "turn_id": turn_id, "turn_status": status,
            "callback": callback, "callback_response_sent": callback_response_sent,
            "final_callback_confirmation": final_seen}


def run_fixture(binary: str | None, timeout: int) -> dict[str, Any]:
    if timeout < 5 or timeout > MAX_TIMEOUT_SECONDS:
        raise FixtureError("timeout must be between 5 and 120 seconds")
    executable, verified_executable = resolve_codex(binary)
    version, digest = verify_codex(executable, verified_executable)
    started_at = time.monotonic()
    evidence: dict[str, Any] = {
        "script_sha256": sha256_file(Path(__file__).resolve()),
        "codex_version": version,
        "codex_binary_sha256": digest,
        "provider_bind_address": None,
        "provider_url_loopback": False,
        "provider_request_count": 0,
        "provider_accepted_connections": 0,
        "provider_rejected_connections": 0,
        "provider_rejected_requests": 0,
        "advertised_function_tool_names": [],
        "target_dynamic_tool_advertised": False,
        "callback_correlation": None,
        "callback_response_sent": False,
        "next_provider_request_has_matching_function_call_output": False,
        "function_call_output_count": 0,
        "turn_status": None,
        "final_callback_confirmation": False,
        "elapsed_ms": None,
        "os_network_sandbox_applied": False,
        "os_network_sandbox_status": "NOT_APPLIED",
        "provider_spend": "USD 0, scripted loopback provider",
        "synthetic_key_only": True,
        "success": False,
    }
    provider = None
    provider_thread = None
    client = None
    process = None
    turn = None
    failure = None
    scratch = None
    cleanup_complete = True
    try:
        scratch = Path(tempfile.mkdtemp(prefix="ar-codex-appserver-local-"))
        root = scratch
        home = root / "home"
        workspace = root / "workspace"
        for directory in (home, workspace, home / "tmp", home / "config", home / "cache", home / "data"):
            directory.mkdir(mode=0o700, parents=True, exist_ok=True)
        try:
            provider = LocalProvider("127.0.0.1")
            provider_thread = threading.Thread(target=provider.serve_forever, daemon=True)
            provider_thread.start()
            host, port = provider.server_address[:2]
            require_loopback(str(host))
            base_url = f"http://{host}:{port}/v1"
            if not is_loopback_url(base_url):
                raise FixtureError("provider URL is not loopback")
            evidence["provider_bind_address"] = str(host)
            evidence["provider_url_loopback"] = True
            config = f'''model = "{MODEL}"
model_provider = "local_fixture"
approval_policy = "never"
sandbox_mode = "read-only"
web_search = "disabled"

[features]
shell_tool = false
unified_exec = false
apps = false

[model_providers.local_fixture]
name = "Local scripted fixture"
base_url = "{base_url}"
wire_api = "responses"
requires_openai_auth = false
supports_websockets = false
'''
            (home / "config.toml").write_text(config, encoding="utf-8")
            env = isolated_env(executable, home)
            process = subprocess.Popen(
                [str(executable), "app-server", "--listen", "stdio://"],
                stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                cwd=workspace, env=env, start_new_session=True,
            )
            client = AppServer(process)
            turn = run_turn(client, workspace, timeout)
        except FixtureError as exc:
            failure = str(exc)
        except (OSError, KeyError, TypeError, ValueError, json.JSONDecodeError):
            failure = "local fixture failed before evidence was complete"
        finally:
            # Keep the isolated home/workspace alive until all children and the
            # loopback server have stopped using them.
            cleanup_errors = []
            try:
                if client is not None:
                    evidence["app_server_exit_code"] = client.stop()
                elif process is not None:
                    _kill_group(process)
                    evidence["app_server_exit_code"] = process.returncode
            except (FixtureError, OSError):
                cleanup_errors.append("app-server cleanup incomplete")
            if provider is not None:
                if provider_thread is not None and provider_thread.ident is not None:
                    provider.shutdown()
                try:
                    provider.server_close()
                except (FixtureError, OSError):
                    cleanup_errors.append("provider handler cleanup incomplete")
                if provider_thread is not None and provider_thread.ident is not None:
                    provider_thread.join(timeout=CLEANUP_TIMEOUT_SECONDS)
                    if provider_thread.is_alive():
                        cleanup_errors.append("provider listener cleanup incomplete")
                with provider.lock:
                    evidence["provider_request_count"] = provider.request_count
                    evidence["provider_accepted_connections"] = provider.accepted_connections
                    evidence["provider_rejected_connections"] = provider.rejected_connections
                    evidence["provider_rejected_requests"] = provider.rejected_requests
                    evidence["advertised_function_tool_names"] = provider.function_tools
                    evidence["target_dynamic_tool_advertised"] = TOOL_NAME in provider.function_tools
                    evidence["next_provider_request_has_matching_function_call_output"] = (
                        provider.request_count == 2 and provider.second_request_matching_output
                    )
                    evidence["function_call_output_count"] = provider.output_count
            if cleanup_errors:
                cleanup_complete = False
                raise FixtureError("; ".join(cleanup_errors))
            if turn is not None:
                evidence["callback_correlation"] = turn["callback"]
                evidence["callback_response_sent"] = turn["callback_response_sent"]
                evidence["turn_status"] = turn["turn_status"]
                evidence["final_callback_confirmation"] = turn["final_callback_confirmation"]
                evidence["thread_id"] = turn["thread_id"]
                evidence["turn_id"] = turn["turn_id"]
    except FixtureError as exc:
        failure = str(exc)
    except OSError:
        failure = "local fixture failed before evidence was complete"
    finally:
        if scratch is not None and cleanup_complete:
            shutil.rmtree(scratch)
        evidence["cleanup_complete"] = cleanup_complete
        evidence["elapsed_ms"] = round((time.monotonic() - started_at) * 1000)

    callback_ok = callback_matches(
        evidence.get("callback_correlation"), evidence.get("thread_id"), evidence.get("turn_id")
    )
    evidence["success"] = bool(
        failure is None
        and evidence["provider_url_loopback"]
        and evidence["provider_rejected_connections"] == 0
        and evidence["provider_rejected_requests"] == 0
        and evidence["target_dynamic_tool_advertised"]
        and callback_ok
        and bool(evidence.get("callback_correlation", {}).get("arguments_valid"))
        and evidence["callback_response_sent"]
        and exact_two_request_roundtrip(
            evidence["provider_request_count"],
            evidence["next_provider_request_has_matching_function_call_output"],
        )
        and evidence["turn_status"] == "completed"
        and evidence["final_callback_confirmation"]
    )
    if failure:
        evidence["failure"] = failure
    elif not evidence["success"]:
        evidence["failure"] = "local callback roundtrip evidence was incomplete"
    return evidence


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", help="Codex CLI executable to verify; defaults to codex on PATH")
    parser.add_argument("--timeout", type=int, default=45, help="protocol timeout in seconds, from 5 to 120")
    parser.add_argument("--output", type=Path, help="optional path for sanitized JSON evidence")
    args = parser.parse_args(argv)
    try:
        result = run_fixture(args.binary, args.timeout)
    except FixtureError as exc:
        result = {"success": False, "failure": str(exc),
                  "script_sha256": sha256_file(Path(__file__).resolve()),
                  "os_network_sandbox_applied": False,
                  "os_network_sandbox_status": "NOT_APPLIED", "provider_spend": "USD 0, no provider run"}
    output = json.dumps(result, indent=2, sort_keys=True) + "\n"
    if args.output:
        try:
            args.output.parent.mkdir(parents=True, exist_ok=True)
            args.output.write_text(output, encoding="utf-8")
        except OSError:
            print("could not write sanitized evidence", file=sys.stderr)
            return 2
    print(output, end="")
    return 0 if result.get("success") else 1


if __name__ == "__main__":
    raise SystemExit(main())
