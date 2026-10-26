# Verified patch application and interrupted-owner recovery

This qualification exercises existing in-place application behavior for #132 using public, disposable fixtures on macOS. It does not establish human acceptance of the native t02 result. No production defect was reproduced or fixed in this lot.

The implementation extracts `Runtime::apply` into a private module and passes the existing capture observer to its final Git operation for tests. The public API, approval digest, permission checks and persistent one-shot intent remain unchanged. Tests share the existing runtime fixture instead of duplicating its authority setup.

## Exact revision and useful correction

Eight public-API cases cover:

- Applied bytes and executable mode match the reviewed Git revision, whose parent is the exact approved source revision. Source HEAD and index remain unchanged.
- A different source commit with identical files refuses the old approval.
- Substituted candidate bytes, mode or paths refuse application.
- A source symlink replacement cannot overwrite its external disposable target.
- Two concurrent calls sharing one Runtime/store consume application authority once.
- Unknown objective acceptance cannot authorize application.
- A verified attestation cannot be replaced after review.
- The existing `clamp` correction fails its three criteria on the original source, then passes the same criteria on the candidate, reconstructed revision and actually applied file. These byte sequences must match. Other source content and index remain unchanged; no publication ref is created.

The clamp criteria are `clamp(-5) == 0`, `clamp(0) == 0`, and `clamp(5) == 5`. Its authored verification is a fixture, not a human acceptance record or a replacement for the native t02 oracle evidence.

## Real interruption boundaries

Four scenarios use a separate, disposable owner process and the actual Git application operation:

| Boundary | Observed files before reopening | Persistent evidence |
| --- | --- | --- |
| Owner killed before Git application | Original file; second file absent | Intent only |
| Owner killed after Git writes | Both changed files, including 32,769-byte second file | Intent only |
| File-size limit interrupts Git, then owner killed | First file changed; second file only 4,096 bytes | Intent only |
| Owner exits normally after application | Complete changed files | Intent and applied event |

The three interrupted owners are killed and reaped with SIGKILL after an explicit boundary marker. The partial-write test changes only its child's soft file-size limit, preserves the hard limit, restores it after capture, and checks that the suite parent's limit is unchanged. It does not alter any memory limit. The Runtime test observes the partial bytes and owner exit signal; it does not inspect Git's internal exit signal.

After the owner stops, the test adds staged, unstaged and executable untracked user changes. Reopening and a refused retry preserve these bytes, modes, HEAD, index and recorded events. Even after the test alone restores its disposable source preimage, the consumed intent refuses replay. The runtime performs no rollback or automatic reapplication.

A run's `Verified` state records verification, not successful file application. Missing terminal application evidence remains uncertain. The existing `publication_status == Unknown` assertion concerns ref publication after a legacy in-place intent; it does not introduce an application-status contract.

## Evidence and limits

See [validation.json](validation.json) for local command results and [source-pins.json](source-pins.json) for the hashes of the tested Rust sources. The initial full-workspace run and the final runtime run have separate source manifests: only the eighth, clamp acceptance test was added between them.

The internal test runner also reports an inert child entry point as one passing test. It is not a fifth interruption scenario. Workspace summary totals include nested helper summaries and are not a count of unique acceptance cases.

This does not prove protection against arbitrary editors during live Git writes, exclusion across independent runtime stores, power-loss atomicity, or a new Linux native execution. Existing publication tests do not confer those guarantees on in-place application. See [ADR 0027](../../adr/0027-verified-patch-publication.md).

No model, provider account or real registry is used. The native 8 GiB memory contract (8,589,934,592 bytes, zero swap) and private-file permissions are unchanged. This macOS fixture qualification does not refresh Linux memory measurements. CI was not used as acceptance evidence.
