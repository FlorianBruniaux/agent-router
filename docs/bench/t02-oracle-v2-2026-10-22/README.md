# Versioned t02 oracle and finite fixture binding, 2026-10-22

`t02-score-v2` detects the wrapping-u64 defect missed by the frozen t02 oracle.
It evaluates multiple implementations against 18 explicit public-contract tests.
The separate `finite-campaign --run-score-v2` path records technical validation
for the already reviewed authored fixture only. It keeps the objective UNKNOWN,
run state Verifying and human acceptance absent.

## What changed

The new evaluator is in
[`campaign/t02-v2`](../../../crates/router-runtime/examples/campaign/t02-v2).
Its case and executable digests are distinct from the retained predecessor
`dcf8247d51256f888681b604b29503304eaa7fca89a9be97c3c6f1373c92b821`.
The public formula, signature, work bound and range errors are unchanged.
Historical oracle bytes, R2 manifest, stopped ledger, packets and verdicts are
not edited. The frozen [negative qualification](../t02-oracle-qualification-2026-10-21/README.md)
remains the result for that predecessor.

| Implementation | Independent public label | v2 outcome |
| --- | --- | --- |
| i128 accumulation | Correct | Passed |
| u128 accumulation | Correct | Passed |
| Wrapping u64 accumulation | Known defect | Failed |
| Division before summation | Known defect | Failed |
| Missing component work bound | Known defect | Failed |
| Truncated final i64 conversion | Known defect | Failed |

These six different sources are compiled and executed through the same engine;
correctness is not determined by equality to one solution. Their labels were
established by separate public-contract reasoning before the original oracle
probe. The new 18-case suite extends the earlier public checks with explicit
u64 overflow in each component and separate rounding witnesses.

[qualification.json](qualification.json) records exact source, suite, compiler
and label identities and every case outcome. Missing compiler, compilation
refusal, timeout and excessive output each produce UNKNOWN. The classifier
requires the complete unique named test population plus a consistent libtest
summary and exit status. An incomplete or ignored/filtered run cannot pass.

## Limits and authority

[isolation.json](isolation.json) records the Linux ARM64 cell: 8 GiB, zero swap,
one CPU, 256 PIDs, network none, read-only root, UID 65534, dropped capabilities,
no-new-privileges and no host mounts. Only explicit public files are copied into
private tmpfs. No hidden oracle, account, credentials, provider, model or real
registry is required. Per-child address space stays at 8 GiB, CPU at 10 seconds,
wall time at 20 seconds. Output uses a 16 MiB file cap and 64 KiB capture threshold.
The fixed child process group is killed before its leader is reaped.

The engine's developer qualification can compile the six reviewed public sources.
The runtime entry deliberately retains the existing authored-fixture byte gate
before compilation: it is not a sandbox for arbitrary candidate programs. No new
host execution authority is implied by this matrix.

The v2 child reconstructs the displayed patch from its actual source/export and
checks exact file scope, mode and tree. It emits a new receipt with evaluator,
compiler, case, patch, candidate and Runtime-report hashes. The Rust parent checks
all receipt fields against the pinned versioned contract, permitting only current
report and observed stdout digests to vary. It refuses v1 receipts, substituted
artifacts, missing cases and manufactured human/adoption claims. There is no
external receipt-import command.

The receipt is saved as `t02-v2-receipt.json` before recording through the retained
verifier handle. Infrastructure refusal yields no successful attestation, leaving
validation unknown. A successful attestation sets validation Passed and objective
Unknown; it grants neither human acceptance nor publication. `--run` and
`--run-public-score` keep their prior behavior. Mode selection alone grants no
worker execution: the existing explicit approval channel remains required.

## Qualification versus reserved decisions

Every published `t02_v2_*` case is qualification/tuning material. It is not reused
or relabelled as held-out decision evidence. A separate reviewer sealed 48 additional
cases and their labels before reading the new implementation:

- Rust tests SHA256: `8e0080dfe966b1d9f8af5bbb9a379904ab5d753448eb3cef145946a7a9d500d8`.
- Labels SHA256: `3adaee4a20ef5772d392486542a3faca9a599dcaf5245eafd2e5bbf31a774d65`.

Their values were withheld from the implementer until candidate freeze. The
reviewer independently cross-checked all 48 expectations with Python arbitrary
integers and Rust i128. These cases share semantic contract families with public
qualification; separate IDs and values do not prove semantic independence or
exhaustive adequacy. Once consumed, that reserve cannot be called untouched.
Any correction informed by its results requires a newly sealed reserve.

[reserved-review.json](reserved-review.json) records the one completed consumption:
48 cases on each of six implementations, with both correct controls passed and
all four known defects failed. Source hashes were unchanged before and after.
The private values and raw outputs remain with the reviewer. The reserve is
consumed, and no future decision population is claimed to be reserved.

## Reproduction and evidence boundaries

```sh
python3 docs/bench/t02-oracle-v2-2026-10-22/test_oracle.py
cargo test -p router-runtime --all-features --example finite-campaign --locked --offline
```

`qualify.py` runs only in a disposable Linux cell at the limits above, using the
locally available image pinned in `isolation.json`. Copy the explicitly referenced
public sources into `/task/repo` without host mounts, then run:

```sh
python3 -B /task/repo/docs/bench/t02-oracle-v2-2026-10-22/qualify.py
```

The script creates `/task/v2-results` once. It refuses a pre-existing result folder,
checks six implementations and four infrastructure failures, and exercises the
receipt-producing CLI with a **synthetic** Runtime report plus the published
source/patch/candidate. That synthetic report is binding-test input, not evidence
of a new Runtime execution. The immutable `acceptance.json` embedded in Rust is its
contract fixture. No worker, model or benchmark was run for this qualification.

[distribution.json](distribution.json) records 33 passing exporter tests and a
strict archive with complete license evidence and a passing secret scan. The
finite-campaign example compiles offline from that archive. The exporter permits
only the exact public v2 Python engine path; unrelated Python files, symlinks and
private content remain refused.

## Remaining acceptance

#174/#132 remain open for authentic human acceptance/provenance, durable verdict
history and complete useful-task/integration/recovery evidence. The new mode is
wired and contract-tested; its native worker composition is not replayed in this
lot. Arbitrary-candidate acceptance remains outside the authored-fixture authority.

A future benchmark needs a new sealed protocol and explicit approval referencing
the new evaluator identity. H0/H1 must use identical criteria. Do not edit or reuse
the historical R2 authorization/ledger to make the new oracle appear retroactive.
No quality or cost savings, accounting reconciliation or real trial is claimed.

[validation.json](validation.json) records the exact local gates and source freeze.
The workspace reports 1,934 passing results, zero failures and
43 ignored results across 196 summaries (nested helper results
included, not unique tests). Format, warnings-denied Clippy, harness parity,
11 example tests, five Python checks on macOS and Linux, 33 packaging tests and
Gitleaks pass. No remote CI success is claimed.
