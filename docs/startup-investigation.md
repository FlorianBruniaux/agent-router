# First-launch investigation

NFR003 remains **not satisfied** by the measured macOS candidates. Its external
150 ms startup threshold is unchanged. No production profile or signing change
is proposed from this experiment: every candidate exceeded it on its first
observed launch.

The useful result is localization. An instrumented release CLI took 530.251 ms
on its first launch: 528.744 ms elapsed before entry to `main`, while the main
function took 0.766 ms. Its total child CPU usage was 4.899 ms. The slow interval
therefore precedes the measured CLI work. A minimal Rust program and a minimal
C program reproduce it, so changing CLI parsing or Rust initialization is not
supported as a remedy by these observations.

This does **not** identify a particular security service. Process scheduling,
loader work and operating-system inspection are not individually traced here.
`DYLD_PRINT_STATISTICS=1` produced no timing output on this machine. Attribution
to Gatekeeper, XProtect, a cache or another daemon remains `UNKNOWN`.

## Fixed inputs and measurement boundary

Measurements ran on 2026-09-27 between 10:06 and 10:11 UTC, on macOS 26.5.1,
Darwin 25.5.0, arm64, 18 logical CPUs, Rust/Cargo 1.85.1. No hostname, username,
serial number, credentials or process inventory is included in the evidence.

The experiment copied the workspace's manifests, source and fixtures into a
private temporary directory. The 306-file source fingerprint was unchanged
before and after both profile builds:

```text
9e8edfa79b409695236c7634bada4e9aa16fe40e94a51a931621e6eef892b43f
```

Both default and thin-LTO profiles built from that exact copy with
`--release --locked --offline`. Only candidate environment variables changed:
`CARGO_PROFILE_RELEASE_LTO=thin` and
`CARGO_PROFILE_RELEASE_STRIP=symbols`. No workspace production profile changed.
Unlike the earlier experiment in `performance.md`, source drift is excluded
from this particular profile comparison.

Every series contains 100 new processes invoking `--version`. Time covers the
parent's spawn through child exit, with stdout/stderr captured and no shell.
Percentiles use nearest rank. The first invocation happened before executable
hashing, signature inspection or signature verification. Filesystem and loader
caches were not flushed, so **cold OS cache is unknown**. The freshly linked
binary and the first invocation at a new path are separately identified below.
These are not claims about a downloaded, notarized distribution.

Other development work was not paused. One-minute system load averages ranged
from 70.15 to 107.22 across series; load averages are not CPU utilization. Some
series also overlapped candidate compilation, and the recorded load changed
between profiles. This prevents a credible speedup estimate from their timing
difference. All outliers, including later launches over 150 ms, remain in the
denominator.

The temporary probe writes a wall-clock timestamp at the first statement of
`main` and a monotonic elapsed duration on return. The parent and child use the
same machine's wall clock for the pre-main subtraction. Clock adjustments are
not controlled; scheduling and parent launch overhead are part of that value.
The probe source is separate from both uninstrumented profile builds and has
fingerprint `187e2fc4d6b36885e455c4b2702a55479b170c9f0ce8b0b5cf8ae909d39e3b56`.

## All measured candidates

Durations are milliseconds; sizes are bytes. Every first launch below exceeds
150 ms. The maximum happens on the first launch in each series.

| Candidate | Size | First / maximum | p50 | p95 | Additional samples >=150 ms |
| --- | ---: | ---: | ---: | ---: | --- |
| Default release, newly linked | 10,705,952 | 739.614 | 11.044 | 26.695 | None |
| Same sources, thin LTO and symbol stripping, newly linked | 8,802,480 | 595.494 | 12.034 | 50.740 | None |
| Default code, standard ad hoc signature A | 10,661,824 | 462.600 | 12.244 | 49.300 | #41: 167.090 |
| Default code, standard ad hoc signature B | 10,661,824 | 427.647 | 15.497 | 107.379 | #90: 159.011; #91: 233.149 |
| Byte-identical copy of the already executed default binary, first use of new file | 10,705,952 | 440.002 | 19.969 | 103.504 | #44: 150.762; #65: 255.854; #88: 203.003 |
| Release CLI with temporary main probe, newly linked | 10,722,496 | 530.251 | 8.393 | 21.949 | None |
| Minimal Rust control v2, newly linked | 450,080 | 501.561 | 9.419 | 22.408 | None |
| Minimal C control, newly linked | 33,584 | 498.614 | 8.708 | 51.895 | None |

