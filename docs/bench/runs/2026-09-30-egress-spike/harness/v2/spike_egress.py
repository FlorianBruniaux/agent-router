#!/usr/bin/env python3
"""ADR 0021, batch 4: provider egress spike, v2 (private host configuration).

Measures, per host (Codex, then Claude Code), whether a native run launched the
way `router-runtime` launches it under the `network: false` Seatbelt profile
reaches its provider.

Replicated from agent-router at f263e7d:
- profile text: crates/router-runtime/src/sandbox.rs:428-451 (checked against
  the copy `sandbox.rs.f263e7d` at start, see `profile_template_matches`);
- quoting: sandbox.rs:375-384; wrapper: sandbox.rs:362-374;
- reads = read_roots + cwd + program, env_clear + PATH=/usr/bin:/bin + named
  variables: sandbox.rs:284-322; allowed names include HOME, CODEX_HOME and
  CLAUDE_CONFIG_DIR: sandbox.rs:466-476;
- network flag = required capabilities contain Network: lib.rs:1521;
- argv: crates/adapter-codex/src/execution.rs:31-50,
  crates/adapter-claude/src/execution.rs:34-58 (ToolSelection::Disabled);
- root assistant message: router-adapter-api/src/execution.rs:738-747, :810-845.
Not replicated: the resource helper (rlimits), the approval digest, the router
prompt template.

Change from v1 (run 20260930T170911Z, Codex exit 1 `fs_denied` under a profile
without writes): each host gets a throwaway 0700 directory under the run, named
by CODEX_HOME or CLAUDE_CONFIG_DIR. It is the only writable root of the profile
and does not exist in the runtime's read-only profile at f263e7d. Codex gets a
0600 copy of `auth.json` there, deleted in a `finally`. Claude gets nothing.

Per host:
  S0 `--version`, S1 local auth status: network: false, no model call.
     A failure stops this host only (cause `startup`); the next host still runs.
  C1 program, network: false   (the measurement)
  C2 program, network: true    (control: same profile, one variable changed)
  C3 private copy, network: true (only when C2 completed)
No retry, at most three model calls. Kept: metrics, stderr categories and a
300-byte stderr excerpt with home paths and token-like strings redacted. Never
kept: model text, `auth.json` content.
"""
from __future__ import annotations

import argparse
import datetime as dt
import glob
import hashlib
import json
import os
import re
import shutil
import signal
import socket
import stat
import subprocess
import sys
import tempfile
import time
from pathlib import Path

SCHEMA = "agent-router-egress-spike-v2"
HERE = Path(__file__).resolve().parent
SANDBOX_EXEC = "/usr/bin/sandbox-exec"
CHILD_PATH = "/usr/bin:/bin"
STDOUT_CAP = 8 * 1024 * 1024
STDERR_CAP = 1024 * 1024
EXCERPT_BYTES = 300
PROMPT = "Reply with exactly the word OK and nothing else.\n"

# sandbox.rs:430 at f263e7d, character for character.
PROFILE_BASE = (
    "(version 1)\n(deny default)\n(allow process-exec process-fork sysctl-read mach-lookup)\n"
    "(allow signal (target self))\n(allow file-read-metadata)\n"
    '(allow file-read* (literal "/") (subpath "/bin") (subpath "/sbin") (subpath "/usr") '
    '(subpath "/System") (subpath "/Library") (literal "/dev/null") (literal "/dev/urandom") '
    '(literal "/private/etc/resolv.conf") (literal "/private/etc/hosts"))\n'
    '(allow file-write* (literal "/dev/null"))\n'
)
GIT_DENY = '(deny file-write* (regex #"(^|/)\\.[Gg][Ii][Tt](/|$)"))\n'
NETWORK_ALLOW = "(allow network*)\n"

STDERR_CATEGORIES = [
    ("launcher_missing", r"env: node|no such file or directory|exec format error|bad interpreter"),
    ("net_denied", r"(connect|socket|sendto|network)[^\n]{0,80}(operation not permitted|os error 1\b|eperm)"
                   r"|(operation not permitted|eperm)[^\n]{0,80}(connect|socket)"),
    ("net_resolve", r"could not resolve|failed to lookup address|getaddrinfo|enotfound|nodename nor servname|dns"),
    ("net_connect", r"connection refused|econnrefused|network is unreachable|error sending request"
                    r"|stream disconnected|connection reset|econnreset|unable to connect|fetch failed"),
    ("net_timeout", r"timed out|etimedout|deadline"),
    ("fs_denied", r"read-only file system|erofs|eacces|permission denied|operation not permitted|eperm"),
    ("auth", r"\b401\b|unauthori[sz]ed|not logged in|please log in|/login|authenticat|invalid api key|credential"),
    ("keychain", r"keychain|secitem|security find-generic-password|errsec"),
    ("rate_limit", r"\b429\b|rate limit|quota"),
    ("sandbox", r"sandbox"),
    ("model", r"model[^\n]{0,60}(not found|not supported|unavailable|does not exist)|unsupported value"),
    ("effort", r"effort"),
]

