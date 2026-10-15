#!/usr/bin/env python3
"""Project one pinned historical dossier. No provider or cost calculation."""
import hashlib
import json
from pathlib import Path
import subprocess


HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
BASELINE = "a93b0f731915559bef18e40caf2ca8966a3caacb"
INTEGRATION = "38f2b2d04f84eed3b10e9b29d3f25b36b98abc06"
PROTOCOL = "docs/bench/measurement-m4-v2"
RESULTS = "docs/bench/measurement-m4-v2-result-2026-10-11"


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def compact(value):
    return json.dumps(value, ensure_ascii=False, separators=(",", ":")).encode()


def require(condition, reason):
    if not condition:
        raise ValueError(reason)


def git_bytes(revision, path):
    return subprocess.check_output(["git", "show", f"{revision}:{path}"], cwd=REPO)


def build(result, manifest, receipt, source_hashes):
    """Use existing assignment IDs and byte identities, never physical IDs."""
    participants = result["participants"]
    assigned = manifest["assignments"]
    require(len(participants) == len(assigned) == 3, "missing assigned work")
    require([p["assignment_id"] for p in participants]
            == [a["assignment_id"] for a in assigned], "assignment join")
    require([p["arm_id"] for p in participants] == manifest["order"], "arm join")
    response_ids = [record["response_identity_sha256"] for participant in participants
                    for record in participant["host_evidence"]["response_records"]]
    require(len(set(response_ids)) == len(response_ids), "duplicate host response")
    require(receipt["selected_candidate"] == "router"
            and receipt["accepted_change_count"] == 1
            and receipt["decision"] == "accept_for_integration", "acceptance scope")
    require(receipt["decision_source"] == "explicit_user_request", "acceptance provenance")
    router = participants[0]
    evaluation = router["evaluation"]
    require(evaluation["mechanical_pass"]
            and evaluation["candidate_source_sha256"] == receipt["candidate_file_sha256"]
            and evaluation["patch_sha256"] == receipt["candidate_patch_sha256"],
            "accepted revision substitution")
    require(result["source_file"] == receipt["candidate_file"], "accepted file scope")
    require(result["source_file_sha256"]
            == manifest["source_files_sha256"][result["source_file"]], "source join")
    criteria = source_hashes[f"{PROTOCOL}/manifest.json"]
    assignments = []
    revisions = []
    selections = []
    verdicts = []
    attempts = []
    bindings = {}
    for index, (participant, assignment) in enumerate(zip(participants, assigned), 1):
        capture = participant["host_evidence"]
        require(capture["terminal"] and capture["sum_matches_cumulative"]
                and capture["response_record_count"] == len(capture["response_records"]) == 1,
                "incomplete host response")
        require(capture["requested"] == {"model": assignment["model"], "effort": assignment["effort"]}
                and capture["observed"] == [capture["requested"]], "model identity join")
        usage = capture["cumulative"]
        require(usage == capture["response_records"][0]["usage"]
                and usage == capture["response_records"][0]["cumulative"]
                and usage["input_tokens"] + usage["output_tokens"] == usage["total_tokens"],
                "host usage join")
        assignments.append({
            "assignment_id": assignment["assignment_id"], "start_sequence": index,
            "task_start_sha256": digest(compact(assignment)),
            "source_sha256": result["source_file_sha256"], "criteria_sha256": criteria,
        })
        evaluated = participant["evaluation"]
        revision = evaluated.get("candidate_source_sha256")
        require(bool(revision) == bool(evaluated["mechanical_pass"]), "candidate availability")
        selections.append({"assignment_id": assignment["assignment_id"], "revision_sha256": revision})
        if revision:
            require(evaluated["human_acceptance"] == "pending", "historical verdict changed")
            binding = {
                "assignment_id": assignment["assignment_id"],
                "source_sha256": result["source_file_sha256"], "criteria_sha256": criteria,
                "candidate_sha256": evaluated["patch_sha256"], "revision_sha256": revision,
            }
            bindings[participant["arm_id"]] = binding
            revisions.append({"sequence": 3 + len(revisions) + 1, "binding": binding,
                              "predecessor_sha256": None})
            verdicts.append({"sequence": 6 + len(verdicts), "binding": binding,
                             "status": "pending", "verifier_outcome": "passed",
                             "verifier_sha256": digest(compact(evaluated)),
                             "reviewer_sha256": None, "acceptance_sequence": None,
                             "cost_frontier_sequence": None})
        attempts.append({
            "assignment_id": assignment["assignment_id"], "arm_id": participant["arm_id"],
            "model_effort": capture["requested"],
            "host_response_identity_sha256": capture["response_records"][0]["response_identity_sha256"],
            "host_usage": usage, "host_task_timestamps": participant["host_task_timestamps"],
            "artifact_sha256": evaluated["patch_sha256"], "revision_sha256": revision,
            "historical_human_acceptance": evaluated["human_acceptance"],
            "artifact_outcome": "applicable" if revision else evaluated["status"],
            "current_acceptance": "accepted" if participant["arm_id"] == "router"
                                  else "pending" if revision else "unknown_no_revision",
            "provider_physical_identity": None,
        })
    require(sum(a["host_usage"]["total_tokens"] for a in attempts)
            == result["totals"]["usage"]["total_tokens"] == 156434, "cohort usage total")
    require(result["totals"]["human_accepted"] == 0, "historical acceptance overwritten")
    verdicts.append({
        "sequence": 8, "binding": bindings["router"], "status": "accepted",
        "verifier_outcome": "passed", "verifier_sha256": digest(compact(evaluation)),
        "reviewer_sha256": source_hashes[f"{RESULTS}/integration-acceptance.json"],
        "acceptance_sequence": None, "cost_frontier_sequence": 8,
    })
    cohort = {"complete": True, "assignments": assignments}
    accepted = {
        "schema_version": 1, "cohort": cohort,
        "cohort_sha256": digest(b"accepted-changes/cohort/v1\0" + compact(cohort)),
        "analysis_sequence": 8, "selections": selections, "revisions": revisions,
        "verdicts": verdicts,
        "inventories": [{"kind": kind, "coverage": "unknown", "expected": []}
                        for kind in ("model", "ci", "human")],
        "observations": [],
    }
    dossier = {
        "schema_version": 1, "kind": "existing_data_offline_projection",
        "source_baseline": BASELINE, "protocol_id": result["protocol_id"],
        "source_files_sha256": source_hashes,
        "cohort": {"scope": "M4 v2 sealed participant cohort only",
                   "assigned_count": len(assigned), "host_attempt_count": len(attempts),
                   "host_tokens": 156434, "physical_provider_attempt_count": None},
        "attempts": attempts, "accepted_revision_count": 1,
        "acceptance": {"receipt": f"{RESULTS}/integration-acceptance.json",
                       "binding": bindings["router"], "integration_commit": INTEGRATION,
                       "revision_scope": result["source_file"],
                       "equivalent_output_is_not_second_acceptance": True},
        "separate_prior_scope": {"scope": "M4 v1 coordination, not v2 participant work",
                                 "host_tokens": result["separate_overhead"]["prior_v1_host_tokens"]},
        "incomplete_scopes": {"preparation_coordination_publication_tokens": None,
                              "integration_usage_tokens": None, "estimated_money_microusd": None,
                              "billed_money_microusd": None, "subscription_credits": None,
                              "ci_cost_microusd": None, "end_to_end_elapsed_seconds": None,
                              "human_effort_seconds": None, "human_cost_microusd": None},
        "comparison": {"verdict": "inconclusive", "qualified_evaluator_evidence": None,
                       "required_case_comparison": None, "selection_advantage": None,
                       "financial_savings": None,
                       "reason": "Historical acceptance is retained; #174 qualification and comparable required-case evidence are missing. Router and cheapfixed share model/effort; injected context/cache differ."},
        "projection_limits": [
            "Host response identities are not physical provider request identities. No physical observation is invented.",
            "Projection sequences are local append positions, not observed timestamps or native event IDs.",
            "task_start_sha256 hashes each exact sealed manifest assignment object with compact JSON; it is a derived identity.",
            "candidate_sha256 hashes exact returned patch bytes; revision_sha256 hashes exact resulting file bytes, not a Git commit.",
            "criteria_sha256 hashes the sealed full manifest, including no repair, scope, tests and final review.",
            "verifier_sha256 hashes the retained evaluation object; reviewer_sha256 hashes the later explicit-user receipt. Neither authenticates an independent evaluator.",
            "The calculator receives no supplied money or physical observations. Its zero known subtotals are empty sums, never measured zero consumption.",
            "Unknown inventories preserve null totals and ratios. No CI/human inventory is declared complete or empty.",
            "The rejected exact artifact has no produced revision; its selected calculator verdict is unknown, while artifact rejection remains explicit here.",
        ],
    }
    return {"dossier": dossier, "accepted_changes_input": accepted}


def project():
    paths = [f"{PROTOCOL}/manifest.json", f"{RESULTS}/result.json",
             f"{RESULTS}/integration-acceptance.json", f"{PROTOCOL}/seal.json",
             f"{RESULTS}/approval.json", f"{RESULTS}/verification.json"]
    values = {}
    hashes = {}
    for path in paths:
        raw = (REPO / path).read_bytes()
        require(raw == git_bytes(BASELINE, path), f"source changed from baseline: {path}")
        values[path] = json.loads(raw)
        hashes[path] = digest(raw)
    result = values[f"{RESULTS}/result.json"]
    receipt = values[f"{RESULTS}/integration-acceptance.json"]
    require(digest(git_bytes(result["source_commit"], result["source_file"]))
            == result["source_file_sha256"], "original source bytes")
    require(digest(git_bytes(INTEGRATION, result["source_file"]))
            == receipt["candidate_file_sha256"], "integrated exact revision")
    return build(result, values[f"{PROTOCOL}/manifest.json"], receipt, hashes)


if __name__ == "__main__":
    print(json.dumps(project(), indent=2, ensure_ascii=False))
