---
adr: "0031"
title: Separate macOS physical-memory observation from hard limits
status: proposed
created: 2026-10-11
last_modified: 2026-10-11
scope: Experimental direct-child observation; no production admission or default change.
amended_by: []
supersedes: []
superseded_by: []
related: ["0021", "0029"]
---

# ADR 0031: Separate macOS physical-memory observation from hard limits

## Context

The [minimal memory diagnostic](../bench/macos-memory-followup-2026-10-11/README.md)
reproduces refusal of the requested 8 GiB virtual-address-space limit in both a
C fixture and the product helper. PR #167 is merged at
`f389059ab637a8338353ae85ed37e3b93a326101`; its tree matches the reviewed source.
This is not resident-memory exhaustion: that fixture had about 415 GiB virtual
space and 1.4 MiB resident memory.

The current `ResourceLimits` contract requests address space.
`ResourceSupervisor::memory_enforcement` reports the existing contract;
a new observer must not silently map into `MemoryEnforcement::Enforced`.
The finite-client review binds exact resource arguments and copied executable
identities. Changing an enforcement mechanism therefore changes the reviewed
contract, not only an implementation detail.

## Proposed decision

Keep the current default and its macOS refusal. Evaluate two mechanisms in
isolated, bounded fixtures before exposing a new production mode:

1. A self-only call to the public Mach `task_set_phys_footprint_limit` RPC.
   Record the exact Mach result. Success means installation only until behavior,
   fatality, inheritance and attribution are proven; failure is not retried
   with extra privileges or a larger limit.
2. External `proc_pid_rusage` observation of the parent's own direct child,
   using `ri_phys_footprint`. When a valid sample reaches the explicit threshold,
   request termination through the owner of that child, then record the wait
   result. This is sampled detection and response, not a hard allocation cap.

Neither experiment replaces `address_space_bytes`, changes an approval digest,
adds a production CLI option or satisfies F12a. There is no automatic fallback
from a refused hard requirement to observed memory.

For an eventual observer integration, the contract must retain these distinctions:

| Property | Required meaning |
| --- | --- |
| Metric | Physical footprint in bytes; no automatic RSS or virtual-size substitution |
| Scope | The one retained direct child; not all descendants or aggregate job memory |
| Threshold | Explicit absolute value in the approved request; relative baseline is fixture-only |
| Sampling | Requested interval plus observed monotonic timestamps and sample freshness |
| Exceeded threshold | A valid sample triggers an owner-mediated stop request |
| Missing, stale or mismatched sample | UNKNOWN and stop request; never zero or normal continuation |
| Identity | Owned PID retained until the last possible signal, plus checked process start identity |
| Cleanup | Actual signal and wait results; incomplete cleanup remains explicit |
| Peak and overshoot | Observed values only; no bound on unobserved allocation between samples |
| Hard-memory requirement | An observer receipt cannot satisfy it |

Only the lifecycle owner may signal its retained child. A monitor never signals
a PID supplied in an observation. If a child is reaped, its token is invalidated
before any later signal. The existing watchdog's retained leader and process
group help cleanup, but do not prove memory coverage or containment of detached
descendants. An observer failure must not terminate the cleanup owner first.

## Platform evidence

The local SDK exposes `proc_pid_rusage` and the Mach setter. Its `RLIMIT_RSS`
is an alias of `RLIMIT_AS`, so switching that name does not provide another
memory mechanism. `proc_pid_rusage` returns success/error, not a byte count;
there is no genuine short-result-length case to invent.

Apple's pinned [task implementation](https://github.com/apple-oss-distributions/xnu/blob/f6217f891ac0bb64f3d375211650a4c1ff8ca1ea/osfmk/kern/task.c)
checks footprint privileges, can return `KERN_NO_ACCESS`, and preserves the
process's existing fatal/nonfatal policy. An alternative build branch returns
`KERN_FAILURE`. The [memory-status implementation](https://github.com/apple-oss-distributions/xnu/blob/f6217f891ac0bb64f3d375211650a4c1ff8ca1ea/bsd/kern/kern_memorystatus.c)
requires privileged authority for setting memory-limit properties. The
[process information implementation](https://github.com/apple-oss-distributions/xnu/blob/f6217f891ac0bb64f3d375211650a4c1ff8ca1ea/bsd/kern/proc_info.c)
checks access before reporting usage. These sources describe candidate behavior;
matching that source revision to the installed kernel remains UNKNOWN.

No privilege, entitlement, global configuration or other process's task port is
requested. A successful setter or an unexplained SIGKILL alone is not proof of
a hard limit. A user-callable getter absent from the inspected SDK must not be
invented from a kernel-internal function with a similar name.

## Validation and remaining integration

The [bounded experiment](../bench/macos-memory-protection-2026-10-11/README.md)
retains the fixture, actual results and limitations. Its positive under-threshold
and crossing cases use real memory readings; query-error and identity mismatch
cases are explicitly injected. Timeouts and cleanup are observed separately.

Before any production observer can be admitted, implement and independently
review these units in order:

1. A typed measurement and decision component with explicit missing/stale/error
   behavior and no authority to send signals.
2. An opt-in helper supervision protocol that preserves existing invocations,
   observes only retained identities, and handles timeout, parent EOF and
   cancellation through the existing cleanup owner.
3. A trusted receipt channel separate from worker-controlled output. Include the
   actual stop reason, observation scope, sample times and confirmed cleanup.
4. Approval/configuration binding and replay-compatible evidence that explicitly
   distinguish monitoring from a hard requirement. Existing hard-required inputs
   must continue to refuse the observer.
5. Exact copied-composition tests across exec, child creation, parent death,
   permission failure, stale sampling and cleanup. Broader native network,
   filesystem, lifecycle and physical-send gates remain separate.

The user must accept a changed production guarantee before enabling it. This
proposal and a passing fixture do not grant launch authority. If a hard job-wide
limit remains required, retain macOS NO_GO while qualifying another isolation
mechanism; a sampled root-process guard is insufficient.

## Alternatives not adopted

- Raising or dropping the 8 GiB default would alter the existing guarantee and
  still would not establish a physical-memory budget.
- Renaming `RLIMIT_AS` to `RLIMIT_RSS` changes no mechanism in the inspected SDK.
- A privileged memory-status service or a VM introduces a different deployment
  and authority model. Neither is installed or qualified by this experiment.
- Enumerating all user processes cannot establish ownership or a complete job
  boundary and is outside this direct-child experiment.
