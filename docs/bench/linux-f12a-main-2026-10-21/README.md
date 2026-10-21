# F12a Linux qualification on merged main, 2026-10-21

**GO for the selected Linux ARM64 F12a composition.** The [source-bound evidence record](evidence.json) records the exact environment, program pins, cases and retained failures. This closes the OS-composition prerequisite for this cell, without closing the roadmap or useful-task issue.

The tested product source is `fc1a4ea88b3effdb768cbeb1ca2c352c10b5d542`. This qualification uses Codex 0.159.2 Linux ARM64 musl through the owned finite Runtime, with written responses instead of model inference. The checkout archive contains 1,685 tracked files; every file matched its SHA-256 inventory in the isolated build. No production source changed during this qualification.

## Qualified boundary

The selected cell is Linux ARM64 with kernel `6.12.76-linuxkit`, bubblewrap 0.8.0 and the recorded guard/helper/client hashes. The container and worker retain their existing controls:

- Aggregate cgroup memory is 8,589,934,592 bytes, with zero swap. The worker retains its 8 GiB address-space limit. A refused 9 GiB reservation is not a physical-memory OOM test.
- The outer container has no network, no host bind mounts, a read-only root and UID 65534. All capabilities are dropped before the worker; the existing user/namespace setup policy is unchanged.
- The retained broker is the only allowed network service. Native configuration and executable copies remain protected. Finite tools retain the existing file, call, output, execution and scratch bounds.
- The original public build cache was read-only; compilation used a separate copy. No home directory, account credential, real campaign ledger or global agent configuration entered Linux.

## Observed results

The ten Runtime cases passed: q01 report, q02 bounded replacement, q01 timeout/cancellation, q03 MCP replacement, q04 MCP creation, q03 timeout/cancellation and subscription relay with known/unknown synthetic usage. Each receipt records namespace termination, zero outstanding admission grants, one released grant and unchanged original source.

The unknown-usage case retains 600 known synthetic tokens and 3,275 known equivalent microcredits, stops subsequent work and captures no patch. It does not explain or reconcile the historical real campaign's unknown consumption.

Both parent-EOF cases passed. The exact helper stop marker confirms namespace termination; two reopens retain private state and admission (one grant, zero releases) with cleanup `UNKNOWN`. This is a refusal to resume ambiguous work, not automatic filesystem recovery. The legacy broker is refused before a dispatch command exists.

Ten kernel guard canaries passed, including protected files, network denial, 8 GiB reservation enforcement, actual RED/GREEN feedback and stopped detached descendants. Linux Runtime unit tests passed 248 cases with 29 ignored.

Two public launcher cells also passed: t02 H1 used five coded responses and four actual tool returns, running four real feedback tests with the expected failing suite verdict; t05 H0 used four coded responses and three tool returns to create its declared JSON output. Both exported only the declared submission after namespace termination and admission release. Their preceding ledger rows are synthetic fixtures; they are not real or additionally executed benchmark trials.

## Local quality and evidence limits

Format, warnings-denied workspace Clippy, the all-feature locked workspace suite, harness parity and Gitleaks pass on the tested source. The workspace runner reports 1,920 passed, 43 ignored and 498 filtered-out results across 196 suites; these totals include nested subprocess results. Ignored results are not native successes. The 141 controller tests and 26 historical host tests pass on Linux Python 3.11.2. The 27 local release tests pass with Homebrew Python 3.15.

The retained initial failures are harness/environment failures: Docker was stopped; copying into a tmpfs through Docker's archive API failed; the first macOS controller check could not bind its local test server inside the development sandbox; a shallow capsule copy lacked the host checker's repository-relative manifest; system Python 3.9 lacked `tomllib` for the release suite; and the legacy-broker case hit `ENOSPC` after the two EOF cases intentionally retained about 478 MiB of programs in the unchanged 512 MiB temporary filesystem. The full first-cell evidence and retained programs were archived and verified before that container was stopped. The remaining cases used a fresh cell with identical sources, executable hashes and limits; the legacy refusal then passed. The replays used streaming transfer, the original repository layout and installed compatible runtimes. No product assertion, timeout, memory limit or permission rule was relaxed.

Linux Clippy is unavailable in the existing offline toolchain; no component was installed. Clippy passed on macOS. No CI run, new independent review, release reproducibility or broader host/OS matrix is claimed by this replay. Historical independent reviews retain their original scope.

macOS remains NO_GO under its unchanged hard-memory contract. A scoped Linux qualification does not establish F12b, automatic daily routing, real provider admission or measured savings. Successful synthetic runs remain `Verifying`; objective/validation and human acceptance are not upgraded by transport success. Issue #132 still needs the useful-task acceptance chain; #133/#120 and the stopped real campaign remain separate.

## Replay inputs

Rebuild the finite client, helper, Runtime test binary and `finite-campaign` example from the pinned source using Rust 1.85.1, the existing offline vendor and recorded image. Verify the public native archive and ELF hashes before staging. Use new test roots and fresh discovery for every run. Preserve the 300-second proof lifetime and the existing worker/host deadlines.

The Runtime cases are in `crates/router-runtime/src/finite_client/linux_e2e.rs`. The EOF driver in `../linux-finite-dispatch-2026-10-16/parent-eof.py` was reused with local path and test-binary rebinding; the MCP variant selects the existing q03 controller/recovery tests and q02 run ID. The guard checks use the unchanged `check.py` and `probe.py` from `../linux-finite-tools-guard-2026-10-16/` with the current guard and its historical negative control. The public launcher replay uses the unchanged `../finite-campaign-launcher-2026-10-18/qualify-offline.py` and an exact namespace recipe with current program pins.

The development image and vendor are locally retained inputs, not a cold-install or public-release guarantee. Raw Runtime/SDK records and ephemeral proof material remain in private local evidence; the published record contains only bounded summaries and hashes.
