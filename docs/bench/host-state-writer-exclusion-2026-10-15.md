# Host-state writer exclusion, 2026-10-15

Runtime attempts and native-version probes now refuse to dispose of exposed HostState without a matching, locked proof that no worker started or its trusted PID namespace terminated. A process-group marker is insufficient. [ADR 0033](../adr/0033-host-state-writer-exclusion.md) defines the ownership and recovery contract.

## Reproduced failures and correction

The first regression failed against merged PR #173: a version probe with an active helper lease lost its private home when error unwinding dropped HostState. Arming the state before capture now retains the home and copied program on failure or future abandonment. The same proof lock protects disposal and durable settlement; pending is invalidated before a delayed helper can acquire its lease.

Independent review found a second defect in the initial implementation. A valid NotStarted proof followed by a substituted home correctly refused deletion, but reopening the store accepted the process marker and cleared quarantine. The restart regression reproduced this failure. The final design records the filesystem obligation before dispatch and clears it only after successful owner-mediated deletion; both attempt and probe recovery retain unresolved obligations.

A child fixture calls setsid and continues writing its home heartbeat after a group-clean marker. Cleanup refuses and leaves the heartbeat writable until the test explicitly kills and reaps that owned child. Other tests cover active/held/replaced/symlinked leases, a foreign proof, missing lease, unsupported namespace mode, pending invalidation, namespace protocol success, async abandonment, restart refusal and successful durable disposal. Protocol markers supplied by fixtures do not prove native namespace enforcement.

## Local validation

All 32 selected local quality checks pass on macOS: formatting, both warnings-denied Clippy configurations, all-features workspace tests, no-default-features API refusal, harness parity, script/fixture suites and both Gitleaks scans. The workspace reports 1,786 successful result lines and 42 ignored results. These totals sum Cargo result rows including nested subprocess output, not unique test definitions. The standalone library run reports 179 passed and 2 ignored.

The same 7 Rust file hashes pass an offline Linux library run: 180 passed, 3 ignored in its final summary. The existing ARM64 Rust 1.85.1 image is pinned by local content ID. The container has a read-only root, UID/GID 65534, no network, no host mounts, dropped capabilities, memory/swap bounds of 8589934592 bytes and RLIMIT_AS of 8388608 KiB. Its absence was verified after completion. A separate inert control using the same image and bounds observes memory.max=8589934592, memory.swap.max=0 and RLIMIT_AS=8388608 KiB; this is separate from the library test container. This tests source behavior, including the real detached writer; ignored native sandbox tests and the finite copied-client campaign were not executed.

[Compact source-bound receipt](host-state-writer-exclusion-2026-10-15.json) retains the base, 7 reviewed/tested Rust hashes, check results, source-archive hash and Linux log hash. The raw quality receipt remains at `/private/tmp/ar-writer-exclusion-quality-20261015/summary.json`; private build archives and logs remain under `/private/tmp/ar-writer-exclusion-20261015`. Before/after source snapshots match for the full quality run. This report and compact receipt were finalized afterwards; the tested Rust bytes are unchanged. Remote CI is separate and is not claimed here.

Independent read-only security review identified and then rechecked the restart correction, with no unresolved findings on the final 7 Rust hashes. The reviewer read results but executed no tests. Initial fixture import/synchronization failures and the read-only Linux toolchain-selection refusal remain in the private logs; the final Linux invocation selects the already installed exact 1.85.1 toolchain and downloads nothing.

## Remaining boundary

The sandbox must prevent workload writes to the private parent, lease and executable-copy directories. The lock does not neutralize an unrelated process of the same UID outside isolation. The finite copied-client preparation/external diagnostic retains its separate lifecycle and is not a newly activated product path.

The bound remains 8589934592 bytes (8 GiB). Production ControllerBrokerOnly, physical-send admission and F12a remain NO_GO. Next, bind the finite-client composition to its approved namespace owner and durable runtime lifecycle, then qualify the useful simulated task. No model or provider is invoked in this lot.
