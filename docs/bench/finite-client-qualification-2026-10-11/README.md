# F12a owned resource diagnostic, 11 October 2026

F12a remains **NO_GO** for the macOS owned finite-client composition. The copied
helper refuses the unchanged 8 GiB address-space requirement. Native launch,
model turns and provider sends are not attempted. This is the lot-3 resource
stop, not completed OS qualification or useful-task execution.

## Reproduction

Supply the explicit trusted helper and the previously pinned Codex native.
The native is read and copied, never invoked. No personal configuration,
credentials, global permission or authenticated host proof is used.

```sh
cargo build -p router-sandbox-helper --locked
AR_SANDBOX_HELPER=/absolute/router-sandbox-helper \
AR_CODEX_NATIVE=/absolute/pinned/codex \
cargo run -p router-runtime --example finite_client_qualification --locked
```

The example creates an owner-private temporary workspace, producer-owned Codex
state and all five retained copies with the existing library. It reviews the
exact resource arguments, then invokes only the helper copy's bounded probe.
It uses a preparation-only approval, never `Runtime::approve`. Original/copy
pins, identities, private state and held broker are checked before and after.
The binding is run-specific and cannot admit a later run. Successful diagnostic
completion can print `NO_GO`; inspect the verdict instead of treating exit zero
as qualification. An unexpected probe error projects UNKNOWN, never GO.

## Memory diagnosis

The direct helper refused 8, 16, 64 and 128 GiB. Separate diagnostic probes at
512 and 1024 GiB returned the applied-rlimit banner; the control with no requested
memory limit also passed. None of these alternatives changes the reviewed
8 GiB requirement or proves allocation enforcement.

A separate supervisor-wait control observed VSZ 410060880 KiB, about 391 GiB,
before any worker spawn. This measures the waiting supervisor map, not the
failed probe's map or native RSS. Apple's current XNU implementation rejects a
lowered address-space limit when the current map exceeds it:
[RLIMIT_AS in kern_resource.c](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/kern/kern_resource.c).
This is a plausible explanation for the observed refusals, not a verified
match between this machine's kernel build and that moving source revision.
The result supports a composition-specific refusal, not an OS-wide assertion
that every macOS address-space limit is unsupported.

## Qualification requirements

| Requirement | Current verdict | Reason |
| --- | --- | --- |
| Exact 8 GiB memory request | NO_GO | Copied helper refuses the reviewed limit |
| Exact broker port and other-port refusal | NOT_ATTEMPTED | Native composition stops at resource preflight |
| Asset/configuration writes, unlink and rename | NOT_ATTEMPTED | Mode/hash checks are preparation checks |
| Copied system-wrapper compatibility | NOT_ATTEMPTED | Wrapper copied but never executed |
| Mach/DNS channels | NOT_ATTEMPTED | No fresh selected-profile test |
| Task descendant containment | NOT_ATTEMPTED | NPROC remains per user, not per task |
| Physical provider send admission | NOT_ATTEMPTED | No provider send owner composed |

The product `ControllerBrokerOnly` refusal stays unchanged. No historical TCP
canary or synthetic proof is promoted to qualification. Before lots 4/5, select
and approve a memory mechanism with a meaningful bound, then test real
allocation refusal, exact-port access, protected assets, Mach/DNS and escaped
descendants on the same owned composition. Raising the virtual-space ceiling or
sampling RSS is not evidence of enforcing the present requirement.

## Verification

The contract tests use synthetic trusted helper scripts to observe copied-path
execution, exact arguments and the absence of helper effects on refusal. They
cover manifest-only reviews, limit changes, source/copy drift, protocol refusal
and changes during a successful probe. These fixtures do not qualify the OS.
The example's projection test prevents a successful rlimit probe from producing
an F12a GO verdict.

[Observed diagnostic](diagnostic.json) and [source-bound verification](verification.json)
retain the live result and local gate separately. Local checks do not establish
GitHub Actions execution, Linux qualification, savings or an independent review.

The [independent component campaign](../macos-component-qualification-2026-10-11/README.md)
adds 47 main-source component results and a separately instrumented helper
refusal receipt. Its errno/VSZ observations are diagnostic-copy measurements;
they do not replace the unmodified copied-helper result or qualify the OS.

The first full local run retained one unrelated M3 suite failure: its historical
T1 intentionally expected the report-cost bug already fixed by #164. The local
runner now uses the existing M3_REPO mechanism with an exact, hash-checked archive
of the corpus's declared Git base. Current campaign report regressions remain
separate and run against current source. No frozen candidate, task, acceptance
criterion or historical receipt is modified. This checks historical harness
fixtures and does not requalify solved tasks for a new measurement campaign.
