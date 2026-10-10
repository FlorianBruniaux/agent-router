Produce one unified diff correcting Pilot.report() in docs/bench/campaign-next/pilot/run_pilot.py. The task and exact unmodified method are supplied below. Limit edits to this method. Retain the schema and all unrelated behavior. Do not add a test file: the operator owns the frozen acceptance tests.

For this approved experiment only, use no tools, skills, file reads, shell commands, network calls or subagents. Do not load instructions from disk. Answer once with the unified diff only, without a Markdown fence, explanation or skill footer. If the supplied information is insufficient, answer INSUFFICIENT_CONTEXT. No follow-up or repair turn is allowed. This participant-specific exception requires approval of the v2 protocol; it changes no global configuration.

# T1: keep failed-run costs in the historical pilot report

Base: `e65a22ecb0a294b67a18e7e7ba867b506aba427f`.
The local reproduction records one accepted valid run costing 1 USD and one
invalid run costing 2 USD. `Pilot.report()` reports a total and cost per accepted
task of 1 USD; both must be 3 USD. No provider invocation is needed.

Return a unified diff changing only `Pilot.report()` in
`docs/bench/campaign-next/pilot/run_pilot.py`. Do not add a test file.
For each provider and arm, include all recorded runs (valid and invalid) in
the existing list and product cost totals and cost-per-accepted ratios.
Keep accepted/valid/rejected counts unchanged. A missing cost on any included
run makes the corresponding total and ratio null, including an invalid run.
`cost_unknown_runs` counts unknown list costs across those runs. No accepted
run means a null ratio. Preserve the report schema and unrelated behavior.

This bounded task covers records already selected by `report()`. Historical
retry-directory inclusion and a new assignment denominator are separate work.
Acceptance: synthetic accepted/invalid combinations, zero accepted runs and
unknown invalid costs. The operator runs the frozen tests; do not run the pilot dispatcher.

Review: inspect the diff for scope, unchanged quality counts and no host calls.

M4 v2 requests one final patch response per configuration, with no tool use or follow-up. All acceptance cases are public. Do not inspect the workspace, call a provider, spawn an agent or load a skill. Do not modify files, commit or publish. The operator applies and tests the exact returned patch separately. Preserve unrelated behavior.

Source excerpt, original file lines 707-768:

```python
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
```
