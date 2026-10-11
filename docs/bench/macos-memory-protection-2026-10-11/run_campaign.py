#!/usr/bin/env python3
"""Inert local campaign: fixed six cases, one execution each, <=60 MiB explicit buffers."""
import hashlib, json, os, pathlib, selectors, signal, subprocess, sys, time
ROOT = pathlib.Path(__file__).resolve().parent
SOURCE = ROOT / "tests/memory_observer_fixture.c"
BINARY = ROOT / "memory-observer-fixture"
OUT = ROOT / "campaign"
CASES = ("normal", "growth", "failread", "mismatch", "deadline", "self")
BUDGETS = (4, 24, 0, 0, 0, 32)
CAP = 1024 * 1024

def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()

def capture(case):
    start = time.monotonic()
    child = subprocess.Popen([str(BINARY), case], stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True,
        env={"PATH": "/usr/bin:/bin"})
    sel = selectors.DefaultSelector()
    buffers = {"stdout": bytearray(), "stderr": bytearray()}
    for name in buffers:
        stream = getattr(child, name)
        os.set_blocking(stream.fileno(), False)
        sel.register(stream, selectors.EVENT_READ, name)
    signals = []
    cause = None
    exited = None
    def send(sig):
        # This parent has not reaped the session leader. No foreign PID is accepted.
        try:
            os.killpg(child.pid, sig)
            result = "sent"
        except ProcessLookupError:
            result = "group_absent"
        signals.append({"signal": sig, "elapsed_seconds": time.monotonic()-start,
                        "result": result, "leader_observed_exited": exited is not None})
    try:
        while True:
            elapsed = time.monotonic()-start
            exited = os.waitid(os.P_PID, child.pid, os.WEXITED|os.WNOHANG|os.WNOWAIT)
            if elapsed >= 7 and not any(x["signal"] == signal.SIGKILL for x in signals):
                cause = cause or "deadline"
                send(signal.SIGKILL)
            elif elapsed >= 6 and not signals:
                cause = cause or "deadline"
                send(signal.SIGTERM)
            if elapsed >= 7.5:
                cause = cause or "collection_deadline"
                break
            if exited is not None and not sel.get_map():
                break
            for key, _ in sel.select(timeout=0.02):
                part = os.read(key.fileobj.fileno(), 16384)
                if not part:
                    sel.unregister(key.fileobj)
                    key.fileobj.close()
                    continue
                size = sum(len(x) for x in buffers.values())
                available = CAP-size
                buffers[key.data].extend(part[:max(0, available)])
                if len(part) > available:
                    cause = "output_cap"
                    if not any(x["signal"] == signal.SIGKILL for x in signals): send(signal.SIGKILL)
            if cause == "output_cap": break
        # An abnormal fixture cannot leave an owned group alive. Leader PID remains reserved.
        abnormal = exited is None or exited.si_code != os.CLD_EXITED or exited.si_status != 0
        if (cause or abnormal) and not any(x["signal"] == signal.SIGKILL for x in signals):
            send(signal.SIGKILL)
        remaining = max(0.001, 8-(time.monotonic()-start))
        try: code = child.wait(timeout=remaining)
        except subprocess.TimeoutExpired:
            code = None
            cause = "reap_unconfirmed"
    finally:
        sel.close()
        for name in buffers:
            stream = getattr(child, name)
            if not stream.closed: stream.close()
    for name, content in buffers.items(): (OUT / f"{case}.{name}").write_bytes(content)
    return {"case": case, "exit_code": code, "outer_cause": cause,
            "duration_seconds": time.monotonic()-start, "outer_signals": signals,
            "deadline_seconds": 8, "output_cap_bytes": CAP,
            "stdout_sha256": hashlib.sha256(buffers["stdout"]).hexdigest(),
            "stderr_sha256": hashlib.sha256(buffers["stderr"]).hexdigest(),
            "leader_reaped": code is not None}

def main():
    required = ("waitid", "WNOWAIT", "P_PID", "WEXITED", "WNOHANG", "CLD_EXITED")
    if not all(hasattr(os, name) for name in required):
        raise SystemExit("Refusing before spawn: this Python lacks waitid/NOWAIT support")
    if OUT.exists(): raise SystemExit("Refusing replay: campaign directory already exists")
    OUT.mkdir()
    source_hash, binary_hash = sha(SOURCE), sha(BINARY)
    summary = {"base_commit": "f389059ab637a8338353ae85ed37e3b93a326101",
               "source_sha256": source_hash, "binary_sha256": binary_hash,
               "platform": list(os.uname())[:1]+list(os.uname())[2:],
               "maximum_explicit_buffer_bytes": 60*1048576,
               "production_modified": False, "provider_called": False,
               "native_qualification": "NO_GO", "cases": []}
    for case, mib in zip(CASES, BUDGETS):
        assert sha(SOURCE)==source_hash and sha(BINARY)==binary_hash
        result = capture(case)
        result["case_allocation_max_bytes"] = mib*1048576
        summary["cases"].append(result)
        (OUT/"summary.json").write_text(json.dumps(summary,indent=2)+"\n")
        print(json.dumps(result), flush=True)
        if result["outer_cause"] or result["exit_code"] != 0: break
    return 0 if len(summary["cases"])==6 and all(x["exit_code"]==0 and not x["outer_cause"] for x in summary["cases"]) else 1
if __name__ == "__main__": sys.exit(main())
