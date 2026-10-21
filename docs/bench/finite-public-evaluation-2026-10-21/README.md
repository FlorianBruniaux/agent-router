# Fixed public score evaluation through the finite owner

The explicit `finite-campaign --run-public-score` example retains the verifier
handle before consuming the approved run. After native execution, namespace
cleanup, admission release and export, it runs the public score checker from
PR #191 and records technical validation against the captured patch.

The mode accepts only `t02-Lm-r1-H1`. The default `--run` behavior is unchanged.
The synthetic launcher opts in with both `--useful-score` and
`--evaluate-public-score`; it never imports a live sender. Existing approval,
60-second worker deadline, token bound, 8 GiB memory and zero-swap checks remain
unchanged. The evaluator has its own 120-second deadline after worker cleanup.

## Evidence and authority

The checker, twelve public cases, sealed packets and published authored solution
are embedded in the example. The parent stages private copies and supplies the
actual captured patch, checked export and exact immutable Runtime report.
The checker reconstructs the patch, verifies scope/mode/tree and executes all
required cases on the source and candidate. It refuses candidates other than
the published authored solution before compiling them. This is deliberately a
fixed fixture runner, not an evaluator for arbitrary generated code.

Only a successful child process and its freshly produced, bounded receipt can
reach the Runtime attestation. Receipt fields must match the reviewed public
baseline; current report, patch, candidate, checker and case hashes are checked.
Only the case stdout hashes may vary, and must remain valid digests. There is no
external receipt-import command or worker-selected verifier executable.

The parent terminates the evaluator's process group before reaping its leader,
including on success; timeout or wait failure also terminates the group. This
is cleanup for the fixed reviewed checker, not a general sandbox for arbitrary
or detached evaluator code. Worker namespace ownership remains unchanged.

The exact receipt is saved before recording. `evaluation-result.json` reports
`validation=passed`, `objective=unknown`, `state=verifying`, and null human
acceptance. `runtime-result.json` remains the immutable pre-evaluation report.
An evaluator failure/refusal leaves verification UNKNOWN. A failure to write
the post-attestation summary does not erase a committed record; the saved
receipt and Runtime store remain the recovery evidence. No replay is automatic.

The hidden campaign oracle remains unqualified under #174. No source revision
is published, no human verdict is inferred, and no real model, account, provider
request or campaign ledger is used. This advances #132 without closing its
native automatic-entry, integration, human-acceptance or accounting criteria.

## Local qualification

The Linux ARM64 replay used six authored responses and five real tool returns.
The source failed eight of the twelve public cases; the captured correction
passed all twelve. A separate read of SQLite confirmed the persisted
`fixed-public-score-v1` record, with validation Passed and objective UNKNOWN.
The source stayed unchanged, cleanup was `namespace_terminated`, and admission
was released. All three evidence files were mode `0600`.

Ten example tests pass on both macOS and Linux, including the process-group
regression, receipt substitutions and private-file refusals. Seventeen Python
binding checks and three CLI checks pass. The Linux cell has no network or host
bind mounts, uses the same pinned programs and 8 GiB/zero-swap limits, and is
stopped after its private archive was verified.

See [qualification.json](qualification.json) for exact source hashes, local
quality results, memory observation and evidence limits, and
[acceptance.json](acceptance.json) for the checker output bound to this replay.
The historical #191 receipt and pre-evaluation Runtime report are unchanged.
