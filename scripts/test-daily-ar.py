#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Florian Bruniaux
# SPDX-License-Identifier: Apache-2.0
"""Behavior checks for integrations/daily/ar.py.

No model or provider is contacted. The host executables are fakes placed on
PATH; the advisory commands run the real agent-router binary (AR_BIN, else
target/debug/agent-router, built here when missing) against a temporary HOME
and AR_HOME.
"""

import importlib.util
import json
import os
import shutil
import stat
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SCRIPT = ROOT / "integrations" / "daily" / "ar.py"
PRICING = ROOT / "integrations" / "daily" / "claude-pricing.json"
CATALOGS = ROOT / "assets" / "catalogs"
BIN = Path(os.environ.get("AR_BIN") or ROOT / "target" / "debug" / "agent-router")

CLAUDE_HOST = {
    "id": "my-claude-code",
    "harness": "claude",
    "supported_models": ["claude-fable-5-1", "claude-haiku-5-5", "claude-opus-5-5", "claude-sonnet-5-5"],
    "supported_efforts": {m: ["low", "medium", "high", "xhigh", "max"]
                          for m in ["claude-fable-5-1", "claude-haiku-5-5", "claude-opus-5-5", "claude-sonnet-5-5"]},
    "capabilities": [],
}


def iso(ts):
    return time.strftime("%Y-%m-%dT%H:%M:%S.000000Z", time.gmtime(ts))


def codex_cache(fetched_at):
    return {"fetched_at": iso(fetched_at), "client_version": "0.153.4", "models": [
        {"slug": "gpt-5.6-luna", "supported_reasoning_levels": [{"effort": "low"}, {"effort": "medium"}, {"effort": "high"}]},
        {"slug": "gpt-5.6-sol", "supported_reasoning_levels": [{"effort": "high"}, {"effort": "xhigh"}, {"effort": "max"}]},
        {"slug": "gpt-5.6-terra", "supported_reasoning_levels": [{"effort": "low"}, {"effort": "medium"}, {"effort": "high"}, {"effort": "xhigh"}]},
    ]}


def usage(i, o, cc=0, cr=0, sub=None):
    value = {"input_tokens": i, "output_tokens": o, "cache_creation_input_tokens": cc,
             "cache_read_input_tokens": cr, "service_tier": "standard"}
    if sub is not None:
        value["cache_creation"] = sub
    return value


def assistant(mid, model, use):
    return {"type": "assistant", "sessionId": "s", "message": {"id": mid, "model": model, "usage": use}}


# Main transcript: m1 appears twice (the last line wins), m2 once, plus noise.
MAIN = [
    {"type": "user", "message": {"content": "hi"}},
    assistant("m1", "claude-opus-5-5", usage(1000, 10, 400, 2000)),
    assistant("m1", "claude-opus-5-5", usage(1000, 500, 400, 2000)),
    assistant("m2", "claude-sonnet-5-5", usage(3000, 1000, 0, 5000)),
]
# Subagent transcript: opus with 5m/1h split, plus a model without a shipped price.
SUB = [
    assistant("m3", "claude-opus-5-5", usage(100, 50, 400, 0, {"ephemeral_5m_input_tokens": 100, "ephemeral_1h_input_tokens": 300})),
    assistant("m4", "claude-haiku-5-5", usage(7, 3, 0, 0)),
]

FAKE_HOST = """#!{python}
import json, os, sys, pathlib
argv = sys.argv[1:]
pathlib.Path(os.environ["FAKE_ARGV_OUT"]).write_text(json.dumps(argv))
if os.path.basename(sys.argv[0]) == "claude" and "--session-id" in argv:
    sid = argv[argv.index("--session-id") + 1]
    base = pathlib.Path.home() / ".claude" / "projects" / "-tmp-slug"
    (base / sid / "subagents").mkdir(parents=True)
    spec = json.loads(pathlib.Path(os.environ["FAKE_TRANSCRIPT"]).read_text())
    with open(base / (sid + ".jsonl"), "w") as f:
        for row in spec["main"]:
            f.write(json.dumps(row) + "\\n")
        f.write("not json\\n")
    with open(base / sid / "subagents" / "agent-1.jsonl", "w") as f:
        for row in spec["sub"]:
            f.write(json.dumps(row) + "\\n")
sys.exit(int(os.environ.get("FAKE_EXIT", "0")))
"""


