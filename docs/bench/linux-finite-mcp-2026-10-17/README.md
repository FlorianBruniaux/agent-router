# Finite MCP workers connected to runtime supervision

This slice connects authored MCP calls to the runtime's real process capture, resource helper and namespace cleanup leases. It does not enable Runtime Tools admission or start the native Codex SDK. No model, provider, account or sealed trial is used. The native launcher was refused by automatic approval review and is absent from this branch.

Pinned SDK source inspection found legacy `_meta` on list/call requests. The codec accepts this bounded context (4096 bytes, 128 nodes, depth 8), discards it and leaves tool arguments and permissions unchanged. This is source-backed compatibility, not a native SDK run.

`ToolSession` retains the file policy and budgets across connections, accepts only Unix streams, and applies an absolute session deadline to both reads and writes. The fixed `--mcp-tools` entry connects stdio to `/router-tools/rpc.sock`; its 90-second process deadline also terminates blocked stdin/stdout. It does not provide cleanup authority. `ExecSandbox` uses the existing helper/capture path, requires a stopped exact PID namespace lease before returning a result, keeps program descriptors retained through that boundary, and refuses timeouts, incomplete capture and truncation. Normal exit1 feedback is returned as a normal result.

The tool view contains flat public files read-only, the declared system toolchain read-only, and one persistent scratch mount. Scratch must be tmpfs with capacity at most 16777216 bytes before any lease/process is created. An empty read-only `/usr/local/bin` exposes only the pinned compiler adapter, so a private file next to its source stays absent. The approved guard source stays byte-identical; no socket type, private mount or memory grant is added. Read-only mounts protect metadata as well as contents. The system linker is selected by absolute path through the previously qualified adapter.

## Reproduce without a model

The local image and public build-cache volume must already exist; the script never pulls an image, installs software or mounts the host checkout. This private development cache is not a public release artifact. The runner verifies the effective nonprivileged, network-none container settings, 8589934592-byte memory bound and zero swap. It transfers only an explicit public file list with sizes and hashes, verifies the cached Rust sources/manifests against the published dependency plus that overlay, and selects the test executable from Cargo's current JSON output. Archive timestamps use the current time to avoid stale Cargo artifacts.

```sh
python3 -B docs/bench/linux-finite-mcp-2026-10-17/reproduce.py \
  --output /absolute/new/owned/evidence-directory
```

The runner compiles the actual fixed bridge and the kernel tests offline, creates one 16 MiB scratch mount shared across successive tool calls, and runs only `finite_tools::` tests. It does not select the legacy native-client test modules. It also deliberately stops reading the bridge's stdout while keeping stdin open, observes pending output, and requires the actual fixed process deadline to terminate it. A missing deadline fails the external observer's bound. Only the runner's own labelled container is removed; the cache is preserved.

The outer lab retains the previously qualified rootless nested-sandbox setup: read-only container root, uid 65534, no capabilities, NoNewPrivs, no external network, unmasked proc paths and outer seccomp unconfined. The tool helper, new PID/network namespace, unchanged Landlock/seccomp guard and read-only mounts are separate inner boundaries. `/tmp` must exist in the outer lab so nested bubblewrap can initialize; it is a new private temporary filesystem, not a personal host directory.

## Evidence and limits

The retained [Linux result](evidence/result.json) is `PASS_LOCAL_COMPONENT_ONLY`: 33 Rust checks pass, and the blocked bridge exits 125 after 90.01543350000429 seconds with 64206 bytes pending in stdout. The [source check](evidence/compiled-source-check.json) matches 420 Rust/manifests against the published base plus the explicit public overlay. The PR separately records the full local quality gate. Tests cover actual red feedback, the scoped correction and green feedback; scratch write/compile/execute across calls; exclusive absent-output creation through the fixed CLI bridge; private/network/public-write refusal; allowed small allocation and refused 9 GiB mapping under the 8 GiB address-space bound; timeout of a live detached writer followed by a stable heartbeat; and refusal before effect for an oversized scratch filesystem. Unit checks cover malformed framing, retained budgets, protocol-failure stickiness, incomplete input, peer EOF and output backpressure. The bridge watchdog is a separate observed process test.

A missing private path demonstrates mount absence, not a Landlock denial. The observed scratch entry/depth bound is not an in-flight kernel inode quota. Retained executable descriptors are not an atomic pathname lock; a future outer owner must exclude trusted competing writers until spawn/mount finishes. No concurrent-writer or native-startup qualification is inferred from this component lab.

The native Runtime entrance still refuses tools and new-output execution. Generated MCP startup, actual Codex 0.159.2 catalogue/tool decoding, opaque tool approval, fresh live Tools proofs, trusted terminal observation before private cleanup, and end-to-end cancellation/parent-EOF of the combined native/tool owner remain unqualified. Tool journals are telemetry. The existing guard's normal/timeout/parent-EOF component canaries remain separate evidence in [ADR 0039](../../adr/0039-finite-tool-guard-canaries.md). The 24 sealed trials remain `not_run`; common trial budgets and model quality/cost are not measured here. See [ADR 0040](../../adr/0040-supervised-finite-mcp-workers.md).
