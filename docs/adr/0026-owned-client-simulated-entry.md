---
adr: "0026"
title: Dedicated Codex client for simulated patch work
status: accepted
created: 2026-10-08
last_modified: 2026-10-08
scope: "Select the dedicated direct client plus broker for F02a simulated artifact work only. Full ACP parity, production integration, OS admission and real-provider budgets remain unqualified."
amended_by: []
supersedes: []
superseded_by: []
related: ["0022", "0023", "0025"]
---

# ADR 0026: Dedicated Codex client for simulated patch work

## Decision and boundary

Select **direct app-server + local broker + controlled cancellation** as the
entry candidate for F02a simulated useful-patch work. The
[qualification](../bench/owned-entry-qualification-2026-10-08/README.md) records
its exact evidence and limits. This bounded selection amends the next-step stop
in [ADR 0023](0023-owned-task-entry.md); the NO-GO for unmodified interfaces and
real-provider admission remains valid. It does not complete issue #131.

The dedicated client owns task submission before `thread/start` and `turn/start`.
It binds task, requested model/effort and native thread/turn, preserves protocol
evidence, rejects unapproved callbacks and stops on rerouting or evidence loss.
The broker checks the outgoing task/model/effort before its single physical-send
path, reserves one local HTTP-request unit and preserves ambiguous reservations.
The controller owns cancellation and physical cleanup. An interrupt acknowledgement
alone is insufficient.

The selected pair is the repository diagnostic client at its recorded source
hash and Codex 0.159.2 native SHA-256
`16593cc2f422d5f398a8e40f550ebbaf1245392528957be342c295920a300704`.
It requests `gpt-6.1-sol/low`, `on-request`, reviewer `user`, ephemeral thread and
read-only sandbox with explicit `networkAccess: false` in the returned policy.
The outer diagnostic network restriction permits only the broker. These tests
qualify this pair on macOS with a simulated provider, not arbitrary versions or
filesystem isolation. A provider request naming a model does not prove real inference.

## Evidence for the choice

Three native scenarios cover one admitted local send, refusal before the local
provider, and cancellation when the broker is absent at startup. Separate client
fixtures cover unsupported model/effort before transport, permission callbacks,
rerouting, RPC identities, malformed and oversized output, cancellation, late
results and ambiguous interruption. The source-bound report retains initial
failures and independent review; local test success is not native admission.

Nine [ACP component observations](../bench/owned-entry-qualification-2026-10-08/acp-comparison.md)
execute pinned upstream modules. The unchanged native JSON reader drops a malformed
line and then accepts a later successful response without reporting evidence loss.
That necessary-contract counterexample excludes the inspected unchanged ACP path.
The observations are not nine equivalent full ACP/native sessions. ACP's locked
Codex 0.159.1 versus the installed 0.159.2 remains a compatibility gap. Completing
that comparative acceptance remains in #131 and does not grant production authority.

## Next implementation and ownership

F02a may now build and verify a simulated patch artifact. It must bind the source
revision, file preimages, contents, modes, allowed paths and verification results.
Applying that artifact is a separate implementation and acceptance requirement:
refuse changed preimages, duplicates, concurrent edits and symlink substitution;
retain interrupted multi-file effects. No patch application is admitted by this ADR.
Planning and verification can be deterministic local operations; extra model stages
are not required for this first artifact contract.

Production contracts stay in `router-types`, host protocol in `adapter-codex`,
supervision/broker/journal in `router-runtime` and application I/O in
`agent-router-cli`. The Python diagnostic under `docs/bench/` is evidence tooling,
not a second production runtime. No global hook or existing Codex conversation is
intercepted. The existing `codex exec` adapter is unchanged.

F12a must prove full OS supervision for the chosen host; F02b must compose the
native useful-patch path; F03b must qualify the integrated per-send reservation and
settlement protocol. F04b/F05/F06/F07b must connect observation and accounting.
A real-provider trial still needs its bounded manifest and exact approval. No
monetary/token ceiling, paid inference, accepted real patch or saving is established.

## Reopening

A changed client/native hash, lost evidence, unsupported or rerouted selection,
uncertain cleanup, or broker bypass invalidates the affected qualification.
Retain reservations and stop; do not silently retry. Requalify the changed boundary
before extending the support claim. A repaired ACP path may be reconsidered with
the same contract and a complete comparison rather than assumed parity.
