# Native proof binding, 2026-10-23

One fresh Linux execution now links the owning Runtime's verification receipt to
the exact approved task revision and both complete public Git trees. The authored
candidate passes 18/18 fixed public criteria; the original source passes 6/18.
Technical validation passes, objective remains UNKNOWN, human acceptance is
absent, and the source remains unchanged. The run ends in Verifying (exit 3).

This follows the merges of #202, #204 and #205. It advances #132 and #11 without
closing them or promoting the separate application/recovery fixtures to native
integration evidence.

## Exact identities

| Identity | Value |
| --- | --- |
| Product code commit | `f6d4b5961d83ff73289e68e65831bcafbf10b6f0` |
| Product code tree | `7940ad7a3558fc4aea65ccf8c8663010726093b8` |
| Task source revision | `d674cbfdc683bc1a4e190934f594f925bf8ef143` |
| Complete source tree | `ef46b6f37e9fb1dbb3f608cc6886a2ca5deac180` |
| Complete candidate tree | `c3344fa47fb691e990ddc376357376e06be3ad91` |
| Binding version | `complete_public_tree_v1` |
| Native driver digest | `10784f5a3fb12396474fb7b1602df915f0eb183d2069ed65178301061974e5a2` |
| Compiled build digest | `cf8254bb0572831978168f087b7ac45cd11afdf4b8d83e79c68f5be397963dbf` |
| Receipt SHA-256 | `a8da5a4609e59bda53c504bc6d9423a250e2f6182640ad8b5ed102e9c21dd416` |

The product commit identifies the implementation; the task revision identifies
the disposable repository being corrected. They are different identities.
The candidate tree is reconstructed privately from all six public input files,
including context and feedback files. It is neither a published commit nor an
applied user-workspace correction. All six files have private filesystem mode
0600 and Git mode 100644.

Both approval challenges display the binding version and complete trees. The
entry checks the exact source commit and full tree before checking and again
before attestation, within one verification deadline. The receipt, qualification
report, durable projection and single `verification.attested.v1` event agree.
Historical receipts missing these fields are refused by the new checker.

## Observed native result

The real local native client used the fixed scripted transport for six authored
requests in one attempt. One distinct permission denial was observed. No model,
provider account or real registry was used. The parent read the owning Runtime's
recorded result without writing its verdict. Thirty retained evidence files
were checked against their collection manifest before this public projection.

The namespace terminated, the admission ticket was released, and both build and
native cells are stopped without OOM. The native cgroup reports a peak of
1,140,826,112 bytes, memory.max 8,589,934,592 bytes and memory.swap.max 0. No
memory event counter increased. Network remained disabled at the outer cell,
with no host mounts; the existing private-access rules and namespace security
configuration were preserved.

The low-level transport result still contains pre-verification UNKNOWN fields.
The technical verdict comes from `qualification.verification.validation` and its
matching durable attestation. This distinction does not grant objective success.

## Local validation and review

- Full macOS gates on `ee122ba4078c8f3c3eeda998b1ed3022c1d6f5c9`: formatting,
  warnings-denied workspace Clippy, all-feature workspace tests, harness, ADR and
  secret checks pass. Workspace output reports 1,985 passes, zero failures and
  43 ignored results across 197 summaries, including nested helpers.
- Seven Python contracts pass, covering complete-tree reconstruction, changed
  inputs and compiler, substituted evidence, modes, paths and strict receipts.
- The first Linux build stopped after an existing descendant-test witness
  encountered ESRCH while reading a vanished `/proc` entry. That failed run is
  retained. The Linux-only test correction at `f6d4b596` accepts ESRCH as absence
  but continues to reject access errors; production runner code is unchanged.
- Final `f6d4b596` formatting and workspace Clippy pass. Its offline Linux build
  passes 10 entry tests, 25 runner/contract tests, seven Python contracts and
  packaging (33 pass, one skip). The ordinary binary excludes all checker assets
  and refuses qualification mode. The qualified binary includes the assets.
- The final Linux tests cover the new ESRCH regression, all six process-control
  scenarios and source/deadline contracts. Seven inert parent tests pass. The
  full macOS workspace was not repeated for the Linux-only test delta.
- Independent source reviews approve the four production/contract paths and the
  later Linux test-only path. The exact driver also received independent review.
  Its compiled build and post-run evidence were checked by the primary session;
  a separate peer readback was unavailable because its tool quota was exhausted.
  No independent post-build or post-run approval is claimed.

CI was not used as acceptance evidence. No remote rerun or registry access was
needed for these checks. The documentation commit changes no tested source.

## Evidence and limits

- [Qualification projection](qualification.json): actual native result, binary
  hashes, cell limits, cleanup, receipt identities and review limits.
- [Local validation](validation.json): commands, log hashes, exact tested states
  and failed preparation history.
- [Source pins](source-pins.json): the five reviewed code/test paths.
- [Qualification contract](../../fresh-finite-qualification.md).
- [Separate application/recovery evidence](../application-recovery-2026-10-23/README.md).

Raw approvals, configurations, request/response frames and private paths stay
in local retained evidence. This projection cannot independently reproduce a
native session or authenticate its history from hashes alone.

This qualifies one fixed public authored fixture. It does not establish human
acceptance, native application or interrupted integration recovery, arbitrary
task correctness, global automatic host integration, the wider OS/host matrix,
provider costs or savings. The nine prior fresh-entry refusal controls were not
replayed in this run. The next #132 slice must connect exact candidate acceptance
to an explicitly approved native integration path and qualify its interruption
and recovery behavior.
