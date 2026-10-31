---
adr: "0051"
title: Advisory route for one normalized task
status: implemented
created: 2026-10-28
last_modified: 2026-10-28
scope: "The `advise` command and its version 1 report. Advisory only: no execution, permission, budget admission or capability proof."
amended_by: []
supersedes: []
superseded_by: []
related: []
---

# ADR 0051: Advisory route for one normalized task

## Context

Issue #217 asks for a path from one real task to a recommendation or an
explained refusal, with the next manual action in the host. On `bb2d71a`,
`task-guide` exports a `TaskInput` (`crates/agent-router-cli/src/task_guide.rs`)
and `route` reads a `RouteRequest` (`crates/router-types/src/task.rs`). No
command converts one into the other, so a user had to write the request by hand.

Two facts constrain the design. A cache-only Codex observation keeps `tools`
`unknown` (`crates/agent-router-cli/tests/codex_catalog.rs`), so a task that
requires `tools` gets `no_route`. `route` replaces request capabilities with the
snapshot's and refuses any non-unknown capability without a proof verifier
(`ValidatedHostObservation::from_observation_with_verifier`). Operator
attestations keep support `unknown` in the observation and only phase routing
with an explicit acceptance list consumes them. Neither path lets an operator
state a fact for advisory use.

## Decision

Add `agent-router advise`. It normalizes the task with the existing risk
normalizer, builds a `RouteRequest` with an explicit field mapping, routes it
with `Router::stable_v1` and returns a version 1 report with `status`,
`questions`, `mapping`, `facts`, `skills`, `decision`, `next_manual_action` and
`output_directory`, plus `mode: "advisory"` and `executed: false`.

- Host facts come from exactly one source: a validated observation
  (`discovered`), a generated catalog's embedded observation, or an operator
  `HostSnapshot` file (`declared`). In a declared file, an unlisted capability
  stays `unknown`.
- `--declare-capability` overlays one capability as supported and labels it
  `declared`. With an overlay, the decision is not bound to the snapshot hash.
  `network` cannot be declared.
- A host model absent from the catalog is refused (`advisory.model.unknown`).
- `--skill` adds operator candidates with source `operator-declared` and score
  0. Each requested skill is reported with its status; an absent skill is never
  suggested.
- Scope, budget, mutations, sensitivity, deliverables, constraints and
  dependencies are reported as `not_enforced_by_advisory_routing` with their
  values. The routed risk is the normalizer's effective risk.
- `--output-directory` publishes `task.json`, optional `risk-context.json`,
  `route-request.json`, `decision.json` and `advice.json` through the same
  no-clobber private publication as `task-guide`.

## Consequences

The owner can route a personal task offline and replay the saved request with
`route`. The decision contract and the router are unchanged. A declared
capability can produce a recommendation that a verified one would not; the
report and summary say so. The command has no Codex-specific code path: any catalog
using the existing contracts is read the same way. It was not run against the
Claude Code catalog from #218, which is absent from this branch. Retrieval indexes,
proof stores and budget admission stay out of this command.

## Rejected options

- A documented recipe only: the `TaskInput` to `RouteRequest` conversion and
  the provenance labels would be hand-written and unverifiable.
- Feeding `capability attest` records into `advise`: it needs an execution
  configuration and a proof store, and attestations deliberately keep support
  `unknown`.
- Treating unlisted declared capabilities as unsupported, as `route` does:
  absence of a declaration is not a declaration of absence.
