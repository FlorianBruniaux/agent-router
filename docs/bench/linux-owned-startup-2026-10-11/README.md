# Producer-owned Linux startup, 11 October 2026

This lot removes the installation-ID bootstrap from the Linux diagnostic controller and makes startup part of the private production preparation contract. It keeps exactly 8589934592 bytes for worker RLIMIT_AS and the container memory ceiling, with no additional swap. **F12a remains NO_GO.** Preparation and component observations do not grant product execution or provider admission.

## Owned startup contract

The existing producer creates a fresh installation ID under private tmp and a retained exact relative alias in the protected home. Both identities and the ID bytes join the owned-copy approval digest before any diagnostic execution. CODEX_SQLITE_HOME points at private tmp. [ADR 0032](../../adr/0032-finite-client-owned-startup.md) records the mode-0644 slot under a mode-0700 ancestor, retained symlink descriptors, corruption/substitution refusal and owned proof v4. Manifest-only v2 remains compatible; old owned v2/v3 proofs cannot qualify this composition. The serialization-only macOS preview gains the same derived environment, but native behavior here is qualified only under Linux.

The diagnostic client receives exactly the producer's five-key environment in hashed stdin, checks it before effects and refuses missing, redirected or extra variables. The controller no longer initializes the installation slot after review. It checks state and copied identities before and after the worker, and retains owners until external namespace teardown.

## Observations and reproduction

The [new evidence](evidence.json) records source/build inputs and the four-case campaign, independently of [PR171's historical campaign](../linux-copied-client-2026-10-11/README.md). The old captures and source hashes remain frozen to its tested head 8299c3c148ac203b584bef33e7f98946aaf109f3. Its receipt tests compare frozen product hashes to final build inputs and still hash retained fixture bytes, rather than expecting current product files to equal historical sources. This is consistency checking of recorded provenance, not a new build reproduction of PR171.

The existing bounded [collector](../linux-copied-client-2026-10-11/run.py), [inside driver](../linux-copied-client-2026-10-11/inside.py) and [Landlock/seccomp fixture](../linux-copied-client-2026-10-11/isolate.c) stay unchanged. The new wrapper adds producer-startup receipt requirements. Protected-copy writes, unlink/rename and metadata mutation are refused, broker TCP reaches only the allowed loopback port, a detached descendant remains confined and a 9 GiB allocation is refused while a small allocation succeeds. Native initialize/thread-start complete without submitting a model turn. Normal, deadline, EOF and helper death require externally confirmed namespace destruction and independently stable heartbeat after teardown; helper death alone is never a cleanup success.

```sh
python3.11 -B docs/bench/linux-owned-startup-2026-10-11/run.py \
  --binaries /private/tmp/ar-f12a-linux-startup-20261011/binaries \
  --native /private/tmp/ar-linux-client-20261011/codex-linux \
  --output-dir /private/tmp/ar-linux-owned-startup-replay-unique
python3.11 -B scripts/test-linux-owned-startup-evidence.py
```

Use a fresh output path and the recorded already-built ARM64 binaries. The wrapper uses the exact already-present image, no pulls, no host-directory mounts and no network. It inherits the per-case deadline, bounded captures, immutable container identity/labels and independent read-only heartbeat observers. Raw private logs, archives, vendored packages and binaries are retained outside Git. Supplied binaries do not prove their own source; the frozen Git base plus hashed overlays and offline build receipts record the composition actually tested. Reproducible builds and remote image provenance remain unqualified.

[verification.json](verification.json) separates local macOS gates, Linux contract/client/library tests and the actual diagnostic campaign. The initial controller input required a whitespace-only formatting correction; both build inputs are retained and final qualification uses the formatted controller. The pinned image has no usable Linux Clippy component; no installation was performed. MacOS warnings-denied Clippy and Linux execution tests are distinct evidence.

All four cases passed at the fixed bound. Each namespace exited 137 after destruction by its external owner; container and volume absence were verified. Linux release checks passed 37 preparation tests, 10 client tests and 160 library tests, with 3 native tests ignored.

| Case | Helper exit / lease | Heartbeat before teardown | Stable after teardown |
| --- | --- | --- | --- |
| normal | 0 / `group-clean` | [215, 262] | [274, 274, 274] |
| deadline | 124 / `group-clean` | [2019, 2052] | [2069, 2069, 2069] |
| eof | 125 / `group-clean` | [322, 365] | [384, 384, 384] |
| helper-kill | None / `active` | [428, 521] | [548, 548, 548] |

## Remaining gate

This lot formalizes startup ownership. The next runtime lot must bind the Linux namespace owner, retained cleanup lease and immutable policy/assets to durable PendingRun/ApprovedRun dispatch and fresh authenticated OS evidence. A receipt from this external diagnostic owner cannot upgrade a product lease or grant ControllerBrokerOnly. Product broker-only execution remains refused. Exercise a bounded simulated-provider task after these gates, then request the real task and ceiling approval under [#132](https://github.com/FlorianBruniaux/agent-router/issues/132) and [#133](https://github.com/FlorianBruniaux/agent-router/issues/133). No useful native correction, provider reception or savings are established here.
