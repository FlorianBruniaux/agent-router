# Authored finite-context corpus

Date: 2026-10-21. [corpus.json](corpus.json) contains 16 synthetic cases written for Agent Router. Source text, streams and expectations are literal JSON strings, including escaped ESC and CRLF bytes. No provider response, user source file or private content was copied. The runner checks the strict JSON input bound of 262144 bytes and uses the existing 32768-byte file, 8192-byte result, 32768-byte session and eight-call limits.

[Offline protocol and results](../../../docs/bench/rtk-context-offline/README.md) describe the report. The integration target is [finite_context_corpus.rs](../../../crates/router-runtime/tests/finite_context_corpus.rs), with a test-only runner in [support](../../../crates/router-runtime/tests/support/finite_context_corpus.rs). No process callback invokes a program.

| Fixture | Exact expectation |
| --- | --- |
| `crlf_utf8_tail` | Second line is exactly `é\r\n`; full-file SHA, total three lines and following unterminated tail remain verifiable |
| `literal_escape` | Both source spelling `\x1b[31m` and actual ESC inside a source string remain intact; window keeps the second source line |
| `json_large_integer` | Literal `9007199254740993`, escaped ESC and duplicate array items remain byte-exact |
| `useful_duplicates` | Both occurrences of `key` and `context` remain in order |
| `long_line` | Selected 8193-byte line returns the admitted 8192-byte prefix, truncation and halt; no complete-window metadata |
| `quota_exact` | 8192-byte line is complete, retains its full source hash and is not marked truncated |
| `stderr_only` | Empty stdout, `error: invariant broken\r\n` on stderr, exit 2; SGR view then one full raw recall |
| `nonzero_ok` | `ok\n` on stdout still exits 7; stderr remains `detail\n` |
| `cursor_escape` | Cursor CSI cancels the entire projection, including otherwise removable SGR |
| `osc_escape` | OSC cancels the entire projection; no partial rewrite |
| `hint_overhead` | One colored `error` returns raw when complete metadata outweighs the saving |
| `already_truncated` | Incomplete callback result remains exact and halts; no handle or valid-fallback count |
| `structured_diagnostic` | Authored JSON diagnostic keeps E0308, the primary span and all twelve repeated notes |
| `source_replaced` | Replacing `before\r\n` with `after\r\n` refuses the window with `precondition_failed` and halts |
| `foreign_handle` | A handle from another private FilePolicy session refuses with zero returned bytes and no exec callback |
| `exact_call_and_byte_quotas` | One execution and complete recall reach both configured limits; the next recall refuses and returns no partial result |

The output fixtures fix `elapsed_ms` at 1000 for authored serialization. The source rows use that same fixed metadata basis. Actual dispatch timing is checked separately and is preserved on raw recall. Handles and paths are excluded from the report; deterministic test-store handles are used only to serialize an authored complete result.
