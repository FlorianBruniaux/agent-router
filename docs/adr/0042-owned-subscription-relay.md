---
adr: "0042"
title: "Bind subscription transport and observations to the finite Runtime owner"
status: proposed
created: 2026-10-18
last_modified: 2026-10-18
scope: "Approval-bound private subscription channel; no provider run or budget approval"
amended_by: []
supersedes: []
superseded_by: []
related: []
---

# ADR 0042: Bind subscription transport and observations to the finite Runtime owner

## Context

A native MCP component and an authored local broker do not establish subscription execution or consumption. Relabeling a subscription as `local_fixture` would select the wrong admission domain and confuse simulated counters with observed usage. A worker-provided cleanup flag cannot exclude writers.

The separate host controller holds credentials and the fixed HTTPS transport. The Linux worker must retain its existing public-only mounts, fixed tools, loopback-only namespace and 8 GiB limit.

## Decision

`OwnedSubscriptionController::pair` creates a fresh anonymous Unix stream pair with close-on-exec descriptors. The Runtime retains one end. Its trusted parent retains the peer and the provenance of any cross-host bridge. A serialized binding or an arbitrary inherited descriptor does not establish that provenance. The handle is not serializable or clonable. Its fresh nonce, channel instance, account hash, controller hash, configuration hash and sealed assignment enter the finite review digest.

A prepared subscription profile without this owner is refused. A matching owner selects `codex_subscription_relay` admission, never fixture quota. Fixed model and effort must match the assignment. This entrance permits only public finite tools and private patch artifacts.

The owned broker serves the fixed local canary itself. Before native worker spawn, `runtime_ready` binds the run, attempt and combined approval digest. The parent persists its unique result-state intent before `controller_ready`. Provider requests, returned chunks and control frames have bounded sizes, counts and time windows. Credentials and an outbound network route never enter Linux.

The host persists and verifies each cumulative usage receipt before its `controller_request_finished` barrier. Unknown usage or a local limit cancels the native worker. It preserves the known prefix and keeps the cleanup channel available. Equivalent credits are observations, not micro-USD, a provider billing cap or an exact subscription debit.

The Runtime emits `runtime_cleanup` only after its retained namespace proof excludes writers and the cleanup transaction commits. The receipt is bound to the same run, attempt, approval and channel. A final controller ACK does not mint Runtime authority. The host results manager remains the single owner of campaign progress and consumption. An incomplete observation or lost cleanup ACK stops the campaign.

The host has a versioned subscription entry requiring an explicit positive budget and an exact approval digest tied to the destination and code. Importing or preparing the modules cannot send a request. No real campaign state, account request or budget approval is part of offline qualification.

## Consequences

Qualification can exercise the real native client, tools, Runtime cleanup and durable result state using coded upstream responses. Its synthetic usage must remain labeled and must not populate the 24 real benchmark rows.

The trusted parent owns bridge process lifecycle and validates the current account before a later approved launch. The Runtime rejects a copied JSON receipt, but cannot authenticate a malicious replacement of its trusted parent. The existing OS policies and 8 GiB bounds are unchanged. A provider call already in flight can overshoot an observed stop threshold.

A real provider request, current subscription debit, objective benchmark outcomes and savings remain unverified until a separate approved campaign runs.

## Rejected options

- Treating a subscription as `local_fixture` hides the real quota and accounting identity.
- Opening a Linux network route or mounting host credentials expands the worker boundary.
- Accepting a worker JSON cleanup flag substitutes a declaration for writer exclusion.
- Maintaining a second campaign ledger permits duplicated or inconsistent cumulative costs.
