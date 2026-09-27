# Local performance observations

These measurements characterize bounded synthetic cases on one development
machine. They are not a blocking CI timing gate, a human quality evaluation or
proof of cold operating-system caches.

The functional specification sets NFR-001 to deterministic decision p95 below
100 ms, NFR-002 to retrieval p95 below 50 ms on 10,000 assets, and NFR-003 to CLI
startup below 150 ms. The decision and retrieval measurements below meet their
thresholds for the measured boundaries. Full startup acceptance is not claimed:
a recorded process launch took 503.939 ms, and the first invocation after an
explicit standard-profile CLI rebuild took 620.105 ms. Cold-cache behavior
remains unknown.

## Machine and conditions

- Measured 2026-09-27, initially 09:27 to 09:29 UTC.
- macOS 26.5.1, Darwin 25.5.0, arm64, 18 logical CPUs. No hostname, account name
  or machine serial is recorded. CPU model and memory capacity were unavailable
  through the sandboxed system-information command.
- Rust 1.85.1 and Cargo 1.85.1, release profile, default product features,
  `--locked --offline`; the optional semantic sidecar was not enabled.
- Benchmark children ran sequentially. Other development work and OS processes
  continued; no task was stopped and no system setting was changed. Initial
  1/5/15-minute load averages ranged from 15.44/22.17/28.66 to
  21.21/24.60/30.10. These are load averages, not CPU utilization percentages.
- Percentiles use nearest rank, `ceil(p * samples) - 1`. Outliers are retained.

## Decision and retrieval

The phase benchmark runs 50 warmups followed by 1,000 measured decisions per
case. Each measurement includes cloning the parsed request, constructing the
validated `PhaseRouter` context, hard constraints, scoring and receipt generation.
It excludes file I/O, JSON input parsing, host discovery, embeddings and models.
The two cases contain 1 or 64 candidates, two synthetic models and no registered
agents, tools or skills. The larger case exercises the maximum candidate count,
not every independently bounded metadata dimension at its maximum.

| Case | Run 1 p50 / p95 ms | Run 2 p50 / p95 ms | Run 3 p50 / p95 ms |
| --- | ---: | ---: | ---: |
| Decision, 1 candidate | 0.034 / 0.054 | 0.021 / 0.026 | 0.030 / 0.033 |
| Decision, 64 candidates | 0.362 / 0.476 | 0.216 / 0.256 | 0.318 / 0.383 |
| BM25, selective query | 0.071 / 0.155 | 0.043 / 0.065 | 0.062 / 0.104 |
| BM25, broad query | 8.930 / 9.961 | 5.836 / 6.212 | 8.087 / 9.691 |

Retrieval uses the existing synthetic capacity example: 10,000 skill metadata
entries, 5,167,856 serialized bytes, 100 selective queries and 100 broad queries
per run. Selective queries must choose the expected synthetic ID; broad identical
descriptions must abstain. No body is read. Up to 80,000 postings were visited.
There are no human relevance labels, so these observations establish no recall@5.

Index parsing and construction took 120.369, 83.938 and 113.548 ms respectively.
That work is excluded from the query timings. The current standalone CLI search
reconstructs its index per invocation; the hot-index numbers therefore do not
prove a CLI search round trip below 50 ms.

## New-process startup

Each command is launched in a new process 100 times. The timer covers spawn
through exit, with stdout/stderr captured and no shell. It includes Python
launcher, scheduler and pipe overhead. It does not isolate Rust initialization
from OS executable validation or scheduling.

| Run / command | First recorded ms | p50 ms | p95 ms | Maximum ms |
| --- | ---: | ---: | ---: | ---: |
| 1 / `--version` | **503.939** | 5.856 | 7.803 | **503.939** |
| 1 / `--help` | 6.232 | 5.414 | 6.461 | 10.652 |
| 2 / `--version` | 7.981 | 2.887 | 4.845 | 7.981 |
| 2 / `--help` | 6.070 | 3.163 | 4.658 | 11.777 |
| 3 / `--version` | 10.851 | 6.735 | 9.683 | 15.199 |
| 3 / `--help` | 4.899 | 5.141 | 5.899 | 10.345 |

Run 1 followed an incremental release build. An earlier measurement attempt had
already launched that binary but failed while collecting RSS with `/usr/bin/time`
because its `sysctl kern.clockrate` call was denied. Its startup samples were not
retained and are not reconstructed here. Thus 503.939 ms is the first recorded
sample of the complete run, not proof of the first invocation after compilation.
Runs 2 and 3 reused the same binary without rebuilding.

The process itself starts fresh each time. The filesystem, code-signing, dynamic
loader and CPU caches are not flushed, and no reboot is part of this method.
Cold OS cache performance is **UNKNOWN**. The outlier's cause is also unknown;
the faster subsequent samples do not erase it or establish a compiler effect.

## Fresh compilation and optional profile experiment

