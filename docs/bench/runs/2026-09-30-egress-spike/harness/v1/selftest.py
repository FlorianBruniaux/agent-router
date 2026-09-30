#!/usr/bin/env python3
"""Validate spike_egress.py with fake hosts. Spends no token, launches no real host.

Seatbelt cannot be applied from inside another Seatbelt sandbox, so these runs
use `--wrapper none --fake`: the fake reads SPIKE_FAKE_NETWORK instead of
meeting the profile. What this covers: flow, call budget, stop rule, verdicts,
cost arithmetic, copy handling, absence of model text in the results, and the
refusal guards. What it does not cover: the Seatbelt profile's effect.
"""
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
SPIKE = HERE / "spike_egress.py"
SECRET = "FAKE-SECRET-MODEL-TEXT"

FAKE = r"""#!/bin/sh
MODE='__MODE__'
if [ "$1" = "--version" ]; then echo "fake-host 1.0.0"; exit 0; fi
cat >/dev/null
case "$0" in */private-bin/*) if [ "$MODE" = copyfail ]; then
  echo "Error: No such file or directory: codex-resources" >&2; exit 3; fi;; esac
case "$MODE" in
  crash) kill -SEGV $$ ;;
  hang) sleep 600 ;;
  garbage) echo "this is not json"; exit 0 ;;
esac
if [ "$SPIKE_FAKE_NETWORK" = 1 ] || [ "$MODE" = reachable ]; then
  if [ "$1" = exec ]; then
    echo '{"type":"thread.started","thread_id":"t"}'
    echo '{"type":"turn.started"}'
    echo '{"type":"item.completed","item":{"id":"i","type":"agent_message","text":"__SECRET__"}}'
    echo '{"type":"turn.completed","usage":{"input_tokens":64579,"cached_input_tokens":37376,"output_tokens":192,"reasoning_output_tokens":46}}'
  else
    echo '{"type":"system","subtype":"init","model":"claude-haiku-4-5","claude_code_version":"9.9.9"}'
    echo '{"type":"assistant","message":{"type":"message","role":"assistant","model":"claude-haiku-4-5-20251001","content":[{"type":"text","text":"__SECRET__"}]}}'
    echo '{"type":"result","subtype":"success","is_error":false,"result":"__SECRET__","total_cost_usd":0.13,"usage":{"input_tokens":2,"cache_creation_input_tokens":65165,"cache_read_input_tokens":0,"output_tokens":4,"cache_creation":{"ephemeral_1h_input_tokens":65165,"ephemeral_5m_input_tokens":0}}}'
  fi
  exit 0
fi
echo "error sending request for url (https://api.example.invalid/v1): Connection refused (os error 61)" >&2
if [ "$1" = exec ]; then echo '{"type":"error","message":"stream disconnected before completion"}'; fi
exit 1
"""


def fake(directory: Path, name: str, mode: str) -> str:
    path = directory / name
    path.write_text(FAKE.replace("__MODE__", mode).replace("__SECRET__", SECRET))
    path.chmod(0o755)
    return str(path.resolve())


COUNTER = iter(range(1000))


def run(tmp: Path, codex_mode: str, claude_mode: str, *extra: str) -> tuple[int, dict, Path]:
    tag = f"{next(COUNTER)}-{codex_mode}-{claude_mode}"
    bins = tmp / f"bins-{tag}"
    bins.mkdir()
    codex = fake(bins, "codex", codex_mode)
    entry = bins / "codex.js"
    entry.write_text("#!/usr/bin/env node-not-on-path\n")
    entry.chmod(0o755)
    claude = fake(bins, "claude", claude_mode)
    out = tmp / f"out-{tag}"
    cmd = [sys.executable, str(SPIKE), "--fake", "--wrapper", "none", "--out", str(out),
           "--codex-entry", str(entry), "--codex-program", codex,
           "--claude-entry", claude, "--claude-program", claude, "--timeout", "3", *extra]
    proc = subprocess.run(cmd, capture_output=True, text=True, timeout=120)
    runs = sorted(out.iterdir())
    assert len(runs) == 1, runs
    manifest = json.loads((runs[0] / "manifest.json").read_text())
    for f in runs[0].rglob("*.json"):
        assert SECRET not in f.read_text(), f"model text leaked into {f}"
    return proc.returncode, manifest, runs[0]


