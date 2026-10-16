# Approved finite-client Linux qualification, 2026-10-16

This is local execution of the real pinned Codex binary through the actual Runtime approval, admission, capture and cleanup path. A controller-authored local SSE response replaces model inference. The final cohort uses the corrected network profile; the original port-only composition is rejected. There are zero model calls and no external provider, credential or private-user configuration. This qualifies this composition and these tasks only. F12a and the 24-cell model benchmark remain **NO_GO**.

## Recorded cases

| Case | Actual behavior | Cleanup and admission | Evidence |
| --- | --- | --- | --- |
| q01 | Native JSONL contains exactly the two authored high-severity findings | Namespace terminated, admission released | [receipt](evidence/q01-current/receipt.json), [transcript](evidence/q01-current/stdout.jsonl) |
| t02 existing-file write | Fixed client applies the authored complete replacement to private lib.rs; Runtime captures its patch | Namespace terminated, admission released; original three files byte-identical | [receipt](evidence/q02-current/receipt.json), [patch](evidence/q02-current/change.patch), [submission](evidence/q02-current/submission/lib.rs) |
| Timeout after local request | Actual capture expires with the local response withheld | Namespace terminated, admission released | [receipt](evidence/q01-timeout-current/receipt.json) |
| Cancellation after local request | Actual capture cancels with the local response withheld | Namespace terminated, admission released | [receipt](evidence/q01-cancel-current/receipt.json) |
| Parent EOF | External driver terminates only its exact controller after observing a local request; helper stops namespace | Filesystem cleanup UNKNOWN after two reopens, private state retained, admission granted=1/released=0 | [parent marker](evidence/q01-eof-current/parent-eof.json), [recovery](evidence/q01-eof-current/eof-recovery.json) |

[The run log](evidence/lifecycle.log) records the passing cases and the legacy broker refusal. [The source inventory](evidence/executed-source-inventory.json) identifies the exact executed source snapshot before documentation additions and isolation of the two Git integration-test helpers. No compiled production or Linux controller input changed afterwards. Program, namespace, configuration, catalogue, stdin, combined review, original file and concrete resource pins are retained in each case's pins.json. Original controller paths are historical provenance, not runnable filesystem pointers or capabilities. Per-case result.json contains the actual journal and attempts. An interrupted attempt has no completed attempt-evidence record; the EOF test separately verifies the durable summary's attempt count is one.

The coordinating session independently checked q01 with its strict oracle, decoded the four streams and checked bytes, journal order, patch and EOF receipt consistency. Its t02 oracle replay was blocked by a nested macOS sandbox error; the runtime owner executed the same frozen external oracle successfully (4 tests), and the other session independently checked the frozen bytes and report hash. [The reported review](independent/current-reported-review.json) preserves these distinct provenances. The common executed-source inventory preserves every path/hash entry and the original raw manifest digest. Final reports distinguish independent checks reported through the coordinating chat from retained controller evidence. Those checks do not rewrite Runtime objective/validation UNKNOWN or human acceptance null. Native reported usage is scripted fixture telemetry. Observed attempt tokens/cost remain null, executed model/effort remain UNKNOWN, and successful transport remains Verifying. The [independent network witnesses](network-address-fixed-components/scope.json) separately verify reservation, actual broker destinations, no external route and refusal before a sentinel executes when another interface is active. They are component evidence, not substitutes for these Runtime runs. The local metadata describes direct mode only; it does not qualify real Luna CodeModeOnly support or context overhead.

Independent review of the earlier pre-fix final2 receipts exposed a missing network boundary during independent review. They are historical transport evidence only and do not qualify the corrected composition. [The independent RED witness](network-address-red/witness.json) demonstrates that Landlock restricts TCP ports, not IP addresses: another service on 127.0.0.2 at the same port was reachable.

The corrected profile is linux-isolated-broker-v2. It refuses routed namespaces and active interfaces other than the actual loopback. Inactive kernel fallback devices may exist only with IFF_UP and IFF_RUNNING clear, verified per device with actual ioctls before exec. All five capability sets must be zero and NoNewPrivs must be one; IPv4 routes must be empty. The controller retains the exact network namespace descriptor and hashes its identity into the broker instance. An exclusive IPv4 wildcard listener has both SO_REUSEADDR and SO_REUSEPORT zero. Kernel namespace checks and the reservation are repeated at review, composition and immediately before dispatch; the guard independently checks the actual interface flags before exec. The guard itself still filters ports only. This composition supports the owned local namespace, never a routed general host.

The detached descendant canary grows during the probe and stops after namespace exit. Allowed broker TCP, denied other ports/families, private-file refusal, configuration/catalogue protection, read-only source, sacrificial positive write, bounded allocation and refused 9 GiB reservation are mandatory actual effects before fresh proofs are minted. Private-home system-skill install warnings remain visible in stderr; no private access is widened to suppress them.

One coordination duplicate was started on /task/root-q01-final and interrupted by its owning session. It is outside these five cases and has UNKNOWN cleanup, never PASS. It did not execute a model. No benchmark cell or success count is inferred from it.

## Reproduce on an isolated Linux ARM64 host

