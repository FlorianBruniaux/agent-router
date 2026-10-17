---
adr: "0039"
title: "Test approved finite tool guard modes before runtime integration"
status: implemented
created: 2026-10-16
last_modified: 2026-10-16
scope: "Approved C guard modes and local Linux canaries; no native Tools admission"
amended_by: []
supersedes: []
superseded_by: []
related: ["0037", "0038"]
---

# ADR 0039: Test approved finite tool guard modes before runtime integration

## Context

ADR 0038 delivered preparation and inert tool primitives. Automatic approval review initially refused a change to the guard. The operator then explicitly approved applying and testing the presented patch, with unchanged private access, memory limits and no model calls. The patch and preimage were verified before application; its SHA256 is `2366c3babcf9e408cb6b118e748d7e4d1cccd3ceb6a8d5bcfb7ef68e09e5cdb2`.

## Decision

Retain the original entry mode. Add an explicit native-tools guard mode permitting Unix stream sockets in the visible namespace and refusing workspace-write grants. Add a separate tool-exec mode with no addressed sockets, read-only public input and writable scratch. Anonymous stream socket pairs remain allowed. Metadata operations needed by compilation are permitted only in the tool mode; their protection outside scratch depends on the read-only namespace mounts, including descriptor and symlink routes.

Keep the approved patch byte-identical. Its resulting source SHA256 is `71cc65477c1badc89a6f87487b5daf75798f63904241ad9d5c4637db8b1143c5`. The [local Linux canaries](../bench/linux-finite-tools-guard-2026-10-16/README.md) exercise the old/new legacy boundary, native IPC, tool filesystem/network restrictions, memory, descendants and actual supplied Rust feedback. The tested compiler adapter chooses an immutable absolute linker path without widening the filter or mounting `/etc`.

A guard report is observation, not authority. The current Runtime, capability admission, generated home, native MCP configuration and fixed client entry are unchanged. No Tools proof or native end-to-end qualification follows from these canaries. New modes require a separately reviewed composition and live evidence before product dispatch can use them.

## Consequences

Local guard behavior and resource enforcement are now measured, with reproducible public fixtures and retained source/program/container provenance. Ten tests pass under 8 GiB and zero swap with 16 MiB scratch. Private controls remain unmounted, model/provider calls and the 24 sealed trials remain unrun. Native MCP transport, trusted tool ownership, terminal observation and cumulative approved trial budgets still need implementation and qualification.
