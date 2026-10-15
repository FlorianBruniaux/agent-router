# Finite-client namespace ownership

This lot transfers the approved preparation's producer-owned home, startup pins, five private executable/profile copies and reserved broker into HostState. It closes the ownership gap before a dedicated execution backend; it does not enable production dispatch. **F12a remains NO_GO.** See [ADR 0035](../../adr/0035-finite-client-namespace-ownership.md).

## Checks and evidence

The inert regressions reproduce lost copies/listener ownership during a naive handoff and the generic sandbox's unwanted home-write grant. They cover successful prepared disposal, exact pending/namespace proof disposal, abandoned exposed ownership, group-only refusal, changed startup identities and namespace-mode refusal. Lease marker fixtures test control flow, not kernel isolation. Manifest-only approval cannot produce an owned namespace state.

The separate [Linux collector](run.py) exercises normal return, deadline, stdin EOF and helper death using the actual copied Codex 0.159.2 ARM64 musl executable, resource helper, retained bubblewrap 0.8.0 and [Landlock/seccomp fixture](isolate.c). The reviewed profile binds the namespace wrapper's hash and arguments. The driver observes actual detached writes before stopping, then stable heartbeat after inner namespace completion. An external observer independently checks stability before and after fallback container destruction.

Normal, deadline and EOF require the matching helper namespace-clean lease before disposing the home and all five copies, releasing the broker and disposing the diagnostic namespace wrapper. Helper death retains an active lease and every owner despite a stable heartbeat. It stays UNKNOWN until the outer container is destroyed; the diagnostic never synthesizes a namespace proof from that observation.

[Evidence](evidence.json) records exact source/input/image hashes and four case receipts. Source hashes describe the recorded build, not later HEADs. [Local verification](verification.json) records the initial 32 quality checks and completed workspace suites separately from Linux observations. An unchanged CPU termination test exceeded its ten-second wall-clock assertion once; isolated and complete limits-suite reruns pass, followed by all remaining packages and workspace doctests. Its cause remains UNKNOWN and the initial failure is retained. The collector is an explicit diagnostic, never an automatic native batch or runtime capability issuer.

## Observed result

| Case | Helper result | Exact lease | Bundle cleanup |
| --- | --- | --- | --- |
| Normal | 0 | namespace-clean | Complete, broker released |
| Deadline | 124 | namespace-clean | Complete, broker released |
| Stdin EOF | 125 | namespace-clean | Complete, broker released |
| Helper killed | Signal | active | UNKNOWN, owners and broker retained |

All four cases observe increasing heartbeat before stop and stable values after inner namespace termination. The independent observer confirms stability before outer fallback teardown. All task containers and heartbeat volumes are then removed by exact owned identity. These observations apply to this fixed canary and composition.

Earlier failures are retained in the evidence: two diagnostic-only compile references, the initial readonly errno expectation and a 60-second collector timeout. Correcting the witness and extending only the collector window did not change the 8 GiB bounds or the helper's 25-second workload deadline.

## Integrated main follow-up

While the lot was under verification, PR #177 merged into main. Integrate `446c648eee2288477a617a182725cae89cd08d0d`, reserve ADR 0035 and repeat the full local quality gate. Its result is recorded under `integration` in [local verification](verification.json). Product sources remain stable throughout that gate.

Rebuild the actual helper, copied client and diagnostic controller offline with the integrated sources, then repeat all four Linux cases. The `integrated_campaign` and `integrated_build` fields in [evidence](evidence.json) retain these newer binaries and the complete source-manifest binding separately from the initial campaign. Both campaigns are required by the receipt test. The same cleanup outcomes, zero provider turns and unchanged 8 GiB apply. F12a stays NO_GO.

## Reproduction and limits

Build the workspace offline with Rust 1.85.1 and the pinned lockfile. Build `router-sandbox-helper`, the `finite_client_supervision` example and router-runtime's unit-test executable in release mode. Compile this folder's `isolate.c` with `cc -O2 -Wall -Wextra -Werror`. Place the resulting files under names `helper`, `client`, `controller`, `wrapper` and the retained Debian bubblewrap binary under `namespace`. Supply the pinned native binary separately:

```sh
python3.11 run.py --binaries /absolute/private/bin --native /absolute/private/native --output-dir /absolute/private/new-evidence
```

The collector allows 120 seconds until the provisional report, including preparation and the owner cleanup performed before that report. Independent observation and fallback teardown then use separate bounded calls. The helper still bounds the workload itself to 25 seconds. The collector refuses image pulls and host mounts. It requires the recorded local ARM64 image. Nested unprivileged bubblewrap needs Docker's `seccomp=unconfined` and `systempaths=unconfined` settings; the outer container still drops all capabilities, has a read-only root, no external network, no-new-privileges, bounded PIDs and exactly 8589934592 bytes of memory with zero swap. The inner workload has its own Landlock/seccomp restrictions. These settings are specific diagnostic infrastructure, not a recommended production profile.

The helper installs an exact **8589934592-byte address-space limit**, unchanged. A small allocation succeeds and a 9 GiB reservation is refused. Seven protected assets resist mutation, other listener ports and socket families are refused, and the scoped file remains writable. The native RPC performs initialize and thread-start only: **zero provider/model turns**, no useful patch task and no savings claim.

The product still refuses ControllerBrokerOnly. Next: bind the dedicated backend to PendingRun/ApprovedRun, fresh authenticated OS evidence and durable admission before an approved useful simulated-provider task. Quarantined ownership is process-local, not crash recovery. Outside same-UID writers are not excluded by a workload sandbox.
