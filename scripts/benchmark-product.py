#!/usr/bin/env python3
"""Reproducible local capacity measurements, never a blocking CI timing gate."""
import argparse
import datetime
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tempfile
import time


ROOT = Path(__file__).resolve().parents[1]


def checked(args, *, env=None):
    result = subprocess.run(args, cwd=ROOT, env=env, stdin=subprocess.DEVNULL,
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=False)
    if result.returncode:
        # Cargo diagnostics stay on stderr, separate from the shareable JSON report.
        raise RuntimeError(f"command failed: {args[0]}\n{result.stderr.decode(errors='replace')}")
    return result


def peak_process(args):
    """Use wait4 child rusage, not Python's cumulative maximum across children."""
    if not hasattr(os, "wait4"):
        return checked(args), None
    with tempfile.TemporaryFile() as stdout, tempfile.TemporaryFile() as stderr:
        process = subprocess.Popen(args, cwd=ROOT, stdin=subprocess.DEVNULL,
                                   stdout=stdout, stderr=stderr)
        _, status, usage = os.wait4(process.pid, 0)
        process.returncode = os.waitstatus_to_exitcode(status)
        stdout.seek(0)
        stderr.seek(0)
        result = subprocess.CompletedProcess(args, process.returncode, stdout.read(), stderr.read())
    if result.returncode:
        raise RuntimeError(f"measurement child failed: {result.stderr.decode(errors='replace')}")
    multiplier = 1 if platform.system() == "Darwin" else 1024
    return result, int(usage.ru_maxrss) * multiplier


def distribution(samples):
    ordered = sorted(samples)
    return {"samples": len(samples), "first_ms": samples[0],
            "p50_ms": ordered[math.ceil(len(ordered) * .50) - 1],
            "p95_ms": ordered[math.ceil(len(ordered) * .95) - 1],
            "max_ms": ordered[-1], "samples_ms": samples,
            "over_150_ms": [{"sample": index + 1, "elapsed_ms": elapsed}
                            for index, elapsed in enumerate(samples) if elapsed >= 150]}


def source_fingerprint():
    digest = hashlib.sha256()
    count = 0
    for root in (ROOT / "crates", ROOT / "fixtures" / "examples"):
        for path in sorted(root.rglob("*")):
            if path.is_file() and path.suffix in (".rs", ".toml", ".json"):
                digest.update(str(path.relative_to(ROOT)).encode())
                digest.update(b"\0")
                digest.update(path.read_bytes())
                count += 1
    for path in (ROOT / "Cargo.toml", ROOT / "Cargo.lock"):
        digest.update(path.name.encode())
        digest.update(b"\0")
        digest.update(path.read_bytes())
        count += 1
    return {"sha256": digest.hexdigest(), "files": count}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target-dir", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--samples", type=int, default=1000)
    parser.add_argument("--startup-samples", type=int, default=100)
    parser.add_argument("--skip-build", action="store_true")
    options = parser.parse_args()
    if not 100 <= options.samples <= 100_000 or not 10 <= options.startup_samples <= 1000:
        parser.error("samples outside documented measurement bounds")
    target = (options.target_dir or Path(tempfile.mkdtemp(prefix="agent-router-perf-"))).resolve()
    target.mkdir(mode=0o700, parents=True, exist_ok=True)
    cli_before = target / "release" / "agent-router"
    cli_before_hash = hashlib.sha256(cli_before.read_bytes()).hexdigest() if cli_before.is_file() else None
    env = dict(os.environ, CARGO_TARGET_DIR=str(target.resolve()))
    before = source_fingerprint()
    load_before = os.getloadavg() if hasattr(os, "getloadavg") else None
    build_start = time.perf_counter()
    if not options.skip_build:
        cargo = ["rtk", "cargo"] if shutil.which("rtk") else ["cargo"]
        for package, selector in [
            ("router-policy", ["--example", "benchmark_phase"]),
            ("router-retrieval", ["--example", "benchmark"]),
            ("agent-router-cli", ["--bin", "agent-router"]),
        ]:
            checked([*cargo, "build", "-p", package, "--release", *selector,
                     "--locked", "--offline"], env=env)
    build_seconds = time.perf_counter() - build_start
    release = target / "release"
    # First invocation follows the build but caches are neither flushed nor controlled.
    startup = {}
    for flag in ("--version", "--help"):
        durations = []
        for _ in range(options.startup_samples):
            start = time.perf_counter_ns()
            checked([str(release / "agent-router"), flag])
            durations.append((time.perf_counter_ns() - start) / 1_000_000)
        startup[flag] = distribution(durations)
    phase, phase_rss = peak_process([str(release / "examples" / "benchmark_phase"), str(options.samples)])
    retrieval, retrieval_rss = peak_process([str(release / "examples" / "benchmark"), "10000"])
    _, startup_rss = peak_process([str(release / "agent-router"), "--version"])
    after = source_fingerprint()
    paths = {"cli": release / "agent-router", "phase": release / "examples" / "benchmark_phase",
             "retrieval": release / "examples" / "benchmark"}
    report = {
        "schema_version": 1,
        "measured_at_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "machine": {"os": platform.system(), "os_release": platform.release(),
                    "macos_version": platform.mac_ver()[0] or None,
                    "architecture": platform.machine(), "logical_cpus": os.cpu_count()},
        "toolchain": {"rustc": checked(["rustc", "--version"]).stdout.decode().strip(),
                      "cargo": checked(["cargo", "--version"]).stdout.decode().strip()},
        "build": {"profile": "release", "locked": True, "offline": True,
                  "default_features": True, "elapsed_seconds": build_seconds,
                  "profile_overrides": {key: os.environ[key] for key in
                                        ("CARGO_PROFILE_RELEASE_LTO", "CARGO_PROFILE_RELEASE_STRIP")
                                        if key in os.environ},
                  "skipped": options.skip_build,
                  "cli_existed_before_build": cli_before_hash is not None,
                  "cli_sha256_before_build": cli_before_hash,
                  "binary_sha256": {key: hashlib.sha256(path.read_bytes()).hexdigest() for key, path in paths.items()}},
        "source_before": before, "source_after": after, "source_changed": before != after,
        "conditions": {"load_average_before": load_before,
                       "load_average_after": os.getloadavg() if hasattr(os, "getloadavg") else None,
                       "concurrency": "benchmark subprocesses sequential; other development work and OS processes not paused",
                       "startup_boundary": "new process spawn through exit, stdout/stderr captured, no shell",
                       "cold_process": True, "cold_os_cache": "unknown; caches not flushed",
                       "first_process": ("existing release binary; caches not controlled" if options.skip_build
                                         else "after release build; neither linker caches nor file cache controlled"),
                       "rss_method": "wait4 child ru_maxrss; Darwin bytes, Linux KiB normalized to bytes; includes construction, indexing and queries",
                       "percentile_method": "nearest rank ceil(p*n)-1"},
        "phase": json.loads(phase.stdout), "retrieval": json.loads(retrieval.stdout),
        "startup": startup,
        "peak_rss_bytes": {"phase": phase_rss, "retrieval_10000": retrieval_rss, "cli_version": startup_rss},
        "thresholds": {"phase_p95_ms": 100, "retrieval_p95_ms": 50, "cli_startup_ms": 150},
        "acceptance": "observations only; inspect samples and scope before claiming an NFR",
    }
    options.output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