The legacy loopback-only broker also refuses construction through the isolated entry and dispatch composition before any native invocation; [its refusal](evidence/legacy-network2/legacy-refusal.json) is separate from model transport.

The ignored tests require unprivileged namespaces, Landlock ABI 6, seccomp, a verified network namespace with only the loopback active, and the pinned native executable. All five capability masks must already be zero and NoNewPrivs one. Do not change those to make qualification pass. macOS unit tests cannot provide this evidence. Dependencies must already be cached; commands below use Cargo offline and do not install software or call a model. The archived private development Docker image is not a public reproducibility artifact.

The original outer container had no network, no host mounts, a read-only root, uid 65534, no capabilities, no-new-privileges and an aggregate memory.max of **8589934592**, memory.swap.max **0**. The only shared mount is the named build-cache volume ar-pr176-native-build-20261015 at /work, with the prepared public source and offline dependencies. Its [recorded settings](evidence/container.json) distinguish the outer test harness from the inner sandbox. The outer seccomp profile allows nested rootless bubblewrap; the inner fixed guard supplies the tested restrictions. This is not a privileged container. Do not raise memory or add user/private mounts to make a failed probe pass.

Prepare a fresh owned directory outside the checkout. Copy the native executable (Codex 0.159.2 Linux ARM64 musl, SHA-256 113aa5d5952a6fd3950a88323219747d0c91978ebee948509a540c8a460e59a3) and Debian bubblewrap 0.8.0 (recorded SHA-256 ff5add553bea12a45c18ebe6c67d3ed637052b869a1ca2fc5d74a6a2e8c5cce6) there as native and namespace. Build the guard and both Rust programs from this checkout, then place them as guard, client and helper. Write profile with the exact bytes {"kind":"linux-isolated-broker-v2","schema_version":1} followed by a newline. Executables must be mode 0500; profile mode 0400. Do not reuse a program from an earlier source snapshot.

```sh
cargo build --release -p router-runtime --bin finite-codex-client --offline --locked
cargo build --release -p router-sandbox-helper --offline --locked
cc -O2 -Wall -Wextra -Werror crates/router-runtime/support/linux-finite-guard.c -o "$AR_FINITE_PROGRAMS/guard"
# Copy the two just-built executables into AR_FINITE_PROGRAMS as client and helper.
export AR_FINITE_FIXTURE="$PWD/docs/bench/linux-finite-dispatch-2026-10-16/inputs/q01"
export AR_FINITE_WRITE_FIXTURE="$PWD/docs/bench/linux-finite-dispatch-2026-10-16/inputs/t02"
# Set AR_FINITE_ROOT to a new absolute owned directory for each test.
cargo test --release -p router-runtime --lib --offline --locked q01_real_approved_local_report -- --ignored --nocapture --test-threads=1
cargo test --release -p router-runtime --lib --offline --locked q02_real_approved_bounded_replacement -- --ignored --nocapture --test-threads=1
cargo test --release -p router-runtime --lib --offline --locked q01_real_timeout -- --ignored --nocapture --test-threads=1
cargo test --release -p router-runtime --lib --offline --locked q01_real_cancel -- --ignored --nocapture --test-threads=1
```

The q01 controller data and authored t02 replacement under inputs stay outside all worker mounts. Parent EOF requires the separate [external controller](parent-eof.py), with the same program/fixture environment and a new AR_FINITE_ROOT. It selects Cargo's exact test executable, observes a real local request and a live cleanup lease, terminates that exact PID, checks the process-stop marker and reopens twice without releasing unknown private cleanup. It does not delete that private state. Destroying the disposable outer test container is harness disposal, not product recovery evidence.

## Remaining product boundaries

Only one existing flat file of at most 32768 bytes is supported. The response has one replacement string, never a path. The original source remains unchanged. Both the original and private Git view must remain clean at the reviewed head immediately before dispatch. Generic reopen and Mutation::Integrate refuse this opaque finite handle. Failed writes may leave a partial private file; they never publish it. Executed views remain for artifact review and need a future managed disposal contract.

The absent review.json in sealed r2 t05 requires separately authorized file creation. General tools, H1 feedback, CLI benchmark dispatch, native cumulative trial budgets, remote accounts, publication and real CodeModeOnly execution remain unqualified. A scripted t02 patch is not an r2 benchmark cell or evidence of model quality, production safety or savings. See [ADR 0037](../../adr/0037-qualified-local-finite-dispatch.md).

## Local verification

The full workspace suite passed with normal parallelism: 1871 passed, 0 failed, 42 ignored. The Linux finite client contract suite passed 15 tests. The outer memory remained 8589934592 bytes and swap zero for every recorded case. Runtime fixture Git commands now clear inherited configuration and disable user hooks: a global asynchronous Git AI notes hook caused unrelated object writes during a publication refusal test. The publication assertions are unchanged. A separate local quality runner checks formatting, all-targets warnings-denied Clippy, serial workspace contracts, harness parity, Python suites and secrets. Its final report is retained outside this checkout; final delivery records the exact checks and source inventory. CI credit availability is separate and no remote green verdict is claimed.

The merged stdout/stderr parallel log splits one 9-test result across Cargo progress lines; the complete serial report confirms the same 1871 tests. Verbatim native fixture SSE and captured logs intentionally preserve their terminal blank lines.
