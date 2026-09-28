# ADR 0007: Durable cleanup evidence for preparation

Status: implemented and independently reviewed; delivery checks tracked in the PR.

## Problem

A runtime preparation executes Git commands and may launch a sandboxed native
version probe before the model attempt starts. Before schema 8 the probe had a
cleanup lease, but its identity was not bound to the run journal. Recovery treated
`Preparing` without a model-attempt lease as safe. That state alone cannot prove
that these earlier processes have stopped. It must not release locks or future
shared admission capacity.

## Decision

Journal a monotonically sequenced preparation cycle in the same transaction as
the queued-to-preparing transition. Bind it to the run, definition hash, workflow
generation and expected next model attempt. Use its own sequence: failed
preparations do not consume model attempts and may share that expected number.

Schema 8 stores the cycle projection and anchors every update in a
`preparation.cycle.v1` event. Table-only changes, deletion, or a different run
definition do not establish cleanup. This local hash chain does not authenticate
history against an owner able to rewrite the entire database.

The cycle starts `untracked`. The owning task can explicitly record
`lease_only` before any preparation I/O, while it still knows no child has
started. Recovery cannot synthesize that transition. The protocol permits at
most 1,024 cycles per run, 1,024 Git captures and eight probes per cycle, with a
16 KiB maximum cycle event. Probe identifiers contain hashes, not process output,
prompts or command arguments.

Each Git capture receives an internal observer. Before spawn, the observer must
durably mark the current cycle `untracked`; a failed journal operation prevents
spawn. Only a successful return from the process capture closes this window,
before interpreting the Git exit code or its output. A capture error or a crash
inside the window leaves cleanup unknown. The ordinary public Git helpers retain
their interfaces; runtime preparation uses the observed variants, including each
preliminary repository-configuration query.

Between such windows, the phase is `lease_only`. Every native version probe
registers its exact lease identity and probe ID against the cycle before capture.
Its cleanup is resolved under the existing lease lock. A successful preparation
ends in `complete`, after all operations and probe resolution. No Git-return
record is serialized as native lease or PID-namespace evidence.

Recovery requires **both** a safe preparation cycle and safe model-attempt
cleanup. A resolved earlier model attempt or an unstarted current model attempt
does not cover a version probe from the current preparation cycle. Hold all
cleanup locks through the journal commit. Missing or inconsistent bindings,
active/replaced/corrupt probe leases, an open Git window and historical
`Preparing` records without a protocol marker remain unknown. Do not invent a
marker or backfill authority for old records.

This also quarantines historical interrupted `Running` records without a cycle,
even when their model-attempt lease is clean: that lease does not attest the
earlier preparation. Completed historical records remain inspectable. An unsafe
historical preparation cannot be renewed into a new cycle by a retry.

Terminal transitions retain quarantine when the cycle is unresolved, including
blocked and cancelled runs. Reopening and repeated reconciliation are idempotent.
Neither elapsed time, a terminal run state, a PID observation nor dropping an
in-memory object establishes cleanup.

Cancellation remains cancellation when later reconciliation resolves its cleanup;
recovery does not rewrite that terminal result as a new failed model attempt.

## Boundaries

Cycles and probes do not increment model attempts, create escalation failure
evidence or refund R05 reservations. The existing group/namespace evidence
boundaries remain; Git-return observations do not expand OS guarantees.

This change is a prerequisite for cross-instance admission, not its delivery.
The future shared register must reserve all local and shared dimensions atomically
before preparation and retain unresolved reservations. Provider identity must
continue to come from validated asset contracts, never a host label or child text.

## Acceptance

- Reproduce the old interrupted-preparation release with a held probe lease.
- Refuse legacy preparation without a marker even after its unbound file unlocks.
- Resolve registered pending and completed probes; retain active, missing,
  replaced and corrupt leases across reopen.
- Prove a previous resolved attempt cannot cover a current active probe.
- Cover crash/open windows around every Git capture, normal `SourceChanged`
  after completed captures, capture failure and failed observer commits.
- Keep cancellation with unknown cleanup quarantined; reconcile valid proofs
  without duplicate release or duplicate model attempts.
- Exercise actual supervisor termination with a fake version worker under the
  supported native sandbox separately from deterministic store tests.
