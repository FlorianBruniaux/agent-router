#!/usr/bin/env python3
"""Repetition-filtered `run` over the sealed pilot manifest, without resealing it.

`run_pilot.py` is a sealed input of the manifest, so this filter lives in a
separate, unsealed file and imports it unchanged. It keeps only the manifest
entries whose repetition is <= --repetitions, reuses every run that already has
a result.json, records the filter in results/run-filter.jsonl (with this file's
SHA-256) and in the report, and still requires an explicit budget cap.

Commands: plan (read-only count), run, report.
"""
from __future__ import annotations

import argparse
import json
import time

from run_pilot import HERE, Pilot, load_json, log, sha256_file, write_json_atomic

VARIANCE_NOTE = ("Only repetition(s) <= {n} of the sealed order were run: variance between repetitions is not measured, "
                 "and the sizing inputs mix task variance with run-to-run noise.")


class FilteredPilot(Pilot):
    def __init__(self, args):
        super().__init__(args)
        if args.repetitions < 1:
            raise SystemExit("--repetitions must be >= 1")
        self.repetitions = args.repetitions

    def planned(self) -> list[dict]:
        return [e for e in self.manifest()["order"] if e["rep"] <= self.repetitions and e["provider"] in self.providers]

    def done(self, entries: list[dict]) -> list[str]:
        ids = [f"{e['provider']}-{e['arm']}-{e['task']}-r{e['rep']}" for e in entries]
        return [i for i in ids if (self.runs_dir / i / "result.json").exists()]

    def filter_record(self, command: str) -> dict:
        planned = self.planned()
        return {"at_unix": int(time.time()), "command": command, "manifest_sha256": self.manifest()["manifest_sha256"],
                "sealed_order_runs": len(self.manifest()["order"]), "repetitions": self.repetitions,
                "providers": self.providers, "planned_runs": len(planned), "already_done": len(self.done(planned)),
                "extension": "run_r1.py", "extension_sha256": sha256_file(HERE / "run_r1.py")}

    def plan(self) -> None:
        self.check_manifest()
        record = self.filter_record("plan")
        log(f"repetition filter <= {self.repetitions}: {record['planned_runs']} of {record['sealed_order_runs']} sealed runs planned, "
            f"{record['already_done']} already done, {record['planned_runs'] - record['already_done']} to run")

    def run(self) -> None:
        caps = self.budget_caps()
        self.check_manifest()
        record = self.filter_record("run")
        record["budget_cap_usd"] = caps
        self.results.mkdir(parents=True, exist_ok=True)
        with open(self.results / "run-filter.jsonl", "a") as journal:
            journal.write(json.dumps(record, sort_keys=True) + "\n")
        log(f"repetition filter <= {self.repetitions} recorded in run-filter.jsonl: {record['planned_runs']} planned, "
            f"{record['already_done']} already done and skipped")
        self.run_entries(self.planned(), caps)
        self.report()

    def report(self) -> None:
        super().report()
        path = self.results / "report.json"
        report = load_json(path)
        journal = self.results / "run-filter.jsonl"
        records = [json.loads(line) for line in journal.read_text().splitlines() if line.strip()] if journal.exists() else []
        report["repetition_filter"] = {"repetitions": self.repetitions, "journal": records}
        report["variance_between_repetitions"] = "not_measured"
        report["note"] = report["note"] + " " + VARIANCE_NOTE.format(n=self.repetitions)
        write_json_atomic(path, report)
        md = self.results / "report.md"
        md.write_text(md.read_text() + "\n## Repetition filter\n\n" + VARIANCE_NOTE.format(n=self.repetitions)
                      + f" Filter journal: `run-filter.jsonl` ({len(records)} record(s)). The manifest was not resealed.\n")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("command", choices=["plan", "run", "report"])
    parser.add_argument("--repetitions", type=int, required=True, help="keep manifest entries with rep <= this value")
    parser.add_argument("--config", default=str(HERE / "pilot-config.json"))
    parser.add_argument("--providers", default="claude,codex")
    parser.add_argument("--fake", action="store_true")
    parser.add_argument("--budget-cap-usd", help="required by `run`: 'claude=USD,codex=USD' (or PILOT_BUDGET_CAP_USD); no default")
    args = parser.parse_args()
    args.with_d = False
    args.isolated = False
    pilot = FilteredPilot(args)
    getattr(pilot, args.command)()


if __name__ == "__main__":
    main()