TOKEN_PATTERNS = [
    r"eyJ[A-Za-z0-9_\-]{8,}\.[A-Za-z0-9_\-]{8,}(\.[A-Za-z0-9_\-]{4,})?",   # JWT
    r"\b(sk|rk|pk|sess|ghp|gho|xox[abpr])[-_][A-Za-z0-9_\-]{8,}",          # prefixed keys
    r"(?i)(bearer|token|api[_-]?key|authorization|secret|password)([\"'\s:=]+)[^\s\"',;]{6,}",
    r"[\w.+-]+@[\w-]+\.[\w.-]+",                                           # e-mail addresses
]
# A path segment is kept; a long run mixing letters and digits is treated as a token.
OPAQUE = re.compile(r"[A-Za-z0-9+_=\-]{24,}")


class SpikeError(Exception):
    pass


# ---------------------------------------------------------------- profile

def quote(path: str) -> str:
    """sandbox.rs:375-384."""
    if not path.startswith("/") or any(c in path for c in "\0\n\r"):
        raise SpikeError("unsafe path for profile")
    return '"' + path.replace("\\", "\\\\").replace('"', '\\"') + '"'


def render_profile(reads: list[str], writes: list[str], network: bool) -> str:
    """sandbox.rs:428-451."""
    profile = PROFILE_BASE
    for path in reads:
        q = quote(path)
        profile += f"(allow file-read* (literal {q}) (subpath {q}))\n"
    for path in writes:
        q = quote(path)
        profile += f"(allow file-write* (literal {q}) (subpath {q}))\n"
    profile += GIT_DENY
    if network:
        profile += NETWORK_ALLOW
    return profile


def rust_literal(text: str) -> str:
    out, i = [], 0
    while i < len(text):
        c = text[i]
        if c == "\\" and i + 1 < len(text):
            n = text[i + 1]
            out.append({"n": "\n", '"': '"', "\\": "\\"}.get(n, "\\" + n))
            i += 2
        else:
            out.append(c)
            i += 1
    return "".join(out)


def profile_template_matches(source: Path) -> dict:
    """Compare the replicated constants with the Rust source they copy."""
    text = source.read_text()
    fn = text[text.index("fn seatbelt_profile("):]
    fn = fn[: fn.index("\n}\n")]
    base = re.search(r'String::from\(\s*"((?:[^"\\]|\\.)*)",\s*\)', fn, re.S)
    deny = re.search(r'profile\.push_str\("(\(deny file-write\*(?:[^"\\]|\\.)*)"\)', fn)
    net = re.search(r'profile\.push_str\("(\(allow network\*\)(?:[^"\\]|\\.)*)"\)', fn)
    names = re.search(r'"OPENAI_API_KEY"(.*?)\) \|\|', text, re.S)
    checks = {
        "base": bool(base) and rust_literal(base.group(1)) == PROFILE_BASE,
        "git_deny": bool(deny) and rust_literal(deny.group(1)) == GIT_DENY,
        "network": bool(net) and rust_literal(net.group(1)) == NETWORK_ALLOW,
        "read_line": '"(allow file-read* (literal {0}) (subpath {0}))\\n"' in fn,
        "write_line": '"(allow file-write* (literal {0}) (subpath {0}))\\n"' in fn,
        "env_path": '.env("PATH", "/usr/bin:/bin")' in text,
        "env_names": bool(names) and all(f'"{n}"' in names.group(1)
                                         for n in ("CODEX_HOME", "CLAUDE_CONFIG_DIR", "HOME")),
    }
    return {"source_sha256": sha256_file(source), "checks": checks, "ok": all(checks.values())}


# ---------------------------------------------------------------- helpers

def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def canonical(path: str) -> str:
    real = os.path.realpath(path)
    if not os.path.exists(real):
        raise SpikeError(f"missing path: {Path(path).name}")
    return real


