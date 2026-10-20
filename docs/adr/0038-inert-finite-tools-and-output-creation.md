---
adr: "0038"
title: "Prepare finite output creation and inert local tool primitives"
status: implemented
created: 2026-10-16
last_modified: 2026-10-16
scope: "Preparation, pure MCP framing and injected file policy; no tool process authority"
amended_by: []
supersedes: []
superseded_by: []
related: ["0028", "0037"]
---

# ADR 0038: Prepare finite output creation and inert local tool primitives

## Context

The local benchmark needs a new `review.json` and genuine red/write/green feedback. ADR 0037 supports one existing private file and disables native tools. A missing file must remain distinguishable from an empty existing preimage. Local protocol tests do not establish OS confinement or native integration.

## Decision

Add an explicit `FiniteOutputMode` to a separate preparation method. `ReplaceExisting` retains the original one-file behavior and its existing approval bytes. `CreateNew` freezes actual absence in both the original public snapshot and the producer-owned view; it never precreates the output. Directory/file identities, source state and the exact task remain bound. A file or dangling link appearing before execution revokes that absence. The current runtime and Linux backend reject this creation mode before launching anything.

Keep the bounded MCP codec and file/budget engine in `router-runtime::finite_tools`, documented as hidden inert building blocks. They cannot create a runtime approval, capability proof, host policy, process launcher or provider grant. MCP exposes only `read_file`, `write_file` and `exec` through a caller-injected dispatcher. Nonzero feedback exits are ordinary tool results; malformed or oversized protocol data closes the stream. The codec never reads paths or starts a process.

The file policy requires retained directory descriptors, regular single-link files, current identities/preimages, explicit exclusive creation and bounded calls/output/time/scratch observations. Its exec callback is injected by the caller and is not an approved process boundary. It has no runtime caller in this slice. Its redacted journal is policy telemetry, not authenticated runtime evidence or cleanup authority.

The [tool design](../superpowers/specs/2026-10-16-finite-local-tools-design.md) records the proposed native launcher, nested cleanup and kernel boundary. Automatic approval review rejected changes to the seccomp/Landlock guard before they were applied. That profile requires explicit human approval and actual Linux qualification before integration. This ADR does not approve or implement that change.

## Consequences

An authored local effect can create a private output and verify its new-file patch without touching the original source or its index. That test proves capture, not native tool execution. Existing replacement approval bytes remain compatible. New-output execution, actual native MCP handshake, feedback subprocesses, tool cleanup and trusted runtime observation remain unqualified. The 8 GiB memory limits, existing network restrictions and private-file access are unchanged. The 24 sealed benchmark cells remain unrun; no model or savings claim follows.
