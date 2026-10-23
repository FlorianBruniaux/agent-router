# Fresh Runtime verification qualification, 2026-10-23

One fresh Linux session at source `313bb1fa9671c853cb33b98e0b55f95457b9ed0b`
completed the fixed public t02 correction and recorded its technical validation
through the Runtime that owned the approved worker. The parent supplied authored
local responses; no model, provider account or real campaign registry was used.

[Qualification evidence](qualification.json) binds the source archive, reviewed
driver, compiled binaries, actual isolation, retained receipt and durable
verification. [Local checks](validation.json) and [source pins](source-pins.json)
identify the tested code. This report adds evidence only; it does not authorize
replay, source application, publication or acceptance.

| Observed check | Result |
| --- | --- |
| Native attempts / authored worker requests | 1 / 6 |
| Distinct permission refusals | 1 |
| Original source / corrected candidate | 6/18 / 18/18 cases passed |
| Receipt preserved before product attestation | yes |
| Matching durable verification / attestation event | yes / exactly 1 |
| Final state / validation / objective | Verifying / passed / UNKNOWN |
| Human acceptance / source publication | absent / false |
| Original source / namespace cleanup | unchanged / terminated |
| Admission after cleanup | 0 granted, 1 released, 0 waiting |
| Hard memory / swap | 8,589,934,592 bytes / 0 |
| Peak memory / OOM events | 953,446,400 bytes / 0 |
| External network / host mounts | none / none |
| Build and native cells after collection | both stopped |

The checker emitted its receipt before the owning Runtime called the existing
verifier handle. The receipt's historical `runtime_verdict_modified: false`
means the checker itself did not write Runtime state. The parent also only read
the result. The product then recorded validation, including the full
`verification.attested.v1` event and matching durable projection. The exact
receipt bytes remain unchanged. The public projection gives these three scopes
explicitly rather than suggesting that no Runtime write occurred.

The final report preserves two phases explicitly. `report.result.validation`
remains `unknown`: it is the worker-completion snapshot captured before checking.
The new `report.qualification.verification.validation` is `passed` and matches
the durable projection and attestation. `report.summary` and `report.events`
are refreshed after recording. Consumers must read the qualification field for
this later technical verdict; not every nested field is rewritten.

Both fresh approval challenges included the same checker identities. The
receipt binds the actual submission, session, pre-verification report, captured
patch, candidate, adapter, cases, engine and compiler. Independent readback
matches the report, retained receipt, projection and ordered history events.
All 30 captured files were checked against the collection manifest; the manifest
itself is the 31st retained file. Raw session bindings, approvals, configuration,
Runtime reports and database are not published. Their hashes are evidence links,
not executable authority.

The offline build used the exact Git archive, locked public vendored dependencies
and the existing pinned Linux image. It passed 10 entry tests, 22 finite-entry
unit tests including all six Linux process tests, five Python binding tests,
33 packaging tests with one explicit skip, and one ordinary-build refusal test.
The qualified binary embeds the three fixed checker assets; the ordinary binary
contains none of them and rejects the mode. The Linux process tests include real
success and failure, same-group descendants after leader exit, timeout, expired
staging before spawn, null-output handling and a simulated signal refusal.
Detached descendants remain the outer cell owner's responsibility.

Local macOS gates pass: formatting, warnings-denied workspace Clippy, full
all-feature workspace tests, harness, ADR and secret checks. The workspace
reports 1,970 passes, zero failures and 43 ignored results over 197 summaries,
including nested helper output rather than unique tests. Prior failed runs and
the rejected fixed-timeout version remain in the local validation history.
CI availability is separate and remains blocked by billing.

This is one successful fixed-fixture composition. The nine historical native
refusal controls were not replayed, and no wider host/OS matrix, arbitrary-task
oracle, global automatic integration, restart/recovery behavior or human
acceptance is established. The checker was allowed only the planned verify-node
budget, including staging. No memory or private-access rule was relaxed.
#132 remains open; #120 and #133 still gate the corresponding real-provider trial.
