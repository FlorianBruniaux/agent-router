#!/usr/bin/env python3
"""Validate spike_egress.py (v3) with fake hosts. Spends no token, launches no real host.

Seatbelt cannot be applied from inside another Seatbelt sandbox, so these runs
use `--wrapper none --fake`: the fake reads SPIKE_FAKE_NETWORK instead of
meeting the profile. Covered: flow, private config directory, auth.json copy
and deletion (success, startup failure, timeout, exception), stderr redaction,
startup stop rule, call budget, verdicts, cost arithmetic, copy handling,
absence of model text and auth content in the results, refusal guards.
Not covered: the Seatbelt profile's effect on real hosts.
"""
import importlib.util
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
SPIKE = HERE / "spike_egress.py"
SECRET = "FAKE-SECRET-MODEL-TEXT"
AUTH_SECRET = "FAKE-AUTH-CONTENT-7f3a"
TOKEN = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJzcGlrZSJ9.c2lnbmF0dXJlLXZhbHVl"
KEY = "sk-proj-AbCdEf0123456789XyZ"
EMAIL = "someone@example.com"

FAKE = r"""#!/bin/sh
MODE='__MODE__'
CFG="${CODEX_HOME:-$CLAUDE_CONFIG_DIR}"
if [ "$1" = "--version" ]; then echo "fake-host 1.0.0"; exit 0; fi
if [ "$1 $2" = "login status" ] || [ "$1 $2" = "auth status" ]; then
  if [ "$MODE" = startupfail ]; then
    echo "Error: Operation not permitted (os error 1) writing $HOME/.cache/x and $CFG/log, opening /opt/homebrew/lib/node_modules/@openai/codex-darwin-arm64/vendor/resources; token __TOKEN__ key __KEY__ user __EMAIL__" >&2
    exit 1
  fi
  if [ "$1" = login ] && [ ! -f "$CFG/auth.json" ]; then echo "Not logged in" >&2; exit 1; fi
  echo "state" > "$CFG/state.db"
  echo "Logged in as __EMAIL__"
  exit 0
fi
cat >/dev/null
case "$0" in */private-bin/*) if [ "$MODE" = copyfail ]; then
  echo "Error: No such file or directory: codex-resources" >&2; exit 3; fi;; esac
case "$MODE" in
  prefs|prefs_all)
    if [ -z "$SPIKE_FAKE_PREFS" ] || { [ "$MODE" = prefs_all ] && [ "$SPIKE_FAKE_PREFS" != all ]; }; then
      echo "Error: thread/start: thread/start failed: failed to load configuration: Failed to synchronize managed preferences (code -32600) at $$" >&2
      exit 1
    fi ;;
  crash) kill -SEGV $$ ;;
  hang) sleep 600 ;;
  garbage) echo "this is not json"; exit 0 ;;
esac
if [ "$SPIKE_FAKE_NETWORK" = 1 ] || [ "$MODE" = reachable ]; then
  if [ "$MODE" = rotate ] && [ -f "$CFG/auth.json" ]; then echo "refreshed" >> "$CFG/auth.json"; fi
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

COUNTER = iter(range(1000))


def fake(directory: Path, name: str, mode: str) -> str:
    path = directory / name
    text = (FAKE.replace("__MODE__", mode).replace("__SECRET__", SECRET).replace("__TOKEN__", TOKEN)
            .replace("__KEY__", KEY).replace("__EMAIL__", EMAIL))
    path.write_text(text)
    path.chmod(0o755)
    return str(path.resolve())


def setup(tmp: Path, codex_mode: str, claude_mode: str):
    tag = f"{next(COUNTER)}-{codex_mode}-{claude_mode}"
    bins = tmp / f"bins-{tag}"
    bins.mkdir()
    home = tmp / f"home-{tag}"
    (home / ".codex").mkdir(parents=True)
    auth = home / ".codex" / "auth.json"
    auth.write_text(json.dumps({"tokens": {"access_token": AUTH_SECRET}}))
    auth.chmod(0o600)
    entry = bins / "codex.js"
    entry.write_text("#!/usr/bin/env node-not-on-path\n")
    entry.chmod(0o755)
    args = ["--fake", "--wrapper", "none", "--out", str(tmp / f"out-{tag}"),
            "--codex-entry", str(entry), "--codex-program", fake(bins, "codex", codex_mode),
            "--claude-entry", fake(bins, "claude", claude_mode),
            "--claude-program", str((bins / "claude").resolve()), "--timeout", "3"]
    return args, home, tmp / f"out-{tag}", auth


def outputs(out: Path) -> Path:
    runs = sorted(out.iterdir())
    assert len(runs) == 1, runs
    for f in runs[0].rglob("*.json"):
        text = f.read_text()
        for secret in (SECRET, AUTH_SECRET, TOKEN, KEY, EMAIL):
            assert secret not in text, f"{secret[:12]} leaked into {f}"
    assert not list(runs[0].glob("*/config/auth.json")), "auth.json survived the run"
    return runs[0]


def run(tmp: Path, codex_mode: str, claude_mode: str, *extra: str):
    args, home, out, auth = setup(tmp, codex_mode, claude_mode)
    env = dict(os.environ, HOME=str(home))
    env.pop("CODEX_HOME", None)
    proc = subprocess.run([sys.executable, str(SPIKE), *args, *extra], capture_output=True, text=True,
                          timeout=180, env=env)
    d = outputs(out)
    assert auth.read_text() == json.dumps({"tokens": {"access_token": AUTH_SECRET}}), "source auth.json changed"
    return proc.returncode, json.loads((d / "manifest.json").read_text()), d


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
        check("profile template and env names match runtime source", m["profile_template"]["ok"])
        check("profiles compile with libsandbox", m["profile_compiles"]["ok"] is True)
        check("codex verdict unreachable + runs from copy",
              cx["verdict"]["egress"] == "provider_unreachable_under_network_false"
              and cx["verdict"]["copy"] == "runs_from_copy" and not cx["verdict"]["abnormal"])
        check("private config is 0700, sole write root, named by CODEX_HOME",
              cx["private_config"]["mode"] == "0700" and cx["write_roots"] == [cx["private_config"]["path"]]
              and cx["private_config"]["variable"] == "CODEX_HOME"
              and cx["environment_names"] == ["CODEX_HOME", "HOME", "PATH"])
        check("claude uses CLAUDE_CONFIG_DIR and copies nothing",
              cl["private_config"]["variable"] == "CLAUDE_CONFIG_DIR"
              and cl["private_config"]["auth_json"] == {"copied": False}
              and "CLAUDE_CONFIG_DIR" in cl["environment_names"])
        a = cx["private_config"]["auth_json"]
        check("auth.json copied 0600 then deleted on success",
              a["copied"] and a["mode"] == "0600" and a["deleted"] is True
              and a["present_after_delete"] is False and a["signature_changed_by_host"] is False)
        check("startup check wrote state in the private directory",
              cx["startup"]["ok"] and "state.db" in cx["startup"]["config_entries_created"])
        check("codex 3 model calls, cost 2 x 6419 microusd",
              cx["calls"] == 3 and cx["list_cost_microusd"] == 2 * 6419)
        check("claude observed dated model, cost 2 x 130352 microusd",
              cl["list_cost_microusd"] == 2 * 130352 and any(
                  l.get("stream", {}).get("observed_models") == ["claude-haiku-4-5-20251001"]
                  for l in cl["launches"]))
        c1 = next(l for l in cx["launches"] if l["label"].startswith("C1"))
        check("C1 stderr excerpt kept and bounded",
              "Connection refused" in c1["stderr_excerpt"] and len(c1["stderr_excerpt"].encode()) <= 300)

        rc, m, d = run(tmp, "startupfail", "normal")
        cx = host(d, "codex")
        s1 = next(l for l in cx["launches"] if l["label"].startswith("S1"))
        check("startup failure: no model call, claude still runs",
              cx["calls"] == 0 and cx["verdict"]["cause"] == "startup" and "claude" in m["hosts"]
              and m["stopped"] is None and m["hosts"]["claude"]["calls"] == 3)
        check("startup failure: auth.json deleted", cx["private_config"]["auth_json"]["deleted"] is True)
        ex = s1["stderr_excerpt"]
        check("stderr redaction: home, config, token, key, e-mail",
              "florianbruniaux" not in ex and "~/.cache/x" in ex
              and "/opt/homebrew/lib/node_modules/@openai/codex-darwin-arm64/vendor/resources" in ex and "<config>/log" in ex
              and "<redacted>" in ex and "fs_denied" in s1["stderr_categories"])

        rc, m, d = run(tmp, "hang", "normal")
        cx = host(d, "codex")
        c1 = next(l for l in cx["launches"] if l["label"].startswith("C1"))
        check("timeout: C1 no_end, stop before claude, auth.json deleted",
              c1["outcome"] == "no_end" and rc == 2 and "claude" not in m["hosts"]
              and cx["private_config"]["auth_json"]["deleted"] is True)

        rc, m, d = run(tmp, "rotate", "normal")
        a = host(d, "codex")["private_config"]["auth_json"]
        check("host rewrite of auth.json is detected, then deleted",
              a["signature_changed_by_host"] is True and a["deleted"] is True)

        for mode, expected in [("reachable", "provider_reachable_under_network_false"),
                               ("crash", "inconclusive"), ("garbage", "inconclusive")]:
            rc, m, d = run(tmp, mode, "normal")
            cx = host(d, "codex")
            check(f"codex {mode}: stop before claude, egress={expected}",
                  rc == 2 and "claude" not in m["hosts"] and cx["verdict"]["egress"] == expected
                  and cx["verdict"]["cause"] == "model_call")

        rc, m, d = run(tmp, "prefs", "normal")
        cx = host(d, "codex")
        check("identical C1/C2 failure: cause config, claude still runs, no stop",
              cx["verdict"]["cause"] == "config" and cx["verdict"]["egress"] == "inconclusive"
              and cx["calls"] == 2 and "claude" in m["hosts"] and m["stopped"] is None and rc == 0)
        check("config failure: auth.json deleted", cx["private_config"]["auth_json"]["deleted"] is True)

        rc, m, d = run(tmp, "prefs", "normal", "--codex-allow-preferences", "domain")
        cx = host(d, "codex")
        check("--codex-allow-preferences domain: egress measured, deviation recorded",
              cx["verdict"]["egress"] == "provider_unreachable_under_network_false"
              and "codex preferences: domain" in cx["profile_deviations"]
              and cx["profile_extra_rules"][0] == '(allow user-preference-read (preference-domain "com.openai.codex"))'
              and m["codex_allow_preferences"] == "domain"
              and "codex preferences: domain" not in host(d, "claude")["profile_deviations"])

        rc, m, d = run(tmp, "prefs_all", "normal", "--codex-allow-preferences", "domain")
        check("domain not enough: config cause, claude still runs",
              host(d, "codex")["verdict"]["cause"] == "config" and "claude" in m["hosts"])
        rc, m, d = run(tmp, "prefs_all", "normal", "--codex-allow-preferences", "all")
        check("--codex-allow-preferences all: egress measured",
              host(d, "codex")["verdict"]["egress"] == "provider_unreachable_under_network_false")

        rc, m, d = run(tmp, "normal", "normal", "--hosts", "claude")
        check("--hosts claude runs Claude only",
              list(m["hosts"]) == ["claude"] and not (d / "codex.json").exists() and rc == 0)

        spec_ = importlib.util.spec_from_file_location("spike_egress_r", SPIKE)
        mod_ = importlib.util.module_from_spec(spec_)
        spec_.loader.exec_module(mod_)
        p0 = mod_.render_profile(["/a"], ["/b"], False, mod_.PREFERENCE_RULES["domain"])
        p1 = mod_.render_profile(["/a"], ["/b"], True, mod_.PREFERENCE_RULES["domain"])
        check("C1 and C2 profiles differ only by the network line",
              p1 == p0 + mod_.NETWORK_ALLOW and mod_.render_profile(["/a"], ["/b"], False) + mod_.PREFERENCE_RULES["domain"] == p0)

        rc, m, d = run(tmp, "copyfail", "normal")
        check("copy failure is a finding, not a stop",
              rc == 0 and host(d, "codex")["verdict"]["copy"] == "copy_clean_failure" and "claude" in m["hosts"])

        rc, m, d = run(tmp, "normal", "normal", "--cap-usd", "0.0001")
        cl = host(d, "claude")
        check("cost cap skips C3", cl["calls"] == 2 and cl["launches"][-1].get("skipped") == "cost cap reached")

        # Exception after the copy: auth.json must still be deleted and logged.
        args, home, out, _ = setup(tmp, "normal", "normal")
        spec = importlib.util.spec_from_file_location("spike_egress", SPIKE)
        mod = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(mod)
        old_home, old_codex = os.environ.get("HOME"), os.environ.pop("CODEX_HOME", None)
        os.environ["HOME"] = str(home)

        def boom(*_a, **_k):
            raise RuntimeError("injected failure")

        mod.parse_stream = boom
        raised = False
        try:
            mod.main(args)
        except RuntimeError:
            raised = True
        finally:
            os.environ["HOME"] = old_home
            if old_codex is not None:
                os.environ["CODEX_HOME"] = old_codex
        d = outputs(out)
        a = host(d, "codex")["private_config"]["auth_json"]
        check("exception: propagates, auth.json deleted and logged, manifest written",
              raised and a["deleted"] is True and (d / "manifest.json").exists())

        guard = subprocess.run([sys.executable, str(SPIKE), "--out", str(tmp / "g1")],
                               capture_output=True, text=True)
        check("real hosts refused without --confirm-spend",
              guard.returncode == 2 and "--confirm-spend" in guard.stderr and not (tmp / "g1").exists())
        guard = subprocess.run([sys.executable, str(SPIKE), "--wrapper", "none", "--confirm-spend",
                                "--out", str(tmp / "g2")], capture_output=True, text=True)
        check("--wrapper none refused without --fake", guard.returncode == 2 and not (tmp / "g2").exists())

    print(f"{len(failures)} failure(s)")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
