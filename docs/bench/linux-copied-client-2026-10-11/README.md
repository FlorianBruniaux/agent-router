# Copied client in Linux isolation, 11 October 2026

This diagnostic exercises the real finite stdio client, Codex 0.159.2 and the unchanged resource helper from producer-owned copies. It retains exactly `8589934592` bytes for worker soft/hard `RLIMIT_AS` and the container memory ceiling. **F12a remains NO_GO:** these OS observations grant no native capability, approval or provider admission.

The [previous helper component](../linux-helper-memory-2026-10-11/README.md) did not qualify detached descendants or the native client. This lot adds a test-only controller in `router-runtime/src/finite_client/linux_qualification.rs`, an ARM64 Landlock/seccomp fixture and an external namespace owner. The ignored Rust fixture is entered explicitly by the diagnostic driver, holds preparation owners until namespace destruction and never produces a passing Rust test result. The outside collector validates the complete observed lifecycle instead.

## Composition and authority

The controller uses the existing `OwnedBroker::prepare_codex_state` and `FiniteClientReview::with_codex_state` producer, retaining its private state, six immutable copies and listener. The six copies are client, native, helper, wrapper, descriptive profile and generated configuration. Source/copy hashes and identities are checked before and after execution; the copied helper installs its unchanged default limits. The finite stdin payload has a separate content digest and reaches EOF before native spawn. The preparation digest is not a product `Runtime::approve` grant and does not bind a new Linux production backend.

The copied client starts the copied Codex through stdio, verifies `initialize` and ephemeral `thread/start`, reads the live native child's 8 GiB limits from `/proc`, and requires native exit 0. There is no `turn/start`, generated patch, provider inference, credential, account or model cost. The broker is a local TCP banner canary, not an HTTP provider or an admission service; its counter proves the positive TCP control only. The exact allowed patch file is written by the fixed C canary.

Codex startup needs mutable state alongside its configuration. For this diagnostic only, `CODEX_SQLITE_HOME` points to the producer's existing private `tmp` and a controller-created `installation_id` alias resolves to an inert file there. The alias and inert content are verified after the run. The configuration directory gets no write/create/rename grant. This is a fixture adaptation, not a change to the production state producer. The pinned [config schema](https://raw.githubusercontent.com/openai/codex/rust-v0.159.2/codex-rs/core/config.schema.json) documents the SQLite environment override; the pinned [installation ID implementation](https://raw.githubusercontent.com/openai/codex/rust-v0.159.2/codex-rs/core/src/installation_id.rs) opens that state read-write at startup. Native PATH-alias/bubblewrap warnings were observed during failed attempts; shell execution is disabled and native tool execution is unqualified.

## Isolation and oracles

The worker has UID/GID 65534, no capabilities, no-new-privileges, a read-only container root and `--network=none`. Landlock ABI 6 restricts inherited filesystem rights, TCP connect/bind ports, abstract Unix sockets and signals across domains. Seccomp permits IPv4 TCP streams and Tokio's anonymous Unix stream pairs; addressable Unix sockets, UDP, IPv6, raw sockets, mount/setns/unshare, ptrace/process-memory access, io_uring and metadata mutation calls are refused. The [Linux 6.12 Landlock documentation](https://www.kernel.org/doc/html/v6.12/userspace-api/landlock.html) describes port-based TCP rules and ABI 6 scopes. The port rule is not an IP-address ACL; the outer network namespace has no external interface and the controller owns the only allowed-port listener.

Canaries verify the allowed broker port, deny a different reachable loopback listener and all TCP binds, and refuse writes, truncation, chmod, unlink and rename of each protected copy. Creation within and renaming of their parent directories are refused. A same-UID control can modify a 0400 file before isolation, distinguishing policy protection from a read-only tmpfs or unchangeable file mode. Workspace reads and exact-file writes succeed, while out-of-scope writes and a private-store read fail. A 16 MiB no-access mapping succeeds; a 9 GiB mapping fails with ENOMEM under the unchanged worker bound. This does not test a physical-memory OOM.

A fixed child calls `setsid`, closes stdio and appends to an inert heartbeat in a fresh task-owned Docker volume. A handshake confirms the child is detached before readiness. Independent observer containers mount only that volume read-only: the size must increase after helper completion/death, then remain constant over three observations after killing the immutable container ID. No worker-supplied PID becomes a signal target. The preparation owner remains live until teardown. `group-clean` is kept distinct from namespace cleanup; helper death leaves the helper lease `active`. The external receipt proves observed stopped activity, stopped namespace and removal of this container and volume, without upgrading a product lease to `namespace-clean`.

There are no host directory mounts, Docker socket mounts or image pulls. Only an explicit ELF/source input archive enters disposable tmpfs storage. The heartbeat is a diagnostic-only writable sink, not a task permission. Its volume is uniquely labelled and removed after all observers finish. The volume initializer is guest UID 0 with dropped capabilities and creates only that inert file; worker and observers remain UID 65534.

