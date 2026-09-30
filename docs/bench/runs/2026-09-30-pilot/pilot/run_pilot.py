#!/usr/bin/env python3
"""Router value pilot driver (host-direct mode).

Arm A takes its per-phase model and effort from `agent-router route`, computed
once at `init` and sealed in the manifest. Every arm then runs through the same
host-direct executor, so the arms differ only by the routing decision. The
Agent Router runtime (`prepare`, `run --approve`, `verify`, `artifacts apply`)
is NOT exercised: see PLAN.md, section "Why host-direct".

Commands: init, selfcheck, calibrate, run, report, status.
Resumable: a run with a result.json is never redone. A run directory without a
result.json is moved aside as an interrupted attempt and redone.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import random
import shutil
import signal
import statistics
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
PHASES = ("plan", "execute", "verify")
SINGLE_SESSION_ARMS = ("E", "D")
TEST_COMMAND = "python3 -m unittest discover -s tests -t ."

PLAN_PROMPT = """You are working in a Python repository in the current directory. Task id: {task_id}.

Task:
{statement}
Phase: plan. Do not modify any file. Read the relevant code and write a short implementation plan and an acceptance checklist. Reply with the plan only."""

EXECUTE_PROMPT = """You are working in a Python repository in the current directory. Task id: {task_id}.

Task:
{statement}
Phase: execute. Implement the task by editing files under pilot_lib/ only. Do not add or change files outside pilot_lib/. You may run `{test_command}`. Plan from the previous phase:

{plan}

Reply with a one-paragraph summary of the change."""

VERIFY_PROMPT = """You are working in a Python repository in the current directory. Task id: {task_id}.

Task:
{statement}
Phase: verify. Do not modify any file. Review the current changes with `git diff` against the task and run `{test_command}`. Reply with PASS or FAIL on the first line, then the reasons."""

OVERHEAD_PROMPT = "Reply with OK"

SINGLE_PROMPT = """You are working in a Python repository in the current directory. Task id: {task_id}.

