# Finite context: offline fidelity and byte comparison

Date: 2026-10-21. T4 compares 16 authored cases through existing finite tools and their pure presentation functions. The [report](report.json) contains 26 comparison rows, four valid raw fallbacks, two stopped fixtures and three refusal checks. This corpus is designed to catch fidelity failures. It does not represent the frequency or size of a real coding workload.

The combined requested windows and terminal views send **415 more authored JSON bytes** than their exact baseline on this corpus, after the requested raw reread. Content alone shrinks by 410 bytes. Keep these primitives optional and explicit; this corpus does not support blanket activation or a cost-saving claim.

## Corpus and comparison protocol

[Fixture provenance and exact expectations](../../../tests/fixtures/finite-context/README.md) enumerate CRLF, UTF-8, unterminated tails, long lines, literal ESC, large integers, useful duplicates, cursor/OSC, stderr errors, nonzero `ok`, metadata overhead, replaced sources, foreign handles and quotas. All payloads are authored. The report pins `corpus.json` by SHA-256.

Each source fixture is compared in exact and requested-window modes. Each output fixture is compared in exact and requested-terminal-view modes. A fresh FilePolicy verifies each result on actual temporary files, with only an injected coded exec callback. Presented results are recovered in full for fidelity, preserving the observed original elapsed value and without calling exec again. The `stderr_only` comparison scenario requests one raw reread; the other comparison scenarios request none. Additional recovery assertions are validation work outside those scenario measurements.

The stable JSON-size comparison uses complete authored ToolResult values with `elapsed_ms=1000` and a deterministic caller-owned RecallStore binding. The pure production presentation function creates the candidate, including its full 64-character handle and metadata. FilePolicy sessions retain their real random bindings and timings. Thus these are reproducible authored serialization sizes, excluding the MCP envelope, tool-call arguments and provider traffic. They are not an observation of native SDK or network bytes. No elapsed value or live handle is overwritten in the product.

`raw_content_bytes` is the admitted exact-result baseline, not the size of an uncaptured or discarded tail. `shown_content_bytes` counts stdout plus stderr on the initial response. `recalled_content_bytes` counts the requested full raw reread once. `raw_json_bytes` measures the complete authored baseline ToolResult JSON; `serialized_bytes` includes the complete initial candidate JSON and every requested raw response JSON. Encoding is UTF-8. No deduplication or float conversion of source JSON occurs.

Rows export fixture ID, policy hash, mode, these byte counts and the fidelity verdict. They contain no payload, path, handle or journal contents. Valid fallbacks use static reasons. Capture stops and scope/budget refusals have separate records and cannot become successful raw fallbacks.

## Measured authored volumes

| Comparison | Baseline content | Delivered content, reread included | Baseline JSON | Delivered JSON | Net JSON reduction |
| --- | ---: | ---: | ---: | ---: | ---: |
| Exact, all 13 payload fixtures | 19591 | 19591 | 23082 | 23082 | 0 |
| Requested windows, six sources | 16535 | 16504 | 17154 | 17923 | -769 |
| Requested terminal view, seven outputs | 3056 | 2677 | 5928 | 5574 | 354 |
| Combined selection, each payload once | 19591 | 19181 | 23082 | 23497 | -415 |

Every comparison uses its own matching baseline subset. Combined selection takes each payload fixture once, while the three refusal fixtures contribute no invented result size. Negative net values mean growth.

The report computes net content as `sum(raw) - sum(shown) - sum(recalled)` and net JSON as `sum(raw_json) - sum(serialized)`. Signed integer basis points equal `net * 10000 / baseline`, truncated toward zero. A zero denominator yields `null`. It never averages fixture percentages or clamps negative totals. Combined JSON reduction is -179 basis points. The single `stderr_only` reread gives -25 content bytes and -290 JSON bytes, even though its first view is smaller.

The windows in this adversarial corpus save little content and add source-provenance metadata. The 8193-byte selected line still halts at the original quota and has no complete-window metadata. Four complete output fixtures return raw: two unsupported escapes, metadata larger than the gain, and a diagnostic with no SGR gain. The already-truncated result remains a stop.

## Reproduce and review

From the repository root, using the existing offline Cargo cache:

```sh
rtk cargo test -p router-runtime --test finite_context_corpus --locked --offline
```

`offline_context_report_is_byte_stable` runs the corpus twice and compares both exports with the checked-in report. It can additionally write one new local file, exclusively created, when an export path is explicitly supplied:

```sh
AR_CONTEXT_OFFLINE_REPORT=/private/tmp/new-finite-context-report.json \
rtk cargo test -p router-runtime --test finite_context_corpus \
  offline_context_report_is_byte_stable --locked --offline
```

Choose an absent file. An existing file or symlink is refused. When fixtures change, review the fidelity assertions and the exported metric diff before updating the checked-in report. The exporter writes its candidate before the golden comparison so a mismatch remains inspectable and the test still fails. The standard test run writes only its private temporary fixtures.

The target also tests weighted signed aggregation, negative reread volume, separate stop/fallback classification and unknown usage/success. Workspace formatting, warnings-denied Clippy, all-feature tests, harness parity and Gitleaks remain separate delivery gates.

## Evidence and next filter decision

Billed tokens, qualified pricing, provider cost and model success remain `unknown` in every export. Byte savings do not feed [supplied usage pricing](../../supplied-usage-pricing.md), [accepted changes](../../accepted-changes.md) or a second accounting ledger. The existing [value protocol](../router-value-protocol.md) still requires observed usage and qualified outcomes for cost/quality comparisons.

The [v4 native qualification](../rtk-context-native-qualification/README.md) remains NOT RUN on the macOS implementation host. T4 launches no model, SDK, provider, classifier, network service or authored feedback program. Its local assertions do not qualify OS containment or native transport.

The `structured_diagnostic` fixture retains 951 content bytes and 1186 JSON bytes because it contains no removable SGR. One follow-up candidate is a typed `rustc` JSON diagnostic presentation, proposed separately and not implemented here. Its admission contract would select an exact reviewed rustc argument form, parse a bounded known diagnostic schema and retain code, severity, source spans and all child diagnostics, including repeated notes. Unknown fields, malformed JSON or incomplete capture would return raw or preserve the existing stop. A complete recoverable raw result and full-JSON size guard would still be required. This one authored diagnostic does not establish that such a filter pays for itself on real workloads.