The profile candidate reduces the file by 17.78%, but does not meet NFR003.
Its first launch is slower than some unoptimized signed copies and the minimal
controls. No reproducible startup gain follows from these differently loaded
series. Its improved size alone is not a reason to present it as the startup fix.

| Instrumented first launch | Before main | In main | Child user + system CPU |
| --- | ---: | ---: | ---: |
| Release CLI | 528.744 | 0.766 | 4.899 |
| Minimal Rust | 500.849 | 0.053 | 4.065 |
| Minimal C | 483.520 | 0.022 | 4.315 |

Across all 100 instrumented CLI launches, the maximum measured main duration
was 1.986 ms. This is evidence about `--version`, not routing, database access,
startup of an external host or every application command.

## Signature and security boundary

The default, thin and instrumented executables were ad hoc linker-signed
Mach-O arm64 files with 4 KiB signature pages. Candidate signatures A and B used
the ordinary `codesign --force --sign - --timestamp=none --identifier ...`
operation on private copies only. They use 16 KiB signature pages and retain
ad hoc status without a team identity. No signing identity or keychain
credential was discovered or used.

The default, both re-signed copies and the byte-identical copy have the same
`__text` machine-code section hash:

```text
e6dbde58e68d2c9ddf1cd0c9668994167f23ffb64af60b40269c624b2da2e6b5
```

Post-measurement strict signature verification passed for those four files,
the instrumented CLI and the thin-profile executable. Their CLI stdout hashes
also match. These checks support the comparison; they are not authentication,
notarization or macOS policy approval.

`com.apple.provenance` remained present on every candidate. No extended
attribute was removed, no quarantine flag was cleared, no policy database or
security setting was changed, and no global process was stopped. First-use
delay persisted even for a byte-identical copied executable, so this evidence
does not justify treating content hash alone as an explanation of OS caching.

## Retained evidence and collector limitation

Raw reports, per-launch journals, frozen source copies, binaries and the
temporary probe live in the preserved private directory:

```text
/private/tmp/agent-router-startup-mwyyjsxh
```

The eight report names are `default.json`, `thin.json`, `adhoc-a.json`,
`adhoc-b.json`, `same-identity.json`, `instrumented.json`, `minimal-v2.json` and
`minimal-c.json`. Each has a corresponding `.launches.jsonl` journal retaining
all 100 individual durations. Reports include binary digests, signatures,
source fingerprints, load observations, CPU timing and every threshold
exceedance. `section-verification.json` records the machine-code comparison.
These local evidence files are not a published distribution or a CI result.

The earlier minimal Rust v1 collector attempt completed 100 launches but
failed during metadata inspection because this Python installation lacks
`os.listxattr`. Its timings were lost before persistence and are explicitly
unusable, recorded in `collector-failure.json`. It was not relabeled as a fresh
run. The v2 control changed its literal/build output before its first launch.
The collector now flushes each observation into a journal immediately, uses
the macOS `xattr` read command when necessary, and retains failed/timeout
launch status before stopping a series. The original 503 ms observation and
all other results in `performance.md` remain unchanged.

To repeat on a newly built candidate, use `scripts/benchmark-startup.py` with
`--binary`, a new `--output`, `--label` and the frozen source's
`--source-sha256`. It refuses to overwrite earlier reports/journals. Repeating
an already measured executable is a warm re-execution and must be labeled that
way. Inspect signatures only after its timed first launch. The script is a
measurement tool for controlled candidates, not a subprocess security boundary.

There is no proposed production diff. A useful next measurement is the actual
packaged artifact on an otherwise idle macOS runner, with OS startup tracing
that identifies the pre-main delay; an independently tested signed/notarized
artifact remains unmeasured. A Linux measurement can establish its own result
but cannot close the macOS criterion. Until comparable distribution evidence
meets the unchanged external threshold, NFR003 stays open.
