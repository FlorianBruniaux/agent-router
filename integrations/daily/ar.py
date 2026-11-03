#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Florian Bruniaux
# SPDX-License-Identifier: Apache-2.0
"""ar: run Claude Code or Codex with an Agent Router recommendation, then record how it went.

This is the operator launching their own host. The product stays advisory and
launches nothing. One run asks `agent-router advise --compare-policies`, picks
the stable-v1 or stable-v2 arm, starts the host with that arm, measures what a
Claude session cost from its transcripts, asks one question and records the
outcome in the feedback store. See docs/daily-trial.md.
"""

import argparse
import json
import os
import subprocess
import sys
import tempfile
import time
import uuid
from fractions import Fraction
from pathlib import Path

DEFAULT_BUDGET = "0:1800000:400000"
SNAPSHOT_MAX_AGE_SECONDS = 20 * 3600
POLICIES = ("stable-v1", "stable-v2")
KEYS = {"a": "accepted-as-is", "r": "accepted-after-rework", "x": "abandoned", "s": None}
PROMPT = "Result? [a]ccepted as is / [r]ework needed / [x] abandoned / [s]kip: "
CONSENT = {"schema_version": 1, "collection_enabled": True, "learning_enabled": False, "retention_seconds": 7776000}


class Stop(Exception):
    def __init__(self, message, code=2):
        super().__init__(message)
        self.code = code


def ar_home():
    return Path(os.environ.get("AR_HOME") or Path.home() / "agent-router-advice")


def ar_bin():
    return os.environ.get("AR_BIN") or "agent-router"


def pricing_path():
    return Path(os.environ.get("AR_PRICING") or Path(__file__).with_name("claude-pricing.json"))


def private_dir(path):
    path = Path(path)
    path.mkdir(mode=0o700, parents=True, exist_ok=True)
    return path


def run(argv, **options):
    try:
        return subprocess.run(argv, capture_output=True, text=True, **options)
    except FileNotFoundError:
        raise Stop(f"cannot start {argv[0]}: not found (set AR_BIN for agent-router)")


def pick_arm(request_hash):
    """Deterministic 50/50: an even last hex digit gives stable-v1, an odd one stable-v2."""
    return POLICIES[int(request_hash[-1], 16) % 2]


# Codex snapshot

def snapshot_age_ok(path, now):
    try:
        observed = json.loads(path.read_text())["observed_at_unix_seconds"]
        return isinstance(observed, int) and now - observed <= SNAPSHOT_MAX_AGE_SECONDS
    except (OSError, ValueError, KeyError, TypeError):
        return False


def refresh_codex_snapshot(home, now=None):
    snapshot = home / "observation.json"
    now = int(now if now is not None else time.time())
    if snapshot.exists() and snapshot_age_ok(snapshot, now):
        return snapshot
    cache = Path.home() / ".codex" / "models_cache.json"
    try:
        version = json.loads(cache.read_text())["client_version"]
        done = run([ar_bin(), "inspect", "--host", "codex", "--model-cache", str(cache),
                    "--observed-version", str(version), "--json"])
        problem = None if done.returncode == 0 else (done.stdout.strip() or done.stderr.strip())[:300]
        if problem is None:
            json.loads(done.stdout)["observed_at_unix_seconds"]
    except (OSError, ValueError, KeyError, TypeError) as error:
        done, problem = None, f"{type(error).__name__}: {error}"
    if problem is not None:
        if not snapshot.exists():
            raise Stop(f"inspect failed and no snapshot exists ({problem})")
        print(f"inspect failed, keeping the previous snapshot ({problem})", file=sys.stderr)
        return snapshot
    handle, name = tempfile.mkstemp(prefix=".observation-", suffix=".tmp", dir=home)
    try:
        with os.fdopen(handle, "w") as out:
            out.write(done.stdout)
        os.replace(name, snapshot)
    finally:
        if os.path.exists(name):
            os.unlink(name)
    return snapshot


# Advice

