#!/usr/bin/env python3
"""Export the pilot results into this folder as metrics only.

Usage: python3 export_results.py <pilot-dir> [--label NAME]

Copies manifest.json, ledger.jsonl, run-filter.jsonl and routes/ unchanged, the
current report as reports/<label>-report.{json,md}, and a reduced JSON per run and
per overhead probe. Never copied: host streams (*.stream.jsonl), prompts
(*.prompt.txt, the execute prompt embeds the model's plan), host stderr,
acceptance logs and worktrees (model-written code). In each phase the model text
excerpt (final_text_head) is dropped, host error events become a count, and the
agent verdict is normalized to PASS, FAIL or OTHER. The export refuses to write a
file that contains a home directory path.
"""
from __future__ import annotations

import argparse
import json
import shutil
from pathlib import Path

HERE = Path(__file__).resolve().parent
FORBIDDEN = ("/Users/", "/home/")


def verdict(text: str) -> str:
    head = (text or "").strip().upper()
    return "PASS" if head.startswith("PASS") else "FAIL" if head.startswith("FAIL") else "OTHER"


def reduce_phase(phase: dict) -> dict:
    out = {k: v for k, v in phase.items() if k not in ("final_text_head", "errors", "_final_text")}
    out["error_count"] = len(phase.get("errors") or [])
    return out


def reduce_run(result: dict) -> dict:
    out = {k: v for k, v in result.items() if k not in ("phases", "agent_verdict")}
    out["agent_verdict"] = verdict(result.get("agent_verdict", ""))
    out["phases"] = [reduce_phase(p) for p in result["phases"]]
    return out


def write_checked(path: Path, text: str) -> None:
    for marker in FORBIDDEN:
        if marker in text:
            raise SystemExit(f"refusing to export {path.name}: contains a home directory path")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("pilot_dir", type=Path)
    parser.add_argument("--label", default="final")
    args = parser.parse_args()
    source = args.pilot_dir / "results"
    target = HERE / "results"
    for name in ("manifest.json", "ledger.jsonl", "run-filter.jsonl"):
        if (source / name).exists():
            write_checked(target / name, (source / name).read_text())
    for path in sorted((source / "routes").glob("*.json")):
        write_checked(target / "routes" / path.name, path.read_text())
    for suffix in ("json", "md"):
        if (source / f"report.{suffix}").exists():
            write_checked(target / "reports" / f"{args.label}-report.{suffix}", (source / f"report.{suffix}").read_text())
    runs = sorted(p for p in (source / "runs").glob("*/result.json"))
    shutil.rmtree(target / "runs", ignore_errors=True)
    for path in runs:
        reduced = reduce_run(json.loads(path.read_text()))
        write_checked(target / "runs" / f"{path.parent.name}.json", json.dumps(reduced, indent=2, sort_keys=True) + "\n")
    probes = sorted((source / "overhead").glob("*/result.json"))
    for path in probes:
        reduced = reduce_phase(json.loads(path.read_text()))
        write_checked(target / "overhead" / f"{path.parent.name}.json", json.dumps(reduced, indent=2, sort_keys=True) + "\n")
    print(f"exported {len(runs)} run(s), {len(probes)} overhead probe(s), label {args.label}")


if __name__ == "__main__":
    main()
