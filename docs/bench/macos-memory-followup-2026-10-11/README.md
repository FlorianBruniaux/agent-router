# macOS memory follow-up, 2026-10-11

The exact 8 GiB address-space request is refused by both a minimal C fixture and
the unchanged product helper, outside the outer development sandbox. **F12a
remains NO_GO.** This narrows the failure beyond Rust-helper overhead; it does
not identify the owner of the large virtual reservation or qualify an alternative
memory mechanism.

Base: `603f3c520840c76fdb64b7aead6896e28f4d009a`, after PR #166.
Host: macOS 26.5.1, Darwin 25.5.0, ARM64. No native model client, provider,
large allocation or global configuration change was performed.

## Observed evidence

[evidence.json](evidence.json) contains the redacted host captures and source,
binary and private-log hashes. [vm-probe.c](vm-probe.c) is the complete inert
fixture. The product helper was rebuilt from the stated base with the locked
local dependencies.

| Observation | Result | Scope |
| --- | --- | --- |
| Minimal C fixture, exact 8 GiB soft/hard request | Exit 125, `setrlimit` returns -1 with errno 22 | Explicit refusal, not successful enforcement |
| Unchanged product helper, exact 8 GiB request | Exit 125, `sandbox-helper.setrlimit.address_space` | Fresh independent reproduction of the product refusal |
| C fixture at entry | 445,746,970,624 virtual bytes; 1,474,560 resident bytes | About 415 GiB virtual versus 1.4 MiB resident, not RAM exhaustion |
| Region inventories | 56 top-level regions and 81 leaves; each walk totals 445,746,970,624 bytes | Two separate views; never add their totals |
| Largest leaf | 412,316,860,416 bytes (384 GiB), protection 0, maximum protection 0, tag 0, share mode 3 | No access; the owning subsystem is UNKNOWN |
| After refused request | Both address-space limits remain unlimited | No installed memory bound |

Both host captures finish before their bounds. Collection allows 8 seconds,
process wait ends at 9 seconds, and forced termination has a separate 1-second
reap wait. Combined output is capped at 2 MiB. The inherited core soft limit is
verified as zero. The source contains no `mmap`, `exec`, explicit allocator or
provider call. It uses fixed arrays, at most 4,096 Mach region calls per walk and
a depth limit of 16, and formats its output only after the observations.

The preliminary sandbox capture also refused the exact request. It had a
slightly different virtual-size total and is not substituted for the host
measurement above. A first host capture lacked a recorded core limit and used
an imprecise deadline label; it is retained privately as superseded evidence.
The final host pair above records both explicitly. Neither preliminary result
is counted as an extra qualification success.

## Interpretation and limits