def describe_binary(path: str) -> dict:
    head = Path(path).read_bytes()[:256]
    shebang = None
    if head.startswith(b"#!"):
        shebang = head.split(b"\n", 1)[0][2:].decode("utf-8", "replace").strip()
    try:
        kind = subprocess.run(["/usr/bin/file", "-b", path], capture_output=True, text=True,
                              timeout=10).stdout.strip()[:200]
    except (OSError, subprocess.TimeoutExpired):
        kind = None
    st = os.stat(path)
    return {
        "path": path,
        "sha256": sha256_file(Path(path)),
        "size_bytes": st.st_size,
        "file": kind,
        "shebang": shebang,
        "kind": "script" if shebang else ("mach-o" if kind and "Mach-O" in kind else "other"),
    }


def classify_text(text: str) -> list[str]:
    low = text.lower()
    return [name for name, pattern in STDERR_CATEGORIES if re.search(pattern, low)]


def make_redactor(replacements: dict[str, str]):
    """Paths first (longest first), then token-like strings."""
    ordered = sorted(((k, v) for k, v in replacements.items() if k), key=lambda kv: -len(kv[0]))

    def redact(text: str) -> str:
        for needle, label in ordered:
            text = text.replace(needle, label)
        text = re.sub(r"/Users/[^/\s\"']+", "/Users/<user>", text)
        for pattern in TOKEN_PATTERNS:
            text = re.sub(pattern, "<redacted>", text)
        return OPAQUE.sub(lambda m: "<redacted>" if re.search(r"\d", m.group())
                          and re.search(r"[A-Za-z]", m.group()) else m.group(), text)

    return redact


def excerpt(text: str, redact) -> str:
    data = redact(text).encode("utf-8")[:EXCERPT_BYTES]
    return data.decode("utf-8", "ignore")


def safe_version(stdout: bytes) -> str | None:
    text = stdout.decode("utf-8", "replace").strip()
    if len(text) <= 120 and re.fullmatch(r"[A-Za-z0-9 ._()+\-/:]*", text):
        return text
    return None


# ---------------------------------------------------------------- process

def launch(cmd: list[str], cwd: str, env: dict, stdin: bytes, timeout: float, redact) -> dict:
    started = time.time()
    out_f = tempfile.TemporaryFile()
    err_f = tempfile.TemporaryFile()
    proc = subprocess.Popen(cmd, cwd=cwd, env=env, stdin=subprocess.PIPE, stdout=out_f,
                            stderr=err_f, start_new_session=True)
    timed_out = False
    try:
        try:
            proc.stdin.write(stdin)
            proc.stdin.close()
        except BrokenPipeError:
            pass
        proc.wait(timeout=timeout)
    except subprocess.TimeoutExpired:
        timed_out = True
    finally:
        if proc.poll() is None:
            for sig in (signal.SIGTERM, signal.SIGKILL):
                try:
                    os.killpg(proc.pid, sig)
                except ProcessLookupError:
                    break
                try:
                    proc.wait(timeout=5)
                    break
                except subprocess.TimeoutExpired:
                    continue
    elapsed_ms = int((time.time() - started) * 1000)
    out_f.seek(0)
    err_f.seek(0)
    stdout = out_f.read(STDOUT_CAP + 1)
    stderr = err_f.read(STDERR_CAP + 1)
    text = stderr.decode("utf-8", "replace")
    rc = proc.returncode
    return {
        "exit_code": rc if rc is not None and rc >= 0 else None,
        "signal": -rc if rc is not None and rc < 0 else None,
        "timed_out": timed_out,
        "termination": "timeout" if timed_out else ("signal" if rc is not None and rc < 0 else "exited"),
        "elapsed_ms": elapsed_ms,
        "stdout_bytes": len(stdout),
        "stdout_truncated": len(stdout) > STDOUT_CAP,
        "stderr_bytes": len(stderr),
        "stdout_sha256": sha256_bytes(stdout),
        "stderr_sha256": sha256_bytes(stderr),
        "stderr_categories": classify_text(text),
        "stderr_excerpt": excerpt(text, redact),
        "_stdout": stdout,
    }