def load_module():
    spec = importlib.util.spec_from_file_location("ar_daily", SCRIPT)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class Env:
    """A temporary HOME, AR_HOME and fake-host PATH."""

    def __init__(self):
        self.dir = Path(tempfile.mkdtemp(prefix="ar-daily-"))
        self.home = self.dir / "home"
        self.ar_home = self.dir / "arhome"
        self.bin = self.dir / "bin"
        for path in (self.home, self.bin):
            path.mkdir()
        self.ar_home.mkdir(mode=0o700)
        self.argv_out = self.dir / "argv.json"
        self.transcript = self.dir / "transcript.json"
        self.transcript.write_text(json.dumps({"main": MAIN, "sub": SUB}))
        for name in ("claude", "codex"):
            path = self.bin / name
            path.write_text(FAKE_HOST.format(python=sys.executable))
            path.chmod(path.stat().st_mode | stat.S_IXUSR)
        shutil.copy(CATALOGS / "claude-core-v2.json", self.ar_home)
        shutil.copy(CATALOGS / "codex-core-v3.json", self.ar_home)
        (self.ar_home / "claude-host.json").write_text(json.dumps(CLAUDE_HOST))
        (self.home / ".codex").mkdir()

    def write_cache(self, fetched_at, client_version="0.153.4", broken=False):
        cache = codex_cache(fetched_at)
        cache["client_version"] = client_version
        if broken:
            cache["models"] = []
        (self.home / ".codex" / "models_cache.json").write_text(json.dumps(cache))

    def env(self, **extra):
        env = {"PATH": f"{self.bin}{os.pathsep}/usr/bin{os.pathsep}/bin", "HOME": str(self.home),
               "AR_HOME": str(self.ar_home), "AR_BIN": str(BIN), "FAKE_ARGV_OUT": str(self.argv_out),
               "FAKE_TRANSCRIPT": str(self.transcript)}
        env.update(extra)
        return env

    def ar(self, host, *args, stdin="", extra_env=None, host_args=()):
        argv = [sys.executable, "-B", str(SCRIPT), host, "--text", "Add a dry-run flag", "--kind", "implement",
                "--risk", "medium", "--scope", "scripts", *args]
        if host_args:
            argv += ["--", *host_args]
        return subprocess.run(argv, input=stdin, capture_output=True, text=True,
                              env=self.env(**(extra_env or {})), timeout=120)

    def store_report(self):
        out = subprocess.run([str(BIN), "feedback", "--store", str(self.ar_home / "store"), "--json", "advisory-report"],
                             capture_output=True, text=True, env=self.env())
        return out.returncode, json.loads(out.stdout)

    def runs(self):
        path = self.ar_home / "runs.jsonl"
        if not path.exists():
            return []
        return [json.loads(line) for line in path.read_text().splitlines() if line.strip()]

    def plan_of(self, stdout, stage):
        """(model, effort) of a stage in the starred (chosen) plan line."""
        line = [l for l in stdout.splitlines() if l.startswith("* ")][0]
        word = line.split(f"{stage} ", 1)[1].split(" ")[0]
        model, effort = word.split("/")
        return model, effort

    def cleanup(self):
        shutil.rmtree(self.dir, ignore_errors=True)


def last_json_line(text):
    lines = [line for line in text.splitlines() if line.strip()]
    return json.loads(lines[-1])


