# macOS physical-memory protection experiment, 2026-10-11

A direct-child memory observer works on the six fixed diagnostic cases below:
five observer cases meet their oracles, and the self-only physical-limit API
returns `KERN_NO_ACCESS` (8). **Native macOS F12a remains NO_GO.** This is a tested
fixture, not an installed production watchdog or a hard memory ceiling.

PR #167 is merged at `f389059ab637a8338353ae85ed37e3b93a326101`. The experiment
builds on that tree; it changes no Rust product source, approval contract,
global configuration, entitlement or provider connection.
[ADR 0031](../../adr/0031-macos-physical-memory-observation.md) proposes the
separate monitoring contract and the remaining integration tests.

## Results

The source, binary and collector hashes, exact outputs, wait results and
independent review are in [evidence.json](evidence.json). Values below are
observations on macOS 26.5.1, Darwin 25.5.0, ARM64, outside the outer development
sandbox. Each observer owns a single inert child, which creates no descendant.

| Case | Actual observation | What it establishes |
| --- | --- | --- |
| Under threshold | 4 MiB mapping touched; footprint rises from 917,696 to 5,128,408 bytes, below 9,306,304; no signal, normal exit and reap | Readable physical footprint and a non-triggering control |
| Threshold crossed | 24 MiB mapping touched; threshold 9,289,920 bytes, observed footprint 26,099,952; owner sends SIGKILL, wait confirms signal 9 | Detection and termination after the observed crossing |
| Injected query failure | Real baseline first, then injected EIO; UNKNOWN; owner terminates and reaps child | Error never becomes a zero reading or permission to continue |
| Injected identity mismatch | Real baseline first, then changed start identity; UNKNOWN; only the lifecycle owner signals its retained child | Invalid sample cannot choose a signal target; cleanup remains possible |
| Elapsed deadline | 100 ms target, observed fixture duration 127,045,000 ns; owner terminates and reaps child | Tested timeout path, not a real-time 100 ms guarantee |
| Physical-limit API | One self-only 16 MiB request returns Mach 8; no allocation follows | Access refusal on this host, not proof that every macOS mechanism is impossible |

The crossing case observes **16,810,032 bytes above its threshold**. It touches
the fixed buffer before reporting completion, so it does not measure continuous
allocation admission. Polling cannot prevent this overshoot or bound unobserved
peaks. Outer collection took about 51 ms for this case; that is not a measured
signal-response latency or a production performance estimate.

The five inner children and all six outer leaders have confirmed wait results
in the corrected campaign. No outer rescue signal, truncation or outer timeout
occurred. The pure reap-token test also passed. The independent security reviewer
checked source and receipts but did not execute the probes.

## Failed collection and corrected interpreter

The first invocation used system Python 3.9.6, which lacks `os.waitid`, although
the local Python 3.14.8 exposes it. The collector failed after spawning only the
normal fixture and before recording output. Its touched bytes and wait/reap
result remain UNKNOWN. A later exact-path process lookup found no fixture;
that lookup does not retroactively prove successful reaping.

The corrected campaign used the compatible interpreter with the same C source,
binary and collection body. The published collector now checks the required
capabilities before creating its output directory or child. Separate checks
verify the pre-spawn refusal under 3.9 and replay refusal under 3.14. The
executed collector hash and the subsequent three-line preflight change are
recorded separately; the native campaign was not rerun for that guard.

Each case has an 8-second outer deadline, 1 MiB combined output cap and fixed
arguments. The child alarm is 4 seconds; the observer alarm is 3 seconds. The
collector reserves its unreaped leader through any group signal, requests stop
at 6 seconds, kills at 7, stops collecting at 7.5, and waits only within the
remaining 8-second budget. The source also bounds observations to 256.

The corrected campaign touched 28 MiB of explicit mappings; the refused SELF
case touched none. Including the incomplete first normal attempt, actual
explicit mappings total at most 32 MiB. Their combined planned maximum was
64 MiB (first 4 plus corrected maximum 60). These bounds concern fixture buffers,
not compiler, libc, kernel pages or total machine memory.

## Reproduce in a fresh temporary directory

This is a macOS-only inert diagnostic. Use a Python exposing `os.waitid` and
`WNOWAIT`. An unsupported interpreter refuses before spawning. Copy the files
first so a new campaign cannot overwrite the frozen receipts:

```sh
AR_MEMORY_PROBE_DIR=$(mktemp -d)
cp -R docs/bench/macos-memory-protection-2026-10-11 "$AR_MEMORY_PROBE_DIR/probe"
cd "$AR_MEMORY_PROBE_DIR/probe"
/usr/bin/clang -std=c11 -O0 -g -Wall -Wextra -Werror tests/memory_observer_fixture.c -o memory-observer-fixture
/usr/bin/clang -std=c11 -O0 -g -Wall -Wextra -Werror tests/reap_guard_test.c -o reap-guard-test
./reap-guard-test
python3 run_campaign.py
```

The collector refuses if `campaign/` already exists and stops after an abnormal
result. It has a closed six-case list and accepts no caller-supplied PID or
memory amount. A refusal of the SELF API is a diagnostic result; a successful
setter alone is only `InstalledOnly`. Do not replace the collector's result
with a hard-cap claim, even if a process later exits with SIGKILL.

For each completed case, independently check its captured output:

```sh
python3 tests/check_receipt.py normal 0 campaign/normal.stdout
python3 tests/check_receipt.py growth 0 campaign/growth.stdout
python3 tests/check_receipt.py failread 0 campaign/failread.stdout
python3 tests/check_receipt.py mismatch 0 campaign/mismatch.stdout
python3 tests/check_receipt.py deadline 0 campaign/deadline.stdout
python3 tests/check_receipt.py self 0 campaign/self.stdout
```

Pass the actual exit code from `campaign/summary.json`, not a substituted zero
if a future run fails. The commands above describe this recorded campaign.
The validator checks receipts, not the host: read the collector timeout,
truncation, hashes and cleanup evidence too. The source-level error cases are
injections; this campaign did not induce a real OS permission loss mid-run.

## Local validation

Both C files compile with warnings denied; the pure ownership guard passes.
The receipt tests replay six captured outcomes and reject nine corrupted claims
without executing native probes:

```sh
python3 docs/bench/macos-memory-protection-2026-10-11/tests/test_check_receipt.py
```

Formatting, warnings-denied Clippy, harness parity, ADR validation, 443 local
links, the secret scan and whitespace checks pass. The Rust workspace suite was
not rerun: Rust product, assets, harness and build sources are unchanged from
PR #167 and its [recorded successful suite](../macos-memory-followup-2026-10-11/verification.json).
Those historical test counts are not added to this experiment's results.

## Production decision still open

The tested option is a physical-footprint alarm with termination for a single
retained child. Codex and other native clients can create additional processes;
this fixture does not protect their whole tree or qualify the copied production
composition. No new user-facing runtime option is installed.

The next review must choose whether a separately named, opt-in monitoring mode
is useful with that weaker guarantee. Implementing it requires typed evidence,
a trusted stop receipt, binding to approval, lifecycle integration and tests
across exec, cancellation, permission failure and child creation. It must remain
ineligible whenever a hard memory bound is required. If the requirement remains
a hard job-wide budget, qualify a different isolation boundary instead.

The 8 GiB address-space default remains unchanged. [#132](https://github.com/FlorianBruniaux/agent-router/issues/132)
and [#133](https://github.com/FlorianBruniaux/agent-router/issues/133) stay open.
No model call, new savings claim or release qualification follows from this
experiment.