def parse_stream(dialect: str, stdout: bytes) -> dict:
    """Metrics only; model text never leaves this function."""
    types: dict[str, int] = {}
    root_assistant = False
    models: list[str] = []
    usage = None
    error_categories: set[str] = set()
    result = None
    bad_lines = 0
    lines = [l for l in stdout.split(b"\n") if l.strip()]
    for line in lines:
        try:
            value = json.loads(line)
        except ValueError:
            bad_lines += 1
            continue
        if not isinstance(value, dict) or not isinstance(value.get("type"), str):
            bad_lines += 1
            continue
        kind = value["type"]
        types[kind] = types.get(kind, 0) + 1
        if dialect == "codex":
            if kind == "item.completed" and (value.get("item") or {}).get("type") == "agent_message":
                root_assistant = True
            elif kind == "turn.completed" and isinstance(value.get("usage"), dict):
                usage = {k: v for k, v in value["usage"].items() if isinstance(v, int)}
            elif kind in ("turn.failed", "error"):
                error_categories.update(classify_text(json.dumps(value)))
        else:
            if kind == "assistant":
                msg = value.get("message") or {}
                root = all(value.get(k) is None for k in ("parent_tool_use_id", "agentId", "subagent_type"))
                if root and msg.get("role") == "assistant" and msg.get("type") == "message":
                    root_assistant = True
                    model = msg.get("model")
                    if isinstance(model, str) and model not in models and len(model) < 128:
                        models.append(model)
            elif kind == "result":
                u = value.get("usage") or {}
                usage = {k: v for k, v in u.items() if isinstance(v, int)}
                cc = u.get("cache_creation")
                if isinstance(cc, dict):
                    usage.update({f"cache_creation.{k}": v for k, v in cc.items() if isinstance(v, int)})
                result = {
                    "subtype": value.get("subtype") if isinstance(value.get("subtype"), str) else None,
                    "is_error": value.get("is_error") if isinstance(value.get("is_error"), bool) else None,
                    "total_cost_usd": value.get("total_cost_usd")
                    if isinstance(value.get("total_cost_usd"), (int, float)) else None,
                }
                if value.get("is_error"):
                    error_categories.update(classify_text(json.dumps(value)))
    return {
        "json_lines": len(lines) - bad_lines,
        "non_json_lines": bad_lines,
        "event_types": types,
        "root_assistant_message": root_assistant,
        "observed_models": models,
        "usage": usage,
        "result": result,
        "error_event_categories": sorted(error_categories),
    }


# ---------------------------------------------------------------- cost

def load_pricing(path: Path, writes_path: Path) -> dict:
    table = json.loads(path.read_text())["models"]
    writes = json.loads(writes_path.read_text())["models"]
    return {"models": table, "writes": writes,
            "sha256": {"pricing": sha256_file(path), "cache_writes": sha256_file(writes_path)}}


def list_cost_microusd(dialect: str, model: str, usage: dict | None, pricing: dict) -> int | None:
    if not usage:
        return None
    price = pricing["models"].get(model)
    if price is None:
        return None
    per = lambda tokens, rate: tokens * rate / 1_000_000
    if dialect == "codex":
        total = usage.get("input_tokens", 0)
        cached = usage.get("cached_input_tokens", 0)
        cost = (per(total - cached, price["input_microusd_per_million"])
                + per(cached, price["cached_input_microusd_per_million"])
                + per(usage.get("output_tokens", 0), price["output_microusd_per_million"]))
        return round(cost)
    write = pricing["writes"].get(model)
    if write is None:
        return None
    created = usage.get("cache_creation_input_tokens", 0)
    w5 = usage.get("cache_creation.ephemeral_5m_input_tokens")
    w1 = usage.get("cache_creation.ephemeral_1h_input_tokens")
    if w5 is None or w1 is None:
        w5, w1 = 0, created  # unknown split: price every write at the 1h rate (upper bound)
    cost = (per(usage.get("input_tokens", 0), price["input_microusd_per_million"])
            + per(usage.get("cache_read_input_tokens", 0), price["cached_input_microusd_per_million"])
            + per(w5, write["write_5m"]) + per(w1, write["write_1h"])
            + per(usage.get("output_tokens", 0), price["output_microusd_per_million"]))
    return round(cost)


# ---------------------------------------------------------------- hosts

def codex_native(entry: str) -> str | None:
    """The executable codex.js spawns (bin/codex.js:78-112 in @openai/codex)."""
    if not entry.endswith("/bin/codex.js"):
        return None
    root = Path(entry).parent.parent
    found = sorted(glob.glob(str(root / "node_modules/@openai/codex-darwin-*/vendor/*-apple-darwin/bin/codex")))
    return canonical(found[0]) if found else None


