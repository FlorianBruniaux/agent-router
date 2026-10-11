# Strict 8 GiB startup follow-up, 11 October 2026

**F12a remains NO_GO.** Four controlled runs of the inert C fixture refuse the
unchanged 8 GiB address-space request. The largest no-access region has exactly
the address and size of the GPU carveout defined by pinned Apple XNU sources.
This is a geometry match, not verified attribution to the installed kernel.

Base: `f389059ab637a8338353ae85ed37e3b93a326101`, containing merged PRs #166 and
#167. The production helper, resource contract, environment and native admission
are unchanged. No native model client, provider call, large physical allocation, global
configuration or region deallocation is attempted.

## Diagnostic and controls

[vm-probe.c](vm-probe.c) extends the
[previous fixed-array fixture](../macos-memory-followup-2026-10-11/vm-probe.c)
with a retained region address and a boolean comparison against two pinned
constants. It does not print addresses or change the measured map. All task
and region observations finish before formatted output. Each run requests
`8589934592` bytes for both soft and hard `RLIMIT_AS` values, never `none` or a
larger limit. Malformed, missing and extra arguments return 64 without output.

[evidence.json](evidence.json) retains source/binary hashes, successful compiler
exit status, exact host identity, bounded-capture
receipts, complete region summaries, geometry matches and limit results.
[verification.json](verification.json) records local source checks separately.
Raw stdout/stderr captures remain in the private diagnostic directory named in
the evidence; their hashes bind the published summaries.

| Control, in execution order | Child environment change | Exact request | Result |
| --- | --- | --- | --- |
| Baseline before | None | 8 GiB soft/hard | Exit 125, errno 22 |
| Private shared cache | `DYLD_SHARED_REGION=private` | 8 GiB soft/hard | Exit 125, errno 22 |
| Avoid control | `DYLD_SHARED_REGION=avoid` | 8 GiB soft/hard | Exit 125, errno 22 |
| Baseline after | None | 8 GiB soft/hard | Exit 125, errno 22 |

The only other child environment variable is `PATH=/usr/bin:/bin`. None is
exported globally or introduced into product code. The private-cache control
changes the region layout, but still retains the large matching region.
The avoid control's effective loader behavior is UNKNOWN; its name alone does
not prove that a cache was removed. All four runs retain unlimited effective
address-space values after the failed request. They prove refusal, not a bound
installed at 8 GiB.

Collection is capped at 2 MiB and 8 seconds per child, process wait at 9 seconds,
with a separate 1-second kill/reap timeout. The inherited core soft limit is
checked as zero. These are capture bounds, not native runtime qualification.
Both inventories complete in every run, with exactly one matching top-level
region and one matching leaf. Those are overlapping views, not two reservations
or independent successes. The region is 412,316,860,416 bytes (384 GiB), with
current and maximum protection zero. It persists across every control.

## Pinned-source interpretation

