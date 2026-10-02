#!/usr/bin/env python3
"""Offline native Claude/MCP admission investigation. No remote provider calls."""
from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import http.server
import json
import os
from pathlib import Path
import re
import signal
import socket
import stat
import subprocess
import sys
import tempfile
import threading
import time
import urllib.parse

HERE = Path(__file__).resolve().parent
KEY = "synthetic-admission-key-never-valid"
MODEL = "claude-haiku-4-5"
NONCE = "adr0022-local-mcp-nonce-v1"
MAX_PROVIDER_BODY_BYTES = 1024 * 1024
MAX_PROVIDER_RECORDS = 100
BASE = '''(version 1)
(deny default)
(allow process-exec process-fork sysctl-read mach-lookup)
(allow signal (target self))
(allow file-read-metadata)
(allow file-read* (literal "/") (subpath "/bin") (subpath "/sbin") (subpath "/usr") (subpath "/System") (subpath "/Library") (literal "/dev/null") (literal "/dev/urandom"))
(allow file-write* (literal "/dev/null"))
'''


def quote(value: Path | str) -> str:
    text = str(value)
    if not text.startswith("/") or any(c in text for c in "\x00\r\n"):
        raise ValueError("unsafe profile path")
    return json.dumps(text)


def profile(reads: list[Path], writes: list[Path], port: int | None) -> str:
    text = BASE
    for path in reads:
        text += f"(allow file-read* (literal {quote(path)}) (subpath {quote(path)}))\n"
    for path in writes:
        text += f"(allow file-write* (literal {quote(path)}) (subpath {quote(path)}))\n"
    if port is not None:
        text += f'(allow network-outbound (remote ip "localhost:{port}"))\n'
    return text


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def clean(value: str, root: Path) -> str:
    value = value.replace(KEY, "<synthetic-key>").replace(str(root), "<scratch>")
    value = value.replace(str(Path.home()), "<home>")
    return re.sub(r"/(?:private/)?var/folders/[^/]+/[^/]+/T/", "<system-temp>/", value)


def read_artifact(path: Path, limit: int, *, excerpt: bool = False, tail: bool = False) -> tuple[bytes | None, str]:
    """Read child output through a pinned regular-file FD with a byte ceiling.

    The artifact parent is a direct child of the supervisor-owned scratch root.
    Refuse a replaced parent as well as a final-component symlink. Nonblocking
    open permits rejecting FIFOs before reading instead of waiting for a writer.
    """
    directory_fd = file_fd = None
    try:
        directory_fd = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_NONBLOCK)
        file_fd = os.open(path.name, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=directory_fd)
        info = os.fstat(file_fd)
        if not stat.S_ISREG(info.st_mode):
            return None, "refused"
        if not excerpt and info.st_size > limit:
            return None, "too_large"
        if tail:
            os.lseek(file_fd, max(0, info.st_size - limit), os.SEEK_SET)
        remaining = limit if excerpt else limit + 1
        data = bytearray()
        while remaining:
            chunk = os.read(file_fd, min(remaining, 8192))
            if not chunk:
                break
            data.extend(chunk)
            remaining -= len(chunk)
        if not excerpt and len(data) > limit:
            return None, "too_large"
        return bytes(data), "truncated" if excerpt and info.st_size > limit else "ok"
    except FileNotFoundError:
        return None, "missing"
    except OSError:
        return None, "refused"
    finally:
        if file_fd is not None:
            os.close(file_fd)
        if directory_fd is not None:
            os.close(directory_fd)


def collect_artifacts(state: Path, workspace: Path, supervisor: Path) -> dict:
    debug, debug_status = read_artifact(state / "debug.log", 20000, excerpt=True, tail=True)
    sample, sample_status = read_artifact(supervisor / "startup-sample.txt", 24000, excerpt=True)
    nonce, nonce_status = read_artifact(workspace / "nonce.txt", len(NONCE.encode()))
    evidence, evidence_status = read_artifact(workspace / "worker-evidence.json", 16384)
    worker_evidence = None
    if evidence is not None:
        try:
            worker_evidence = json.loads(evidence)
            if not isinstance(worker_evidence, dict):
                worker_evidence = None
                evidence_status = "invalid"
        except (ValueError, UnicodeError, RecursionError):
            evidence_status = "invalid"
    return {"debug_excerpt": debug.decode(errors="replace") if debug is not None else None,
            "sample_excerpt": sample.decode(errors="replace") if sample is not None else None,
            "nonce_exact": nonce == NONCE.encode(), "worker_evidence": worker_evidence,
            "artifact_reads": {"debug": debug_status, "sample": sample_status,
                               "nonce": nonce_status, "worker_evidence": evidence_status}}