An additional experiment used only environment overrides
`CARGO_PROFILE_RELEASE_STRIP=symbols` and `CARGO_PROFILE_RELEASE_LTO=thin` in a
second private target directory. No workspace profile was changed. Afterward,
the earlier standard CLI binary was preserved, only that package's private Cargo
build artifacts were cleaned, and the standard CLI was recompiled with its
dependency cache. Neither newly compiled CLI was invoked before its first timed
`--version` measurement. Each distribution contains all 100 samples, including
the first invocation and outliers.

| Fresh binary / command | First ms | p50 ms | p95 ms | Maximum ms |
| --- | ---: | ---: | ---: | ---: |
| ThinLTO + stripped / `--version` | **524.315** | 4.466 | 5.868 | **524.315** |
| ThinLTO + stripped / `--help` | 4.103 | 4.454 | 5.368 | 6.214 |
| Standard rebuilt / `--version` | **620.105** | 6.568 | 8.063 | **620.105** |
| Standard rebuilt / `--help` | 6.224 | 6.223 | 7.760 | 18.356 |

ThinLTO was measured at 09:31 UTC and the rebuilt standard profile at 09:33 UTC.
The initial 1/5/15-minute load averages were 26.80/24.00/28.83 and
25.56/29.08/30.44, respectively. Source changed during the ThinLTO build, and the
rebuilt standard CLI used a later source fingerprint. This is not a controlled
same-source compiler comparison. Both fresh binaries missed the 150 ms first
launch target, so the experiment establishes no solution for NFR-003 and
justifies no production profile change. Cache and OS validation effects remain
uncontrolled.

The additional standard run measured decision p95 of 0.042/0.452 ms for 1/64
candidates, selective/broad retrieval p95 of 0.080/10.789 ms, and index construction
of 141.170 ms. Its process peaks were 3.14 MiB for phase routing, 83.48 MiB for
retrieval and 2.81 MiB for a separate CLI invocation. ThinLTO observed
0.031/0.318 ms decision p95, 0.054/7.024 ms retrieval p95 and 86.268 ms index
construction, with process peaks of 3.16/81.50/2.77 MiB. These observations retain
the same boundary and concurrency qualifications.

The standard rebuilt CLI digest is
`936799ef4eaff12ac5b63416b98120470bfa9ddfbeb6232d54e7a34d2e161617`
(10,390,224 bytes); its source fingerprint remained
`8dc45d8023fcdc2c9369cf36cb45c2b2516acd21a8e6ac7a061955e55f8c4239`
over 140 files during the run. The experimental CLI digest is
`b047b5557d8e34d9d904f94818ad233c847dc8a523c5ca4b6ee1f804e22ae57c`
(8,663,344 bytes). The ThinLTO source fingerprint changed from
`3d0aa1dcb9c257011c6f4cf09b7d1bc01dea311c6c274c0ec47d3cf5fa47d930`
to `cf44769744aa7ce919dff298bf2f711d30baf3dff2cbcc8d330c991c89f27b73`.

## Observed memory

`wait4` records each child's `ru_maxrss`, normalized from Darwin bytes or Linux
KiB into bytes. This is a per-process high-water resident set, not a memory cap,
unique physical footprint or aggregate footprint of the development machine.
The retrieval process peak includes metadata construction, JSON parsing,
indexing and queries; it is not a steady-state index-only allocation.

| Process | Run 1 MiB | Run 2 MiB | Run 3 MiB |
| --- | ---: | ---: | ---: |
| Phase benchmark | 3.14 | 3.06 | 3.13 |
| Retrieval, 10,000 assets | 83.47 | 82.30 | 82.81 |
| CLI `--version`, separate RSS invocation | 2.80 | 2.81 | 2.81 |

## Reproduction and identity

Run from the repository root:

```sh
python3 scripts/benchmark-product.py \
  --target-dir /private/tmp/agent-router-performance-target \
  --output /private/tmp/agent-router-performance-report.json
```

The script builds only the required release targets offline in a separate target
directory. `--skip-build` reuses existing binaries. It emits machine conditions,
sample counts, exact timing boundaries, source fingerprints, binary SHA-256
digests and RSS observations. Source changes during the run are reported.
It does not fail merely because a timing target was missed.

The runner also retains individual startup samples and lists samples at or above
150 ms. Those fields were added after the five summarized runs; their percentile,
first and maximum values were retained, but their individual arrays were not.
No missing historical sample values or outlier counts are inferred.

The first three complete runs used unchanged source fingerprints during each
run. Their common source fingerprint was
`9b17daf295e49d02e451915ed28c39bdf35775a8baca7c1531119293c944b726`
over 139 source/manifest/example-fixture files. Their binary digests were:

- CLI: `50c081cff042558c639251ce890decc4c1883b5414023bb82fd48890cf9746d2`.
- Phase benchmark: `7bdbd183062dde101dab34c0d52a83986c62a96b01cebd176a1f18ac95c86c16`.
- Retrieval benchmark: `1d813d49c29a6fa994cf4cdd5d2e010933d4c4f28eb8e4bd8ad12133736b5da1`.

The retrieval metadata hash was
`9060b8136b60869e59f00a3761860b8f237c322727fb7e8e9b2882f625930c8e`.
Rebuilding different source, dependencies, compiler settings or features requires
new measurements. These local results do not establish Linux performance.
