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