def bounded(argv: list[str], env: dict[str, str], cwd: Path, timeout: float, sample: bool = False, input_bytes: bytes | None = None, sample_dir: Path | None = None) -> dict:
    if sample and sample_dir is None:
        raise ValueError("sampling requires a supervisor-owned output directory")
    with tempfile.TemporaryFile() as out, tempfile.TemporaryFile() as err:
        proc = subprocess.Popen(argv, stdin=subprocess.PIPE if input_bytes else subprocess.DEVNULL, stdout=out, stderr=err,
                                env=env, cwd=cwd, start_new_session=True)
        if input_bytes:
            proc.stdin.write(input_bytes)
            proc.stdin.close()
        started = time.monotonic()
        reason = "exit"
        sampled = False
        try:
            while proc.poll() is None:
                if sample and not sampled and time.monotonic() - started > 3:
                    sampled = True
                    subprocess.run(["/usr/bin/sample", str(proc.pid), "1", "-file", str(sample_dir / "startup-sample.txt")],
                                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=5)
                if time.monotonic() - started >= timeout:
                    reason = "timeout"
                    break
                if os.fstat(out.fileno()).st_size + os.fstat(err.fileno()).st_size > 2 * 1024 * 1024:
                    reason = "output_limit"
                    break
                time.sleep(0.05)
        finally:
            # Also kill descendants after a normal root-process exit.
            try:
                os.killpg(proc.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            proc.wait(timeout=5)
        out.seek(0)
        err.seek(0)
        return {"exit_code": proc.returncode, "termination": reason,
                "elapsed_ms": round((time.monotonic() - started) * 1000),
                "stdout": out.read(2 * 1024 * 1024).decode(errors="replace"),
                "stderr": err.read(2 * 1024 * 1024).decode(errors="replace")}


class Provider(http.server.ThreadingHTTPServer):
    daemon_threads = True

    def __init__(self, mode: str, workspace: Path):
        super().__init__(("127.0.0.1", 0), Handler)
        self.mode = mode
        self.workspace = workspace
        self.requests: list[dict] = []
        self.messages = 0
        self._observation_lock = threading.Lock()
        self._accepted_tcp_connections = 0
        self._active_connections = 0
        self._http_response_events: list[dict] = []
        self._http_response_events_total = 0
        self._http_response_events_dropped = 0
        self._valid_json_posts = 0
        self._valid_json_post_records_dropped = 0

    def get_request(self):
        request, client_address = super().get_request()
        with self._observation_lock:
            self._accepted_tcp_connections += 1
            self._active_connections += 1
        return request, client_address

    def process_request_thread(self, request, client_address):
        try:
            super().process_request_thread(request, client_address)
        finally:
            with self._observation_lock:
                self._active_connections -= 1

    def record_http_response(self, event: dict):
        with self._observation_lock:
            self._http_response_events_total += 1
            if len(self._http_response_events) < MAX_PROVIDER_RECORDS:
                self._http_response_events.append(event)
            else:
                self._http_response_events_dropped += 1

    def record_provider_request(self, request: dict):
        with self._observation_lock:
            self._valid_json_posts += 1
            if len(self.requests) < MAX_PROVIDER_RECORDS:
                self.requests.append(request)
            else:
                self._valid_json_post_records_dropped += 1

    def wait_for_connections(self, count: int, timeout: float) -> bool:
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            with self._observation_lock:
                if self._accepted_tcp_connections >= count:
                    return True
            time.sleep(0.01)
        with self._observation_lock:
            return self._accepted_tcp_connections >= count

    def wait_until_settled(self, timeout: float = 2, quiet_period: float = 0.15) -> bool:
        deadline = time.monotonic() + timeout
        stable_since = None
        previous_count = None
        while time.monotonic() < deadline:
            with self._observation_lock:
                count = self._accepted_tcp_connections
                active = self._active_connections
            if active == 0 and count == previous_count:
                if stable_since is None:
                    stable_since = time.monotonic()
                elif time.monotonic() - stable_since >= quiet_period:
                    return True
            else:
                stable_since = None
            previous_count = count
            time.sleep(0.01)
        return False

    def reset_observation(self):
        with self._observation_lock:
            if self._active_connections:
                raise RuntimeError("cannot reset provider telemetry while a connection is active")
            self._accepted_tcp_connections = 0
            self._http_response_events = []
            self._http_response_events_total = 0
            self._http_response_events_dropped = 0
            self._valid_json_posts = 0
            self._valid_json_post_records_dropped = 0
            self.requests = []

    def observation(self) -> dict:
        with self._observation_lock:
            return {"accepted_tcp_connections": self._accepted_tcp_connections,
                    "http_response_events_total": self._http_response_events_total,
                    "http_response_events": list(self._http_response_events),
                    "http_response_events_dropped": self._http_response_events_dropped,
                    "valid_json_posts": self._valid_json_posts,
                    "valid_json_post_records": list(self.requests),
                    "valid_json_post_records_dropped": self._valid_json_post_records_dropped}


class Handler(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def __init__(self, *args, **kwargs):
        self._telemetry_outcome = "http_error"
        self._content_length_bytes = None
        super().__init__(*args, **kwargs)

    def handle_one_request(self):
        self._telemetry_outcome = "http_error"
        self._content_length_bytes = None
        return super().handle_one_request()

    def log_message(self, *_args):
        pass

    @staticmethod
    def _safe_method(method):
        return method if method in {"GET", "POST", "HEAD", "OPTIONS", "PUT", "DELETE", "PATCH"} else "<other>"

    @staticmethod
    def _safe_path(path):
        try:
            path = urllib.parse.urlsplit(path).path
        except ValueError:
            return "<other>"
        if path in {"/v1/messages", "/v1/count_tokens"}:
            return path
        if path == "/":
            return "<root>"
        if path.startswith("/v1/"):
            return "/v1/<other>"
        segments = path.strip("/").split("/")
        safe_first_segments = {"api", "health", "healthz", "ready", "readyz", "status", "info", ".well-known"}
        if segments[0] in safe_first_segments:
            return f"/{segments[0]}" if len(segments) == 1 else f"/{segments[0]}/<other>"
        return "<other>"

    def send_response(self, code, message=None):
        self.server.record_http_response({"method": self._safe_method(getattr(self, "command", None)),
            "path": self._safe_path(getattr(self, "path", "")), "status": code,
            "content_length_bytes": self._content_length_bytes, "outcome": self._telemetry_outcome})
        super().send_response(code, message)

    def do_POST(self):
        content_length = self.headers.get("Content-Length")
        try:
            length = int(content_length) if content_length is not None else 0
        except ValueError:
            self._telemetry_outcome = "invalid_content_length"
            self.send_error(400)
            return
        self._content_length_bytes = length if length >= 0 else None
        if length <= 0:
            self._telemetry_outcome = "empty_body"
            self.send_error(400)
            return
        if length > MAX_PROVIDER_BODY_BYTES:
            self._telemetry_outcome = "body_too_large"
            self.send_error(413)
            return
        try:
            body = self.rfile.read(length)
            if len(body) != length:
                self._telemetry_outcome = "truncated_body"
                self.send_error(400)
                return
            request = json.loads(body)
        except (ValueError, UnicodeError, RecursionError):
            self._telemetry_outcome = "invalid_json"
            self.send_error(400)
            return
        if not isinstance(request, dict):
            self._telemetry_outcome = "invalid_json"
            self.send_error(400)
            return
        self._telemetry_outcome = "valid_json"
        tools = request.get("tools", [])
        if not isinstance(tools, list):
            tools = []
        messages = request.get("messages", [])
        if not isinstance(messages, list):
            messages = []
        tool_results = []
        for message in messages[:100]:
            if not isinstance(message, dict):
                continue
            content = message.get("content", [])
            if isinstance(content, list):
                for block in content:
                    if isinstance(block, dict) and block.get("type") == "tool_result":
                        result_text = json.dumps(block.get("content", ""))
                        tool_results.append({"is_error": block.get("is_error") is True,
                                             "nonce_present": NONCE in result_text,
                                             "content_bytes": len(result_text.encode())})
                        if len(tool_results) == 32:
                            break
                if len(tool_results) == 32:
                    break
        path = urllib.parse.urlsplit(self.path).path
        tool_names = []
        for tool in tools[:32]:
            if not isinstance(tool, dict):
                continue
            name = tool.get("name")
            if isinstance(name, str) and re.fullmatch(r"[A-Za-z0-9_.:-]{1,80}", name):
                tool_names.append(name)
            else:
                tool_names.append("<other>")
        self.server.record_provider_request({"path": path if path in {"/v1/messages", "/v1/count_tokens"} else "<other>",
            "model": "expected" if request.get("model") == MODEL else "other",
            "tool_count": len(tools), "tool_names": tool_names, "tool_results": tool_results,
            "synthetic_key_received": self.headers.get("x-api-key") == KEY,
            "content_length_bytes": length})
        if path.endswith("/count_tokens"):
            data = b'{"input_tokens":1}'
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)
            return
        if path != "/v1/messages":
            self.send_error(404)
            return
        self.server.messages += 1
        first = self.server.messages == 1
        tool = None
        if first and self.server.mode == "forbidden_builtin":
            tool = {"name": "Bash", "input": {"command": "/usr/bin/touch " + str(self.server.workspace / "forbidden-builtin")}}
        elif first and self.server.mode == "mcp_nonce":
            tool = {"name": "mcp__admission__write_nonce", "input": {"nonce": NONCE}}
        frames = [{"type": "message_start", "message": {"id": "msg_local_fixture",
                   "type": "message", "role": "assistant", "content": [],
                   "model": MODEL, "stop_reason": None, "stop_sequence": None,
                   "usage": {"input_tokens": 1, "output_tokens": 0}}}]
        if tool:
            frames += [{"type": "content_block_start", "index": 0,
                        "content_block": {"type": "tool_use", "id": "toolu_local_fixture",
                                          "name": tool["name"], "input": {}}},
                       {"type": "content_block_delta", "index": 0,
                        "delta": {"type": "input_json_delta", "partial_json": json.dumps(tool["input"])}}]
        else:
            frames += [{"type": "content_block_start", "index": 0,
                        "content_block": {"type": "text", "text": ""}},
                       {"type": "content_block_delta", "index": 0,
                        "delta": {"type": "text_delta", "text": "Local fixture complete."}}]
        frames += [{"type": "content_block_stop", "index": 0},
                   {"type": "message_delta", "delta": {"stop_reason": "tool_use" if tool else "end_turn",
                    "stop_sequence": None}, "usage": {"output_tokens": 5}}, {"type": "message_stop"}]
        data = "".join("event: " + frame["type"] + "\ndata: " + json.dumps(frame) + "\n\n" for frame in frames).encode()
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        try:
            self.wfile.write(data)
        except (BrokenPipeError, ConnectionResetError):
            pass


def worker(args):
    workspace = Path(args.workspace)
    for line in sys.stdin:
        request = json.loads(line)
        if "id" not in request:
            continue
        method = request.get("method")
        if method == "initialize":
            result = {"protocolVersion": request["params"]["protocolVersion"],
                      "capabilities": {"tools": {}}, "serverInfo": {"name": "admission-fixture", "version": "1"}}
        elif method == "tools/list":
            result = {"tools": [{"name": "write_nonce", "description": "Write the supplied nonce in the fixture workspace.",
                      "inputSchema": {"type": "object", "properties": {"nonce": {"type": "string"}},
                                      "required": ["nonce"], "additionalProperties": False}}]}
        elif method == "tools/call":
            nonce = request["params"].get("arguments", {}).get("nonce")
            if request["params"].get("name") != "write_nonce" or nonce != NONCE:
                result = {"isError": True, "content": [{"type": "text", "text": "fixture input refused"}]}
            else:
                evidence = {"nonce": nonce, "credential_in_environment": any(k in os.environ for k in ["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN"])}
                for name, port in [("provider", args.provider_port), ("other", args.other_port)]:
                    try:
                        with socket.create_connection(("127.0.0.1", port), timeout=1):
                            evidence[name + "_connect"] = "allowed"
                    except OSError as exc:
                        evidence[name + "_connect"] = "denied"
                        evidence[name + "_errno"] = exc.errno
                try:
                    Path(args.private_canary).read_bytes()
                    evidence["controller_state_read"] = "allowed"
                except OSError as exc:
                    evidence["controller_state_read"] = "denied"
                    evidence["controller_state_errno"] = exc.errno
                (workspace / "nonce.txt").write_text(nonce)
                (workspace / "worker-evidence.json").write_text(json.dumps(evidence, indent=2) + "\n")
                result = {"content": [{"type": "text", "text": json.dumps(evidence)}]}
        else:
            result = {}
        print(json.dumps({"jsonrpc": "2.0", "id": request["id"], "result": result}), flush=True)


def validate_socket_control(control: dict, provider_port: int, other_port: int) -> dict:
    clean_exit = control.get("exit_code") == 0 and control.get("termination") == "exit"
    lines = set(control.get("stdout", "").splitlines())
    provider_allowed = clean_exit and f"{provider_port} allowed" in lines
    other_denied = clean_exit and any(line == f"{other_port} denied" or
                                      line.startswith(f"{other_port} denied ") for line in lines)
    return {"clean_exit": clean_exit, "provider_allowed": provider_allowed,
            "other_port_denied": other_denied}


def settled_observation(provider: Provider, timeout: float = 2) -> dict:
    if not provider.wait_until_settled(timeout=timeout):
        raise RuntimeError("provider observation did not settle after child termination")
    return provider.observation()


def run_case(binary: Path, mode: str, timeout: float, restricted: bool = True, sample: bool = False) -> dict:
    with tempfile.TemporaryDirectory(prefix="ar-claude-admission-", dir="/private/tmp") as scratch:
        root = Path(scratch).resolve()
        home, state, workspace, workerhome = [root / name for name in ["home", "state", "workspace", "worker-home"]]
        for path in [home, state, workspace, workerhome]:
            path.mkdir(mode=0o700)
        # Neither child profile grants writes here or to its parent. The child
        # must not choose the path a privileged diagnostic process will write.
        supervisor = root / "supervisor"
        supervisor.mkdir(mode=0o700)
        private = state / "private-canary"
        private.write_text("synthetic-controller-private-state")
        provider = Provider(mode, workspace)
        other = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        other.daemon_threads = True
        provider_thread = threading.Thread(target=lambda: provider.serve_forever(poll_interval=0.05), daemon=True)
        provider_thread.start()
        # The second listener is never served: connect success is enough to fail.
        port, otherport = provider.server_port, other.server_port
        worker_profile = root / "worker.sb"
        worker_profile.write_text(profile([HERE, workspace, workerhome], [workspace, workerhome], None))
        settings = root / "settings.json"
        settings.write_text("{}")
        mcp_config = root / "mcp.json"
        command = ["-i", "PATH=/usr/bin:/bin", "HOME=" + str(workerhome), "PYTHONDONTWRITEBYTECODE=1",
                   "/usr/bin/sandbox-exec", "-f", str(worker_profile), "/usr/bin/python3", "-B", str(HERE / "claude_local.py"),
                   "--mcp-worker", "--workspace", str(workspace), "--provider-port", str(port),
                   "--other-port", str(otherport), "--private-canary", str(private)]
        mcp_config.write_text(json.dumps({"mcpServers": {"admission": {"command": "/usr/bin/env", "args": command}}}
                                         if mode == "mcp_nonce" else {"mcpServers": {}}))
        controller_profile = profile([binary, HERE, root], [home, state, workspace, workerhome], port)
        env = {"PATH": "/usr/bin:/bin", "HOME": str(home), "CLAUDE_CONFIG_DIR": str(state),
               "TMPDIR": str(state), "CLAUDE_CODE_TMPDIR": str(state),
               "ANTHROPIC_API_KEY": KEY, "ANTHROPIC_BASE_URL": f"http://127.0.0.1:{port}",
               "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC": "1", "DISABLE_AUTOUPDATER": "1"}
        argv = [str(binary), "--bare", "--print", "--output-format", "stream-json", "--verbose",
                "--debug-file", str(state / "debug.log"),
                "--model", MODEL, "--tools", "", "--strict-mcp-config", "--mcp-config", str(mcp_config),
                "--setting-sources", "", "--settings", str(settings), "--permission-prompts", "none",
                "--no-session-persistence", "--max-turns", "3"]
        if restricted:
            argv.append("--restricted")
        if mode == "mcp_nonce":
            argv += ["--allowedTools", "mcp__admission__write_nonce"]
        argv += ["Run the local fixture instruction and finish."]
        try:
            # Prove the profile permits precisely its local endpoint before CLI use.
            probe = "import socket,sys\nfor p in sys.argv[1:]:\n try:\n  s=socket.create_connection(('127.0.0.1',int(p)),1);s.close();print(p,'allowed')\n except OSError as e: print(p,'denied',e.errno)"
            control = bounded(["/usr/bin/sandbox-exec", "-p", controller_profile, "/usr/bin/python3", "-c", probe, str(port), str(otherport)], env, workspace, 5)
            # Keep the positive preflight socket test separate from child-run
            # evidence. The listener count lets us wait until accept() ran.
            socket_control = validate_socket_control(control, port, otherport)
            if not all(socket_control.values()):
                raise RuntimeError("socket preflight did not prove allowed and denied controls; refusing to launch case")
            provider_preflight_connected = socket_control["provider_allowed"]
            preflight_accept_observed = provider.wait_for_connections(1, 2)
            if not preflight_accept_observed or not provider.wait_until_settled():
                raise RuntimeError("provider preflight telemetry did not settle; refusing to reset counters")
            provider.reset_observation()
            if mode in {"mcp_protocol_control", "mcp_standalone_control"}:
                requests = [{"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2024-11-05"}},
                            {"jsonrpc": "2.0", "id": 2, "method": "tools/list"},
                            {"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "write_nonce", "arguments": {"nonce": NONCE}}}]
                argv = ["/usr/bin/env"] + command
                launch = argv if mode == "mcp_standalone_control" else ["/usr/bin/sandbox-exec", "-p", controller_profile] + argv
                result = bounded(launch, env, workspace, timeout,
                                 input_bytes=("\n".join(json.dumps(r) for r in requests) + "\n").encode())
            else:
                result = bounded(["/usr/bin/sandbox-exec", "-p", controller_profile] + argv, env, workspace, timeout,
                                 sample=sample, sample_dir=supervisor)
            provider_observation = settled_observation(provider)
            provider_observation_settled = True
            summary = {"mode": mode, "controller_argv": argv, "environment": env,
                       "controller_profile": controller_profile, "worker_profile": worker_profile.read_text(),
                       "mcp_configuration": json.loads(mcp_config.read_text()), "socket_control": control,
                       "provider_preflight_connected": provider_preflight_connected,
                       "provider_preflight_accept_observed": preflight_accept_observed,
                       "process": result, "provider_requests": provider_observation["valid_json_post_records"],
                       "provider_observation": provider_observation,
                       "provider_observation_settled": provider_observation_settled,
                       **collect_artifacts(state, workspace, supervisor),
                       "forbidden_builtin_created": (workspace / "forbidden-builtin").exists(),
                       }
            return json.loads(clean(json.dumps(summary), root))
        finally:
            provider.shutdown()
            provider.server_close()
            other.server_close()
            provider_thread.join(timeout=2)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", default=str(Path.home() / ".local/bin/claude"))
    parser.add_argument("--output", type=Path)
    parser.add_argument("--timeout", type=float, default=25)
    parser.add_argument("--case", choices=["inventory", "forbidden_builtin", "mcp_nonce", "mcp_protocol_control", "mcp_standalone_control", "all"], default="all")
    parser.add_argument("--omit-restricted", action="store_true")
    parser.add_argument("--sample-startup", action="store_true")
    parser.add_argument("--mcp-worker", action="store_true")
    parser.add_argument("--workspace")
    parser.add_argument("--provider-port", type=int)
    parser.add_argument("--other-port", type=int)
    parser.add_argument("--private-canary")
    args = parser.parse_args()
    if args.mcp_worker:
        worker(args)
        return
    if sys.platform != "darwin" or not args.output:
        parser.error("macOS and --output required")
    binary = Path(args.binary).resolve(strict=True)
    cases = ["inventory", "forbidden_builtin", "mcp_nonce"] if args.case == "all" else [args.case]
    report = {"schema": "agent-router-claude-local-admission-v2", "created_at": dt.datetime.now(dt.timezone.utc).isoformat(),
              "native_binary": str(binary).replace(str(Path.home()), "<home>"), "native_sha256": digest(binary),
              "script_sha256": digest(Path(__file__)), "platform": sys.platform,
              "scope": "local scripted provider, synthetic key, no native admission", "cases": []}
    for mode in cases:
        report["cases"].append(run_case(binary, mode, args.timeout, not args.omit_restricted, args.sample_startup))
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report, indent=2) + "\n")
        case = report["cases"][-1]
        observation = case["provider_observation"]
        print(json.dumps({"mode": mode, "exit": case["process"]["exit_code"], "termination": case["process"]["termination"],
                          "provider_tcp_connections": observation["accepted_tcp_connections"],
                          "provider_http_response_events": observation["http_response_events_total"],
                          "provider_valid_json_posts": observation["valid_json_posts"],
                          "nonce_exact": case["nonce_exact"]}), flush=True)


if __name__ == "__main__":
    main()