Task:
{statement}
Work in three steps in this single session.
1. Plan: read the relevant code and write a short implementation plan and an acceptance checklist.
2. Execute: implement the task by editing files under pilot_lib/ only. Do not add or change files outside pilot_lib/. You may run `{test_command}`.
3. Verify: review your changes with `git diff` against the task and run `{test_command}`.
Reply with PASS or FAIL on the first line, then a one-paragraph summary."""


# ---------------------------------------------------------------- utilities

def sha256_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def sha256_text(text: str) -> str:
    return hashlib.sha256(text.encode()).hexdigest()


def write_json_atomic(path: Path, value) -> None:
    tmp = path.with_suffix(path.suffix + ".tmp")
    tmp.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")
    os.replace(tmp, path)


def load_json(path: Path):
    return json.loads(path.read_text())


def log(message: str) -> None:
    print(f"[pilot {time.strftime('%H:%M:%S')}] {message}", flush=True)


class Pilot:
    def __init__(self, args):
        self.args = args
        self.config_path = Path(args.config).resolve()
        self.config = load_json(self.config_path)
        self.fake = args.fake
        self.results = HERE / ("results-fake" if self.fake else "results")
        self.runs_dir = self.results / "runs"
        self.manifest_path = self.results / "manifest.json"
        self.pricing = load_json(HERE / "pricing.json")
        self.cache_writes = load_json(HERE / "pricing-cache-writes.json")
        self.providers = [p for p in args.providers.split(",") if p]
        for provider in self.providers:
            if provider not in self.config["providers"]:
                sys.exit(f"unknown provider {provider}")

    # ------------------------------------------------------------ programs

    def program(self, provider: str) -> list[str]:
        if self.fake:
            return [sys.executable, str(HERE / "fake" / "fake_host.py"), provider]
        return [self.config["programs"][provider]]

    def agent_router(self) -> str:
        return str((HERE / self.config["agent_router"]).resolve())

    def host_version(self, provider: str) -> str:
        if self.fake:
            return "fake-host-1"
        out = subprocess.run(self.program(provider) + ["--version"], capture_output=True, text=True, timeout=60)
        return (out.stdout.strip().splitlines() or ["UNKNOWN"])[-1]

    # ------------------------------------------------------------ corpus

    def task(self, task_id: str) -> dict:
        folder = HERE / "corpus" / "tasks" / task_id
        meta = load_json(folder / "meta.json")
        meta["statement"] = (folder / "statement.md").read_text()
        meta["hidden_test"] = folder / "hidden_test.py"
        return meta

    def corpus_files(self) -> list[Path]:
        root = HERE / "corpus"
        return sorted(p for p in root.rglob("*") if p.is_file() and "__pycache__" not in p.parts)

    def build_base_repo(self) -> Path:
        base = self.results / "base-repo"
        if base.exists():
            return base
        shutil.copytree(HERE / "corpus" / "base", base)
        env = dict(os.environ, GIT_CONFIG_GLOBAL="/dev/null", GIT_CONFIG_NOSYSTEM="1",
                   GIT_AUTHOR_DATE="2026-09-30T00:00:00Z", GIT_COMMITTER_DATE="2026-09-30T00:00:00Z")
        for cmd in (["git", "init", "-q"], ["git", "add", "-A"],
                    ["git", "-c", "user.name=Pilot", "-c", "user.email=pilot@example.invalid",
                     "commit", "--no-gpg-sign", "-qm", "pilot base"]):
            subprocess.run(cmd, cwd=base, env=env, check=True)
        return base

    # ------------------------------------------------------------ arm A

    def route_decision(self, provider: str, task: dict) -> dict:
        spec = self.config["providers"][provider]
        catalog = HERE / spec["catalog"]
        models = load_json(catalog)["models"]
        request = {
            "task": {"objective": task["statement"].strip()[:500], "kind": task["kind"], "risk": task["risk"],
                     "required_capabilities": ["tools"], "acceptance": ["hidden unit tests pass"]},
            "host": {"id": f"pilot-{provider}", "harness": spec["harness"],
                     "supported_models": sorted(m["id"] for m in models),
                     "supported_efforts": {m["id"]: m["efforts"] for m in models},
                     "capabilities": ["tools"]},
            "skill_candidates": [],
        }
        folder = self.results / "routes"
        folder.mkdir(parents=True, exist_ok=True)
        request_path = folder / f"{provider}-{task['id']}-request.json"
        request_path.write_text(json.dumps(request, indent=2) + "\n")
        out = subprocess.run([self.agent_router(), "route", "--request", str(request_path), "--catalog", str(catalog), "--json"],
                             capture_output=True, text=True, timeout=60)
        (folder / f"{provider}-{task['id']}-decision.json").write_text(out.stdout)
        if out.returncode != 0:
            raise SystemExit(f"route failed for {provider}/{task['id']}: exit {out.returncode} {out.stderr.strip()}")
        stages = json.loads(out.stdout)["decision"]["selected"]["plan"]["stages"]
        return {s["kind"]: {"model": s["model"], "effort": s["effort"]} for s in stages}

    def arm_phases(self, provider: str, arm: str, task_id: str) -> dict:
        spec = self.config["providers"][provider]
        if arm == "A":
            return self.manifest()["routes"][provider][task_id]
        fixed = {"B": (spec["frontier"], "high"), "G": (spec["frontier"], "medium"),
                 "C": (spec["cheaper"], "medium"), "E": (spec["frontier"], "high"), "D": (None, None)}[arm]
        return {phase: {"model": fixed[0], "effort": fixed[1]} for phase in PHASES}

    # ------------------------------------------------------------ manifest

    def sealed_inputs(self) -> dict:
        files = {str(p.relative_to(HERE)): sha256_file(p) for p in self.corpus_files()}
        for name in ("pricing.json", "pricing-cache-writes.json", "pricing-sources.md", "run_pilot.py"):
            files[name] = sha256_file(HERE / name)
        files[str(self.config_path.relative_to(HERE))] = sha256_file(self.config_path)
        for provider in self.config["providers"].values():
            files[provider["catalog"]] = sha256_file(HERE / provider["catalog"])
        if self.fake:
            files["fake/fake_host.py"] = sha256_file(HERE / "fake" / "fake_host.py")
        return files

    def manifest(self) -> dict:
        if not hasattr(self, "_manifest"):
            if not self.manifest_path.exists():
                sys.exit("no manifest: run `init` first")
            self._manifest = load_json(self.manifest_path)
        return self._manifest

    def init(self) -> None:
        self.results.mkdir(parents=True, exist_ok=True)
        if self.manifest_path.exists():
            log(f"manifest exists, init is a no-op: {self.manifest_path}")
            return
        self.build_base_repo()
        tasks = [self.task(t) for t in self.config["tasks"]]
        routes = {p: {t["id"]: self.route_decision(p, t) for t in tasks} for p in self.config["providers"]}
        arms = list(self.config["arms"]) + (list(self.config["optional_arms"]) if self.args.with_d else [])
        rng = random.Random(self.config["seed"])
        blocks = [(t["id"], rep) for t in tasks for rep in range(1, self.config["repetitions"] + 1)]
        rng.shuffle(blocks)
        order = []
        for task_id, rep in blocks:
            providers = sorted(self.config["providers"])
            rng.shuffle(providers)
            for provider in providers:
                block_arms = list(arms)
                rng.shuffle(block_arms)
                order.extend({"task": task_id, "rep": rep, "provider": provider, "arm": arm} for arm in block_arms)
        templates = {"plan": PLAN_PROMPT, "execute": EXECUTE_PROMPT, "verify": VERIFY_PROMPT, "single": SINGLE_PROMPT}
        manifest = {
            "schema_version": 1,
            "created_at_unix_seconds": int(time.time()),
            "mode": "fake" if self.fake else "host-direct",
            "host_config": "isolated" if self.args.isolated else "user-config",
            "arms": arms,
            "routes": routes,
            "order": order,
            "inputs": self.sealed_inputs(),
            "prompt_templates_sha256": {k: sha256_text(v) for k, v in templates.items()},
            "agent_router_sha256": sha256_file(Path(self.agent_router())),
            "host_versions": {p: self.host_version(p) for p in self.config["providers"]},
        }
        manifest["manifest_sha256"] = sha256_text(json.dumps(manifest, sort_keys=True))
        write_json_atomic(self.manifest_path, manifest)
        log(f"sealed manifest {manifest['manifest_sha256']} with {len(order)} runs")
        for provider, per_task in routes.items():
            recipes = {json.dumps(v, sort_keys=True) for v in per_task.values()}
            log(f"arm A on {provider}: {len(recipes)} distinct per-phase recipe(s): {sorted(recipes)[0]}")

    def check_manifest(self) -> None:
        manifest = self.manifest()
        body = {k: v for k, v in manifest.items() if k != "manifest_sha256"}
        if sha256_text(json.dumps(body, sort_keys=True)) != manifest["manifest_sha256"]:
            sys.exit("manifest hash mismatch: the manifest was edited")
        current = self.sealed_inputs()
        drift = sorted(k for k in set(current) | set(manifest["inputs"]) if current.get(k) != manifest["inputs"].get(k))
        if drift:
            sys.exit(f"sealed inputs changed since init, refusing: {drift}")
        if sha256_file(Path(self.agent_router())) != manifest["agent_router_sha256"]:
            sys.exit("agent-router binary changed since init, refusing")
        for provider in self.providers:
            version = self.host_version(provider)
            if version != manifest["host_versions"][provider]:
                sys.exit(f"{provider} CLI version changed ({manifest['host_versions'][provider]} -> {version}); runs from here on would be invalid")

    # ------------------------------------------------------------ host argv

    def isolated(self) -> bool:
        return self.manifest()["host_config"] == "isolated"

    def argv(self, provider: str, model, effort, write: bool, workspace: Path, isolated: bool | None = None) -> list[str]:
        if isolated is None:
            isolated = self.isolated()
        isolation = self.config["host_isolation_variant"].get(provider, []) if isolated else []
        if provider == "claude":
            argv = self.program("claude") + ["--print", "--output-format", "stream-json", "--verbose",
                                             "--input-format", "text", "--no-session-persistence",
                                             "--permission-prompts", "none",
                                             "--permission-mode", "acceptEdits" if write else "manual",
                                             "--allowedTools", "Read", "Glob", "Grep",
                                             "Bash(python3 -m unittest:*)", "Bash(git diff:*)", "Bash(git status:*)"]
            if model:
                argv += ["--model", model, "--effort", effort]
            return argv + isolation
        argv = self.program("codex") + ["exec", "--json", "--ephemeral", "--color", "never", "--cd", str(workspace),
                                        "--skip-git-repo-check", "--sandbox", "workspace-write" if write else "read-only"]
        if model:
            argv += ["--model", model, "--config", f'model_reasoning_effort="{effort}"']
        return argv + isolation + ["-"]

    # ------------------------------------------------------------ streams

    def parse_stream(self, provider: str, text: str) -> dict:
        events = []
        for line in text.splitlines():
            line = line.strip()
            if line.startswith("{"):
                try:
                    events.append(json.loads(line))
                except json.JSONDecodeError:
                    pass
        out = {"final_text": "", "usage": None, "models": [], "sessions": [], "host_cost_usd": None,
               "errors": [], "usage_events": 0}
        if provider == "claude":
            for event in events:
                kind = event.get("type")
                if kind == "system" and event.get("subtype") == "init":
                    out["sessions"].append(event.get("session_id"))
                elif kind == "assistant" and event.get("parent_tool_use_id") is None:
                    model = (event.get("message") or {}).get("model")
                    if model and model not in out["models"]:
                        out["models"].append(model)
                elif kind == "result":
                    out["usage_events"] += 1
                    out["final_text"] = event.get("result") or ""
                    out["host_cost_usd"] = event.get("total_cost_usd")
                    if event.get("is_error"):
                        out["errors"].append(f"result:{event.get('subtype')}:{event.get('api_error_status')}")
                    usage = event.get("usage") or {}
                    base, write, read = (usage.get(k) for k in ("input_tokens", "cache_creation_input_tokens", "cache_read_input_tokens"))
                    split = usage.get("cache_creation") or {}
                    if isinstance(base, int) and isinstance(usage.get("output_tokens"), int):
                        out["usage"] = {
                            "input_tokens": base + (write or 0) + (read or 0),
                            "cached_input_tokens": read or 0,
                            "cache_write_input_tokens": write or 0,
                            "cache_write_5m_tokens": split.get("ephemeral_5m_input_tokens"),
                            "cache_write_1h_tokens": split.get("ephemeral_1h_input_tokens"),
                            "output_tokens": usage["output_tokens"],
                            "reasoning_output_tokens": None,
                            "thinking_tokens_reported": (usage.get("output_tokens_details") or {}).get("thinking_tokens"),
                        }
            if len(set(out["sessions"])) > 1 or out["usage_events"] > 1:
                out["usage"] = None
        else:
            for event in events:
                kind = event.get("type")
                if kind == "thread.started":
                    out["sessions"].append(event.get("thread_id"))
                elif kind == "item.completed" and (event.get("item") or {}).get("type") == "agent_message":
                    out["final_text"] = event["item"].get("text") or ""
                elif kind == "turn.completed":
                    out["usage_events"] += 1
                    usage = event.get("usage") or {}
                    if all(isinstance(usage.get(k), int) for k in ("input_tokens", "cached_input_tokens", "output_tokens")):
                        out["usage"] = {
                            "input_tokens": usage["input_tokens"],
                            "cached_input_tokens": usage["cached_input_tokens"],
                            # The product decoder (lot 2) does not read this field; Codex 0.159.2
                            # declares it (seen as 0). Absent stays None, never 0.
                            "cache_write_input_tokens": usage["cache_write_input_tokens"] if isinstance(usage.get("cache_write_input_tokens"), int) else None,
                            "output_tokens": usage["output_tokens"],
                            "reasoning_output_tokens": usage.get("reasoning_output_tokens"),
                        }
                elif kind in ("turn.failed", "error"):
                    out["errors"].append(json.dumps(event)[:300])
            if out["usage_events"] > 1:
                out["usage"] = None
        return out

    # ------------------------------------------------------------ pricing

    def cost(self, model, usage) -> dict:
        """product: router_eval::price_usage semantics (cache writes priced as input).
        list: adds the Claude cache write premium from pricing-cache-writes.json."""
        price = self.pricing["models"].get(model or "")
        if usage is None or price is None or usage["cached_input_tokens"] > usage["input_tokens"]:
            return {"product_microusd": None, "list_microusd": None, "reason": "usage_unknown" if usage is None else "model_unpriced_or_invalid"}
        uncached = usage["input_tokens"] - usage["cached_input_tokens"]
        product = (uncached * price["input_microusd_per_million"] + usage["cached_input_tokens"] * price["cached_input_microusd_per_million"]
                   + usage["output_tokens"] * price["output_microusd_per_million"])
        product_micro = -(-product // 1_000_000)
        list_micro = product_micro
        writes = self.cache_writes["models"].get(model)
        if usage.get("cache_write_input_tokens"):
            w5, w1 = usage.get("cache_write_5m_tokens"), usage.get("cache_write_1h_tokens")
            if writes is None or w5 is None or w1 is None:
                list_micro = None
            else:
                premium = w5 * (writes["write_5m"] - price["input_microusd_per_million"]) + w1 * (writes["write_1h"] - price["input_microusd_per_million"])
                list_micro = -(-(product + premium) // 1_000_000)
        return {"product_microusd": product_micro, "list_microusd": list_micro, "reason": None}

    # ------------------------------------------------------------ one run

    def run_phase(self, run_dir: Path, name: str, provider: str, model, effort, write: bool, workspace: Path, prompt: str,
                  isolated: bool | None = None) -> dict:
        argv = self.argv(provider, model, effort, write, workspace, isolated)
        (run_dir / f"{name}.prompt.txt").write_text(prompt)
        env = dict(os.environ)
        if self.fake:
            env["PILOT_FAKE_SOLUTIONS"] = str(HERE / "corpus" / "solutions")
        started = time.monotonic()
        started_wall = time.time()
        with open(run_dir / f"{name}.stream.jsonl", "w") as out, open(run_dir / f"{name}.stderr.txt", "w") as err:
            process = subprocess.Popen(argv, cwd=workspace, stdin=subprocess.PIPE, stdout=out, stderr=err,
                                       text=True, env=env, start_new_session=True)
            try:
                process.communicate(prompt, timeout=self.config["phase_timeout_seconds"])
                timed_out = False
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()
                timed_out = True
        elapsed_ms = int((time.monotonic() - started) * 1000)
        parsed = self.parse_stream(provider, (run_dir / f"{name}.stream.jsonl").read_text())
        requested = model
        announced = parsed["models"]
        if requested is None:
            model_check = "host_default"
        elif not announced:
            model_check = "unknown"
        else:
            model_check = "match" if all(m == requested or m.startswith(requested + "-") for m in announced) else "mismatch"
        priced_model = requested or (announced[0] if announced else None)
        return {"phase": name, "provider": provider, "model": model, "effort": effort, "write": write,
                "exit_code": process.returncode, "timed_out": timed_out, "elapsed_ms": elapsed_ms,
                "started_at_unix": started_wall, "ended_at_unix": time.time(),
                "announced_models": announced, "model_check": model_check, "sessions": len(set(parsed["sessions"])),
                "usage": parsed["usage"], "usage_events": parsed["usage_events"], "host_cost_usd": parsed["host_cost_usd"],
                "errors": parsed["errors"], "final_text_sha256": sha256_text(parsed["final_text"]),
                "final_text_head": parsed["final_text"][:200], "_final_text": parsed["final_text"],
                "cost": self.cost(priced_model, parsed["usage"])}

    def acceptance(self, run_dir: Path, workspace: Path, task: dict) -> dict:
        target = run_dir / "acceptance"
        if target.exists():
            shutil.rmtree(target)
        shutil.copytree(workspace, target, ignore=shutil.ignore_patterns(".git", "__pycache__"))
        shutil.copy(task["hidden_test"], target / "tests" / "test_hidden_acceptance.py")
        started = time.monotonic()
        try:
            out = subprocess.run([self.config["python"], "-m", "unittest", "tests.test_hidden_acceptance"], cwd=target,
                                 capture_output=True, text=True, timeout=self.config["acceptance_timeout_seconds"])
            code, tail = out.returncode, (out.stdout + out.stderr)[-2000:]
        except subprocess.TimeoutExpired:
            code, tail = None, "timeout"
        (run_dir / "acceptance.log").write_text(tail)
        diff = subprocess.run(["git", "status", "--porcelain"], cwd=workspace, capture_output=True, text=True).stdout.splitlines()
        touched = sorted(line[3:] for line in diff)
        outside = [path for path in touched if not path.startswith("pilot_lib/")]
        return {"passed": code == 0, "exit_code": code, "elapsed_ms": int((time.monotonic() - started) * 1000),
                "touched": touched, "touched_outside_scope": outside}

    def execute_run(self, entry: dict) -> dict:
        run_id = f"{entry['provider']}-{entry['arm']}-{entry['task']}-r{entry['rep']}"
        run_dir = self.runs_dir / run_id
        if (run_dir / "result.json").exists():
            return load_json(run_dir / "result.json")
        if run_dir.exists():
            attempt = 1
            while (self.runs_dir / f"{run_id}.interrupted-{attempt}").exists():
                attempt += 1
            os.replace(run_dir, self.runs_dir / f"{run_id}.interrupted-{attempt}")
        run_dir.mkdir(parents=True)
        task = self.task(entry["task"])
        workspace = run_dir / "wt"
        subprocess.run(["git", "clone", "-q", "--no-hardlinks", str(self.results / "base-repo"), str(workspace)], check=True)
        subprocess.run(["git", "remote", "remove", "origin"], cwd=workspace, check=True)
        clean = subprocess.run(["git", "status", "--porcelain"], cwd=workspace, capture_output=True, text=True).stdout.strip() == ""
        fields = {"task_id": task["id"], "statement": task["statement"], "test_command": TEST_COMMAND}
        phases_cfg = self.arm_phases(entry["provider"], entry["arm"], entry["task"])
        started = time.monotonic()
        started_wall = time.time()
        phases = []
        if entry["arm"] in SINGLE_SESSION_ARMS:
            cfg = phases_cfg["plan"]
            phases.append(self.run_phase(run_dir, "single", entry["provider"], cfg["model"], cfg["effort"], True, workspace,
                                         SINGLE_PROMPT.format(**fields)))
        else:
            plan = self.run_phase(run_dir, "plan", entry["provider"], phases_cfg["plan"]["model"], phases_cfg["plan"]["effort"],
                                  False, workspace, PLAN_PROMPT.format(**fields))
            phases.append(plan)
            phases.append(self.run_phase(run_dir, "execute", entry["provider"], phases_cfg["execute"]["model"],
                                         phases_cfg["execute"]["effort"], True, workspace,
                                         EXECUTE_PROMPT.format(plan=plan["_final_text"] or "(the plan phase returned no text)", **fields)))
            phases.append(self.run_phase(run_dir, "verify", entry["provider"], phases_cfg["verify"]["model"],
                                         phases_cfg["verify"]["effort"], False, workspace, VERIFY_PROMPT.format(**fields)))
        accept = self.acceptance(run_dir, workspace, task)
        total_ms = int((time.monotonic() - started) * 1000)
        for phase in phases:
            phase.pop("_final_text", None)
        invalid = []
        if not clean:
            invalid.append("worktree_not_clean_at_start")
        if any(p["model_check"] == "mismatch" for p in phases):
            invalid.append("announced_model_differs")
        if entry["arm"] in SINGLE_SESSION_ARMS and any(p["sessions"] > 1 for p in phases):
            invalid.append("more_than_one_session")
        infra = [p for p in phases if p["timed_out"] or (p["exit_code"] != 0 and p["usage"] is None)]
        if infra:
            invalid.append("infrastructure_failure")
        def total(key):
            values = [p["cost"][key] for p in phases]
            return None if any(v is None for v in values) else sum(values)
        def tokens(key):
            values = [(p["usage"] or {}).get(key) for p in phases]
            return None if any(v is None for v in values) else sum(values)
        verify_text = next((p["final_text_head"] for p in phases if p["phase"] in ("verify", "single")), "")
        result = {
            "run_id": run_id, **entry, "phases": phases, "acceptance": accept,
            "accepted": accept["passed"] and not accept["touched_outside_scope"],
            "agent_verdict": verify_text.strip().split("\n", 1)[0][:20],
            "invalid": invalid, "started_at_unix": started_wall,
            "time_to_acceptance_ms": total_ms,
            "inter_phase_gaps_ms": [int((b["started_at_unix"] - a["ended_at_unix"]) * 1000) for a, b in zip(phases, phases[1:])],
            "cost_product_microusd": total("product_microusd"), "cost_list_microusd": total("list_microusd"),
            "host_cost_usd": None if any(p["host_cost_usd"] is None for p in phases) else sum(p["host_cost_usd"] for p in phases),
            "tokens": {k: tokens(k) for k in ("input_tokens", "cached_input_tokens", "cache_write_input_tokens", "output_tokens", "reasoning_output_tokens")},
            "manifest_sha256": self.manifest()["manifest_sha256"],
        }
        write_json_atomic(run_dir / "result.json", result)
        with open(self.results / "ledger.jsonl", "a") as ledger:
            ledger.write(json.dumps({k: result[k] for k in ("run_id", "accepted", "invalid", "cost_list_microusd", "time_to_acceptance_ms")}) + "\n")
        shutil.rmtree(run_dir / "acceptance", ignore_errors=True)
        return result

    # ------------------------------------------------------------ loops

    def spent_usd(self, provider: str) -> float:
        total = 0
        for path in self.runs_dir.glob(f"{provider}-*/result.json"):
            result = load_json(path)
            value = result["cost_list_microusd"] if result["cost_list_microusd"] is not None else result["cost_product_microusd"]
            if value is None and result.get("host_cost_usd") is not None:
                value = int(result["host_cost_usd"] * 1_000_000)
            total += value or 0
        return total / 1_000_000

    def budget_caps(self) -> dict:
        """Explicit caps only: --budget-cap-usd or PILOT_BUDGET_CAP_USD, e.g. 'claude=40,codex=25'. No default."""
        raw = self.args.budget_cap_usd or os.environ.get("PILOT_BUDGET_CAP_USD", "")
        caps = {}
        for item in filter(None, (part.strip() for part in raw.split(","))):
            provider, _, value = item.partition("=")
            try:
                caps[provider.strip()] = float(value)
            except ValueError:
                sys.exit(f"invalid budget cap {item!r}; expected provider=USD")
            if caps[provider.strip()] <= 0:
                sys.exit(f"budget cap for {provider} must be positive")
        missing = [p for p in self.providers if p not in caps]
        if missing:
            sys.exit(f"`run` refuses to start without an explicit budget cap for {missing}: "
                     "pass --budget-cap-usd 'claude=USD,codex=USD' or set PILOT_BUDGET_CAP_USD")
        return caps

    def run_entries(self, entries: list[dict], caps: dict | None) -> None:
        self.check_manifest()
        self.runs_dir.mkdir(parents=True, exist_ok=True)
        for index, entry in enumerate(entries, 1):
            if entry["provider"] not in self.providers:
                continue
            run_id = f"{entry['provider']}-{entry['arm']}-{entry['task']}-r{entry['rep']}"
            if (self.runs_dir / run_id / "result.json").exists():
                continue
            spent = self.spent_usd(entry["provider"])
            cap = None if caps is None else caps[entry["provider"]]
            if cap is not None and spent >= cap:
                log(f"budget cap reached for {entry['provider']}: {spent:.2f} >= {cap:.2f} USD; skipping its remaining runs")
                continue
            log(f"[{index}/{len(entries)}] {run_id} (spent {entry['provider']}: {spent:.2f} USD)")
            result = self.execute_run(entry)
            if "infrastructure_failure" in result["invalid"] and not (self.runs_dir / f"{run_id}.retried").exists():
                log(f"{run_id}: infrastructure failure, one retry")
                os.replace(self.runs_dir / run_id, self.runs_dir / f"{run_id}.retried")
                result = self.execute_run(entry)
            log(f"{run_id}: accepted={result['accepted']} invalid={result['invalid']} list_cost_usd="
                f"{None if result['cost_list_microusd'] is None else result['cost_list_microusd'] / 1e6}")

    def calibrate(self) -> None:
        order = self.manifest()["order"]
        first = order[0]
        block = [e for e in order if e["task"] == first["task"] and e["rep"] == first["rep"]]
        self.check_manifest()
        self.measure_overhead()
        # The calibration block is bounded by its size (one task, one repetition, every arm), not by a cap.
        self.run_entries(block, None)
        self.report()
        report = load_json(self.results / "report.json")
        runs_per_arm = len(self.config["tasks"]) * self.config["repetitions"]
        log("calibration done; projected list cost of the whole pilot per provider (block cost per arm x runs per arm):")
        for provider, arms in report["per_arm"].items():
            unknown = sum(a["cost_unknown_runs"] for a in arms.values())
            spent = sum(a["cost_list_usd_total"] or 0 for a in arms.values())
            projected = sum((a["cost_list_usd_total"] or 0) / max(a["valid"], 1) * runs_per_arm for a in arms.values())
            note = f"; UNKNOWN for {unknown} run(s) without usage, projection is a floor" if unknown else ""
            log(f"  {provider}: block spent {spent:.4f} USD, projected {projected:.2f} USD{note}")
        log("stopped after the calibration block; choose the caps, then: run --budget-cap-usd 'claude=USD,codex=USD'")

    # ------------------------------------------------------------ report

    def report(self) -> None:
        results = [load_json(p) for p in sorted(self.runs_dir.glob("*/result.json"))
                   if ".interrupted-" not in p.parent.name and not p.parent.name.endswith(".retried")]
        per_arm = {}
        for provider in sorted({r["provider"] for r in results}):
            per_arm[provider] = {}
            for arm in self.manifest()["arms"]:
                rows = [r for r in results if r["provider"] == provider and r["arm"] == arm]
                if not rows:
                    continue
                valid = [r for r in rows if not r["invalid"]]
                accepted = [r for r in valid if r["accepted"]]
                list_costs = [r["cost_list_microusd"] for r in valid]
                product_costs = [r["cost_product_microusd"] for r in valid]
                def ratio(costs):
                    if not accepted or any(c is None for c in costs):
                        return None
                    return round(sum(costs) / len(accepted) / 1e6, 6)
                times = sorted(r["time_to_acceptance_ms"] / 1000 for r in accepted)
                token_totals = {}
                for key in ("input_tokens", "cached_input_tokens", "cache_write_input_tokens", "output_tokens", "reasoning_output_tokens"):
                    values = [r["tokens"][key] for r in valid]
                    token_totals[key] = {"sum_known": sum(v for v in values if v is not None), "unknown_runs": sum(v is None for v in values)}
                per_arm[provider][arm] = {
                    "runs": len(rows), "valid": len(valid), "invalid": len(rows) - len(valid),
                    "invalid_reasons": sorted({reason for r in rows for reason in r["invalid"]}),
                    "accepted": len(accepted), "rejected": len(valid) - len(accepted),
                    "accepted_rate": round(len(accepted) / len(valid), 4) if valid else None,
                    "cost_list_usd_total": None if any(c is None for c in list_costs) else round(sum(list_costs) / 1e6, 6),
                    "cost_product_usd_total": None if any(c is None for c in product_costs) else round(sum(product_costs) / 1e6, 6),
                    "cost_unknown_runs": sum(c is None for c in list_costs),
                    "cost_per_accepted_list_usd": ratio(list_costs),
                    "cost_per_accepted_product_usd": ratio(product_costs),
                    "host_reported_cost_usd": round(sum(r["host_cost_usd"] or 0 for r in valid), 6),
                    "time_to_acceptance_s": {"p50": times[len(times) // 2] if times else None,
                                              "p90": times[min(len(times) - 1, int(0.9 * len(times)))] if times else None},
                    "tokens": token_totals,
                    "model_check_unknown_phases": sum(p["model_check"] == "unknown" for r in valid for p in r["phases"]),
                    "agent_pass_but_rejected": sum(r["agent_verdict"].upper().startswith("PASS") and not r["accepted"] for r in valid),
                }
        sizing = self.sizing(results)
        overhead = self.overhead_report(results)
        report = {"schema_version": 1, "mode": self.manifest()["mode"], "manifest_sha256": self.manifest()["manifest_sha256"],
                  "pricing_version": self.pricing["version"], "host_config": self.manifest()["host_config"],
                  "per_arm": per_arm, "fixed_overhead": overhead, "sizing": sizing,
                  "note": "10 tasks give a direction and variance estimates to fix N; no adoption claim follows from this pilot."}
        write_json_atomic(self.results / "report.json", report)
        lines = ["# Pilot report", "", f"Mode: {report['mode']}. Manifest `{report['manifest_sha256']}`. Pricing `{report['pricing_version']}`.", "",
                 "| Provider | Arm | Valid/runs | Accepted | Cost list USD | Cost/accepted list | Cost/accepted product | p50 time s | Cost unknown |",
                 "| --- | --- | --- | --- | --- | --- | --- | --- | --- |"]
        for provider, arms in per_arm.items():
            for arm, a in arms.items():
                lines.append(f"| {provider} | {arm} | {a['valid']}/{a['runs']} | {a['accepted']} | {a['cost_list_usd_total']} | "
                             f"{a['cost_per_accepted_list_usd']} | {a['cost_per_accepted_product_usd']} | {a['time_to_acceptance_s']['p50']} | {a['cost_unknown_runs']} |")
        lines += ["", f"## Fixed host overhead (active host config: {report['host_config']})", "",
                  "One `Reply with OK` probe per provider, model and host config, read-only, in a clone of the base repo. "
                  "The per-arm share multiplies the probe's list cost by the arm's phase count; it is an estimate, since "
                  "provider prompt caching changes the prefix cost from one call to the next.", "",
                  "```json", json.dumps(overhead, indent=2), "```"]
        lines += ["", "## Sizing inputs for N", "", "```json", json.dumps(sizing, indent=2), "```", "", report["note"], ""]
        (self.results / "report.md").write_text("\n".join(lines))
        log(f"report written: {self.results / 'report.md'}")

    # ------------------------------------------------------------ fixed host overhead

    def measure_overhead(self) -> None:
        """One read-only `Reply with OK` call per provider, pool model and host config (user-config and isolated),
        whatever config the arms use, so the fixed prefix cost of each config is reported apart from the tasks."""
        folder = self.results / "overhead"
        folder.mkdir(parents=True, exist_ok=True)
        for provider in self.providers:
            spec = self.config["providers"][provider]
            for model in (spec["frontier"], spec["cheaper"]):
                for variant in ("user-config", "isolated"):
                    target = folder / f"{provider}-{model}-{variant}"
                    if (target / "result.json").exists():
                        continue
                    shutil.rmtree(target, ignore_errors=True)
                    target.mkdir()
                    workspace = target / "wt"
                    subprocess.run(["git", "clone", "-q", "--no-hardlinks", str(self.results / "base-repo"), str(workspace)], check=True)
                    phase = self.run_phase(target, "probe", provider, model, "medium", False, workspace, OVERHEAD_PROMPT,
                                           isolated=variant == "isolated")
                    phase.pop("_final_text", None)
                    shutil.rmtree(workspace, ignore_errors=True)
                    write_json_atomic(target / "result.json", {"variant": variant, **phase})
                    usage = phase["usage"] or {}
                    log(f"overhead {provider} {model} {variant}: input {usage.get('input_tokens')} "
                        f"(cached {usage.get('cached_input_tokens')}, cache write {usage.get('cache_write_input_tokens')}), "
                        f"list {phase['cost']['list_microusd']} micro-USD")

    def overhead_report(self, results: list[dict]) -> dict:
        probes = {}
        for path in sorted((self.results / "overhead").glob("*/result.json")):
            probe = load_json(path)
            probes[f"{probe['provider']}:{probe['model']}:{probe['variant']}"] = {
                "usage": probe["usage"], "exit_code": probe["exit_code"], "host_cost_usd": probe["host_cost_usd"],
                "list_microusd": probe["cost"]["list_microusd"], "product_microusd": probe["cost"]["product_microusd"]}
        active = self.manifest()["host_config"]
        per_arm = {}
        for provider in sorted({r["provider"] for r in results}):
            for arm in self.manifest()["arms"]:
                rows = [r for r in results if r["provider"] == provider and r["arm"] == arm and not r["invalid"]]
                if not rows:
                    continue
                estimates = []
                for r in rows:
                    values = [(probes.get(f"{provider}:{p['model']}:{active}") or {}).get("list_microusd") for p in r["phases"]]
                    estimates.append(None if any(v is None for v in values) else sum(values))
                arm_total = [r["cost_list_microusd"] for r in rows]
                known = None if any(v is None for v in estimates) else sum(estimates)
                total = None if any(v is None for v in arm_total) else sum(arm_total)
                per_arm[f"{provider}:{arm}"] = {
                    "estimated_overhead_usd": None if known is None else round(known / 1e6, 6),
                    "arm_list_cost_usd": None if total is None else round(total / 1e6, 6),
                    "estimated_share": None if known is None or not total else round(known / total, 4)}
        return {"host_config": active, "probes": probes, "per_arm_estimate": per_arm}

    def sizing(self, results: list[dict]) -> dict:
        """Discordance and log cost ratio SD per pair, then N with the protocol's formulas."""
        z = statistics.NormalDist().inv_cdf
        z_power = z(0.80)
        out = {}
        pairs = [("A", "B"), ("A", "C"), ("B", "E"), ("B", "G"), ("G", "C")]
        valid = {(r["provider"], r["arm"], r["task"], r["rep"]): r for r in results if not r["invalid"]}
        for provider in sorted({k[0] for k in valid}):
            for x, y in pairs:
                keys = sorted({(t, rep) for (p, arm, t, rep) in valid if p == provider and arm == x}
                              & {(t, rep) for (p, arm, t, rep) in valid if p == provider and arm == y})
                if not keys:
                    continue
                disc = sum(valid[(provider, x, t, rep)]["accepted"] != valid[(provider, y, t, rep)]["accepted"] for t, rep in keys) / len(keys)
                logs = []
                for task in sorted({t for t, _ in keys}):
                    cx = [valid[(provider, x, task, rep)]["cost_list_microusd"] for t, rep in keys if t == task]
                    cy = [valid[(provider, y, task, rep)]["cost_list_microusd"] for t, rep in keys if t == task]
                    if cx and cy and all(cx) and all(cy):
                        logs.append(math.log(statistics.fmean(cx) / statistics.fmean(cy)))
                sd = statistics.stdev(logs) if len(logs) >= 2 else None
                row = {"pairs": len(keys), "discordance": round(disc, 4), "log_cost_ratio_sd_per_task": None if sd is None else round(sd, 4)}
                for label, alpha in (("alpha_5", 0.05), ("alpha_1_25", 0.0125)):
                    za = z(1 - alpha)
                    row[f"n_success_10pt_{label}"] = math.ceil((za + z_power) ** 2 * max(disc, 1e-9) / 0.10 ** 2) if disc > 0 else None
                    row[f"n_cost_ratio_0_8_{label}"] = None if sd is None else math.ceil(((za + z_power) * sd / math.log(0.8)) ** 2)
                out[f"{provider}:{x}-vs-{y}"] = row
        return out

    def status(self) -> None:
        manifest = self.manifest()
        done = {p.parent.name for p in self.runs_dir.glob("*/result.json")}
        todo = [e for e in manifest["order"] if f"{e['provider']}-{e['arm']}-{e['task']}-r{e['rep']}" not in done]
        log(f"{len(done)} runs done, {len(todo)} to do; spent " + ", ".join(f"{p} {self.spent_usd(p):.2f} USD" for p in self.config["providers"]))

    def selfcheck(self) -> None:
        """Each hidden test must fail on the base repo and pass with the reference solution."""
        base = self.build_base_repo()
        failures = []
        for task_id in self.config["tasks"]:
            task = self.task(task_id)
            for label, solution in (("base", None), ("solution", HERE / "corpus" / "solutions" / f"{task_id}.py")):
                target = self.results / "selfcheck" / f"{task_id}-{label}"
                shutil.rmtree(target, ignore_errors=True)
                shutil.copytree(base, target, ignore=shutil.ignore_patterns(".git"))
                if solution:
                    shutil.copy(solution, target / task["target"])
                shutil.copy(task["hidden_test"], target / "tests" / "test_hidden_acceptance.py")
                code = subprocess.run([self.config["python"], "-m", "unittest", "tests.test_hidden_acceptance"], cwd=target,
                                      capture_output=True, text=True, timeout=60).returncode
                ok = (code != 0) if label == "base" else (code == 0)
                print(f"{task_id} {label:8} exit={code} {'OK' if ok else 'UNEXPECTED'}")
                if not ok:
                    failures.append(f"{task_id}-{label}")
        shutil.rmtree(self.results / "selfcheck", ignore_errors=True)
        if failures:
            sys.exit(f"selfcheck failed: {failures}")
        log("selfcheck passed: every hidden test fails on base and passes with its reference solution")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("command", choices=["init", "selfcheck", "calibrate", "run", "report", "status"])
    parser.add_argument("--config", default=str(HERE / "pilot-config.json"))
    parser.add_argument("--providers", default="claude,codex")
    parser.add_argument("--fake", action="store_true", help="use fake hosts; no model call, results in results-fake/")
    parser.add_argument("--with-d", action="store_true", help="include the optional host-default arm D at init")
    parser.add_argument("--isolated", action="store_true",
                        help="at init only: add host_isolation_variant options (Claude --safe-mode; Codex --ignore-user-config, "
                             "--ignore-rules, --disable hooks/plugins/apps). Off by default; sealed in the manifest")
    parser.add_argument("--budget-cap-usd", help="required by `run`: 'claude=USD,codex=USD' (or PILOT_BUDGET_CAP_USD); no default")
    args = parser.parse_args()
    pilot = Pilot(args)
    if args.command == "init":
        pilot.init()
    elif args.command == "selfcheck":
        pilot.results.mkdir(parents=True, exist_ok=True)
        pilot.selfcheck()
    elif args.command == "calibrate":
        pilot.calibrate()
    elif args.command == "run":
        pilot.run_entries(pilot.manifest()["order"], pilot.budget_caps())
        pilot.report()
    elif args.command == "report":
        pilot.report()
    else:
        pilot.status()


if __name__ == "__main__":
    main()
