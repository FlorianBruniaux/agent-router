#!/usr/bin/env python3
"""Verify pinned source, historical artifacts and the existing calculator."""
import argparse
import json
from pathlib import Path
import subprocess
import sys
import tempfile

from project import HERE, REPO, RESULTS, project, require


def check_report(report):
    require(report["evidence_trust"] == "supplied_unverified", "calculator evidence trust")
    require(report["selected_accepted_changes"] == 1, "calculator accepted count")
    require({row["assignment_id"]: row["status"] for row in report["selected"]} == {
        "m4v2-t1-router-r1": "accepted", "m4v2-t1-cheapfixed-r1": "pending",
        "m4v2-t1-strongfixed-r1": "unknown",
    }, "calculator exact selected verdicts")
    require(len(report["cohort"]["assignments"]) == 3, "calculator assigned denominator")
    require(report["observations"] == [], "physical identities were invented")
    require([v["status"] for v in report["verdict_history"]]
            == ["pending", "pending", "accepted"], "calculator historical verdicts")
    for kind in ("model", "ci", "human"):
        total = report["cohort_costs"][kind]
        require(total["total_microusd"] is None and total["inventory_coverage"] == "unknown",
                f"unknown {kind} total became known")
        require(report["cost_per_selected_accepted_change"][kind] is None,
                f"unknown {kind} ratio became known")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", required=True, type=Path,
                        help="Existing agent-router executable; this script never builds or installs it")
    parser.add_argument("--write", action="store_true", help="Write the three derived JSON files after verification")
    args = parser.parse_args()
    projection = project()
    historical = subprocess.run([sys.executable, "-B", str(REPO / RESULTS / "verify.py")],
                                cwd=REPO, capture_output=True, text=True, check=False)
    require(historical.returncode == 0, f"historical verifier: {historical.stdout}{historical.stderr}")
    with tempfile.TemporaryDirectory(prefix="ar-m4-accepted-") as temporary:
        input_path = Path(temporary) / "accepted-changes-input.json"
        input_path.write_text(json.dumps(projection["accepted_changes_input"]) + "\n")
        calculated = subprocess.run([str(args.cli.resolve()), "eval", "accepted-changes",
                                     "--input", str(input_path), "--json"],
                                    cwd=REPO, capture_output=True, text=True, check=False)
        require(calculated.returncode == 0, f"accepted-changes calculator: {calculated.stderr}")
        report = json.loads(calculated.stdout)
        check_report(report)
        repeated = subprocess.run([str(args.cli.resolve()), "eval", "accepted-changes",
                                  "--input", str(input_path), "--json"],
                                 cwd=REPO, capture_output=True, text=True, check=False)
        require(repeated.returncode == 0 and repeated.stdout == calculated.stdout,
                "calculator output is not deterministic")
    snapshots = {"dossier.json": projection["dossier"],
                 "accepted-changes-input.json": projection["accepted_changes_input"],
                 "accepted-changes-report.json": report}
    for name, value in snapshots.items():
        path = HERE / name
        encoded = json.dumps(value, indent=2, ensure_ascii=False) + "\n"
        if args.write:
            path.write_text(encoded)
        else:
            require(path.read_text() == encoded, f"derived file changed: {name}")
    print("PASS: pinned M4 artifacts, all three host attempts, one exact Router acceptance; "
          "existing calculator deterministic, monetary totals/ratios UNKNOWN. "
          "No provider, qualified evaluator or efficiency trial ran.")


if __name__ == "__main__":
    main()
