---
adr: "0044"
title: "Bind finite context policy to a distinct native profile"
status: proposed
created: 2026-10-21
last_modified: 2026-10-21
scope: "Optional exact windows and terminal raw recall; native qualification pending"
amended_by: []
supersedes: []
superseded_by: []
related: []
---

# ADR 0044: Bind finite context policy to a distinct native profile

## Context

Exact line windows change the read schema. Session-local raw recall adds a fourth tool when terminal presentation is enabled. Existing v3 startup files, approvals and OS proof bindings cannot establish those contracts. Terminal JSON and SDK discovery remain observations, not parent authority.

## Decision

Prepare the context explicitly with one validated parent policy. Bind that policy and the deterministic MCP catalogue into the private startup, FilePolicy preparation, bootstrap, finite review and schema-2 terminal. The manifest uses the distinct `linux-isolated-finite-tools-context-v4` profile. Runtime request allowlists include every selected finite tool and still require Tools and WorkspaceWrite capabilities.

Absent policy returns the historical bytes and hash tuples, without an extra `None`. The v3 profile and schema-1 terminal remain supported. Each profile rejects the other's policy and terminal. Retained bootstrap hashes and identities are rechecked before dispatch; terminal fields are consumed only behind the existing writer-exclusion fence and retained producer state.

A diagnostic catalogue checksum may travel to the fixed live probe. It does not select a policy, expand permissions or authorize a process. The owner selects policy only from its sealed bootstrap. No guard, network route, seccomp, Landlock or resource limit is widened.

## Consequences

Context startup is a separate API so private per-tool configuration can advertise raw recall without changing v3 files. Context preparation does not qualify Linux or authorize execution. The [T3 canary](../bench/rtk-context-native-qualification/README.md) uses written responses and shares the existing 8 GiB namespace/capture/cleanup composition. Its raw native catalogue must be observed and reviewed; no new SDK checksum is invented or adopted automatically.

The implementation is checked locally on macOS. Native Linux ARM64 qualification is NOT RUN. No model or provider call is part of this decision. T4 volume comparisons and any claim about billed savings remain separate work.