Apple's [ARM64 VM constants](https://github.com/apple-oss-distributions/xnu/blob/f6217f891ac0bb64f3d375211650a4c1ff8ca1ea/osfmk/mach/arm/vm_param.h#L159)
place the GPU carveout between `0x1000000000` and `0x7000000000`.
The [reserved-region table](https://github.com/apple-oss-distributions/xnu/blob/f6217f891ac0bb64f3d375211650a4c1ff8ca1ea/osfmk/arm64/machine_routines.c#L186)
names this range. The [exec map setup](https://github.com/apple-oss-distributions/xnu/blob/f6217f891ac0bb64f3d375211650a4c1ff8ca1ea/osfmk/vm/vm_map.c#L14151)
adds these reserved entries with no access and permanent mapping flags.
The same source's
[size-limit setter](https://github.com/apple-oss-distributions/xnu/blob/f6217f891ac0bb64f3d375211650a4c1ff8ca1ea/osfmk/vm/vm_map.c#L22084)
compares the requested bound with the internal map size.

The observed geometry and source behavior are consistent with a reservation
installed by the kernel before the fixture's code runs, rather than one removed
by changing its dyld cache mode. Neither the internal map size nor the installed
kernel's implementation is read here. Matching this source revision to the
running kernel remains UNKNOWN; do not claim universal macOS impossibility.

Pinned [dyld configuration](https://github.com/apple-oss-distributions/dyld/blob/fd8d0c4d52320ebf64db34f3cb280310d905c5ae/dyld/DyldProcessConfig.cpp)
also distinguishes private-cache selection from simulator-only avoid handling.
The corresponding installed loader revision is UNKNOWN. All source file hashes
are retained in the evidence.

## Bounded Linux candidate

The existing local Docker daemon and an already-present ARM64 Python image
allow a separate diagnostic without installing software, pulling an image or
starting a daemon. [linux-probe.py](linux-probe.py) is the complete fixture;
[linux-evidence.json](linux-evidence.json) records its exact source hash,
content-addressed image, command, actual output and confirmed container removal.
The image's remote registry origin is not independently authenticated here.

The container has no network, host mounts or added capabilities. Its root
filesystem is read-only, it runs as UID 65534 with no-new-privileges, one CPU,
16 PIDs and an explicit 8 GiB cgroup memory setting with no extra swap allowance.
Those cgroup settings are read back, not tested through a physical OOM event.

At fixture entry, after Python startup and before mapping or child creation,
`setrlimit(RLIMIT_AS, (8589934592, 8589934592))` succeeds. A 16 MiB no-access
virtual mapping succeeds and is closed; a 9 GiB no-access mapping returns ENOMEM.
No large physical memory allocation occurs. Attempts to raise or remove the
hard address-space bound are refused. Both a retained fork child and a separately
executed Python child report the exact inherited soft/hard values. The fixture
waits for both children and exits zero; the container is then confirmed absent.
Collection has a 25-second outer timeout; the exec child has a 5-second timeout.
The fixed one-line fixture output is checked against a 32 KiB cap after collection.

The first Docker CLI attempt rejected an unsupported `--ulimit=as` flag.
That refusal is retained in the evidence, not reported as a passing container
run. The successful diagnostic installs the same address-space value using
Python's real resource API, in addition to the separate cgroup setting.
It does not assert a pre-Python startup address-space bound.

This demonstrates the unchanged requirement in one inert Linux composition.
It does not test the product helper, copied client, Seatbelt replacement,
provider broker, escaped descendants or complete task cleanup. It cannot grant
macOS F12a admission. The next Linux unit is to build and pin the actual helper,
then repeat exact-bound probe, allocation refusal and child inheritance under
an explicitly described isolation adapter before composing the real client.

To reproduce, use the pinned local image from the receipt with `--pull=never`
and its recorded isolation flags. Send this fixture through stdin to the fixed
`/usr/local/bin/python3 -I -` entrypoint. Keep the outer timeout and wait/removal
check. Do not mount the repository, credentials or home directory, and do not
replace the exact 8 GiB address-space requirement with the cgroup memory value.

## Reproduction and remaining gate

Compile this directory's fixture into a new private temporary directory:

```sh
AR_STRICT_PROBE_DIR=$(mktemp -d)
/usr/bin/clang -std=c11 -O0 -g -Wall -Wextra -Werror \
  vm-probe.c -o "$AR_STRICT_PROBE_DIR/vm-probe"
```

Use the bounded collector and core-limit check described in the
[previous reproduction](../macos-memory-followup-2026-10-11/README.md#reproduce-the-minimal-fixture).
Run the four child environments above in order, always with `--exact-8gib`.
Retain fresh captures rather than overwriting this frozen evidence. For the
four negative cases, use no argument, `--none`, `--exact-16gib`, and
`--exact-8gib extra`; expect exit 64 and empty stdout/stderr.

No tested startup control installs the reviewed 8 GiB bound on this host.
Keep native admission closed. Removing a permanent reserved region, raising the
cap or replacing virtual-space refusal with sampled physical memory would
change the guarantee and is outside this follow-up. A physical-memory observer
from another workstream cannot satisfy this unchanged requirement.

To pursue the strict requirement, the next qualification needs a different
isolation composition that can install it, followed by allocation, fork/exec
and descendant enforcement tests. The Linux fixture above establishes a
candidate memory mechanism; the actual helper and complete isolated runtime
remain unqualified. Network, copied-asset writes,
Mach/DNS, lifecycle cleanup and physical-send admission still need their own
positive evidence. The existing
[F12a qualification table](../finite-client-qualification-2026-10-11/README.md#qualification-requirements)
remains in force.