## Observed lifecycle

All four external diagnostic cases passed. Each reached both native RPCs and all inherited-policy canaries before teardown; protected identities remained unchanged. Heartbeat counts below are byte sizes, not process counts.

| Case | Helper exit / lease | Detached heartbeat after helper result | After namespace teardown |
| --- | --- | --- | --- |
| normal | 0 / `group-clean` | [143, 160] | [165, 165, 165] |
| deadline | 124 / `group-clean` | [2095, 2117] | [2124, 2124, 2124] |
| eof | 125 / `group-clean` | [140, 157] | [162, 162, 162] |
| helper-kill | None / `active` | [144, 160] | [165, 165, 165] |

Every namespace exited 137 after the outside owner killed its immutable ID. Exact container and volume removal were verified. `None` means the helper died by signal, not a successful exit.

## Retained failures

The evidence preserves the initial overflowing integer literal and missing helper-export builds, then startup refusals for Tokio's anonymous socket pair and SQLite in the protected home. A subsequent generic EACCES followed the SQLite override; the pinned startup source suggested the installation-ID write, and the fixture alias permitted initialization without granting writes to the configuration parent. That generic error was not syscall-traced, so its exact syscall attribution remains UNKNOWN. One failed attempt did not retain native stderr.

The first four-case campaign completed normal, deadline and EOF, then rejected helper death because the fixture incorrectly expected `pending`. The real helper writes `active` before child spawn. The final oracle requires exactly `active` after helper death, holds the owner and demands independent namespace teardown; it never treats helper death as group cleanup. All failed runtime containers and their heartbeat volumes were confirmed removed. A private build-script quoting error occurred before container creation and was corrected separately.

## Build and replay

[evidence.json](evidence.json) retains image/binary/source hashes, exact commands, build receipts, four lifecycle receipts and unsuccessful attempts. The native archive is the official [Linux ARM64 musl release asset](https://github.com/openai/codex/releases/tag/rust-v0.159.2), SHA-256 `472ee4d49464a4f8792bbf0162bd9ac18d7036dbc00900a03e95f715887e930f`; its single extracted ELF is `113aa5d5952a6fd3950a88323219747d0c91978ebee948509a540c8a460e59a3`.

The base is merged main `cfafdafbe96ea6e7be1517895d082cfba23fae59`. A Git archive of Cargo manifests, crates, integrations and xtask receives the explicitly hashed Rust overlays and C fixture. Existing locked offline vendor input is reused from the previous helper build. No personal Cargo configuration or credentials enter the archive. Existing ARM64 Rust 1.85.1 and Python 3.11 images are pinned by local content IDs; remote registry provenance is UNKNOWN. Builds use no network, no host mounts, 8 GiB memory with no additional swap, `ulimit -v 8388608`, two CPUs and an executable private 2 GiB tmpfs. Build receipts retain the exact release commands. Runtime uses a 1 GiB executable private tmpfs, one CPU and 256 PIDs. The cleanup supervisor remains outside the worker's resource limits; the 8 GiB container bound applies to all processes together.

Replay requires Docker, the exact already-present image and already-built ARM64 client/controller/helper/wrapper ELFs:

```sh
python3.11 -B docs/bench/linux-copied-client-2026-10-11/run.py \
  --binaries /private/tmp/ar-linux-client-20261011/binaries \
  --native /private/tmp/ar-linux-client-20261011/codex-linux \
  --output-dir /private/tmp/ar-linux-copied-replay-unique
python3.11 -B scripts/test-linux-copied-client-evidence.py
```

Use a fresh output path. `run.py` cannot change the memory bound, refuses missing images, validates immutable container identity/labels/settings and preserves failed capture receipts. Each case has a 60-second collector deadline and a monitored 128 KiB output budget per stream; polling can overshoot the capture budget. Cleanup calls are separately bounded. Raw private logs/archives/binaries/vendors are not committed. A replay does not prove a supplied binary's source provenance; use the recorded frozen source hashes and offline build commands for a new binary.

## Evidence limits and next gate

[verification.json](verification.json) separates macOS local quality gates, Linux runtime/client tests and the real copied-client campaign. Linux Clippy could not run because its component is absent in the pinned image; it was not installed. Local checks do not claim executed remote CI, cross-host equivalence, exhaustive syscall hardening, native shell tooling, provider reception, cost savings or an application correction.

The next lot must formalize the Linux namespace owner and startup-state producer as an approved runtime backend, with fresh authenticated host evidence, exact approval binding, interruption and retained leases until verified teardown. Product `ControllerBrokerOnly` remains fail-closed. The pending/approved lifecycle and broker admission must then be exercised with a bounded simulated provider before a human-approved real model task under [#132](https://github.com/FlorianBruniaux/agent-router/issues/132) and [#133](https://github.com/FlorianBruniaux/agent-router/issues/133).
