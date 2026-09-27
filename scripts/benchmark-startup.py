#!/usr/bin/env python3
"""Measure every launch, including the first, before inspecting the executable.

This is a local diagnostic, not a CI gate or an OS-cold-cache claim. A candidate
may print STARTUP_ENTRY_UNIX_NS and STARTUP_MAIN_NS on stderr to separate its
main function from process startup. No security settings or xattrs are changed.
"""
import argparse
import datetime
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import resource
import subprocess
import time


def distribution(samples):
    ordered = sorted(samples)
    return {"count": len(samples), "first_ms": samples[0],
            "p50_ms": ordered[math.ceil(len(samples) * .50) - 1],
            "p95_ms": ordered[math.ceil(len(samples) * .95) - 1],
            "max_ms": ordered[-1],
            "over_150_ms": [{"sample": index + 1, "elapsed_ms": sample}
                            for index, sample in enumerate(samples) if sample >= 150]}


def launch(binary, argument):
    usage_before = resource.getrusage(resource.RUSAGE_CHILDREN)
    wall_start = time.time_ns()
    start = time.perf_counter_ns()
    termination = "exited"
    try:
        result = subprocess.run([str(binary), argument], stdin=subprocess.DEVNULL,
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                timeout=10, check=False)
        exit_code, stdout, stderr = result.returncode, result.stdout, result.stderr
    except subprocess.TimeoutExpired as error:
        termination, exit_code = "timeout", None
        stdout, stderr = error.stdout or b"", error.stderr or b""
    except OSError:
        termination, exit_code, stdout, stderr = "spawn_failed", None, b"", b""
    elapsed = (time.perf_counter_ns() - start) / 1_000_000
    usage_after = resource.getrusage(resource.RUSAGE_CHILDREN)
    output_limit_exceeded = len(stdout) + len(stderr) > 64 * 1024
    markers = {}
    for line in stderr[:64 * 1024].decode(errors="replace").splitlines():
        for marker in ("STARTUP_ENTRY_UNIX_NS", "STARTUP_MAIN_NS"):
            if line.startswith(marker + "="):
                try:
                    markers[marker] = int(line.split("=", 1)[1])
                except ValueError:
                    pass
    return {"elapsed_ms": elapsed, "termination": termination,
            "exit_code": exit_code, "output_limit_exceeded": output_limit_exceeded,
            "user_cpu_ms": (usage_after.ru_utime - usage_before.ru_utime) * 1000,
            "system_cpu_ms": (usage_after.ru_stime - usage_before.ru_stime) * 1000,
            "pre_main_ms": ((markers["STARTUP_ENTRY_UNIX_NS"] - wall_start) / 1_000_000
                            if "STARTUP_ENTRY_UNIX_NS" in markers else None),
            "main_ms": (markers["STARTUP_MAIN_NS"] / 1_000_000
                        if "STARTUP_MAIN_NS" in markers else None),
            "stdout_sha256": hashlib.sha256(stdout).hexdigest()}


def signature(binary):
    if platform.system() != "Darwin":
        return None
    # Deliberately after all timed launches: signature verification may warm OS
    # state and cannot precede the first-launch observation.
    result = subprocess.run(["/usr/bin/codesign", "-d", "--verbose=4", str(binary)],
                            capture_output=True, check=False)
    accepted = ("Format=", "CodeDirectory ", "Signature=", "Page size=",
                "CDHash=", "TeamIdentifier=")
    return {"inspection_exit_code": result.returncode,
            "fields": [line for line in result.stderr.decode(errors="replace").splitlines()
                       if line.startswith(accepted)]}


def xattr_names(binary):
    if hasattr(os, "listxattr"):
        return sorted(os.listxattr(binary))
    if platform.system() == "Darwin":
        result = subprocess.run(["/usr/bin/xattr", str(binary)], capture_output=True, check=False)
        return sorted(result.stdout.decode().splitlines()) if result.returncode == 0 else None
    return None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--label", required=True)
    parser.add_argument("--source-sha256", required=True)
    parser.add_argument("--samples", type=int, default=100)
    parser.add_argument("--argument", choices=("--version", "--help"), default="--version")
    options = parser.parse_args()
    if not 10 <= options.samples <= 1000:
        parser.error("sample count must be 10..1000")
    if len(options.source_sha256) != 64 or any(c not in "0123456789abcdef" for c in options.source_sha256):
        parser.error("source fingerprint must be a lowercase SHA256 digest")
    if options.output.exists():
        parser.error("refusing to overwrite an earlier measurement")
    load_before = os.getloadavg()
    samples = []
    # Persist each observation before any optional metadata collection. A later
    # collector error must not lose the first-launch evidence.
    with options.output.with_suffix(".launches.jsonl").open("x") as journal:
        for _ in range(options.samples):
            sample = launch(options.binary, options.argument)
            samples.append(sample)
            journal.write(json.dumps(sample) + "\n")
            journal.flush()
            if sample["exit_code"] != 0 or sample["output_limit_exceeded"]:
                break
    report = {
        "schema_version": 1, "label": options.label,
        "measured_at_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "source_sha256": options.source_sha256,
        "machine": {"os": platform.system(), "release": platform.release(),
                    "architecture": platform.machine(), "logical_cpus": os.cpu_count()},
        "conditions": {"load_average_before": load_before, "load_average_after": os.getloadavg(),
                       "other_work_paused": False, "cold_os_cache": "unknown",
                       "security_settings_changed": False,
                       "boundary": "parent spawn through exit; captured stdout/stderr; no shell",
                       "pre_main_clock": "child SystemTime minus parent time.time_ns; wall-clock adjustment not controlled",
                       "first_launch_preceded_metadata_inspection": True},
        "binary": {"sha256": hashlib.sha256(options.binary.read_bytes()).hexdigest(),
                   "bytes": options.binary.stat().st_size,
                   "xattr_names": xattr_names(options.binary),
                   "signature": signature(options.binary)},
        "argument": options.argument, "requested_samples": options.samples, "samples": samples,
        "summary": distribution([sample["elapsed_ms"] for sample in samples]),
        "threshold_ms": 150, "acceptance": "observations only; no cold-cache or signed distribution proof",
    }
    with options.output.open("x") as output:
        output.write(json.dumps(report, indent=2) + "\n")
    print(json.dumps({"label": options.label, "summary": report["summary"],
                      "first_sample": samples[0], "binary": report["binary"]}, indent=2))


if __name__ == "__main__":
    main()