class DailyWrapper(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if not BIN.exists():
            subprocess.run(["cargo", "build", "-p", "agent-router-cli", "--locked"], cwd=ROOT, check=True)
        cls.mod = load_module()

    def setUp(self):
        self.t = Env()
        self.addCleanup(self.t.cleanup)

    # arm choice
    def test_arm_parity_is_deterministic_and_balanced(self):
        arms = [self.mod.pick_arm("0" * 63 + digit) for digit in "0123456789abcdef"]
        self.assertEqual(arms.count("stable-v1"), 8)
        self.assertEqual(arms.count("stable-v2"), 8)
        self.assertEqual(self.mod.pick_arm("ab" * 32), "stable-v2")
        self.assertEqual(self.mod.pick_arm("ab" * 31 + "ac"), "stable-v1")
        self.assertEqual(self.mod.pick_arm("ab" * 32), self.mod.pick_arm("ab" * 32))

    def test_same_request_gets_the_same_arm_twice_and_policy_overrides_it(self):
        first = self.t.ar("claude", "--dry-run")
        second = self.t.ar("claude", "--dry-run")
        self.assertEqual(first.returncode, 0, first.stderr)
        self.assertIn("Arm: ", first.stdout)
        arm = [l for l in first.stdout.splitlines() if l.startswith("Arm: ")][0]
        self.assertIn(arm, second.stdout)
        for forced in ("stable-v1", "stable-v2"):
            out = self.t.ar("claude", "--dry-run", "--policy", forced)
            self.assertEqual(out.returncode, 0, out.stderr)
            self.assertIn(f"Arm: {forced} (forced by --policy)", out.stdout)

    # dry-run argv
    def test_claude_dry_run_prints_the_exact_argv_and_launches_nothing(self):
        out = self.t.ar("claude", "--dry-run", "--policy", "stable-v2", "--skill", "tdd", host_args=("--permission-mode", "plan"))
        self.assertEqual(out.returncode, 0, out.stderr)
        argv = last_json_line(out.stdout)
        self.assertFalse(self.t.argv_out.exists(), "dry-run must not launch the host")
        self.assertEqual(argv[0], "claude")
        self.assertEqual(argv[1], "--model")
        self.assertTrue(argv[2].startswith("claude-"))
        self.assertEqual(argv[3], "--effort")
        self.assertIn(argv[4], ("low", "medium", "high", "xhigh", "max"))
        self.assertEqual(argv[5], "--session-id")
        self.assertRegex(argv[6], r"^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$")
        agents = json.loads(argv[argv.index("--agents") + 1])
        self.assertEqual(sorted(agents), ["ar-planner", "ar-verifier"])
        for agent in agents.values():
            self.assertTrue(agent["model"].startswith("claude-"))
            self.assertIn(agent["effort"], ("low", "medium", "high", "xhigh", "max"))
            self.assertTrue(agent["description"] and agent["prompt"])
        plan = self.t.plan_of(out.stdout, "plan")
        verify = self.t.plan_of(out.stdout, "verify")
        self.assertEqual((agents["ar-planner"]["model"], agents["ar-planner"]["effort"]), plan)
        self.assertEqual((agents["ar-verifier"]["model"], agents["ar-verifier"]["effort"]), verify)
        prompt = argv[argv.index("--append-system-prompt") + 1]
        self.assertIn("ar-planner", prompt)
        self.assertIn("ar-verifier", prompt)
        self.assertEqual(argv[-3:-1], ["--permission-mode", "plan"])
        self.assertEqual(argv[-1], "Add a dry-run flag")

    def test_codex_dry_run_prints_the_exact_argv(self):
        self.t.write_cache(time.time())
        out = self.t.ar("codex", "--dry-run", "--policy", "stable-v2", host_args=("--sandbox", "read-only"))
        self.assertEqual(out.returncode, 0, out.stderr)
        argv = last_json_line(out.stdout)
        self.assertEqual(argv[0:2], ["codex", "-m"])
        self.assertTrue(argv[2].startswith("gpt-"))
        self.assertEqual(argv[3], "-c")
        self.assertRegex(argv[4], r"^model_reasoning_effort=(low|medium|high|xhigh|max)$")
        self.assertEqual(argv[5:7], ["--sandbox", "read-only"])
        self.assertEqual(argv[-1], "Add a dry-run flag")
        self.assertIn("plan", out.stdout.lower())
        self.assertFalse(self.t.argv_out.exists())

    # transcript measurement and cost
    def test_transcript_dedupes_by_message_id_and_sums_per_model(self):
        sid = "11111111-1111-4111-8111-111111111111"
        base = self.t.home / ".claude" / "projects" / "-x"
        (base / sid / "subagents").mkdir(parents=True)
        (base / f"{sid}.jsonl").write_text("\n".join(json.dumps(r) for r in MAIN) + "\nnot json\n")
        (base / sid / "subagents" / "agent-1.jsonl").write_text("\n".join(json.dumps(r) for r in SUB) + "\n")
        pricing = json.loads(PRICING.read_text())
        measured = self.mod.measure_claude(self.t.home, sid, pricing)
        by = measured["per_model"]
        self.assertEqual(by["claude-opus-5-5"]["input"], 1100)
        self.assertEqual(by["claude-opus-5-5"]["output"], 550)
        self.assertEqual(by["claude-opus-5-5"]["cache_creation"], 800)
        self.assertEqual(by["claude-opus-5-5"]["cache_read"], 2000)
        self.assertEqual(by["claude-sonnet-5-5"]["output"], 1000)
        self.assertEqual(measured["tokens"], (1100 + 550 + 800 + 2000) + (3000 + 1000 + 5000) + 10)

    def test_cost_arithmetic_is_exact_and_uses_the_cache_write_split(self):
        pricing = json.loads(PRICING.read_text())
        row = {"input": 1000, "output": 500, "cache_creation": 400, "cache_read": 2000,
               "cache_creation_5m": 400, "cache_creation_1h": 0}
        # 1000*4 + 500*20 + 2000*0.2 + 400*5 = 4000 + 10000 + 400 + 2000
        self.assertEqual(self.mod.cost_microusd("claude-opus-5-5", row, pricing), 16400)
        split = dict(row, cache_creation_5m=100, cache_creation_1h=300)
        # cache writes: 100*5 + 300*8 = 2900 instead of 2000
        self.assertEqual(self.mod.cost_microusd("claude-opus-5-5", split, pricing), 17300)

    def test_transcript_cost_total_and_unknown_price(self):
        sid = "22222222-2222-4222-8222-222222222222"
        base = self.t.home / ".claude" / "projects" / "-x"
        (base / sid / "subagents").mkdir(parents=True)
        (base / f"{sid}.jsonl").write_text("\n".join(json.dumps(r) for r in MAIN) + "\n")
        pricing = json.loads(PRICING.read_text())
        # Without the haiku-5-5 subagent line every model has a shipped price.
        (base / sid / "subagents" / "agent-1.jsonl").write_text(json.dumps(SUB[0]) + "\n")
        measured = self.mod.measure_claude(self.t.home, sid, pricing)
        opus = 1100 * 4 + 550 * 20 + 2000 * 0.2 + 300 * 8 + 500 * 5  # microusd per token = USD per million
        sonnet = 3000 * 2 + 1000 * 10 + 5000 * 0.2
        self.assertEqual(measured["per_model"]["claude-opus-5-5"]["cost_microusd"], round(opus))
        self.assertEqual(measured["cost_microusd"], round(opus) + round(sonnet))
        # With a model that has no shipped price, the total is unknown, the priced rows stay.
        (base / sid / "subagents" / "agent-2.jsonl").write_text(json.dumps(SUB[1]) + "\n")
        measured = self.mod.measure_claude(self.t.home, sid, pricing)
        self.assertIsNone(measured["per_model"]["claude-haiku-5-5"]["cost_microusd"])
        self.assertIsNone(measured["cost_microusd"])
        self.assertEqual(measured["per_model"]["claude-opus-5-5"]["cost_microusd"], round(opus))

    def test_missing_transcript_is_unknown_not_zero(self):
        pricing = json.loads(PRICING.read_text())
        measured = self.mod.measure_claude(self.t.home, "33333333-3333-4333-8333-333333333333", pricing)
        self.assertIsNone(measured["tokens"])
        self.assertIsNone(measured["cost_microusd"])

    # full runs
    def test_claude_run_records_outcome_and_prints_progress(self):
        out = self.t.ar("claude", "--policy", "stable-v2", stdin="a")
        self.assertEqual(out.returncode, 0, out.stderr)
        argv = json.loads(self.t.argv_out.read_text())
        self.assertIn("--session-id", argv)
        self.assertIn("collection consent", out.stdout)
        runs = self.t.runs()
        self.assertEqual(len(runs), 1)
        run = runs[0]
        self.assertEqual(run["arm"], "stable-v2")
        self.assertEqual(run["host"], "claude")
        self.assertEqual(run["result"], "accepted-as-is")
        self.assertRegex(run["request_hash"], r"^[0-9a-f]{64}$")
        self.assertIsNone(run["cost_microusd"], "claude-haiku-5-5 has no shipped price")
        self.assertEqual(run["tokens"], 4450 + 9000 + 10)
        self.assertEqual(run["per_model"]["claude-opus-5-5"]["output"], 550)
        code, report = self.t.store_report()
        self.assertEqual(code, 0, report)
        self.assertEqual(report["decisions"], 1)
        self.assertEqual([g["policy_version"] for g in report["groups"]], ["stable-v2"])
        self.assertIn("stable-v1: 0", out.stdout)
        self.assertIn("stable-v2: 1", out.stdout)
        self.assertIn("5 decisions", out.stdout)
        # a second run reuses the consent without announcing it again
        again = self.t.ar("claude", "--policy", "stable-v1", stdin="r")
        self.assertEqual(again.returncode, 0, again.stderr)
        self.assertNotIn("collection consent", again.stdout)
        self.assertEqual([r["result"] for r in self.t.runs()], ["accepted-as-is", "accepted-after-rework"])
        self.assertIn("stable-v1: 1", again.stdout)

    def test_abandoned_key_and_host_exit_are_recorded(self):
        out = self.t.ar("claude", "--policy", "stable-v1", stdin="x", extra_env={"FAKE_EXIT": "3"})
        self.assertEqual(out.returncode, 3, out.stderr)  # ar returns the host exit code
        run = self.t.runs()[0]
        self.assertEqual(run["result"], "abandoned")
        self.assertEqual(run["host_exit"], 3)

    def test_skip_records_nothing(self):
        out = self.t.ar("claude", "--policy", "stable-v2", stdin="s")
        self.assertEqual(out.returncode, 0, out.stderr)
        self.assertEqual(self.t.runs(), [])
        store = self.t.ar_home / "store"
        self.assertTrue(not store.exists() or not any(store.iterdir()), "skip must not create store content")
        self.assertNotIn("collection consent", out.stdout)

    def test_codex_run_leaves_measurements_unknown(self):
        self.t.write_cache(time.time())
        out = self.t.ar("codex", "--policy", "stable-v1", stdin="a")
        self.assertEqual(out.returncode, 0, out.stderr)
        argv = json.loads(self.t.argv_out.read_text())
        self.assertEqual(argv[0], "-m")
        run = self.t.runs()[0]
        self.assertEqual(run["host"], "codex")
        self.assertIsNone(run["tokens"])
        self.assertIsNone(run["cost_microusd"])

    # codex snapshot
    def inspect_at(self, ts, path):
        cache = self.t.dir / "old-cache.json"
        cache.write_text(json.dumps(codex_cache(ts)))
        done = subprocess.run([str(BIN), "inspect", "--host", "codex", "--model-cache", str(cache), "--observed-version",
                               "0.153.4", "--now", str(int(ts)), "--json"], capture_output=True, text=True, check=True)
        path.write_text(done.stdout)

    def test_stale_codex_snapshot_is_regenerated_and_a_fresh_one_is_kept(self):
        snapshot = self.t.ar_home / "observation.json"
        self.t.write_cache(time.time())
        self.inspect_at(time.time() - 19 * 3600, snapshot)
        before = snapshot.read_bytes()
        out = self.t.ar("codex", "--dry-run", "--policy", "stable-v1")
        self.assertEqual(out.returncode, 0, out.stderr)
        self.assertEqual(snapshot.read_bytes(), before, "a 19 h snapshot is fresh")
        self.inspect_at(time.time() - 21 * 3600, snapshot)
        before = snapshot.read_bytes()
        out = self.t.ar("codex", "--dry-run", "--policy", "stable-v1")
        self.assertEqual(out.returncode, 0, out.stderr)
        self.assertNotEqual(snapshot.read_bytes(), before, "a 21 h snapshot is regenerated")
        self.assertGreater(json.loads(snapshot.read_text())["observed_at_unix_seconds"], time.time() - 600)
        snapshot.unlink()
        out = self.t.ar("codex", "--dry-run", "--policy", "stable-v1")
        self.assertEqual(out.returncode, 0, out.stderr)
        self.assertTrue(snapshot.exists(), "a missing snapshot is generated")

    def test_failed_inspect_keeps_the_previous_snapshot(self):
        snapshot = self.t.ar_home / "observation.json"
        self.inspect_at(time.time() - 21 * 3600, snapshot)
        before = snapshot.read_bytes()
        self.t.write_cache(time.time(), broken=True)
        out = self.t.ar("codex", "--dry-run", "--policy", "stable-v1")
        self.assertEqual(snapshot.read_bytes(), before)
        self.assertEqual(out.returncode, 0, out.stderr)
        self.assertIn("inspect failed", out.stderr + out.stdout)
        self.assertEqual([p.name for p in self.t.ar_home.iterdir() if p.name.startswith(".")], [], "no temp file is left behind")

    def test_failed_inspect_without_any_snapshot_stops(self):
        self.t.write_cache(time.time(), broken=True)
        out = self.t.ar("codex", "--dry-run", "--policy", "stable-v1")
        self.assertNotEqual(out.returncode, 0)
        self.assertFalse((self.t.ar_home / "observation.json").exists())


class PricingTable(unittest.TestCase):
    def test_rows_keep_their_source_and_match_the_repo_tables(self):
        table = json.loads(PRICING.read_text())
        self.assertIn("source", table)
        base = ROOT / "docs" / "bench" / "campaign-next" / "pilot"
        pricing = json.loads((base / "pricing.json").read_text())["models"]
        writes = json.loads((base / "pricing-cache-writes.json").read_text())["models"]
        self.assertTrue(table["models"])
        for model, row in table["models"].items():
            self.assertTrue(model.startswith("claude-"))
            self.assertEqual(row["input_microusd_per_million"], pricing[model]["input_microusd_per_million"])
            self.assertEqual(row["cached_input_microusd_per_million"], pricing[model]["cached_input_microusd_per_million"])
            self.assertEqual(row["output_microusd_per_million"], pricing[model]["output_microusd_per_million"])
            self.assertEqual(row["cache_write_5m_microusd_per_million"], writes[model]["write_5m"])
            self.assertEqual(row["cache_write_1h_microusd_per_million"], writes[model]["write_1h"])
        self.assertNotIn("claude-haiku-5-5", table["models"])


if __name__ == "__main__":
    unittest.main()
