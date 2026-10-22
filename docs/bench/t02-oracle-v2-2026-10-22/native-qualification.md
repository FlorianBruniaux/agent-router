# Native v2 fixture qualification, 2026-10-22

The real bounded Linux entry executed one authored `t02-Lm-r1-H1` fixture
from source commit `a16e3646f6d55d1bb2e475891cb0ebb31b4e7c19`.
Six coded responses produced five tool returns, a captured patch and a v2
receipt bound to the actual Runtime report. No model or provider was called.

[Public evidence](native-qualification.json) links the retained raw report,
executable, exact source export, driver overlay, patch, receipt and database
readback by SHA256. [native-receipt.json](native-receipt.json) is the unchanged
receipt. Its hashes match the independently retained files; the persisted
verification and run summary equal the emitted evaluation result. Private
Runtime reports, SQLite files, session bindings and host paths are not published.
The public projection is evidence, not an importable execution capability.

| Check | Observed result |
| --- | --- |
| Native feedback, before / after correction | 1/4 passed / 4/4 passed |
| v2 evaluation, before / after correction | 6/18 passed / 18/18 passed |
| Persisted verification | validation passed, objective unknown |
| Run state / human acceptance | verifying / absent |
| Source publication / source modified | false / false |
| Namespace cleanup | namespace_terminated |
| Admission after cleanup | 0 granted, 1 released, 0 waiting |
| Memory / swap | 8,589,934,592 bytes / 0 |
| Peak memory / OOM events | 1,423,245,312 bytes / 0 |
| Network / host mounts | none / none |
| Receipt, report, evaluation modes | 0600 |
| Cell after collection | stopped |

The build used the exact Git export (1,743 source files, all 20 PR paths),
unchanged lockfile, public locked dependencies, Rust 1.85.1 and the already
qualified Linux profile. It ran offline with one build job. The driver reused
`finite-campaign-launcher-2026-10-18/qualify-offline.py` with a frozen overlay
selecting `--run-score-v2`, checking all 18 case names, and comparing the
read-only SQLite readback to the result. Its plan and executed overlay hashes
are retained in the projection. Reproduction requires the pinned public
executables, a disposable no-network cell and fresh explicit approval; this
report does not authorize replay or import into a campaign ledger.

One earlier preparation attempt built successfully but failed before Runtime
invocation because the minimal environment could not locate Docker. Binary
collection also failed on that attempt. Its immutable report hash and zero
native-invocation count are retained. An explicitly authorized manual retry
fixed preparation and retained the executable before the one native dispatch.
No failed native run was hidden or automatically replayed.

The upstream dependency scan exited 1 with 120 alerts classified as public
upstream test keys, manifests, documentation or generated constants. It was
not a clean scan. The separate project source scan passed. No scanner rule,
private-access boundary, memory limit or dependency source was relaxed.

The old `json_shape_valid: false` field is t05-only and not applicable to t02;
it is not a failed t02 JSON check. Original receipts remain unchanged. The
reported cost observation uses scripted fixture values, not real usage or
subscription debit, and is deliberately excluded from this projection.

This supplements the earlier synthetic binding qualification. It proves the
specific native fixture composition and persistence, not arbitrary candidate
acceptance, human approval, automatic fresh-session routing, recovery coverage
or a comparative benchmark. The 48 reserved evaluator cases were already
consumed; a future comparison needs new sealed decision evidence. #132 and
#174 remain open. Remote CI did not execute because of billing restrictions.