def advise(args, home, host_args, run_dir):
    argv = [ar_bin(), "advise", "--compare-policies", "--json", "--text", args.text, "--kind", args.kind,
            "--risk", args.risk, "--budget", args.budget, "--catalog", str(host_args["catalog"]),
            host_args["flag"], str(host_args["file"]), "--declare-capability", "tools",
            "--output-directory", str(run_dir)]
    for scope in args.scope:
        argv += ["--scope", scope]
    for skill in args.skill:
        argv += ["--skill", skill]
    done = run(argv)
    if done.returncode != 0:
        raise Stop((done.stdout.strip() or done.stderr.strip()) + f"\nagent-router advise exited {done.returncode}",
                   done.returncode if done.returncode in (2, 3) else 2)
    compare = json.loads((run_dir / "compare.json").read_text())
    plans = {}
    for arm in compare["arms"]:
        advice = json.loads((run_dir / arm["directory"] / "advice.json").read_text())
        plans[arm["policy"]] = {"status": arm["status"], "phases": {p["stage"]: p for p in
                                (advice.get("next_manual_action") or {}).get("phases", [])}}
    return compare["request_hash"], plans


def plan_line(phases):
    parts = []
    for stage in ("plan", "execute", "verify"):
        phase = phases.get(stage)
        if phase:
            skills = ",".join(phase["skills"]) or "no skill"
            parts.append(f"{stage} {phase['model']}/{phase['effort']} [{skills}]")
    return " -> ".join(parts)


# Host argv

def claude_argv(args, phases, arm, extra, session_id):
    plan, execute, verify = phases["plan"], phases["execute"], phases["verify"]
    agents = {
        "ar-planner": {"description": "Plans the task before any edit is made.", "model": plan["model"], "effort": plan["effort"],
                       "prompt": "You plan the task. Return a short ordered plan, its risks and the checks that will "
                                 "prove it is done. Do not edit files."},
        "ar-verifier": {"description": "Verifies the finished work against the task.", "model": verify["model"], "effort": verify["effort"],
                        "prompt": "You verify the work. Check it against the task and its acceptance criteria, run the "
                                  "relevant checks, and report what passes, what fails and what you could not check."},
    }

    def skills(phase):
        return ", ".join(phase["skills"]) or "none"

    prompt = (f"Agent Router advice, policy {arm}. Use the subagent ar-planner to produce the plan first. Do the "
              f"execution yourself in this session. Before finishing, use the subagent ar-verifier to check the "
              f"result. Recommended skills: plan {skills(plan)}; execute {skills(execute)}; verify {skills(verify)}.")
    return ["claude", "--model", execute["model"], "--effort", execute["effort"], "--session-id", session_id,
            "--agents", json.dumps(agents, sort_keys=True), "--append-system-prompt", prompt, *extra, args.text]


def codex_argv(args, phases, extra):
    execute = phases["execute"]
    return ["codex", "-m", execute["model"], "-c", f"model_reasoning_effort={execute['effort']}", *extra, args.text]


# Measurement (Claude only)

def cost_microusd(model, row, pricing):
    """Estimated list-price cost of one model's tokens in whole microusd, or None without a shipped price."""
    counts = [row["input"], row["output"], row["cache_creation"], row["cache_read"]]
    if not any(counts):
        return 0
    price = pricing["models"].get(model)
    if price is None:
        return None
    total = (row["input"] * price["input_microusd_per_million"]
             + row["output"] * price["output_microusd_per_million"]
             + row["cache_read"] * price["cached_input_microusd_per_million"]
             + row["cache_creation_5m"] * price["cache_write_5m_microusd_per_million"]
             + row["cache_creation_1h"] * price["cache_write_1h_microusd_per_million"])
    exact = Fraction(total, 1_000_000)
    return (exact.numerator * 2 + exact.denominator) // (exact.denominator * 2)


def _count(value):
    return value if isinstance(value, int) and not isinstance(value, bool) and value >= 0 else 0