def host_spec(name: str, args) -> dict:
    home = canonical(os.environ["HOME"])
    if name == "codex":
        entry = canonical(args.codex_entry or shutil.which("codex") or "")
        program = canonical(args.codex_program) if args.codex_program else codex_native(entry)
        if not program:
            raise SpikeError("codex native executable not found; pass --codex-program")
        auth = args.codex_auth or os.path.join(os.environ.get("CODEX_HOME") or os.path.join(home, ".codex"),
                                               "auth.json")
        argv = ["exec", "--model", args.codex_model, "--config",
                f"model_reasoning_effort={json.dumps(args.effort)}", "--sandbox", "read-only",
                "--json", "--ephemeral", "--color", "never", "--cd", "{workspace}", "-"]
        return {"host": "codex", "dialect": "codex", "entry": entry, "program": program,
                "model": args.codex_model, "argv": argv, "status_argv": ["login", "status"],
                "config_var": "CODEX_HOME", "auth_source": auth, "read_roots": [], "home": home}
    entry = canonical(args.claude_entry or shutil.which("claude") or "")
    program = canonical(args.claude_program) if args.claude_program else entry
    argv = ["--print", "--output-format", "stream-json", "--verbose", "--input-format", "text",
            "--model", args.claude_model, "--effort", args.effort, "--permission-mode", "plan",
            "--permission-prompts", "none", "--no-session-persistence", "--tools", ""]
    reads = []
    if args.claude_keychain_read_root:
        reads.append(canonical(os.path.join(home, "Library", "Keychains")))
    return {"host": "claude", "dialect": "claude", "entry": entry, "program": program,
            "model": args.claude_model, "argv": argv, "status_argv": ["auth", "status"],
            "config_var": "CLAUDE_CONFIG_DIR", "auth_source": None, "read_roots": reads, "home": home}


def make_workspace(root: Path) -> str:
    ws = root / "workspace"
    ws.mkdir(parents=True)
    (ws / "README.md").write_text("egress spike workspace\n")
    git = ["/usr/bin/git", "-c", "user.name=spike", "-c", "user.email=spike@invalid",
           "-c", "commit.gpgsign=false"]
    for cmd in (["init", "-q"], ["add", "README.md"], ["commit", "-q", "-m", "spike"]):
        subprocess.run(git + cmd, cwd=ws, check=True, capture_output=True, timeout=30)
    return canonical(str(ws))


