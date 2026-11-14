#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Florian Bruniaux
# SPDX-License-Identifier: Apache-2.0
"""aradv: run Claude Code or Codex with an Agent Router recommendation, then record how it went.

This is the operator launching their own host. The product stays advisory and
launches nothing. One run asks `agent-router advise --compare-policies`, picks
the stable-v1 or stable-v2 arm and starts one host session on that arm's
execute model, with its effort capped. The session plans and executes itself;
an independent ar-verifier subagent on the verify model is added only for high
or critical risk (or --verify). Afterwards it reads the Claude transcripts to
measure cost and to check which models really ran, asks one question and
records the outcome, as followed or overridden, in the feedback store.
See docs/daily-trial.md.
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
EFFORT_ORDER = ("none", "minimal", "low", "medium", "high", "xhigh", "max", "ultra")
EFFORT_CAPS = ("low", "medium", "high", "xhigh", "max")
SYNTHETIC_MODEL = "<synthetic>"  # Claude Code writes it for local errors and interrupts; no model ran
DEFAULT_EFFORT_CAP = "high"
VERIFY_RISKS = ("high", "critical")
VERIFIER = "ar-verifier"
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


def cap_effort(effort, cap):
    """The lower of the advised effort and the cap; an effort outside the known order is kept."""
    if effort in EFFORT_ORDER and cap in EFFORT_ORDER and EFFORT_ORDER.index(effort) > EFFORT_ORDER.index(cap):
        return cap
    return effort


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

def expected_models(phases, cap, verify):
    """What the session should run: the execute model for the main loop, the verify model for ar-verifier."""
    execute = phases["execute"]
    expected = {"main": {"model": execute["model"], "effort": cap_effort(execute["effort"], cap)}, "verifier": None}
    if verify:
        expected["verifier"] = {"model": phases["verify"]["model"], "effort": cap_effort(phases["verify"]["effort"], cap)}
    return expected


def claude_argv(args, phases, arm, extra, session_id, expected):
    main, verifier = expected["main"], expected["verifier"]
    skills = sorted({skill for phase in phases.values() for skill in phase["skills"]})
    prompt = f"Agent Router advice, policy {arm}. Plan and do the work yourself in this session."
    argv = ["claude", "--model", main["model"], "--effort", main["effort"], "--session-id", session_id]
    if verifier is not None:
        agents = {VERIFIER: {"description": "Independently verifies the finished work against the task.",
                             "model": verifier["model"], "effort": verifier["effort"],
                             "prompt": "You verify the work. Check it against the task and its acceptance criteria, run "
                                       "the relevant checks, and report what passes, what fails and what you could not "
                                       "check. Do not edit files."}}
        argv += ["--agents", json.dumps(agents, sort_keys=True)]
        prompt += (f" Before finishing, call the subagent {VERIFIER} once to check the result independently. Do not "
                   f"pass a model or an effort when you call it: its definition sets them.")
    if skills:
        prompt += " Recommended skills: " + ", ".join(skills) + "."
    return [*argv, "--append-system-prompt", prompt, *extra, args.text]


def codex_argv(args, phases, extra, expected):
    main = expected["main"]
    return ["codex", "-m", main["model"], "-c", f"model_reasoning_effort={main['effort']}", *extra, args.text]


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
        return {"tokens": None, "cost_microusd": None, "per_model": {}, "main_models": [], "subagents": [],
                "unreadable": 0}
    messages = {}
    main_models, subagents, unreadable = set(), [], 0
    for path in files:
        is_main = path.parent.name != "subagents"
        file_models = set()
        try:
            lines = path.read_text(errors="replace").splitlines()
        except OSError:
            unreadable += 1
            continue
        for number, line in enumerate(lines):
            try:
                entry = json.loads(line)
            except ValueError:
                continue
            if not isinstance(entry, dict):
                continue
            message = entry.get("message")
            if entry.get("type") != "assistant" or not isinstance(message, dict) or not isinstance(message.get("usage"), dict):
                continue
            key = message.get("id") or f"{path}:{number}"
            messages[key] = (message.get("model") or "unknown", message["usage"])  # the last line of an id wins
            if message.get("model") != SYNTHETIC_MODEL:
                file_models.add(message.get("model") or "unknown")
        if is_main:
            main_models |= file_models
        elif file_models:  # a subagent file without any model reply says nothing about which model ran
            subagents.append({"agent_type": subagent_type(path), "models": sorted(file_models)})
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
    return {"tokens": tokens, "cost_microusd": total_cost, "per_model": per_model,
            "main_models": sorted(main_models), "subagents": subagents, "unreadable": unreadable}


def subagent_type(path):
    """The agentType Claude Code stores beside a subagent transcript, or UNKNOWN."""
    try:
        value = json.loads(path.with_name(path.stem + ".meta.json").read_text()).get("agentType")
    except (OSError, ValueError, AttributeError):
        return "UNKNOWN"
    return value if isinstance(value, str) and value else "UNKNOWN"


def adherence(expected, measured):
    """followed, overridden or UNKNOWN, from the models the transcripts show. Efforts are not observable."""
    def unknown(reason):
        return {"adherence": "UNKNOWN", "reasons": [reason], "used_model": None,
                "extra_subagents": [a for a in measured.get("subagents", []) if a["agent_type"] != VERIFIER]}

    if measured.get("tokens") is None:
        return unknown("no transcript was found")
    if measured.get("unreadable"):
        return unknown(f"{measured['unreadable']} transcript file(s) could not be read")
    if not measured["main_models"]:
        return unknown("the main session recorded no model reply")
    # advisory-record needs the model used instead for an override: the first substitute found is kept,
    # and runs.jsonl keeps every model that ran.
    reasons, substitutes = [], []
    main = expected["main"]["model"]
    if measured["main_models"] != [main]:
        reasons.append(f"main session ran {', '.join(measured['main_models'])} instead of {main}")
        substitutes += [model for model in measured["main_models"] if model != main]
    verifiers = [agent for agent in measured["subagents"] if agent["agent_type"] == VERIFIER]
    if expected["verifier"] is not None:
        want = expected["verifier"]["model"]
        if not verifiers:
            if any(agent["agent_type"] == "UNKNOWN" for agent in measured["subagents"]):
                return unknown(f"{VERIFIER} not found, and a subagent of unknown type ran")
            reasons.append(f"{VERIFIER} was not called")
            substitutes += measured["main_models"]  # the main session did whatever checking was done
        for agent in verifiers:
            if agent["models"] != [want]:
                reasons.append(f"{VERIFIER} ran {', '.join(agent['models'])} instead of {want}")
                substitutes += [model for model in agent["models"] if model != want]
    extra = [agent for agent in measured["subagents"] if agent["agent_type"] != VERIFIER]
    return {"adherence": "overridden" if reasons else "followed", "reasons": reasons,
            "used_model": substitutes[0] if reasons else None, "extra_subagents": extra}


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


def record(home, store, run_dir, arm, result, measured, checked):
    private_dir(store)
    ensure_consent(home, store)
    overridden = checked["adherence"] == "overridden"
    argv = ["advisory-record", "--advice", str(run_dir), "--policy", arm,
            "--overridden" if overridden else "--followed", "--result", result]
    if overridden:
        argv += ["--used-model", checked["used_model"]]
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
    parser = argparse.ArgumentParser(prog="aradv", description="Run Claude Code or Codex with Agent Router advice.")
    parser.add_argument("host", choices=("claude", "codex"))
    parser.add_argument("--text", required=True, help="the task, in one sentence")
    parser.add_argument("--kind", required=True, help="explore, implement, review, architecture, security or migration")
    parser.add_argument("--risk", required=True, help="low, medium, high or critical")
    parser.add_argument("--scope", action="append", required=True,
                        help="relative workspace path in scope; repeatable (agent-router refuses '.')")
    parser.add_argument("--budget", default=DEFAULT_BUDGET, help="MICROUSD:MILLISECONDS:TOKENS")
    parser.add_argument("--skill", action="append", default=[], help="candidate skill id; repeatable")
    parser.add_argument("--policy", choices=POLICIES, help="force the arm instead of the 50/50 pick")
    parser.add_argument("--effort-cap", choices=EFFORT_CAPS, default=DEFAULT_EFFORT_CAP,
                        help=f"highest effort passed to the host (default {DEFAULT_EFFORT_CAP})")
    verify = parser.add_mutually_exclusive_group()
    verify.add_argument("--verify", dest="verify", action="store_true", default=None,
                        help="add the ar-verifier subagent (default: only for high or critical risk)")
    verify.add_argument("--no-verify", dest="verify", action="store_false", help="never add ar-verifier")
    parser.add_argument("--dry-run", action="store_true", help="print the host argv as JSON and stop")
    return parser.parse_args(argv), extra


def main(argv=None):
    args, extra = parse(sys.argv[1:] if argv is None else argv)
    try:
        return execute(args, extra)
    except Stop as stop:
        print(f"aradv: {stop}", file=sys.stderr)
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
    verify = args.verify if args.verify is not None else args.risk in VERIFY_RISKS
    if args.host == "codex":
        if args.verify:
            print("--verify is ignored for Codex: it has no subagent definition flag.", file=sys.stderr)
        verify = False
    expected = expected_models(phases, args.effort_cap, verify)
    main = expected["main"]
    line = f"Session: {main['model']}/{main['effort']} plans and executes"
    if expected["verifier"] is not None:
        line += f"; {VERIFIER} on {expected['verifier']['model']}/{expected['verifier']['effort']} checks the result"
    print(line + f" (effort cap {args.effort_cap}).")

    session_id = str(uuid.uuid4())
    if args.host == "claude":
        argv = claude_argv(args, phases, arm, extra, session_id, expected)
    else:
        argv = codex_argv(args, phases, extra, expected)
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
        checked = adherence(expected, measured)
        print(f"Advice: {checked['adherence']}" + (" (" + "; ".join(checked["reasons"]) + ")" if checked["reasons"] else "")
              + (", recorded as followed" if checked["adherence"] == "UNKNOWN" else ""))
        for agent in checked["extra_subagents"]:
            print(f"  other subagent {agent['agent_type']} ran {', '.join(agent['models']) or 'no model'}")
    else:
        measured = {"tokens": None, "cost_microusd": None, "per_model": {}, "main_models": [], "subagents": []}
        checked = {"adherence": "UNKNOWN", "reasons": ["Codex files are not read"], "used_model": None, "extra_subagents": []}
        print("Measured: UNKNOWN (Codex files are not read); advice adherence UNKNOWN, recorded as followed")

    result = ask()
    if result is None:
        print("Skipped: nothing recorded.")
        return host_exit
    store = home / "store"
    record(home, store, run_dir, arm, result, measured, checked)
    entry = {"ts": started, "run_dir": str(run_dir), "arm": arm, "forced": forced, "request_hash": request_hash,
             "host": args.host, "kind": args.kind, "risk": args.risk, "result": result, "host_exit": host_exit,
             "expected": expected, "effort_cap": args.effort_cap, "adherence": checked["adherence"],
             "adherence_reasons": checked["reasons"], "subagents": measured["subagents"],
             "tokens": measured["tokens"], "cost_microusd": measured["cost_microusd"], "per_model": measured["per_model"]}
    with open(home / "runs.jsonl", "a") as out:
        out.write(json.dumps(entry, sort_keys=True) + "\n")
    progress(store)
    return host_exit


if __name__ == "__main__":
    sys.exit(main())
