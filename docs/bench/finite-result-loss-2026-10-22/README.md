# Finite q01 response-loss qualification, 2026-10-22

This local qualification exercises the pinned Linux ARM64 Codex executable through the finite Runtime client. Responses come from the existing authored q01 SSE fixture. It starts no model, uses no provider account or real campaign registry, and grants no additional private-file access.

## Observed cases

| Case | Expected outcome | Worker attempts / local POSTs |
| --- | --- | --- |
| Complete response, existing positive control | Verifying; objective UNKNOWN | 1 / 1 |
| Incorrect approval digest | AwaitingApproval; journal unchanged | 0 / 0 |
| HTTP body cut after `response.created` | Failed; no `turn.completed`, patch or publication | 1 / 1 |

The cut response declares the complete body's length, delivers only the first SSE event, then closes that connection. The broker keeps accepting until the worker returns: a second POST fails the test instead of being hidden by a closed listener. The existing generated client configuration disables request and stream retries. This qualifies that pinned client and that failure point only.

The refusal still performs diagnostic preparation canaries. Zero worker attempts does not mean zero native diagnostic invocations. For executed workers, the checks require namespace termination, private-home removal, the original source unchanged, admission released and refusal of generic reopening. These are observed final states; this report does not independently measure the ordering of cleanup and admission release.

## Evidence and reproduction

`evidence.json` records the public projection of receipts, direct read-only SQLite summaries/events, transport counts, source hashes and container limits. `source-pins.json` ties the executed binaries to main `eeab311fc1eaf50e98f8801a9f7e56d08ac1165a` plus the exact changed test source. Raw local files are retained privately; their hashes identify the inputs to the projection.

Use the existing [Linux qualification recipe](../linux-finite-dispatch-2026-10-16/README.md#reproduce-on-an-isolated-linux-arm64-host), the canonical `LINUX_FINITE_PROFILE_V2` read-only profile, and public q01 inputs. Build `router-runtime` tests and the finite client/helper offline with Rust 1.85.1, then invoke each test explicitly with a separate `AR_FINITE_ROOT` and `--ignored --exact --nocapture --test-threads=1`:

- `finite_client::linux_backend::e2e_tests::q01_real_approved_local_report`
- `finite_client::linux_backend::e2e_tests::q01_refused_approval_never_submits_worker_request`
- `finite_client::linux_backend::e2e_tests::q01_lost_response_fails_without_retry_and_cleans_namespace`

The outer container remains at 8,589,934,592 bytes, zero swap, no Docker network, no host bind mounts, UID 65534, read-only root and dropped capabilities. The inner existing namespace recipe provides only the local broker network; the runtime canaries verify the actual boundary before dispatch. Worker limits remain unchanged.

## Preserved unsuccessful attempts

The first build completed both targets, but Docker's archive API failed to collect a binary from tmpfs. The cell was stopped without native execution. An identical source rebuild and stream-based collection succeeded.

The first native preparation supplied the historical MCP v3 profile to the read-only positive case. The product refused it while constructing the diagnostic command, before executing that command or recording any run. The corrected driver uses a separate copy of the exact canonical broker-v2 bytes. The v3 asset, binaries, Rust assertions and resource limits were not changed; the rejected attempt is retained.

## Limits

This is not a fresh automatic user-session entry test, a model-quality measurement, a useful-task acceptance or proof of savings. Runtime objective remains UNKNOWN and human acceptance remains unset. It does not qualify loss of the Runtime owner, retry after restart, all network failures, or the publication crash boundary. Those are separate cases for #132. Remote CI is not evidence for this run.
