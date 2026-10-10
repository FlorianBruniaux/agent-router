# Cost through an accepted revision

Calculate a report from a supplied offline evidence file:

```sh
cargo run -p agent-router-cli --locked -- eval accepted-changes \
  --input tests/fixtures/evaluation/accepted-changes-v1.json --json
```

Run from a checkout of this revision. The committed input is synthetic. This
command reads that file, emits JSON and opens no feedback or runtime store.
It does not launch Claude, Codex, a provider, or a verification command.

## What the report answers

An accepted revision A does not automatically accept a later revision B.
Selecting B retains A's acceptance history and its earlier cost frontier while
reporting B's own status. A later revocation names the exact acceptance and
changes its current verdict without deleting history.

The cohort keeps every assigned task, including failures and assignments with
no run. Each physical request or job has an explicit authority namespace and
identity. Repeated snapshots count once; an actual retry counts again. Sharing
an external request identifier cannot merge independent physical work.

Model, CI and supplied human costs remain separate. Shared CI costs count once
for the whole cohort; no per-task allocation is invented. The numerator of a
cost-per-accepted-change ratio includes all cohort work, including failed tasks
and later attempts. The denominator counts the explicitly selected accepted
revisions. Zero accepted revisions yields a null ratio.

`known_subtotal_microusd` preserves the supplied covered amount.
`total_microusd` stays null when inventory, usage, pricing or an expected
observation is incomplete. A missing observation is not zero. Complete empty
inventory and missing inventory have different results. No human hourly rate
or subscription-to-USD conversion is supplied by the calculator.

## Reading the synthetic example

The fixture includes an accepted A, a later unverified B, a failed assignment,
an assignment without a run and a CI job shared by two assignments. It tests
calculation and joins only. Its amounts are fabricated test inputs, not measured
prices or usage. The CLI and library tests check the key amounts, bindings and failure
cases; [ADR 0024](adr/0024-accepted-revision-accounting.md) defines the identities,
frontiers and coverage rules.

The analysis-level `cohort_costs` includes later work. Historical acceptance
entries separately report `cohort_costs_at_frontier` and
`assignment_model_cost_at_frontier`. The latter excludes another assignment's
model work. Expected observations without assignment attribution make that
per-assignment total conservatively incomplete. Historical cohort snapshots
must not be summed or described as the price of individual revisions.

## Supply real evidence later

The standalone input and output both use schema version 1. The producer seals
all assignments with `accepted_cohort_sha256`, supplies exact source, candidate,
revision and criteria digests, and selects one revision per assignment. Ordered
verdict history and inventories define the analysis snapshot and cost frontiers.
Work that produces no artifact uses null candidate/revision fields rather than
inventing a patch. Full field and hash rules are in the ADR.

Every report states `evidence_trust: supplied_unverified`. Content hashes detect
inconsistent joins; they do not authenticate the provider, prove reviewer
independence, establish that no assignment was omitted, or verify a patch on
disk. The current calculator sums supplied amounts. It does not reprice tokens,
collect native usage, reconcile invoices, or derive elapsed time.

The [attempt-correlation CLI](attempt-correlation.md) now links supplied attempt and physical-call identities. It does not collect usage or feed amounts into this calculator. Connecting qualified cost observations remains the next measurement slice of
[issue #5](https://github.com/FlorianBruniaux/agent-router/issues/5). A claim that
routing saves time or money also needs the separate
[comparison protocol](bench/router-value-protocol.md) and its approved inputs.
The existing [feedback report and summary](feedback.md) retain their schemas
and historical acceptance semantics.

## Failure behavior

Use `--json` for the complete machine-readable report; without it, the CLI uses
its existing human-output adapter. It refuses unsupported versions, unknown or
duplicate keys, inconsistent identities, changed bindings, invalid sequences and integer
overflow with `eval.accepted_changes.invalid` (exit 2). Input is capped at 4 MiB
before decoding; an oversized file uses the existing input-size refusal. The
library also bounds assignment and evidence counts. No failure creates a store
or changes an approval.