def measure_claude(home, session_id, pricing):
    projects = Path(home) / ".claude" / "projects"
    files = sorted(projects.glob(f"*/{session_id}.jsonl")) + sorted(projects.glob(f"*/{session_id}/subagents/*.jsonl"))
    if not files:
        return {"tokens": None, "cost_microusd": None, "per_model": {}}
    messages = {}
    for path in files:
        try:
            lines = path.read_text(errors="replace").splitlines()
        except OSError:
            continue
        for number, line in enumerate(lines):
            try:
                entry = json.loads(line)
            except ValueError:
                continue
            message = entry.get("message") if isinstance(entry, dict) else None
            if entry.get("type") != "assistant" or not isinstance(message, dict) or not isinstance(message.get("usage"), dict):
                continue
            key = message.get("id") or f"{path}:{number}"
            messages[key] = (message.get("model") or "unknown", message["usage"])  # the last line of an id wins
    per_model = {}
    for model, use in messages.values():
        row = per_model.setdefault(model, {"input": 0, "output": 0, "cache_creation": 0, "cache_read": 0,
                                           "cache_creation_5m": 0, "cache_creation_1h": 0})
        created = _count(use.get("cache_creation_input_tokens"))
        split = use.get("cache_creation")
        five = _count(split.get("ephemeral_5m_input_tokens")) if isinstance(split, dict) else 0
        hour = _count(split.get("ephemeral_1h_input_tokens")) if isinstance(split, dict) else 0
        if five + hour != created:  # the split is absent or disagrees: price every write at the 5 minute rate
            five, hour = created, 0
        row["input"] += _count(use.get("input_tokens"))
        row["output"] += _count(use.get("output_tokens"))
        row["cache_creation"] += created
        row["cache_read"] += _count(use.get("cache_read_input_tokens"))
        row["cache_creation_5m"] += five
        row["cache_creation_1h"] += hour
    total_cost, tokens = 0, 0
    for model, row in per_model.items():
        row["cost_microusd"] = cost_microusd(model, row, pricing)
        tokens += row["input"] + row["output"] + row["cache_creation"] + row["cache_read"]
        total_cost = None if total_cost is None or row["cost_microusd"] is None else total_cost + row["cost_microusd"]
    return {"tokens": tokens, "cost_microusd": total_cost, "per_model": per_model}


# Question and record

def read_key():
    if sys.stdin.isatty():
        import termios
        import tty
        fd = sys.stdin.fileno()
        saved = termios.tcgetattr(fd)
        try:
            tty.setcbreak(fd)
            return sys.stdin.read(1)
        finally:
            termios.tcsetattr(fd, termios.TCSADRAIN, saved)
    return sys.stdin.read(1)


def ask():
    while True:
        print(PROMPT, end="", flush=True)
        key = read_key().lower()
        if key == "":
            print()
            return None
        print(key)
        if key in KEYS:
            return KEYS[key]


def feedback(store, *argv):
    return run([ar_bin(), "feedback", "--store", str(store), "--json", *argv])


def ensure_consent(home, store):
    done = feedback(store, "consent")
    if done.returncode == 0 and json.loads(done.stdout).get("preferences", {}).get("collection_enabled"):
        return
    preferences = home / "consent-preferences.json"
    preferences.write_text(json.dumps(CONSENT))
    preferences.chmod(0o600)
    done = feedback(store, "configure", "--preferences", str(preferences))
    if done.returncode != 0:
        raise Stop("feedback configure failed: " + (done.stdout.strip() or done.stderr.strip()))
    print("The store had no collection consent: enabled collection (learning off, retention 90 days) with "
          f"{preferences}.")


def record(home, store, run_dir, arm, result, measured):
    private_dir(store)
    ensure_consent(home, store)
    argv = ["advisory-record", "--advice", str(run_dir), "--policy", arm, "--followed", "--result", result]
    if measured["cost_microusd"] is not None:
        argv += ["--cost-microusd", str(measured["cost_microusd"])]
    if measured["tokens"] is not None:
        argv += ["--tokens", str(measured["tokens"])]
    done = feedback(store, *argv)
    if done.returncode != 0:
        raise Stop("advisory-record failed: " + (done.stdout.strip() or done.stderr.strip()), 1)


def progress(store):
    done = feedback(store, "advisory-report")
    if done.returncode != 0:
        print("progress unavailable: " + (done.stdout.strip() or done.stderr.strip()))
        return
    report = json.loads(done.stdout)
    counts = {policy: 0 for policy in POLICIES}
    for group in report.get("groups", []):
        counts[group["policy_version"]] = counts.get(group["policy_version"], 0) + group["decisions"]
    print("Decisions recorded: " + ", ".join(f"{policy}: {count}" for policy, count in counts.items()))
    print(f"Rates appear from {report.get('minimum_sample', 5)} decisions per policy and risk group.")


# Entry

def parse(argv):
    extra = []
    if "--" in argv:
        split = argv.index("--")
        argv, extra = argv[:split], argv[split + 1:]
    parser = argparse.ArgumentParser(prog="ar", description="Run Claude Code or Codex with Agent Router advice.")
    parser.add_argument("host", choices=("claude", "codex"))
    parser.add_argument("--text", required=True, help="the task, in one sentence")
    parser.add_argument("--kind", required=True, help="explore, implement, review, architecture, security or migration")
    parser.add_argument("--risk", required=True, help="low, medium, high or critical")
    parser.add_argument("--scope", action="append", required=True,
                        help="relative workspace path in scope; repeatable (agent-router refuses '.')")
    parser.add_argument("--budget", default=DEFAULT_BUDGET, help="MICROUSD:MILLISECONDS:TOKENS")
    parser.add_argument("--skill", action="append", default=[], help="candidate skill id; repeatable")
    parser.add_argument("--policy", choices=POLICIES, help="force the arm instead of the 50/50 pick")
    parser.add_argument("--dry-run", action="store_true", help="print the host argv as JSON and stop")
    return parser.parse_args(argv), extra


