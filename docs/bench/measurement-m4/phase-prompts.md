# Shared participant instructions

These templates are part of the sealed protocol. The coordinator substitutes only the assignment workspace path, the full contents of `task.md`, the phase name and the previous plan for the same assignment. Model/effort selection belongs to the tool arguments, not to a claim made by the worker. Record the exact rendered prompt digest before dispatch. Use the same active project instructions and tools for all arms.

## Common message

You are working on one assigned Agent Router checkout at `{assignment_workspace}`. Use that absolute path for all file and test operations. You are not alone in the project: preserve unrelated work and never write outside your assigned paths. Do not read another assignment, call a provider, use network tools, create another agent, change global configuration, commit or publish. Use only the task statement, this checkout and the permitted output from the previous phase. Any missing tool or permission is a failure to report, not permission to work around a boundary.

Task statement, identical for every configuration:

`{task_statement}`

The available acceptance cases are public. Preserve the reference verifier and all existing tests. Final acceptance is decided separately on the exact patch, not by your self-report. Report an inability to complete; do not claim a test passed unless you ran it.

## Plan phase

Read only. Inspect the report and relevant tests. Return a plan of at most 8 KiB with the minimal implementation steps, regression cases and checks. Do not modify files. This is one assignment, not an invitation to redesign the pilot or fix other defects.

## Execute phase

Plan for this same assignment: `{previous_plan}`.

Implement the stated task, changing only the 2 allowed paths. Add regression tests and run the existing campaign test suite plus the new tests. Stop with the resulting patch and a factual test summary. No delegation, retry after final submission or scope expansion. All intermediate model turns and tool work belong to this attempt.

## Verify phase

Read only. Inspect this assignment's resulting diff against the task, run the unchanged campaign tests and the added tests, and report PASS, FAIL or UNKNOWN with evidence. Do not repair the patch. Do not inspect results from another configuration or supply a final acceptance decision.

## Independent final review

Use a fresh `default` reviewer with `gpt-6.1-sol/high`, no inherited history, no implementation role and no write permission in its assignment instructions. Give it only the task, an anonymous patch label, the base code, the candidate patch and the frozen criteria. Do not pass arm name, author model, the internal verifier verdict or another review. The same host tool set still does not prove OS isolation or complete blinding.

Review the patch for correctness, missing-cost handling, preserved quality counts, scope, tests and absence of provider calls. Return APPROVE, REJECT or UNKNOWN with concrete evidence. Do not repair. The coordinator combines this review with the independent deterministic tests and binds acceptance to the exact patch digest. Record review consumption even if the patch is rejected.

## Coordinator

Start fresh with `gpt-6.1-sol/medium` and `fork_turns="none"`. Read this protocol and the approval receipt, never inherit the preparation conversation. Record all 3 assignments before starting. Follow the sealed order and the per-phase allocations. Do not modify the task or solve it yourself. No paid probe, retry, fallback, extra reviewer or expansion to another task.

Resolve each returned agent ID to an exact local trace before continuing. Keep raw traces private and export only allowed counters and hashes. If this mapping or any counter/model identity is unavailable, stop the block with UNKNOWN and preserve the attempt. Do not use an estimated model price or missing usage as zero. Do not add parent and child consumption into a measured total without evidence of disjoint scopes. A conservative duplicated budget charge is allowed only as the separately named operational stop measure.

If a phase fails terminally or exceeds its allocation, record it and leave dependent phases not started. If the infrastructure, telemetry, identity or protocol changes, stop the whole block. For a functional failure with complete telemetry and respected allocations, continue the remaining independent assignments in their reserved budgets, without repairing the failed one. No next phase is admitted after the common envelope is reached.
