# T1: keep failed-run costs in the historical pilot report

Base: `e65a22ecb0a294b67a18e7e7ba867b506aba427f`.
The local reproduction records one accepted valid run costing 1 USD and one
invalid run costing 2 USD. `Pilot.report()` reports a total and cost per accepted
task of 1 USD; both must be 3 USD. No provider invocation is needed.

Change only `docs/bench/campaign-next/pilot/run_pilot.py` and optionally add
`docs/bench/campaign-next/tests/test_report_costs.py`.
For each provider and arm, include all recorded runs (valid and invalid) in
the existing list and product cost totals and cost-per-accepted ratios.
Keep accepted/valid/rejected counts unchanged. A missing cost on any included
run makes the corresponding total and ratio null, including an invalid run.
`cost_unknown_runs` counts unknown list costs across those runs. No accepted
run means a null ratio. Preserve the report schema and unrelated behavior.

This bounded task covers records already selected by `report()`. Historical
retry-directory inclusion and a new assignment denominator are separate work.
Acceptance: synthetic accepted/invalid combinations, zero accepted runs and
unknown invalid costs. Add regression tests; do not run the pilot dispatcher.

Review: inspect the diff for scope, unchanged quality counts and no host calls.

M4 uses the same plan, execute and verify phases for every configuration. All acceptance cases are public in this diagnostic. Do not call a provider, spawn another agent, use the network, inspect another configuration, or change global settings. You are not alone in the project: preserve unrelated work and operate only in your assigned checkout. Do not commit or publish.
