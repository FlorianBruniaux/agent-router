---
adr: "0045"
title: Fresh entry through the existing finite Runtime
status: accepted
created: 2026-10-22
last_modified: 2026-10-22
scope: "One explicit public task and local scripted parent; no account, publication or resume authority"
amended_by: []
supersedes: []
superseded_by: []
related: []
---

# ADR 0045: Fresh entry through the existing finite Runtime

## Context

The qualified finite client previously required a caller to compose discovery,
phase routing, approval and execution through Rust APIs. A fresh session needs
one product entry that performs that composition without turning a serialized
manifest, an earlier approval or a parent-provided receipt into authority.

## Decision

`agent-router finite-entry --config PATH --json` consumes one strict JSON task
on stdin. Only `local_scripted` transport is supported. The configuration must
explicitly enable the entry, pin six programs/assets and allow the requested
model/effort pair. It names an absent private root beneath a canonical,
user-owned parent without group/other write access. Existing roots refuse;
there is no resume/import path. No global settings or hooks are installed.

The task supplies an explicit inventory of hashed public bytes, mandatory risk
facts, one existing output and bounded permissions. Normalization preserves
risk escalation, including the Medium floor for workspace implementation.
Missing facts, high risk, extra permissions, unsafe names and absent hashes
refuse before creating state. The source is a new private Git repository made
only from those bytes. No caller repository or home directory is scanned.

A random session ID and exact input/configuration hashes bind two challenges.
The first authorizes the real sandbox and tool diagnostics. Their observed
snapshot feeds the ordinary phase router. The catalog represents one explicit
operator selection with synthetic local transport metadata; it makes no model
availability, quality or price claim. Candidate estimates and request timeout
use the execute node's allocated budget, not the whole task budget shared with
verification. The second challenge binds the Runtime's
opaque pending run. Exact configuration bytes, root identity and public source
state are rechecked before dispatch. A configuration/source change after a
challenge refuses even when its digest is echoed correctly.

The approved worker uses existing private finite workspaces, program copies,
namespace ownership, three bounded MCP tools, capture and admission cleanup.
The CLI creates only its own synthetic local admission store. Runtime APIs and
private access rights are unchanged. Linux ARM64, 8 GiB and zero swap are
required; diagnostics still verify the actual sandbox. Read-only discovery
and a private replacement are separate views of the public source.

The native client sends requests to the owned local broker. The CLI forwards
them as bounded JSON events to its parent and accepts only matching session,
submission and request identities. It performs no provider HTTP call. The
parent's model activity is UNKNOWN to the product; the qualification parent
is separately authored and runs without network. Duplicate requests and
replies, unexpected fields, EOF and expired deadlines refuse. Input/output
copies are close-on-exec, output uses bounded nonblocking writes, and HTTP
rejects credential/transfer-encoding headers, oversized or pipelined bodies.

A completed transport returns exit 3 and `Verifying`, the captured patch and
bounded output, source integrity, attempts and cleanup evidence. It records no
objective verification or human acceptance. Errors after attempting dispatch
return exit 6 (`finite_entry.outcome_unknown`); pre-dispatch refusal returns 2.
A local error or lost response is never an automatic retry or resume token.
Private state is retained for bounded inspection rather than silently removed.

### Fixed public t02 qualification

The workspace-only `qualification-fresh-t02` build feature adds the explicit
configuration value `qualification: "fresh_t02_v1"`. Ordinary builds reject
that field and do not contain the oracle. The feature selects only the exact
public t02 fixture; it is not a configurable verifier or a receipt import API.
Both approval challenges display and bind the adapter, unchanged t02-v2 engine,
case inventory and compiler hashes. Existing configuration rechecks remain.

After the worker namespace has ended, the owning process uses its existing
`FiniteClientVerifier` handle to capture the patch. It refuses any source,
candidate or patch different from the reviewed fixture before compiling.
A fresh private capsule runs the fixed checker within the existing verify
node time budget (30 seconds for the 60-second qualification task), with
bounded reads. Staging consumes that same deadline. Missing or exhausted
verification budgets refuse before spawning or recording an attestation; the
separate bounded cleanup wait grants no more checker work time. Its receipt names the actual fresh session, exact submission,
run, route, pre-verification report, patch, candidate and checking assets.
No campaign identities are substituted. All 18 candidate cases must pass;
the original source must retain its known failures. The receipt is retained
exclusively and synced before `record`; the live Runtime must expose the same
record in its projection and in one `verification.attested.v1` history event.
Validation becomes Passed, objective stays UNKNOWN, human acceptance stays
absent and the run stays Verifying. Nothing is published to the source.

This checker executes outside the terminated worker namespace. Its use is
restricted to the separately reviewed disposable Linux qualification cell:
network disabled, no host mounts, 8 GiB, zero swap and reviewed public assets.
See [the qualification checklist](../fresh-finite-qualification.md).
The adapter checks the cgroup limits; those checks alone do not attest network
or mount isolation. Native qualification must separately retain the owner's
cell configuration and teardown evidence. macOS refuses checker execution.
A failed signal, timeout, malformed receipt, storage error or record mismatch
returns the ordinary post-dispatch UNKNOWN result. The cell owner must stop
the entire cell on any result, including detached checker descendants; the
process-group runner is not a replacement for that outer isolation.

## Consequences

The fresh entry is opt-in and limited to a public, explicitly inventoried task.
It does not implement automatic host startup hooks, real account admission,
model execution, dynamic multi-model selection, active revocation after
worker dispatch, durable recovery, general verification recording or publication.
Those remain separate acceptance work for #132. The worker budget excludes
preparation and diagnostics; each parent approval has its own bounded wait.

Unit and CLI tests cover framing/deadlines, fresh identities, exact approvals,
HTTP refusals, private-root replacement, configuration changes and bounded
result reads. Native qualification must independently inspect the real route,
attempts, cleanup, artifact and a useful public correction. Structural tests
alone cannot establish Linux execution or task acceptance.
