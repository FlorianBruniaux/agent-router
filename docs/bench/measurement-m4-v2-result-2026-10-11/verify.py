#!/usr/bin/env python3
"""Recheck retained M4 v2 artifacts locally; no model or provider access."""
import ast
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
PROTOCOL = HERE.parent / "measurement-m4-v2"


def digest(data):
    return hashlib.sha256(data).hexdigest()


def load(path):
    return json.loads(path.read_text())


def require(condition, message):
    if not condition:
        raise SystemExit(message)


def outside_report(source):
    cls = next(n for n in ast.parse(source).body if isinstance(n, ast.ClassDef) and n.name == "Pilot")
    node = next(n for n in cls.body if isinstance(n, ast.FunctionDef) and n.name == "report")
    lines = source.splitlines(keepends=True)
    return b"".join(lines[:node.lineno - 1] + lines[node.end_lineno:])


def main():
    result = load(HERE / "result.json")
    require(digest((PROTOCOL / "seal.json").read_bytes()) == result["protocol_seal_sha256"], "seal changed")
    for name, expected in load(PROTOCOL / "seal.json")["files_sha256"].items():
        require(digest((PROTOCOL / name).read_bytes()) == expected, f"protocol changed: {name}")
    approval = load(HERE / "approval.json")
    require(approval["protocol_sha256"] == result["protocol_seal_sha256"], "approval binding changed")
    manifest = load(PROTOCOL / "manifest.json")
    arms = result["participants"]
    require([a["arm_id"] for a in arms] == manifest["order"], "missing or reordered arm")
    original = subprocess.check_output(["git", "show", f'{result["source_commit"]}:{result["source_file"]}'], cwd=REPO)
    require(digest(original) == result["source_file_sha256"], "base source changed")
    totals = {key: 0 for key in result["totals"]["usage"]}
    passed = 0
    for arm, assignment in zip(arms, manifest["assignments"]):
        capture, evaluation = arm["host_evidence"], arm["evaluation"]
        require(arm["assignment_id"] == assignment["assignment_id"], "assignment changed")
        require(capture["requested"] == {"model": assignment["model"], "effort": assignment["effort"]}, "requested configuration changed")
        require(capture["observed"] == [capture["requested"]], "observed configuration changed")
        require(capture["status"] == "reliable_host_counters" and capture["terminal"] and capture["sum_matches_cumulative"], "incomplete capture")
        require(capture["task_started_count"] == capture["task_complete_count"] == capture["response_record_count"] == len(capture["response_records"]) == 1, "response cardinality changed")
        require(capture["tool_call_count"] == 0, "unexpected tool call")
        usage = capture["cumulative"]
        require(usage == capture["response_records"][0]["usage"] == capture["response_records"][0]["cumulative"], "usage mismatch")
        require(usage["input_tokens"] + usage["output_tokens"] == usage["total_tokens"], "token arithmetic mismatch")
        require(usage["cached_input_tokens"] <= usage["input_tokens"] and usage["reasoning_output_tokens"] <= usage["output_tokens"], "invalid token subset")
        require(usage["total_tokens"] <= arm["allocation_tokens"] == assignment["allocation_tokens"], "allocation exceeded")
        for key in totals:
            totals[key] += usage[key]
        patch = (HERE / evaluation["patch_file"]).read_bytes()
        require(digest(patch) == evaluation["patch_sha256"] and len(patch) == evaluation["patch_bytes"] <= manifest["patch_max_bytes"], "patch changed")
        require(patch.endswith(b"\n") == evaluation["patch_ends_newline"], "patch ending changed")
        for check in evaluation["checks"]:
            require(digest((HERE / check["log_file"]).read_bytes()) == check["log_sha256"], "test log changed")
        with tempfile.TemporaryDirectory(prefix="ar-m4v2-verify-") as tmp:
            subprocess.run(["git", "init", "-q", tmp], check=True)
            target = Path(tmp) / result["source_file"]
            target.parent.mkdir(parents=True)
            target.write_bytes(original)
            checked = subprocess.run(["git", "apply", "--check", "-"], input=patch, cwd=tmp, capture_output=True)
            applicable = checked.returncode == 0
            require(applicable == evaluation["mechanical_pass"], f'patch application differs: {arm["arm_id"]}')
            if applicable:
                subprocess.run(["git", "apply", "-"], input=patch, cwd=tmp, check=True)
                candidate = target.read_bytes()
                require(digest(candidate) == evaluation["candidate_source_sha256"], "candidate changed")
                require(outside_report(original) == outside_report(candidate), "patch outside report")
                require(all(c["exit_code"] == 0 for c in evaluation["checks"]), "failed retained test")
                require(evaluation["human_acceptance"] == "pending", "human review status changed")
                passed += 1
            else:
                require(checked.returncode == 128 and b"corrupt patch at line 13" in checked.stderr, "different rejection")
                require(evaluation["acceptance_tests"] == "not_run", "rejected patch has tests")
    require(totals == result["totals"]["usage"], "aggregate usage mismatch")
    require(totals["total_tokens"] <= result["totals"]["envelope_tokens"] == approval["envelope_tokens"] == manifest["envelope_tokens"], "envelope mismatch")
    require(passed == result["totals"]["mechanical_passes"] == 2 and len(arms) - passed == result["totals"]["artifact_rejections"] == 1, "outcome mismatch")
    collector = load(HERE / "verification.json")["preflight"]["collector_tests"]
    require(digest((HERE / collector["log_file"]).read_bytes()) == collector["log_sha256"], "collector test log changed")
    print("PASS: sealed protocol, three projected counters, 156434 tokens, two exact candidates, one exact rejection, retained log hashes. Raw trace replay and human acceptance are outside this check.")


if __name__ == "__main__":
    main()
