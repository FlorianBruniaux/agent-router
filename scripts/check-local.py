#!/usr/bin/env python3
"""Run selected local checks sequentially and retain their evidence outside Git.

Python 3.11+, standard library only. No installation or real provider calls.
The report describes this host and selected commands, never full CI equivalence.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shlex
import shutil
import signal
import stat
import subprocess
import sys
import tempfile
import time

GROUPS = ("quality", "native", "audit", "packaging", "fuzz")
ROOT = Path(__file__).resolve().parents[1]
PYTHON = str(Path(sys.executable).resolve())


def build_checks(root: Path, output: Path, groups: list[str]) -> list[dict]:
    checks = []

    def add(group, name, argv, **options):
        if group in groups:
            checks.append({"id": name, "group": group, "argv": argv, **options})

    add("quality", "format", ["cargo", "fmt", "--all", "--", "--check"])
    add("quality", "clippy-all", ["cargo", "clippy", "--workspace", "--all-targets", "--all-features", "--locked", "--", "-D", "warnings"])
    add("quality", "clippy-core", ["cargo", "clippy", "-p", "agent-router-cli", "--all-targets", "--no-default-features", "--locked", "--", "-D", "warnings"])
    add("quality", "workspace-tests", ["cargo", "test", "--workspace", "--all-features", "--locked", "--", "--test-threads=1"], require_tests=True)
    add("quality", "local-api-absent", ["cargo", "test", "-p", "agent-router-cli", "--no-default-features", "--locked", "--test", "local_api_absent"], require_tests=True)
    add("quality", "harness", ["cargo", "run", "-p", "xtask", "--locked", "--", "check"])
    add("quality", "host-syntax", [PYTHON, "-B", "scripts/check-host-syntax.py"])
    if "quality" in groups:
        scripts = sorted((root / "scripts").glob("test-*.py"))
        for script in scripts:
            add("quality", "script-" + script.stem, [PYTHON, "-B", script.relative_to(root).as_posix()], require_tests=True)
        if not scripts:
            add("quality", "script-tests-missing", [], unavailable="no scripts/test-*.py suites were found")
    for name, folder, pattern in [("egress-tests", "docs/bench/egress-admission", "test_*.py"),
                                  ("native-probe-tests", "docs/bench/native-integration-u02", "test_*.py"),
                                  ("native-decision-tests", "docs/bench/native-integration-u06", "test_*.py"),
                                  ("native-hook-failure-tests", "docs/bench/native-integration-u07", "test_*.py"),
                                  ("campaign-tests", "docs/bench/campaign-next/tests", "test_*.py"),
                                  ("calibration-tests", "docs/bench/calibration-v2/tests", "test_*.py")]:
        repo_env = {"native-decision-tests": "AR_U06_REPO_ROOT", "native-hook-failure-tests": "U07_REPO_ROOT"}
        options = {"env": {repo_env[name]: str(root)}} if name in repo_env else {}
        add("quality", name, [PYTHON, "-B", "-m", "unittest", "discover", "-s", folder, "-p", pattern, "-v"], require_tests=True, **options)
    for mode in ("dir", "git"):
        add("quality", f"secrets-{mode}", ["gitleaks", mode, ".", "--config", ".gitleaks.toml", "--redact", "--no-banner"])
    sandbox = "sandbox-exec" if sys.platform == "darwin" else "bwrap"
    native_options = {"requires": [sandbox]}
    if sys.platform not in ("darwin", "linux"):
        native_options = {"not_applicable": "native sandbox checks require macOS or Linux"}
    add("native", "native-build", ["cargo", "build", "-p", "agent-router-cli", "-p", "router-sandbox-helper", "--all-features", "--locked"], **native_options)
    add("native", "native-ignored", ["cargo", "test", "--workspace", "--all-features", "--locked", "--", "--ignored", "--test-threads=1", "--nocapture"],
        depends_on="native-build", require_tests=True, **native_options)
    if sys.platform != "linux":
        for name, package, suite, test in [
            ("linux-lifetime", "router-sandbox-helper", "lifetime", "sigkill_bubblewrap_parent_stops_setsid_descendant"),
            ("linux-network", "router-runtime", "runtime", "native_bubblewrap_read_write_and_network_canary")]:
            add("native", name, ["cargo", "test", "-p", package, "--test", suite, test, "--locked", "--", "--ignored", "--exact", "--nocapture"],
                not_applicable="Linux-only sandbox regression is not evidenced on this host")
        add("native", "linux-address-space", ["cargo", "test", "-p", "router-sandbox-helper", "--test", "limits", "address_space_limit_rejects_a_real_allocation", "--locked", "--", "--exact"],
            not_applicable="Linux-only address_space_limit_rejects_a_real_allocation is not evidenced on this host")
    add("audit", "audit-version", ["cargo-audit", "--version"], expected_version="cargo-audit 0.22.2")
    add("audit", "dependency-audit", ["cargo-audit", "audit", "--file", "Cargo.lock", "--db", str(output / "advisory-db"), "--deny", "warnings"],
        depends_on="audit-version", env={"CARGO_HOME": str(output / "audit-home")}, advisory_db=str(output / "advisory-db"))
    add("packaging", "fetch-locked", ["cargo", "fetch", "--locked"])
    archive = str(output / "core.tar.gz")
    add("packaging", "package-core", [PYTHON, "-B", "scripts/package-core.py", "--distribution", "core", "--strict-licenses", "--output", archive], depends_on="fetch-locked")
    add("packaging", "verify-core", [PYTHON, "-B", "scripts/package-core.py", "--verify", archive], depends_on="package-core")
    add("fuzz", "parser-fuzz", ["cargo", "+nightly-2026-09-12", "fuzz", "build", "--fuzz-dir", "fuzz"],
        unavailable="fuzz execution is unsupported until isolated corpus/artifact handling is designed; requires existing cargo-fuzz 0.13.2 and nightly-2026-09-12; nothing is installed")
    return checks


def digest(path: Path) -> str | None:
    return hashlib.sha256(path.read_bytes()).hexdigest() if path.is_file() else None


def git_text(root: Path, args: list[str]) -> str | None:
    try:
        result = subprocess.run(["git", "-C", str(root), *args], capture_output=True, text=True, timeout=10, check=False)
        return result.stdout.strip() if result.returncode == 0 else None
    except (OSError, subprocess.TimeoutExpired):
        return None


def snapshot(root: Path) -> dict:
    status = git_text(root, ["status", "--porcelain=v1", "--untracked-files=all"])
    source = {"source_inventory": None, "source_inventory_sha256": None, "source_sha256": None}
    try:
        listing = subprocess.run(["git", "-C", str(root), "ls-files", "--cached", "--others", "--exclude-standard", "-z"],
                                 capture_output=True, timeout=10, check=False)
        if listing.returncode == 0:
            names = sorted(set(name for name in listing.stdout.split(b"\0") if name))
            inventory = []
            for raw_name in names:
                name = os.fsdecode(raw_name)
                path = root / name
                record = {"path": name}
                try:
                    info = path.lstat()
                    record["mode"] = stat.S_IMODE(info.st_mode)
                    if stat.S_ISLNK(info.st_mode):
                        record.update(kind="symlink", target=os.readlink(path))
                    elif stat.S_ISREG(info.st_mode):
                        record.update(kind="file", sha256=digest(path))
                    else:
                        raise OSError(f"source inventory contains a non-file: {name}")
                except FileNotFoundError:
                    record["kind"] = "missing"
                inventory.append(record)
            encoded = json.dumps(inventory, sort_keys=True, separators=(",", ":")).encode()
            source = {"source_inventory": inventory,
                      "source_inventory_sha256": hashlib.sha256(b"\0".join(names)).hexdigest(),
                      "source_sha256": hashlib.sha256(encoded).hexdigest()}
    except (OSError, subprocess.TimeoutExpired) as error:
        source["source_error"] = str(error)
    return {"head_sha": git_text(root, ["rev-parse", "HEAD"]), **source,
            "dirty": bool(status) if status is not None else None, "git_status": status,
            "lock_sha256": digest(root / "Cargo.lock"), "fuzz_lock_sha256": digest(root / "fuzz/Cargo.lock")}


def stop_process(proc: subprocess.Popen) -> None:
    try:
        if os.name == "posix":
            os.killpg(proc.pid, signal.SIGTERM)
        else:
            proc.terminate()
    except ProcessLookupError:
        pass  # The child may exit between the timeout/interrupt and this signal.
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        if os.name != "posix":
            proc.kill()
    # The leader can exit on TERM while its descendant ignores that signal.
    # Kill the session's remaining group even when the leader was already reaped.
    if os.name == "posix":
        try:
            os.killpg(proc.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
    proc.wait()


def run_check(root: Path, output: Path, check: dict, env: dict, timeout: float, proxy: str | None) -> dict:
    start = time.monotonic()
    result = {"id": check["id"], "group": check["group"], "argv": check["argv"],
              "executed_argv": None, "exit_code": None, "status": "unavailable", "duration_seconds": 0.0}
    for flag, status in [("not_applicable", "not_applicable"), ("unavailable", "unavailable")]:
        if flag in check:
            return {**result, "status": status, "reason": check[flag]}
    missing = [tool for tool in [check["argv"][0], *check.get("requires", [])] if shutil.which(tool, path=env.get("PATH")) is None]
    if missing:
        return {**result, "reason": "missing executable: " + ", ".join(missing)}
    argv = [proxy, "proxy", *check["argv"]] if proxy else check["argv"]
    result["executed_argv"] = argv
    log_path = output / (check["id"] + ".log")
    child_env = {**env, **check.get("env", {})}
    with log_path.open("xb") as log:
        try:
            proc = subprocess.Popen(argv, cwd=root, env=child_env, stdout=log, stderr=subprocess.STDOUT,
                                    start_new_session=os.name == "posix")
            try:
                result["exit_code"] = proc.wait(timeout=timeout)
                result["status"] = "passed" if proc.returncode == 0 else "failed"
            except subprocess.TimeoutExpired:
                stop_process(proc)
                result.update(status="timed_out", exit_code=proc.returncode, reason=f"timeout after {timeout} seconds")
            except KeyboardInterrupt:
                stop_process(proc)
                result.update(status="interrupted", exit_code=proc.returncode, reason="interrupted by user")
        except OSError as error:
            result["reason"] = str(error)
    result.update(duration_seconds=round(time.monotonic() - start, 6), log=log_path.name, log_sha256=digest(log_path))
    text = log_path.read_text(errors="replace")
    if result["status"] == "passed" and check.get("expected_version"):
        result["version"] = text.strip()
        if text.strip() != check["expected_version"]:
            result.update(status="failed", reason="installed tool does not match pinned CI version")
    if result["status"] == "passed" and check.get("require_tests"):
        rust_count = sum(map(int, re.findall(r"test result: ok\. (\d+) passed", text)))
        python_count = sum(map(int, re.findall(r"Ran (\d+) tests?", text)))
        python_skipped = sum(map(int, re.findall(r"^OK \([^\n]*\bskipped=(\d+)", text, re.MULTILINE)))
        result["tests_passed_or_run"] = rust_count + max(0, python_count - python_skipped)
        result["python_tests_skipped"] = python_skipped
        if result["tests_passed_or_run"] == 0:
            result.update(status="failed", reason="no executed tests were evidenced in the log")
    if check.get("advisory_db"):
        database = Path(check["advisory_db"]).resolve()
        database_root = git_text(database, ["rev-parse", "--show-toplevel"])
        result["advisory_db_sha"] = git_text(database, ["rev-parse", "HEAD"]) if database_root and Path(database_root).resolve() == database else None
        result["fresh_advisory_db"] = result["advisory_db_sha"] is not None
        if result["status"] == "passed" and not result["fresh_advisory_db"]:
            result.update(status="failed", reason="fresh advisory database retrieval was not evidenced")
    return result


def execute(root: Path, output: Path, checks: list[dict], *, timeout: float = 3600,
            proxy: str | None = None, env: dict | None = None) -> int:
    # An existing destination is never reused, even if empty.
    output.mkdir(parents=True, exist_ok=False)
    child_env = dict(os.environ if env is None else env)
    child_env.setdefault("CARGO_TARGET_DIR", str(output / "target"))
    child_env["PYTHONDONTWRITEBYTECODE"] = "1"
    report = {"schema_version": 1, "root": str(root), "output_dir": str(output),
              "platform": platform.platform(), "python": PYTHON, "full_ci_equivalent": False,
              "coverage_gaps": ["This run covers one host only; the other CI operating system is not evidenced.",
                                "Container/release workflows and real provider integrations are outside this runner."],
              "cargo_target_dir": child_env["CARGO_TARGET_DIR"], "runner_sha256": digest(Path(__file__)),
              "before": snapshot(root), "checks": []}
    interrupted = False
    for check in checks:
        previous = {item["id"]: item["status"] for item in report["checks"]}
        dependency = check.get("depends_on")
        if interrupted or (dependency and previous.get(dependency) != "passed"):
            result = {"id": check["id"], "group": check["group"], "argv": check["argv"],
                      "executed_argv": None, "exit_code": None, "duration_seconds": 0.0,
                      "status": "cancelled" if interrupted else "blocked",
                      "reason": "prior interruption" if interrupted else f"dependency {dependency} did not pass"}
        else:
            print(f"Running {check['id']}: {shlex.join(check['argv'])}", flush=True)
            result = run_check(root, output, check, child_env, timeout, proxy)
        report["checks"].append(result)
        interrupted |= result["status"] == "interrupted"
        if result["status"] not in ("passed",):
            report["coverage_gaps"].append(f"{result['id']}: {result.get('reason', result['status'])}")
        print(f"{result['id']}: {result['status']}", flush=True)
    report["after"] = snapshot(root)
    success = any(item["status"] == "passed" for item in report["checks"]) and all(item["status"] in ("passed", "not_applicable") for item in report["checks"])
    report["verdict"] = "selected_checks_passed" if success else "incomplete_or_failed"
    if report["before"]["lock_sha256"] != report["after"]["lock_sha256"] or report["before"]["fuzz_lock_sha256"] != report["after"]["fuzz_lock_sha256"]:
        success = False
        report["verdict"] = "incomplete_or_failed"
        report["coverage_gaps"].append("A dependency lockfile changed during verification.")
    if any(report[point][field] is None for point in ("before", "after") for field in ("head_sha", "source_sha256")):
        success = False
        report["coverage_gaps"].append("The source snapshot is unavailable; verification cannot be bound to source bytes.")
    elif any(report["before"][field] != report["after"][field] for field in ("head_sha", "source_sha256")):
        success = False
        report["coverage_gaps"].append("HEAD or source contents/modes/symlinks changed during verification.")
    if not success:
        report["verdict"] = "incomplete_or_failed"
    with (output / "summary.json").open("x") as handle:
        json.dump(report, handle, indent=2)
        handle.write("\n")
    print(f"Evidence: {output / 'summary.json'}", flush=True)
    return 130 if interrupted else (0 if success else 1)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--list", action="store_true", help="list selected commands without executing or creating output")
    parser.add_argument("--group", action="append", choices=GROUPS, help="repeatable; default: all groups")
    parser.add_argument("--output-dir", type=Path, help="new directory outside the checkout; never overwritten")
    parser.add_argument("--timeout", type=float, default=3600, help="positive timeout in seconds per command (default: 3600)")
    args = parser.parse_args(argv)
    if sys.version_info < (3, 11):
        parser.error("Python 3.11 or newer is required")
    if args.timeout <= 0 or not args.timeout < float("inf"):
        parser.error("--timeout must be a finite positive number")
    groups = list(dict.fromkeys(args.group or GROUPS))
    if args.list:
        for check in build_checks(ROOT, Path("<OUTPUT>"), groups):
            print(f"{check['group']}/{check['id']}: {shlex.join(check['argv'])}")
            if check.get("unavailable") or check.get("not_applicable"):
                print("  " + str(check.get("unavailable") or check.get("not_applicable")))
        return 0
    if args.output_dir:
        output = args.output_dir.expanduser().resolve()
        if output == ROOT or ROOT in output.parents:
            parser.error("--output-dir must be outside the checkout")
    else:
        # execute owns creation; retain the unique directory's parent until it does.
        parent = Path(tempfile.mkdtemp(prefix="agent-router-checks-"))
        output = parent / "evidence"
    try:
        return execute(ROOT, output, build_checks(ROOT, output, groups), timeout=args.timeout, proxy=shutil.which("rtk"))
    except FileExistsError:
        parser.error(f"output directory already exists: {output}")
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
