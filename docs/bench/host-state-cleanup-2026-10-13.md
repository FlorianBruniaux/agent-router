# Private host-state cleanup, 2026-10-13

HostState now refuses to recursively delete a substituted home or runtime store. Its held descriptors pin the original directory inodes; device, inode, owner and mode must still match the captured identities before grants or cleanup. Missing paths are refused, including an already removed home, because absence alone does not prove disposal of the owned directory.

Explicit cleanup consumes one attempt. Drop does not retry after refusal or partial failure. The independently owned program copy is still cleaned when home cleanup fails. Runtime callers retain their existing CleanupUnknown failure path; a refusal requires reconciliation of the retained state.

## Acceptance and evidence

The first 3 regression tests failed against the prior implementation: a replacement home directory and replacement store were deleted, and changed modes were accepted. After the fix, all 11 non-native HostState tests pass on this macOS host, with 1 native sandbox test ignored. The 7 new tests cover replacement directories, regular files and symbolic links under explicit cleanup and Drop, replacement stores, mode changes, missing/moved homes, child alias target preservation, single cleanup attempts and grant refusal with independent program-copy cleanup. Only inert files are copied; no client or provider is invoked.

A local APFS diagnostic retained an open descriptor across directory removal. Its link count remained 2 before and after removal. The implementation therefore refuses missing paths rather than inferring successful removal from a Linux-specific link-count expectation.

Base revision: `13d6227d784b45d89bf457e9b58f7b4ece01d10e` (PR #172).

Tested `crates/router-runtime/src/host_state.rs` SHA-256: `001f6cf743eff4f6d66561a10a9b713c7f83cfffe58066305ad301bcfdb6602d`.

The 32-check local quality run passed 31 checks, including formatting, both warnings-denied Clippy configurations, 1,773 workspace Rust tests (42 native/platform tests ignored), the harness, script suites and both Gitleaks scans. The sole failure was the ADR index date, left at 2026-10-11 while ADR 0032 was updated to 2026-10-13. After correcting that index row, all 30 ADR validator tests and the direct ADR check passed. The Rust source above is unchanged; no Rust tests were repeated for that documentation-only correction.

The initial runner receipt remains `incomplete_or_failed`; its before/after source snapshot is stable. Retained local receipt: `/private/tmp/ar-host-state-quality-20261013/summary.json`, SHA-256 `7ebda2f520965b1d51bc5ece37c1f0c1b59a88e1b0b2b14c69e47edecc16606a`. The report and ADR index were finalized after this run; this receipt is not a claim that the final documentation bytes passed a fresh full 32-check run. Remote CI and Linux native execution are not evidenced by these local checks.

Independent read-only security review found no Critical, Important or Minor findings in the implementation with the same Rust source hash. The reviewer inspected grant callers, cleanup error mapping and independent program-copy disposal; it executed no tests. Concurrent same-UID races, namespace ownership, automatic moved-state recovery and broader stale-scratch hardening remain outside this change. Those boundaries are retained below.

## Remaining execution boundary

The identity check and pathname removal are not atomic against a concurrent writer with the same UID. Before a production backend can claim safe lifecycle cleanup, it must stop or exclude workers and protect the private parent throughout deletion. Preserving replaced paths does not dispose of moved original data automatically.

The resource bound remains exactly 8589934592 bytes (8 GiB). Production ControllerBrokerOnly admission, approval and capability evidence formats are unchanged. This lot runs local source tests on macOS; it does not rerun the Linux native campaign or authorize a native model turn. F12a remains NO_GO under issues #132 and #133.
