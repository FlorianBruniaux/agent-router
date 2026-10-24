# Native candidate acceptance and private application

This lot advances #132 after #206, merged as `7eb3861745bcb7ead9f3b8ce0e0ea4c9d24ffcaf`.
The signed code commit is `261eeeb82f054644599afa6e45bfc4cd18deb3ae`,
tree `85e553efb400bc273f087a47c224a55ac8e9c0b5`. The code and public source archive
are pinned in [source.json](source.json) and [source-pins.json](source-pins.json).

## Contract

The opt-in `fresh_t02_apply_v1` entry preserves the technical receipt, then asks
for a third `accept_apply` approval bound to the exact candidate and current
session. The controller records explicit acceptance separately from technical
verification and applies only the existing output in its freshly prepared private
source. Worker permissions remain restricted to artifact production. The verifier
cannot apply; no generic pending run or resumable handle is exported.

The durable intent precedes writing through a retained descriptor. Applied bytes,
identity and mode are checked before reporting success. HEAD and index entries
remain unchanged. Acceptance does not establish human acceptance or publication.
See [ADR 0046](../../adr/0046-finite-controller-application.md).

## Local verification

[Local checks](local-checks.json) report all seven gates passing: Python, format,
warnings-denied workspace Clippy, full all-features workspace tests, harness,
ADRs and Gitleaks. The workspace reports 1,991 passes, zero failures and 43 ignored;
counts include nested helper processes.

[Validation history](validation-history.json) preserves two earlier failures.
The first legacy partial-write child exited before its checkpoint; its cause
remains UNKNOWN. The exact isolated child, targeted application tests and final
workspace pass. Only its failure diagnostic was improved. A separate API test
race was reproduced deterministically: an early complete 503 response can precede
a client write. The test helper now reads that response and accepts only its
specific closed-socket error with status 503. Production server behavior is unchanged.

## Observed Linux application and refusal

The frozen driver `57562936` and compiled build `7695483c` were reviewed separately
before execution. [Linux build evidence](linux-build.json) records 10 entry tests,
25 runner/contracts, five application tests, seven Python tests, 33 packaging
passes with one skip, and one ordinary-build refusal. Four retained ELF binaries
were independently hashed. Ordinary-build asset exclusion relies on the pinned
build probe and log; that ordinary binary was not retained.

[Native results](native-results.json) cover two fresh scripted executions:

| Case | Observed result |
| --- | --- |
| Exact acceptance | Entry exits 0; candidate accepted explicitly and `lib.rs` applied in the private prepared source. |
| Wrong acceptance digest | Entry exits 6; source files unchanged, technical objective UNKNOWN, no integration intent or applied event. |

Each case performs six scripted worker requests, including one distinct permission
refusal, and invokes no model. The positive candidate passes 18/18 fixed criteria
against 6/18 for the original source. Post-application reads establish that the
complete six-file public tree equals the tested candidate tree; only `lib.rs`
changes. Its bytes match the accepted hash and its mode remains 0600. HEAD and
Git index entries remain unchanged; no publication ref is created. The oracle
was not rerun on the applied directory: equality to the tested complete tree is
the application evidence.

The current acceptance record has objective Passed with explicit controller
provenance. Both the technical UNKNOWN attestation and acceptance are retained;
intent precedes applied. Human acceptance remains null. Both third-challenge
digests were independently recomputed from the exact session, submission,
configuration, qualification identity and reviewed owner digest.

[Public evidence projection](native-evidence.json) retains these records and their
original byte digests. All 64 retained files were checked against the
[manifest](retained-manifest.json). Both cells stopped without OOM. Native memory
peak was 834,404,352 bytes; the limit remained 8,589,934,592 bytes with zero swap,
no external network and no host mounts. The two native cases were executed once
each. Earlier failed local test runs remain separately documented above.

[Independent reviews](independent-reviews.json) distinguish source, driver,
compiled build and retained native evidence. None grants merge or model authority.

## Evidence limits

This is a scripted public fixture, with no model, provider account or real
registry. The native limit remains 8 GiB, zero swap, with the existing private
access restrictions. It is not a cost benchmark, arbitrary-candidate qualification
or human approval of a model-produced result.

In-place application is not crash-atomic. Unit tests inject failures before and
after writing and refuse replay after reopen. Those callbacks do not qualify a
real interruption of this new native composition. Native owner termination,
reconciliation without replay, remaining composed lifecycle cases and global host
integration remain follow-up work. #132 stays open; #120/#133 still gate real-provider trials.
