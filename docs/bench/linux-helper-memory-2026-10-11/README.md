# Real helper in Linux isolation, 11 October 2026

**The resource component passes at exactly 8 GiB. F12a remains NO_GO.** The actual
`router-sandbox-helper`, built offline from merged main
`7da1a4725f9639f1dc14c51679bfc90284d04ba4`, installs soft and hard `RLIMIT_AS`
values of `8589934592` bytes in a Linux worker. The worker starts, executes,
forks and execs with that bound. Oversized allocations are refused. Four
supervision cases complete with observed process-group cleanup.

This follows the [Python-only Linux candidate](../macos-strict-memory-startup-2026-10-11/README.md#bounded-linux-candidate).
Here the fixture controller starts with unlimited address space and never
installs the worker limit itself. The real helper owns that operation. Product
Rust sources, lockfile, requested limit and native admission are unchanged.
No model or provider call is made.

## Observed behavior

[evidence.json](evidence.json) binds the exact source, build input, binary,
fixture, image identities, output and container-removal receipts. The successful
binary SHA-256 is
`d1cc1e8658059c6a5040bdd27d9cc8e37a49fc4923a0243ae070c1bcc96a80d4`.

| Scenario | Observed result |
| --- | --- |
| Real helper probe | Exit 0, `address-space=enforced` |
| Direct worker, fork and exec | Every soft/hard bound is 8 GiB; a 16 MiB no-access mapping and a touched 4 MiB allocation succeed |
| Direct and supervised memory workloads | 9 GiB mapping refused with errno 12; 9 GiB bytearray refused with `MemoryError`; raising or removing the hard limit refused |
| Normal supervision | Exit 0, inherited memory checks pass, `group-clean` lease |
| 250 ms deadline | Exit 124, worker and its same-group descendant disappear, `group-clean` lease |
| Parent pipe EOF | Exit 125, same-group cleanup observed |
| Extra parent protocol byte | Exit 125, same-group cleanup observed |
| Controller before/after | Unlimited `RLIMIT_AS` both times; causal control preserved |
| Container after run | Confirmed absent by a successful exact-name listing |

The cleanup supervisor is outside worker resource limits by the
[existing runtime design](../../runtime.md#processes-and-scheduling). Its
observed address-space values remain unlimited. The whole container retains
an 8 GiB cgroup memory ceiling and zero additional swap. `RLIMIT_AS` bounds
virtual address space per worker process; the cgroup setting is a separate
aggregate physical-memory control. Its settings are read back, but a physical
OOM event is **not tested**.

The fixture controller is namespace PID 1 and reaps its adopted, known fixture
child during helper cleanup. No worker-provided PID is used as a signal target.
The helper retains its own cleanup ownership. The receipt is `group-clean`,
not `namespace-clean`; this does not qualify detached descendants or prove
exhaustive namespace cleanup.

## Isolation and build provenance

Both images were already present locally and were selected by content-addressed
ID, with `--pull=never`. Their remote registry origin remains UNKNOWN. The build
uses ARM64 `rust:1.85.1-bookworm`, ID
`sha256:e51d0265072d2d9d5d320f6a44dde6b9ef13653b035098febd68cce8fa7c0bc4`.
The runtime uses ARM64 `python:3.11-slim-bookworm`, ID
`sha256:528257d48c1da0dcecc2e725d1ae34498d60c965f1241e39cd6a85a8859bdf84`.
The observed runtime kernel is `6.12.76-linuxkit` on `aarch64`.

The source input is a Git archive of the pinned base's `Cargo.toml`, `Cargo.lock`,
`crates/`, `integrations/` and `xtask/`. Dependencies are vendored with Cargo's
locked, offline mode using only the existing public package registry cache.
A private source-local Cargo configuration redirects to `/work/vendor`.
No user Cargo configuration, credentials or home directory is copied. Source
and lockfile hashes are checked against both the frozen archive and the pinned
Git objects. Vendoring is preparation outside Docker; the build is offline
inside Docker. The receipt does not claim independent dependency auditing or
reproducible binaries across multiple builds.

The build and runtime have no network or host mounts, a read-only root,
UID/GID 65534, dropped capabilities and no-new-privileges. Both keep
`--memory=8589934592 --memory-swap=8589934592`. The build also has
`ulimit -v 8388608` (KiB), two CPUs, 256 PIDs and a 1 GiB private tmpfs.
It needs an executable tmpfs for compiler build scripts, with `TMPDIR=/work/tmp`;
only that disposable internal filesystem is executable and writable. The runtime
has one CPU, 32 PIDs and a 64 MiB executable private tmpfs. It receives only the
real helper and the fixed [inside.py](inside.py) via an input archive.

Build input is 47,850,188 compressed bytes. The build completed with Rust
1.85.1 and `cargo build --offline --locked -p router-sandbox-helper --release`.
Its full argv, compiler output and hashes are retained in the evidence.
The private build artifacts remain under `/private/tmp/ar-linux-helper-20261011`;
binary, vendor directory and archives are not committed.

## Replay and local tests

[run.py](run.py) replays the fixed runtime scenarios with an already-built real
Linux ARM64 helper. It refuses missing images, creates a new private output
directory, never pulls, never mounts the host and never accepts a configurable
memory bound. Input is capped by file size; capture is monitored at 64 KiB per
stream and 45 seconds, followed by bounded removal of only the generated
container. The collector preserves unsuccessful receipts before rejecting them.
The monitored output budget can overshoot between polls; it is a diagnostic
capture guard, not a native output-limit qualification.

With the existing local binary and pinned runtime image:

```sh
python3.11 -B docs/bench/linux-helper-memory-2026-10-11/run.py \
  --helper /private/tmp/ar-linux-helper-20261011/router-sandbox-helper-linux \
  --output-dir /private/tmp/ar-linux-helper-replay-unique
python3.11 -B scripts/test-linux-helper-evidence.py
```

Use a fresh output path for every attempt. A new binary requires rebuilding the
pinned sources and locked offline vendor input with the build command recorded
in `evidence.json`; runtime replay alone does not prove its source provenance.
The offline suite rejects altered limits, missing inheritance/refusal evidence,
wrong exits, unobserved cleanup, identity mismatch and promotion to native GO.
It checks the fixture hash against the actual successful capture. It is also
included automatically in the repository's local quality group.

[verification.json](verification.json) records local gates separately from the
real-helper run. Remote CI cannot run because of the account's credit limit;
local gates are not described as executed remote CI.

## Retained unsuccessful attempts

The evidence preserves two failed builds: read-only `/tmp`, then the default
noexec tmpfs. Moving compiler temporaries and allowing execution only in the
private build tmpfs resolves those preparation errors without changing 8 GiB.
It also preserves two fixture failures: a little-endian frame where the helper
expects big endian, then an output-oracle failure while namespace PID 1 waited
for the supervisor before reaping its adopted child. The original helper stderr
was not retained in that second failure; its exact error label is UNKNOWN.
Source analysis suggests the unreaped child prevented group cleanup. The
passing fixture uses the actual framed protocol and reaps during cleanup.
No product source is changed.

## Next acceptance boundary

This is a positive Linux resource-component result, not an implemented Linux
host adapter or native Codex launch. The next lot must define and test the owned
client/helper composition in this isolation: copied-asset freshness, read-only
protected assets, broker-only exact endpoint access, blocked other network and
host access, detached-child containment, interruption and cleanup receipts.
Only then can the pending/approved native lifecycle be tested. Authenticated
capability evidence, exact approval digest and provider admission remain
independent gates under [#132](https://github.com/FlorianBruniaux/agent-router/issues/132)
and [#133](https://github.com/FlorianBruniaux/agent-router/issues/133).