def host(run_dir: Path, name: str) -> dict:
    return json.loads((run_dir / f"{name}.json").read_text())


def main() -> int:
    failures = []

    def check(label, condition):
        print(("PASS " if condition else "FAIL ") + label)
        if not condition:
            failures.append(label)

    with tempfile.TemporaryDirectory(dir=HERE) as raw:
        tmp = Path(raw)

        rc, m, d = run(tmp, "normal", "normal")
        cx, cl = host(d, "codex"), host(d, "claude")
        check("expected path: exit 0, no stop", rc == 0 and m["stopped"] is None)
        check("profile template matches runtime source", m["profile_template"]["ok"])
        check("fake mode reports sandbox not effective", m["preflight"]["sandbox_effective"] is False)
        check("codex verdict unreachable + runs from copy",
              cx["verdict"] == {"egress": "provider_unreachable_under_network_false",
                                "copy": "runs_from_copy", "anomalies": [], "abnormal": False})
        check("codex 3 calls, V0 entry launched and failed",
              cx["calls"] == 3 and cx["launches"][0]["label"].startswith("V0")
              and cx["launches"][0]["exit_code"] == 127)
        check("codex entry detected as script", cx["entry"]["kind"] == "script"
              and cx["entry"]["shebang"] == "/usr/bin/env node-not-on-path")
        check("codex C1 categories include net_connect",
              "net_connect" in cx["launches"][2]["stderr_categories"])
        check("codex list cost 2 x 6419 microusd", cx["list_cost_microusd"] == 2 * 6419)
        check("claude observed dated model", cx and
              cl["launches"][2]["stream"]["observed_models"] == ["claude-haiku-4-5-20251001"])
        check("claude list cost 2 x 130352 microusd", cl["list_cost_microusd"] == 2 * 130352)
        check("private copy hash matches", cl["private_copy"]["matches_original"] is True
              and not (d / "claude" / "private-bin").exists())

        for mode, expected in [("reachable", "provider_reachable_under_network_false"),
                               ("crash", "inconclusive"), ("hang", "inconclusive"),
                               ("garbage", "inconclusive")]:
            rc, m, d = run(tmp, mode, "normal")
            cx = host(d, "codex")
            check(f"codex {mode}: stop before claude, egress={expected}",
                  rc == 2 and "claude" not in m["hosts"] and not (d / "claude.json").exists()
                  and cx["verdict"]["egress"] == expected and cx["verdict"]["abnormal"])
        rc, m, d = run(tmp, "crash", "normal")
        check("crash recorded as signal 11", host(d, "codex")["launches"][2]["signal"] == 11)
        rc, m, d = run(tmp, "hang", "normal")
        c1 = host(d, "codex")["launches"][2]
        check("hang recorded as timeout", c1["timed_out"] and c1["outcome"] == "no_end")

        rc, m, d = run(tmp, "copyfail", "normal")
        cx = host(d, "codex")
        check("copy failure is a finding, not a stop",
              rc == 0 and cx["verdict"]["copy"] == "copy_clean_failure" and "claude" in m["hosts"])

        rc, m, d = run(tmp, "normal", "normal", "--cap-usd", "0.0001")
        cl = host(d, "claude")
        check("cost cap skips C3", cl["calls"] == 2
              and cl["launches"][-1].get("skipped") == "cost cap reached")

        guard = subprocess.run([sys.executable, str(SPIKE), "--out", str(tmp / "g1")],
                               capture_output=True, text=True)
        check("real hosts refused without --confirm-spend",
              guard.returncode == 2 and "--confirm-spend" in guard.stderr and not (tmp / "g1").exists())
        guard = subprocess.run([sys.executable, str(SPIKE), "--wrapper", "none", "--confirm-spend",
                                "--out", str(tmp / "g2")], capture_output=True, text=True)
        check("--wrapper none refused without --fake",
              guard.returncode == 2 and not (tmp / "g2").exists())

    print(f"{len(failures)} failure(s)")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
