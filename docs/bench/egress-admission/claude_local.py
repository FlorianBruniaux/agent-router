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


class Handler(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_args):
        pass

    def do_POST(self):
        length = int(self.headers.get("Content-Length", "0"))
        if length <= 0 or length > 1024 * 1024:
            self.send_error(413)
            return
        try:
            request = json.loads(self.rfile.read(length))
        except (ValueError, UnicodeError):
            self.send_error(400)
            return
        tools = request.get("tools", [])
        tool_results = []
        for message in request.get("messages", []):
            content = message.get("content", [])
            if isinstance(content, list):
                for block in content:
                    if isinstance(block, dict) and block.get("type") == "tool_result":
                        text = json.dumps(block.get("content", ""))
                        tool_results.append({"is_error": block.get("is_error", False),
                                             "nonce_present": NONCE in text,
                                             "content_excerpt": text[:1200]})
        path = urllib.parse.urlsplit(self.path).path
        self.server.requests.append({"path": path, "model": request.get("model"),
            "tool_names": [tool.get("name") for tool in tools],
            "tool_results": tool_results,
            "synthetic_key_received": self.headers.get("x-api-key") == KEY})
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
        provider_thread = threading.Thread(target=provider.serve_forever, daemon=True)
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
            summary = {"mode": mode, "controller_argv": argv, "environment": env,
                       "controller_profile": controller_profile, "worker_profile": worker_profile.read_text(),
                       "mcp_configuration": json.loads(mcp_config.read_text()), "socket_control": control,
                       "process": result, "provider_requests": provider.requests,
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
    report = {"schema": "agent-router-claude-local-admission-v1", "created_at": dt.datetime.now(dt.timezone.utc).isoformat(),
              "native_binary": str(binary).replace(str(Path.home()), "<home>"), "native_sha256": digest(binary),
              "script_sha256": digest(Path(__file__)), "platform": sys.platform,
              "scope": "local scripted provider, synthetic key, no native admission", "cases": []}
    for mode in cases:
        report["cases"].append(run_case(binary, mode, args.timeout, not args.omit_restricted, args.sample_startup))
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report, indent=2) + "\n")
        case = report["cases"][-1]
        print(json.dumps({"mode": mode, "exit": case["process"]["exit_code"], "termination": case["process"]["termination"],
                          "provider_requests": len(case["provider_requests"]), "nonce_exact": case["nonce_exact"]}), flush=True)


if __name__ == "__main__":
    main()