def main(argv=None):
    args, extra = parse(sys.argv[1:] if argv is None else argv)
    try:
        return execute(args, extra)
    except Stop as stop:
        print(f"ar: {stop}", file=sys.stderr)
        return stop.code


def execute(args, extra):
    home = private_dir(ar_home())
    if args.host == "claude":
        source = {"catalog": home / "claude-core-v2.json", "flag": "--host", "file": home / "claude-host.json"}
    else:
        source = {"catalog": home / "codex-core-v3.json", "flag": "--host-snapshot", "file": None}
    for path in (source["catalog"], source["file"]):
        if path is not None and not path.exists():
            raise Stop(f"missing {path}; see docs/daily-trial.md, one-time setup")
    if args.host == "codex":
        source["file"] = refresh_codex_snapshot(home)
    run_dir = private_dir(home / "runs") / time.strftime("%Y%m%dT%H%M%S", time.gmtime())
    if run_dir.exists():
        run_dir = run_dir.with_name(f"{run_dir.name}-{os.getpid()}")
    request_hash, plans = advise(args, home, source, run_dir)

    forced = args.policy is not None
    arm = args.policy or pick_arm(request_hash)
    print(f"Arm: {arm} ({'forced by --policy' if forced else 'request_hash ' + request_hash[:12] + ', even last digit is stable-v1'})")
    for policy in POLICIES:
        print(f"{'*' if policy == arm else ' '} {policy}: {plan_line(plans[policy]['phases'])}")
    if plans[arm]["status"] != "recommended" or not {"plan", "execute", "verify"} <= set(plans[arm]["phases"]):
        raise Stop(f"{arm} recommends nothing for this request (status {plans[arm]['status']}); nothing was started", 3)
    phases = plans[arm]["phases"]

    session_id = str(uuid.uuid4())
    if args.host == "claude":
        argv = claude_argv(args, phases, arm, extra, session_id)
    else:
        argv = codex_argv(args, phases, extra)
        print("Codex runs the execute stage only. Do the others yourself: "
              f"plan with {phases['plan']['model']}/{phases['plan']['effort']}, "
              f"verify with {phases['verify']['model']}/{phases['verify']['effort']}.")
    if args.dry_run:
        print(json.dumps(argv))
        return 0

    started = int(time.time())
    try:
        host_exit = subprocess.run(argv).returncode
    except FileNotFoundError:
        raise Stop(f"cannot start {argv[0]}: not found on PATH")
    if args.host == "claude":
        measured = measure_claude(Path.home(), session_id, json.loads(pricing_path().read_text()))
        known = "unknown" if measured["tokens"] is None else f"{measured['tokens']} tokens"
        cost = "UNKNOWN" if measured["cost_microusd"] is None else f"{measured['cost_microusd'] / 1_000_000:.4f} USD (estimated)"
        print(f"Measured: {known}, cost {cost}")
        for model, row in sorted(measured["per_model"].items()):
            print(f"  {model}: in {row['input']}, out {row['output']}, cache write {row['cache_creation']}, "
                  f"cache read {row['cache_read']}, cost {row['cost_microusd'] if row['cost_microusd'] is not None else 'UNKNOWN'} microusd")
    else:
        measured = {"tokens": None, "cost_microusd": None, "per_model": {}}
        print("Measured: UNKNOWN (Codex files are not read)")

    result = ask()
    if result is None:
        print("Skipped: nothing recorded.")
        return host_exit
    store = home / "store"
    record(home, store, run_dir, arm, result, measured)
    entry = {"ts": started, "run_dir": str(run_dir), "arm": arm, "forced": forced, "request_hash": request_hash,
             "host": args.host, "kind": args.kind, "risk": args.risk, "result": result, "host_exit": host_exit,
             "tokens": measured["tokens"], "cost_microusd": measured["cost_microusd"], "per_model": measured["per_model"]}
    with open(home / "runs.jsonl", "a") as out:
        out.write(json.dumps(entry, sort_keys=True) + "\n")
    progress(store)
    return host_exit


if __name__ == "__main__":
    sys.exit(main())
