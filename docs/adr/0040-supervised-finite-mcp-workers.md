---
adr: "0040"
title: "Reuse runtime supervision for finite MCP workers"
status: accepted
created: 2026-10-17
last_modified: 2026-10-17
scope: "MCP worker sessions and nested process supervision; native Runtime admission remains disabled"
amended_by: []
supersedes: []
superseded_by: []
related: ["0037", "0038", "0039"]
---

# ADR 0040: Reuse runtime supervision for finite MCP workers

## Decision

Connect the bounded MCP codec and file policy to the existing process capture, resource arguments and exact namespace cleanup lease. Legacy request `_meta` on tools/list and tools/call is accepted only as a bounded object (4096 encoded bytes, 128 nodes, depth 8). It is discarded before dispatch, never merged into arguments or treated as authority.

One `ToolSession` retains its policy, file identities, scratch and budgets across connections. It accepts Unix streams with an absolute session I/O deadline. Malformed input, exhausted budgets, incomplete output and failed execution stop further effects. A normal nonzero feedback exit remains a result the caller can inspect.

Each exec starts the existing helper and a new bubblewrap PID/network namespace, then the unchanged approved tool guard. The command receives only flat public files, the system toolchain and the same scratch mount. The scratch filesystem must be tmpfs with capacity at most 16777216 bytes before any lease or process is created. Read-only mounts protect public metadata as well as contents. Bind only the pinned compiler adapter file into an otherwise empty, read-only `/usr/local/bin`; its source siblings must remain absent. Keep the default 8589934592-byte address-space limit and the lab's 8 GiB cgroup with zero swap.

A callback returns only after the exact namespace lease excludes detached writers. Timeout, truncation and incomplete capture refuse instead of returning successful feedback. Retained program descriptors outlive this boundary, but are not an atomic pathname lock. A future trusted outer owner must exclude pathname writers throughout spawn/mount. The current component qualification has no competing trusted writer and does not claim that future owner is implemented.

The fixed client gains `--mcp-tools`, a stdio bridge to `/router-tools/rpc.sock`. Its own 90-second absolute process deadline covers blocked stdin/stdout. Socket I/O also has bounded waits and framing. That transport deadline does not prove namespace cleanup; the existing outer helper must retain that responsibility.

## Authority boundary

These worker APIs and their JSON data cannot construct an approved Runtime run, a fresh Tools proof, an admission ticket or a trusted terminal observation. The Runtime entrance still refuses tools and new-output execution. No native MCP configuration or native launcher is enabled. Tool journals remain unauthenticated telemetry.

Automatic approval review refused adding a launcher that calls the native worker because it could enter a model path. The refused code was not written or executed. This slice instead uses authored MCP callers and the actual fixed bridge binary, with no native SDK or model invocation. The [qualification](../bench/linux-finite-mcp-2026-10-17/README.md) records its results separately from product activation.

## Remaining integration

The runtime owner still needs to bind the tools preparation and startup into its opaque approval, launch the controller and native client inside the exact outer namespace, mint fresh Tools evidence from actual effects, and read the terminal observation under the stopped outer lease before private cleanup. Actual Codex 0.159.2 MCP startup/catalogue/feedback decoding is unqualified. The 24 sealed trials, common trial budgets, real models and source publication remain outside this slice.
