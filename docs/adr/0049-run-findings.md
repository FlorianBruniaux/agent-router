---
adr: "0049"
title: Advisory run findings from verified observation
status: implemented
created: 2026-10-29
last_modified: 2026-10-29
scope: "Opt-in observation v5: five deterministic advisory rules over the v4 report (#119); no action, no timing rule."
amended_by: []
supersedes: []
superseded_by: []
related: ["0025", "0048"]
---

# ADR 0049: Advisory run findings from verified observation

## Context

[#119](https://github.com/FlorianBruniaux/agent-router/issues/119) asks for
deterministic findings that point to evidence and name a discriminating check.
No run-level findings engine exists: the feedback advisories
(`router-feedback`) describe route learning per cohort, not one run. The
observation v4 report ([ADR 0048](0048-run-timeline-projection.md)) is the first
verified, bounded and redacted per-run input. The journal has no trusted
timestamp, no tool-call events and no producer for Advisor results.

## Decision

`derive_findings(&RunObservationV4)` is a pure function with no store access,
clock, network or side effect. `observe_run_v5` and `observe --schema-version 5`
return the v4 report with `schema_version: 5`, digest domain
`run-observation-v5` and a `findings` object. Versions 1 to 4 keep their
projection code.

- **Five rules, one outcome each.** Every rule yields exactly one finding or one
  named suppression reason. The rules and reasons are in the
  [guide](../run-observation.md#read-the-v5-findings). Corrupt or oversized input
  refuses the whole observation before any rule runs.
- **Evidence requirements.** A positive finding needs the evidence its rule
  names: validated attempt evidence for failures, a terminal state for cleanup,
  a cancel request and a finished marker for a late result. Incomplete evidence
  suppresses the rule with a reason; it never produces a weaker positive.
- **Content.** A finding states a fixed fact, hypothesis, requirements to
  preserve and a check, plus attempts and event references from the same
  snapshot. Texts are static strings; no event detail, identifier or path is
  copied.
- **Identity.** `id` digests the rule, run key and supporting event hashes under
  `run-finding-v1`. It ignores the journal horizon. `source_snapshot_sha256`
  records the v4 digest the finding came from.
- **v4 amendment.** Findings needed the cancel request that applied to an attempt
  after a later retry cleared it, so each v4 timeline attempt now carries
  `cancel_request_sequence`. No other v4 field changed.
- **Passive.** `automatic_action` is `none`. No finding changes routing,
  approval, execution, admission, feedback or policy.
- **Time.** `attempt_duration` is always suppressed with `timing_not_qualified`
  until #117 qualifies durations.

## Consequences

Humans and tools read the same value; the human view prints each field. A new
rule needs a schema enum entry, a suppression path and negative fixtures.

Counting failures uses the supervisor's recorded termination and exit code, so a
failure of the task that exits zero is not seen. Attempts without evidence cannot
count, which can hide a repeated failure behind `attempt_evidence_incomplete`.
The cleanup rule can fire on a store whose preparation record is absent, because
the existing cleanup reader then reports `unknown`.

## Rejected options

- Tool-call loop and hung-process rules: no verified tool-call evidence, and a
  missing event is not proof of a hang.
- Findings inside `status` or `events`: it would change established contracts.
- A second collector or findings store: another source of truth.
- Elapsed-time rules: wait for qualified timing.
