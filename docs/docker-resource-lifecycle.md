# Docker resource lifecycle for qualification runs

`scripts/docker_lifecycle.py` gives Linux qualification launchers one ownership, teardown, reconciliation and cache-retention contract for Docker containers and volumes (issue #212). It is development tooling. It grants no product authority, calls no model or provider, and uses the Python standard library only (POSIX, `fcntl`).

It never runs a global prune. Every removal names one container ID or one volume name that was re-inspected immediately before. Resources without the labels below are only reported.

## What this change does not do

- It does not delete the historical inventory (about 64 stopped containers and three large volumes dated 2026-10-26). Those resources predate the labels, are retained as unlabelled, and any migration or removal needs its own decision.
- It does not change the existing launchers under `docs/bench/`, whose files are hash-bound evidence. The section "Adopting the module" shows how a launcher opts in.
- It does not install a scheduled cleanup service.

## Metadata

Every managed container and volume carries these labels (`Run.label_args()` emits them):

| Label | Value |
| --- | --- |
| `org.agent-router.lifecycle.project` | project namespace, for example `ar-qualification` |
| `org.agent-router.lifecycle.run` | owning run id (`[A-Za-z0-9][A-Za-z0-9_.-]*`) |
| `org.agent-router.lifecycle.class` | `ephemeral`, `shared-cache` or `persistent` |
| `org.agent-router.lifecycle.schema` | `1` |

Classes decide what may happen:

| Class | Per-run teardown | Reconcile after a crash | Cache eviction |
| --- | --- | --- | --- |
| `ephemeral` | removed by its owner, evidence exported first | removed once the owner is provably dead | no |
| `shared-cache` | never | never | only when unused and over budget |
| `persistent` | never | never | never |

Removal also requires all four labels (non-empty project, run, class, `schema` equal to `1`). A missing, unknown or contradictory class, an unsafe run id, a foreign project label or a missing lease record all mean "ambiguous": the resource is retained with a reason.

## Leases and the check-then-remove race

Liveness comes from `flock`, held by the process that owns the resource. The OS drops the lock on any exit including `SIGKILL`, so a free lease is proof that the owner is gone, with no pid reuse or heartbeat file to trust. The state directory defaults to `~/.local/state/agent-router/docker-lifecycle` (`--state-dir` or `AGENT_ROUTER_DOCKER_STATE`) and must be on a local filesystem.

- `runs/<run>.lock`: held exclusively by a live run. Reconciliation takes the lock without blocking and keeps it until its removals finish, so the owner cannot reappear mid-cleanup. A busy lock means "live owner"; a missing file means "no lease record" and the resource is retained.
- `caches/<name>.gate`: each consumer of a shared cache holds it shared (`Run.acquire_cache`). The evictor takes it exclusively and non-blocking, holds it across the final identity check and the removal, and a consumer that arrives meanwhile gets `CacheBusy`. The gate file mtime records last use and drives least-recently-used order.

Probe effects: `preview` and `reconcile` without `--apply` take only shared, non-blocking probes on run leases, so they cannot make a launcher fail. A cache preview needs a brief exclusive, non-blocking probe on the gate to detect shared holders. A consumer arriving in that instant retries for up to 1 second (`Run.acquire_cache(wait=...)`) before `CacheBusy`.

Lease files are opened with `O_NOFOLLOW`, must belong to the current user, and live in directories created `0700` that are refused when owned by someone else or group/world writable. `flock` is not reliable on network filesystems: keep the state directory on a local disk (not checked by the code).

Remaining race window, stated plainly: a process that does not use this module and has not yet created a container referencing the resource can still acquire it between the final inspect and the remove call. Docker itself refuses to remove a volume that any container (running or stopped) references, which closes the case of an existing container. Lease files are never unlinked, because unlinking a lock file reopens a race.

## Commands

All commands need `--project`. They print one JSON receipt; the exit code is 0 when no removal failed, 1 when one did, 2 when Docker is unreachable or arguments are invalid.

```sh
python3 scripts/docker_lifecycle.py preview --project ar-qualification
python3 scripts/docker_lifecycle.py reconcile --project ar-qualification --apply
python3 scripts/docker_lifecycle.py evict-cache --project ar-qualification --budget 20GB --min-idle-seconds 3600
python3 scripts/docker_lifecycle.py evict-cache --project ar-qualification --budget 20GB --apply
```

`preview` and any command without `--apply` change nothing in Docker and write no receipt. `reconcile --run <id>` limits the pass to one run.

## Receipt

With `--apply` and for every teardown, an intent receipt (`status: started`, planned entries) is written before the first removal and completed in place afterwards (`status: complete`), so a crash mid-cleanup leaves a trace. A JSON receipt is written under `<state>/receipts/` (`schema: agent-router.docker-lifecycle.receipt/1`). Entries list `kind`, `name`, `id`, `class`, `run`, `outcome` (`removed`, `would-remove`, `retained`, `failed`, `absent`), `reason` and `bytes`. The receipt also carries:

- `disk`: host free bytes before and after, Docker volume bytes before and after, bytes reported removed. Docker sizes are rounded allocations, not host space recovered.
- `disk` sizes come from `docker system df -v --format json` (`Volumes[].Size`, a rounded string such as `12.56GB`), observed on Docker 29.5.3. If a Docker version prints something else, sizes are unknown, nothing is evicted for lack of a measure and `budget.met` is `UNKNOWN`.
- `failures` and `ok`: a failed removal stays in the Docker inventory and in the receipt, and a later reconcile retries it. Removing an already absent resource is `absent`, not an error.
- `budget` (cache eviction): limit, projected total, active caches that block the target and `met` (`true`, `false` or `UNKNOWN` when a cache size could not be measured).
- `race_note`.

## Adopting the module

A launcher imports the module and wraps its run:

```python
import sys; sys.path.insert(0, "scripts")
import docker_lifecycle as dl

with dl.Run("ar-qualification") as run:
    run.add_evidence_export(copy_logs_and_results_outside_docker)
    run.acquire_cache("ar-build-cache")           # shared lease, optional
    docker("volume", "create", *run.label_args(), run.name("work"))
    docker("create", *run.label_args(), "--name", run.name("c1"), ...)
    ...
```

On success, test failure and `SIGTERM`/`SIGHUP` (converted to `KeyboardInterrupt`), exit runs the evidence exports first and then removes the run's own ephemeral containers and volumes and verifies they are gone. A failed export keeps every resource and raises. `SIGTERM` and `SIGHUP` are ignored while the teardown itself runs. A cache volume is created with `run.label_args(dl.SHARED_CACHE)`; persistent data with `run.label_args(dl.PERSISTENT)`.

How the current launchers differ (read in this change, not modified): `docs/bench/linux-namespace-owner-2026-10-15/run.py` labels with `agent-router.diagnostic=<nonce>` and removes its own container and volume in `finally`; `docs/bench/linux-finite-tools-guard-2026-10-16/reproduce.py` labels with `agent-router.task=<label>`, checks container identity, force-removes the container and keeps a pre-existing build volume. Neither uses lifecycle labels or leases, so their leftovers are reported as unlabelled and retained.

## Validation

`python3 -B scripts/test-docker-lifecycle.py` runs the unit tests against an in-memory Docker (no daemon needed) and the integration tests against a real daemon when one is reachable. The integration tests label everything with a fresh session id, use a locally present image (they never pull) and skip otherwise.

Not verified here: that `docker volume rm` refuses a volume referenced by a stopped container (documented Docker behavior, not exercised), the `SIGHUP` path (only `SIGTERM` is tested), behavior against a real daemon in sandboxed sessions where the Docker socket is blocked for child processes, concurrent access from non-module launchers, and Windows (no `fcntl`).
