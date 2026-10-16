---
adr: "0036"
title: "Bind finite-client preparation to durable runtime approval"
status: implemented
created: 2026-10-16
last_modified: 2026-10-16
scope: "Combined review, approval and live evidence validation; no dispatch backend"
amended_by: []
supersedes: []
superseded_by: []
related: ["0028", "0032", "0035"]
---

# ADR 0036: Bind finite-client preparation to durable runtime approval

## Decision

`Runtime::review_finite_client` consumes the producer-owned review and registers it through the existing deterministic routing, source, policy, permission and admission-configuration gates. It derives a `runtime-finite-client-definition-v1` hash from the ordinary runtime definition and the exact finite-client review digest. The existing `runtime-approval-v1` digest then covers this combined definition. Ordinary definitions keep their existing bytes. No database migration or second approval store is added.

The nonserializable, noncloneable `PendingFiniteClientRun` retains both reviews. The client task ID, requested model and effort, source, scope, cwd, rendered private stdin, timeout, output bound, snapshot and authorization must match the runtime request. Require producer-owned copies and startup state under this runtime's private store. The retained review already binds all copies, file identities, broker instance and concrete helper arguments. This entrance requires the exact 8589934592-byte address-space bound and explicit Network capability. It refuses both removal and increase of that memory bound.

`Runtime::approve_finite_client` rechecks files and the request before committing the existing AwaitingApproval to Queued transition. Only the combined digest is accepted. The preparation digest alone does not approve the runtime. A `finite_client.review.v1` event records only the preparation digest in the existing hash-chained journal; private stdin and paths are not copied into that event.

`Runtime::validate_finite_client` requires the same queued definition, current runtime snapshot and exact preparation, then reopens the existing live authenticated proof verifier. Historical views, expiry, later authenticated failure, forged evidence and cancelled approval refuse validation. Its borrowed result retains the exact broker listener. Validation creates no admission ticket, worktree, lease, attempt or process.

Generic approval, resume, retry and dispatch cannot accept a finite binding. Reopening a finite run as a plain runtime definition fails its definition comparison. No API converts either combined owner into a generic execution handle or a Command. Dropping the unexposed owner disposes preparation resources; the durable record remains observable, but cannot reconstruct ephemeral owners after restart. Cancel that record and prepare a new run for a new client instance.

## Remaining dispatch boundary

This decision connects approval, not end-to-end execution. The production client is still a diagnostic canary, and ControllerBrokerOnly remains unsupported. No useful simulated-provider task, paid provider request, native qualification or F12a GO is claimed.

The next execution lot must resolve the reviewed-cwd boundary explicitly: WorkspaceWrite creates a private runtime worktree after approval, while startup preparation binds the original cwd. A backend must obtain approval for its actual final workspace and startup composition rather than silently relocating this review. It must then use fresh authenticated OS evidence, the existing durable admission/attempt budget and exact namespace cleanup lease. Selected Linux diagnostics from ADR 0035 cannot be promoted to that authority.

## Verification

The runtime integration tests exercise the real private store, generated startup state, owned copies, loopback listener and authenticated synthetic proof store. They assert durable approval, no attempts or worktrees, cross-task and memory refusal, changed files, plain-definition substitution, expiry, historical evidence, revocation and cancellation. These are contract tests, not OS sandbox evidence. Compile-fail examples cover the generic-handle boundary. A deliberate omission of the finite digest makes the substitution regression fail; the original source is restored before the complete local gate.