def copy_secret(source: str, target: Path) -> None:
    """Copy bytes without logging them; the target is created 0600, never pre-existing."""
    fd = os.open(target, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(fd, "wb") as out, open(source, "rb") as src:
        shutil.copyfileobj(src, out)


def stat_signature(path: Path) -> list | None:
    try:
        st = os.stat(path)
    except FileNotFoundError:
        return None
    return [st.st_size, st.st_mtime_ns]


def private_copy(program: str, root: Path) -> tuple[str, dict]:
    directory = root / "private-bin"
    directory.mkdir(mode=0o700)
    target = directory / Path(program).name
    shutil.copyfile(program, target)
    os.chmod(target, stat.S_IRUSR | stat.S_IXUSR)
    os.chmod(directory, stat.S_IRWXU)
    copy_hash = sha256_file(target)
    return canonical(str(target)), {"sha256": copy_hash, "matches_original": copy_hash == sha256_file(Path(program))}


def command(program: str, argv: list[str], profile: str, wrapper: str) -> list[str]:
    if wrapper == "none":
        return [program, *argv]
    return [SANDBOX_EXEC, "-p", profile, program, *argv]


def delete_auth(record: dict, auth: Path) -> None:
    log = record["private_config"]["auth_json"]
    if not log["copied"]:
        return
    original = log.pop("_signature", None)
    current = stat_signature(auth)
    # Size and mtime only: the content is never read. A change means the host
    # rewrote its credentials (for example a token refresh) in the private copy.
    log["absent_before_delete"] = current is None
    log["signature_changed_by_host"] = current is not None and current != original
    try:
        auth.unlink()
        log["deleted"] = True
    except FileNotFoundError:
        log["deleted"] = "already_absent"
    except OSError as error:
        log["deleted"] = False
        log["delete_error"] = type(error).__name__
    log["deleted_at"] = dt.datetime.now(dt.timezone.utc).isoformat(timespec="seconds")
    log["present_after_delete"] = auth.exists()


# ---------------------------------------------------------------- preflight

def preflight(wrapper: str, scratch: Path) -> dict:
    """Same shape as the runtime's own probe (sandbox.rs:174-265)."""
    listener = socket.socket()
    listener.bind(("127.0.0.1", 0))
    listener.listen(4)
    port = str(listener.getsockname()[1])
    denied = scratch / "preflight-denied"
    checks = {}
    try:
        control = subprocess.run(["/usr/bin/nc", "-z", "-w", "1", "127.0.0.1", port],
                                 capture_output=True, timeout=10, env={"PATH": CHILD_PATH})
        checks["control_nc_reaches_loopback"] = control.returncode == 0
        for network in (False, True):
            prof = render_profile([], [], network)
            run = subprocess.run(command("/usr/bin/nc", ["-z", "-w", "1", "127.0.0.1", port], prof, wrapper),
                                 capture_output=True, timeout=10, env={"PATH": CHILD_PATH})
            checks[f"nc_network_{str(network).lower()}_exit"] = run.returncode
            if run.returncode == 71 and b"sandbox_apply" in run.stderr:
                checks["sandbox_apply_refused"] = True
        prof = render_profile([], [], False)
        subprocess.run(command("/usr/bin/touch", [str(denied)], prof, wrapper),
                       capture_output=True, timeout=10, env={"PATH": CHILD_PATH})
        checks["write_outside_roots_denied"] = not denied.exists()
    finally:
        listener.close()
        if denied.exists():
            denied.unlink()
    effective = (checks.get("control_nc_reaches_loopback") is True
                 and checks.get("nc_network_false_exit") not in (0, None)
                 and checks.get("nc_network_true_exit") == 0
                 and checks.get("write_outside_roots_denied") is True
                 and not checks.get("sandbox_apply_refused"))
    return {"wrapper": wrapper, "checks": checks, "sandbox_effective": effective}


# ---------------------------------------------------------------- spike

def run_host(spec: dict, root: Path, args, pricing: dict) -> dict:
    root.mkdir(parents=True)
    config = root / "config"
    config.mkdir(mode=0o700)
    os.chmod(config, stat.S_IRWXU)
    config_dir = canonical(str(config))
    auth = config / "auth.json"
    record = {
        "host": spec["host"],
        "model_requested": spec["model"],
        "entry": describe_binary(spec["entry"]),
        "program": describe_binary(spec["program"]),
        "private_config": {"variable": spec["config_var"], "path": config_dir, "mode": "0700",
                           "auth_json": {"copied": False}},
        "read_roots": spec["read_roots"],
        "write_roots": [config_dir],
        "launches": [],
        "calls": 0,
        "list_cost_microusd": 0,
        "list_cost_complete": True,
    }
    redact = make_redactor({str(root.parent): "<run>", config_dir: "<config>", spec["home"]: "~"})
    try:
        if spec["auth_source"]:
            if not os.path.isfile(spec["auth_source"]):
                raise SpikeError("codex auth.json not found")
            copy_secret(spec["auth_source"], auth)
            record["private_config"]["auth_json"].update(
                {"copied": True, "mode": "0600", "_signature": stat_signature(auth)})
        workspace = make_workspace(root)
        argv = [workspace if a == "{workspace}" else a for a in spec["argv"]]
        env = {"PATH": CHILD_PATH, "HOME": spec["home"], spec["config_var"]: config_dir}
        record["argv"] = [redact(a) for a in argv]
        record["environment_names"] = sorted(env)
        reads = spec["read_roots"] + [config_dir, workspace]

        def profile_for(program: str, network: bool) -> str:
            return render_profile(reads + [program], [config_dir], network)

        def local(label: str, program: str, extra: list[str]) -> dict:
            res = launch(command(program, extra, profile_for(program, False), args.wrapper),
                         workspace, env, b"", args.version_timeout, redact)
            out = res.pop("_stdout")
            res.update({"label": label, "kind": "local", "argv": extra, "network": False,
                        "stdout_excerpt": excerpt(out.decode("utf-8", "replace"), redact)
                        if extra != ["--version"] else None,
                        "version": safe_version(out) if extra == ["--version"] and res["exit_code"] == 0
                        else None})
            record["launches"].append(res)
            return res

        def model_call(label: str, program: str, network: bool) -> dict:
            if record["calls"] >= 3:
                raise SpikeError("call budget exhausted")
            if record["list_cost_microusd"] >= args.cap_usd * 1_000_000:
                res = {"label": label, "kind": "model", "skipped": "cost cap reached"}
                record["launches"].append(res)
                return res
            prof = profile_for(program, network)
            record["calls"] += 1
            call_env = dict(env)
            if args.fake:
                # Test seam: without Seatbelt a double cannot observe the profile.
                call_env["SPIKE_FAKE_NETWORK"] = "1" if network else "0"
            res = launch(command(program, argv, prof, args.wrapper), workspace, call_env,
                         PROMPT.encode(), args.timeout, redact)
            stream = parse_stream(spec["dialect"], res.pop("_stdout"))
            cost = list_cost_microusd(spec["dialect"], spec["model"], stream["usage"], pricing)
            if stream["usage"] and cost is None:
                record["list_cost_complete"] = False
            record["list_cost_microusd"] += cost or 0
            res.update({"label": label, "kind": "model", "program": program, "network": network,
                        "profile_sha256": sha256_bytes(prof.encode()), "stream": stream,
                        "list_cost_microusd": cost})
            res["outcome"] = outcome(res)
            record["launches"].append(res)
            return res

        if spec["entry"] != spec["program"]:
            local("V0 entry --version", spec["entry"], ["--version"])
        before = set(os.listdir(config))
        s0 = local("S0 program --version", spec["program"], ["--version"])
        s1 = local("S1 program " + " ".join(spec["status_argv"]), spec["program"], spec["status_argv"])
        after = set(os.listdir(config))
        record["startup"] = {
            "version_ok": s0["exit_code"] == 0,
            "status_ok": s1["exit_code"] == 0,
            "config_entries_created": sorted(after - before),
        }
        record["startup"]["ok"] = record["startup"]["version_ok"] and record["startup"]["status_ok"]
        if not record["startup"]["ok"]:
            record["verdict"] = {"egress": "inconclusive", "cause": "startup", "copy": None,
                                 "anomalies": [], "abnormal": True}
            return record

        c1 = model_call("C1 program network:false", spec["program"], False)
        c2 = model_call("C2 program network:true (control)", spec["program"], True)
        c3 = None
        if c2.get("outcome") == "completed":
            copy, info = private_copy(spec["program"], root)
            record["private_copy"] = {"path": copy, **info}
            local("V2 copy --version", copy, ["--version"])
            c3 = model_call("C3 private copy network:true", copy, True)
        else:
            record["launches"].append({"label": "C3 private copy network:true", "kind": "model",
                                       "skipped": "control C2 did not complete"})
        record["verdict"] = verdict(c1, c2, c3)
        return record
    finally:
        delete_auth(record, auth)
        record["private_config"]["entries_at_end"] = sorted(os.listdir(config)) if config.exists() else None
        shutil.rmtree(root / "private-bin", ignore_errors=True)
        (root.parent / f"{spec['host']}.json").write_text(json.dumps(record, indent=2, sort_keys=True) + "\n")


def outcome(res: dict) -> str:
    if res.get("timed_out"):
        return "no_end"
    if res.get("signal"):
        return "crash"
    stream = res["stream"]
    if res["stdout_truncated"] or stream["non_json_lines"]:
        return "unexpected_output"
    if stream["root_assistant_message"] and res["exit_code"] == 0:
        return "completed"
    if stream["root_assistant_message"]:
        return "assistant_then_failure"
    return "clean_failure"


def verdict(c1: dict, c2: dict, c3: dict | None) -> dict:
    anomalies = [c["label"] for c in (c1, c2, c3) if c and c.get("outcome") in
                 ("no_end", "crash", "unexpected_output")]
    if c1.get("outcome") in ("completed", "assistant_then_failure"):
        egress = "provider_reachable_under_network_false"
    elif c1.get("outcome") == "clean_failure" and c2.get("outcome") == "completed":
        egress = "provider_unreachable_under_network_false"
    else:
        egress = "inconclusive"
    copy = None
    if c3:
        copy = "runs_from_copy" if c3.get("outcome") == "completed" else f"copy_{c3.get('outcome')}"
    abnormal = bool(anomalies) or egress != "provider_unreachable_under_network_false"
    return {"egress": egress, "cause": "model_call" if abnormal else None, "copy": copy,
            "anomalies": anomalies, "abnormal": abnormal}


def sweep_auth(out: Path) -> list[str]:
    """Last resort after an unexpected error: no auth.json survives the run."""
    removed = []
    for path in out.glob("*/config/auth.json"):
        try:
            path.unlink()
            removed.append(str(path.relative_to(out)))
        except OSError:
            pass
    return removed


def main(argv=None) -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--hosts", default="codex,claude", help="ordered, comma separated")
    p.add_argument("--codex-model", default="gpt-5.6-luna")
    p.add_argument("--claude-model", default="claude-haiku-4-5")
    p.add_argument("--effort", default="low")
    p.add_argument("--codex-entry"), p.add_argument("--codex-program"), p.add_argument("--codex-auth")
    p.add_argument("--claude-entry"), p.add_argument("--claude-program")
    p.add_argument("--claude-keychain-read-root", action="store_true",
                   help="add ~/Library/Keychains as a read root for Claude (off by default)")
    p.add_argument("--timeout", type=float, default=90.0, help="seconds per model call")
    p.add_argument("--version-timeout", type=float, default=20.0, help="seconds per local check")
    p.add_argument("--cap-usd", type=float, default=1.0, help="list-price cap per host")
    p.add_argument("--out", default=str(HERE / "runs"))
    p.add_argument("--wrapper", choices=["seatbelt", "none"], default="seatbelt",
                   help="'none' runs without Seatbelt: fake hosts only")
    p.add_argument("--fake", action="store_true", help="hosts are test doubles; required with --wrapper none")
    p.add_argument("--confirm-spend", action="store_true", help="required for real hosts")
    p.add_argument("--runtime-source", default=str(HERE / "sandbox.rs.f263e7d"))
    args = p.parse_args(argv)

    if args.wrapper == "none" and not args.fake:
        p.error("--wrapper none is only allowed with --fake")
    if not args.fake and not args.confirm_spend:
        p.error("real hosts spend tokens: pass --confirm-spend")
    hosts = [h.strip() for h in args.hosts.split(",") if h.strip()]
    if any(h not in ("codex", "claude") for h in hosts):
        p.error("hosts are codex and claude")
    # SIGTERM unwinds through the `finally` blocks that delete auth.json.
    signal.signal(signal.SIGTERM, lambda *_: sys.exit(143))

    stamp = dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    out = Path(args.out) / stamp
    out.mkdir(parents=True)
    pricing = load_pricing(HERE / "pricing.json", HERE / "pricing-cache-writes.json")
    manifest = {
        "schema": SCHEMA,
        "started_at": stamp,
        "script_sha256": sha256_file(Path(__file__)),
        "pricing_sha256": pricing["sha256"],
        "fake": args.fake,
        "hosts_order": hosts,
        "cap_usd_per_host": args.cap_usd,
        "timeout_seconds": args.timeout,
        "prompt_sha256": sha256_bytes(PROMPT.encode()),
        "stop_rule": "stop after the first host only when it is abnormal for a cause other than startup",
        "claude_keychain_read_root": args.claude_keychain_read_root,
        "profile_template": profile_template_matches(Path(args.runtime_source)),
        "preflight": preflight(args.wrapper, out),
        "hosts": {},
        "stopped": None,
    }
    try:
        if not manifest["profile_template"]["ok"]:
            manifest["stopped"] = "replicated profile differs from runtime source"
        elif not manifest["preflight"]["sandbox_effective"] and not args.fake:
            manifest["stopped"] = "sandbox preflight failed: no host launched"
        else:
            for index, host in enumerate(hosts):
                spec = host_spec(host, args)
                record = run_host(spec, out / host, args, pricing)
                manifest["hosts"][host] = {"verdict": record["verdict"], "calls": record["calls"],
                                           "startup": record.get("startup"),
                                           "auth_json": record["private_config"]["auth_json"],
                                           "list_cost_microusd": record["list_cost_microusd"],
                                           "list_cost_complete": record["list_cost_complete"]}
                v = record["verdict"]
                if index == 0 and len(hosts) > 1 and v["abnormal"] and v["cause"] != "startup":
                    manifest["stopped"] = f"first host {host} abnormal: {v}"
                    break
    except SpikeError as error:
        manifest["stopped"] = f"error: {error}"
    finally:
        manifest["auth_json_swept"] = sweep_auth(out)
        (out / "manifest.json").write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"out": str(out), "stopped": manifest["stopped"],
                      "preflight_effective": manifest["preflight"]["sandbox_effective"],
                      "hosts": {h: {"verdict": r["verdict"], "startup": r["startup"], "calls": r["calls"],
                                    "list_cost_microusd": r["list_cost_microusd"]}
                                for h, r in manifest["hosts"].items()}}, indent=2))
    return 0 if manifest["stopped"] is None else 2


if __name__ == "__main__":
    sys.exit(main())