The pinned Apple [RLIMIT_AS implementation](https://github.com/apple-oss-distributions/xnu/blob/f6217f891ac0bb64f3d375211650a4c1ff8ca1ea/bsd/kern/kern_resource.c)
returns EINVAL when the map-size limit cannot be installed.
[`vm_map_set_size_limit`](https://github.com/apple-oss-distributions/xnu/blob/f6217f891ac0bb64f3d375211650a4c1ff8ca1ea/osfmk/vm/vm_map.c)
refuses a limit below the existing map size. Public `task_info` virtual sizes
use `vm_map_adjusted_size`, not an atomic read of that internal field. The
[task implementation](https://github.com/apple-oss-distributions/xnu/blob/f6217f891ac0bb64f3d375211650a4c1ff8ca1ea/osfmk/kern/task.c)
and [exec implementation](https://github.com/apple-oss-distributions/xnu/blob/f6217f891ac0bb64f3d375211650a4c1ff8ca1ea/bsd/kern/kern_exec.c)
also make successful installation, inheritance and post-exec enforcement
separate questions.

These sources are consistent with the observed refusal. Their correspondence
to the installed XNU build remains UNKNOWN. The diagnostic is not an atomic
kernel-map trace, and its own startup/instrumentation affects layout. No tag or
protection value identifies the large reservation as an allocator or dyld
mapping. The earlier allocator toggle failed and is not repeated here.

## Next decision

Do not silently raise or remove the 8 GiB limit, or substitute a RAM watchdog.
The unchanged contract remains unqualified on this host. Continuing via a
different memory mechanism first requires a reviewed contract that states what
is bounded: virtual space or resident memory, each process or the whole job,
hard refusal or monitored overshoot, and behavior across fork/exec and cleanup.
No alternative mechanism is qualified by this campaign.

If a future composition can install the unchanged contract, it still needs
post-exec and descendant enforcement tests. Otherwise, retain macOS NO_GO while
reviewing the replacement contract. Only after a positive memory gate can the
exact copied client/wrapper/helper/profile undergo network, filesystem and
lifecycle qualification. Physical-send admission and real-provider measurement
remain separate gates in [#132](https://github.com/FlorianBruniaux/agent-router/issues/132)
and [#133](https://github.com/FlorianBruniaux/agent-router/issues/133).

## Reproduce the minimal fixture

From this directory, build into a fresh temporary directory:

```sh
AR_VM_PROBE_DIR=$(mktemp -d)
/usr/bin/clang -std=c11 -O0 -g -Wall -Wextra -Werror vm-probe.c -o "$AR_VM_PROBE_DIR/vm-probe"
```

Run the inert child with an external timeout, core-limit precheck and empty
environment except `PATH`. This reproduction writes a fresh capture next to the
new binary and leaves the frozen evidence unchanged:

```sh
python3 - "$AR_VM_PROBE_DIR/vm-probe" <<'PYCODE'
import pathlib, resource, subprocess, sys
binary = pathlib.Path(sys.argv[1]).resolve()
if resource.getrlimit(resource.RLIMIT_CORE)[0] != 0:
    raise SystemExit("refused: inherited core soft limit must be zero")
result = subprocess.run(
    [str(binary), "--exact-8gib"], stdin=subprocess.DEVNULL,
    stdout=subprocess.PIPE, stderr=subprocess.PIPE,
    env={"PATH": "/usr/bin:/bin"}, timeout=8,
)
if len(result.stdout) + len(result.stderr) > 2 * 1024 * 1024:
    raise SystemExit("refused: diagnostic output exceeds 2 MiB")
(binary.parent / "capture.jsonl").write_bytes(result.stdout)
(binary.parent / "capture.stderr").write_bytes(result.stderr)
print("diagnostic exit:", result.returncode)
PYCODE
```

This short reproduction checks the output size after collection; the original
host collector additionally enforced its cap while reading. The source's fixed
region limits bound the fixture output. The fixture changes only its own
address-space limits, never the parent's. Exit 125 means refusal; exit 0 means
installation only, not post-exec enforcement or native qualification. Preserve
OS, source and binary hashes with any new result. No larger-limit or
allocator-flag search is part of this protocol.

The malformed-argument check (`--none`) must return 64 without output. All Mach
return codes and complete region walks must be checked before interpreting
size fields; a failed or partial measurement is UNKNOWN.

## Verification of the combined delivery

[verification.json](verification.json) binds the reviewed measurement sources to
formatting, warnings-denied Clippy, the complete all-features Rust workspace,
harness parity and the secret scan. The corrected workspace reports 1,762
successful result lines, 0 failures and 42 ignored opt-in native results. These
counts include nested helpers, not unique test definitions; focused checks are
not additive. An independent Rust reviewer found no remaining blocking defect
in the v3 projection or these diagnostic claims.

The first workspace run failed 2 unchanged doctor tests because the coordinator
exported `RUST_TEST_THREADS=1`. Their child test's output then placed the marker
on the harness's progress line; the parent expects an exact marker line. A
controlled comparison reproduced that formatting difference. Removing the
inherited variable and passing `--test-threads=1` only to the outer test harness
restored the marker. The 5 doctor tests and the full workspace passed without a
product/test source change for this failure. The initial run and its 2 failures
remain recorded; it is not relabeled as a passing run.

These source checks and diagnostic refusals establish neither native macOS
execution, Linux qualification, cloud-CI success nor measured savings.
