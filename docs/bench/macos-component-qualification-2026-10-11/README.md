# macOS component qualification, 2026-10-11

The tested components pass, but the native finite-client path remains **F12a NO_GO**.
The unchanged helper refuses the approved 8 GiB address-space limit on this host.
No native model client or provider was launched by this campaign.

Source: merged main `727dcd21849e9f0513c69c740fd1aade1cd540bd`,
tree `620269b63934fcafd3e4ebce7dc50be8c07d8d92`, after [PR #165](https://github.com/FlorianBruniaux/agent-router/pull/165).
The helper diagnostic was derived from `f8d27d40811006da288cdb9d2489ba740f90df2c`,
which has the same tree. Receipts pin the observed binaries separately.

## What passed, and what it proves

| Check | Passed | Scope and limit |
| --- | ---: | --- |
| Finite-client contracts | 34 | Synthetic copied assets and preparation contracts; no composed native launch. |
| Network authority | 4 | Includes refusal of product broker-only admission without qualified authority. |
| Memory limit test | 1 | Requests 1 GiB and permits explicit rejection. Passing is not proof of enforcement. |
| Helper lifetime | 6 | Standalone shell fixtures with address-space limit disabled for those fixtures; no 8 GiB qualification. |
| Seatbelt canary | 1 | Explicit macOS write/network canary for the existing system-wrapper test composition. |
| Host-state canary | 1 | Explicit state/workspace/store canary for its existing test composition. |
| **Total** | **47** | Six successful commands; independent component evidence only. |

[verification.json](verification.json) records exact commands, source identity,
exit status and private-log hashes. The two ignored macOS canaries were explicitly
selected and executed outside the outer development sandbox. These checks do not
establish confinement of the copied native client, wrapper and helper together.

## Memory refusal

The unmodified helper was probed three times with exactly 8,589,934,592 bytes:
baseline, a child-only `MallocNanoZone=0` diagnostic, then baseline again.
All three returned 125 with `sandbox-helper.setrlimit.address_space`.
No global environment or product configuration changed.
The allocator hypothesis did not unblock this host.
See [allocator-control.json](allocator-control.json).

A separate instrumented copy then recorded, in one process:

- requested and clamped address-space limit: 8,589,934,592 bytes;
- inherited and effective soft/hard limits: unlimited;
- failed `setrlimit`: errno 22 (`EINVAL`);
- own VSZ before and after the call: 435,300,480 KiB;
- exit 125, with no success marker.

See [memory-refusal.json](memory-refusal.json). The outer collection deadline was
10 seconds with an 8 KiB combined-output cap; neither bound was reached.
The process was a diagnostic helper, not the native client or a provider.

The VSZ samples bracket the call and are not atomic kernel-map measurements.
Instrumentation, formatting and the `ps` child can affect virtual layout.
This result must not be relabeled as a measurement from the unmodified product
binary. VSZ is virtual address space, not resident RAM.

In the pinned Apple source, the [RLIMIT_AS branch](https://github.com/apple-oss-distributions/xnu/blob/f6217f891ac0bb64f3d375211650a4c1ff8ca1ea/bsd/kern/kern_resource.c#L1647)
maps a rejected map-size limit to EINVAL, and
[`vm_map_set_size_limit`](https://github.com/apple-oss-distributions/xnu/blob/f6217f891ac0bb64f3d375211650a4c1ff8ca1ea/osfmk/vm/vm_map.c#L22084)
rejects a new limit below the current map size. This is consistent with the
diagnostic, but the source-to-installed-kernel match remains **UNKNOWN**.
See [apple-source-reference.json](apple-source-reference.json).

Host: macOS 26.5.1, Darwin 25.5.0, ARM64. These results apply to the recorded
host and binaries; they are not a universal macOS qualification.

## What must pass next

1. Preserve the reviewed 8 GiB per-process virtual-address-space contract while
   establishing whether the intended helper and client can start under it.
   A larger limit, no limit, or a resident-RAM watchdog changes the contract.
   None is adopted here.
2. If that gate becomes feasible, test the exact copied helper, wrapper, client,
   profile and owned state as one composition. Bind their bytes, resource
   arguments and fresh capability evidence to the reviewed request.
3. Demonstrate permitted state writes and denied workspace/store/asset writes,
   then direct TCP, UDP and DNS restrictions plus the intended broker exception.
   Run controlled local fixtures first. Existing component canaries do not
   substitute for these composition checks.
4. Exercise below/above-limit allocation after exec and in descendants, timeout,
   cancellation and cleanup. Report per-process limits separately from aggregate
   job memory. Treat untested descendant escapes as UNKNOWN.
5. Only then evaluate native product admission. `ControllerBrokerOnly` remains
   refused in the current product path; diagnostic results confer no execution
   authority. A real provider trial also needs its own approved measurement
   protocol and reliable counters.

The current limit fails before those composition checks can qualify this host.
This dossier supports a refusal and a narrower next experiment, not a native GO
or an economy claim.

## Reproduce the inert diagnostic

[diagnostic-build.json](diagnostic-build.json) pins the source, patch, build inputs,
binary and preparation checks. [diagnostic/](diagnostic/) preserves the patch and
standalone build manifests, outside the product workspace.

From the repository root, reconstruct into a fresh temporary directory:

```sh
AR_DIAG_DIR=$(mktemp -d)
AR_EVIDENCE="$PWD/docs/bench/macos-component-qualification-2026-10-11"
mkdir "$AR_DIAG_DIR/src"
git show f8d27d40811006da288cdb9d2489ba740f90df2c:crates/router-sandbox-helper/src/main.rs > "$AR_DIAG_DIR/src/main.rs"
cp "$AR_EVIDENCE/diagnostic/Cargo.toml" "$AR_EVIDENCE/diagnostic/Cargo.lock" "$AR_DIAG_DIR/"
patch "$AR_DIAG_DIR/src/main.rs" "$AR_EVIDENCE/diagnostic/diagnostic.patch"
cd "$AR_DIAG_DIR"
shasum -a 256 src/main.rs
rtk proxy cargo build --offline --locked
rtk proxy cargo test --offline --locked
rtk proxy cargo clippy --offline --locked --all-targets -- -D warnings
rtk proxy cargo fmt --all -- --check
```

Expected source SHA-256:
`e20d14294883e15bec701bdcc287fa40ca6bd8b83ca52e014028536547a96bc9`.
Offline builds require the locked dependencies in the local cache.
These commands build and run two pure tests; they do not run the native probe.
The patch removes worker/supervisor entry points and admits only the exact
8 GiB probe arguments. Re-running that probe requires bounded outer collection,
an empty child environment except `PATH=/usr/bin:/bin`, and a fresh receipt.
A rebuilt binary may have a different hash.

The original diagnostic preparation passed formatting, Clippy and two pure tests.
The coordinator separately reviewed the diagnostic source and ran the bounded
probe. These two pure tests are not included in the 47 product-component results.
