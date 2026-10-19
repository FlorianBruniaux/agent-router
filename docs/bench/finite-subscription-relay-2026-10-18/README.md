# Finite Codex subscription relay: offline qualification

Offline qualification of code commit `1ab71472f1b20a49ef317eece606bb752bfe7840` passed with the actual Codex 0.159.2 Linux ARM64 client. The finite Runtime, parent-owned subscription relay and durable consumption ledger were exercised in three fresh cases. The upstream returned coded responses. No provider request, real model trial or real budget approval occurred.

The 24 benchmark assignments remain `not_run`. Synthetic counters below are test inputs, not an observed subscription debit or a model-quality result.

## Verified behavior

| Case | Result | Observations |
| --- | --- | --- |
| Linux controller and native Runtime | PASS | Four coded responses, three actual tool returns, 2,400 synthetic tokens and 13,100 synthetic equivalent microcredits; private patch captured after namespace cleanup |
| Unknown usage on the second coded response | PASS | The 600-token and 3,275-microcredit known prefix survives; further work stops, cleanup is confirmed, no private patch is promoted |
| Mac controller and ledger, Linux native Runtime | PASS | Same run, approval, channel nonce and cleanup fingerprint on both hosts; four receipts and three actual tool returns |

The worker keeps the qualified public-only mounts, fixed finite tools and 8 GiB bound. The qualification container also has an aggregate 8 GiB limit, no swap and no external network route. No home directory or credential file was mounted into Linux. The guard enforcement policy is unchanged. Its probe was updated for the existing MCP mode; the exact source is covered by the retained Linux qualification.

The initial interactive bridge used `std::io::copy` and failed to deliver the controller ACK within the existing five-second window on Linux. Explicit bounded reads and writes passed the same prelude in 0.18 seconds. The window was not increased. The underlying standard-library mechanism was not traced.

## Source and activation contract

`OwnedSubscriptionController::pair` creates the private channel. `FiniteClientReview::with_subscription_controller` binds its account, controller, configuration, assignment and fresh instance into the review. The public Runtime review, approval and finite execution methods use that owner. A prepared profile without the owner is refused. Subscription admission uses `codex_subscription_relay`, never `local_fixture`.

Before worker spawn, the host persists `begin_trial` and acknowledges `runtime_ready`. Every response passes a durable consumption barrier before the Runtime closes its HTTP stream. The Runtime emits its cleanup event after the exact stopped-namespace proof and cleanup transaction. A copied JSON cleanup declaration cannot create that event.

The versioned host entry, `controller/relay-controller-results-v2.py::run_subscription`, requires an explicit positive budget and the exact reviewed launch digest. The digest binds code, account, configuration and destination. Credential reads occur only inside the fixed host sender after those gates. Qualification never invokes that sender against the real account. Equivalent credits remain separate from micro-USD and from the actual subscription debit. An observed stop threshold is not a provider billing cap.

[ADR 0042](../../adr/0042-owned-subscription-relay.md) records the boundary. The frozen source candidate has SHA-256 `9652bae15567801105f42c5c791bcd5acd6abe976cb2e538441da6ec2173952f`, based on `2670585c180a2b7dc0699e67ec8cae981a16d81b`. The separate OpenSSL fix is `c67dbb2634f656c2ae1509a7853280e218cd84de`; its four files are excluded from the coupling patch.

## Reproduce the host checks

Copy this folder to a fresh temporary directory before running the checker, because it writes its own local test receipts.

```sh
python3 -B check-offline.py
python3 -B qualification/relay-controller-macos-e2e-test.py
```

The host capsule has 141 passing tests and the portable Mac bridge has four. The corrected code passed all 11 CI checks, including workspace tests on Linux and macOS and native Linux enforcement. [The CI receipt](qualification/ci-1ab7147.json) binds these results to `1ab71472`. [The fresh native replay](qualification/current-native-replay.json) records the three subscription cases, 1,666 tracked files checked without mismatch, and container shutdown after cleanup. Hook activation remains `UNKNOWN`.

The earlier local quality run remains 33/34: the helper exceeded its unchanged ten-second wall-time assertion and the runner reported an EPERM collection error. [Its receipt](qualification/local-quality.json) and [CPU diagnostic](qualification/cpu-diagnostic.json) remain unchanged. The diagnostic measured only 0.48 to 0.61 CPU seconds over ten elapsed seconds under heavy host contention. CI success does not rewrite that local result. The previous CI billing block is historical.

A native replay requires the pinned public SDK, guard, helper, compiler, image and verified Linux namespace composition. [The inert recipe template](qualification/recipe.template.json) has `ready=false` and unresolved container, binary and fresh-root placeholders. It cannot launch as supplied. The owner must verify those pins and use fresh Linux and Mac output roots for another offline qualification. This recipe is not a real benchmark launcher.

## Published evidence and privacy

[The qualification record](qualification/qualification.json) and [cross-host summary](qualification/crosshost-summary.json) contain counters, bindings and hashes. Full Runtime receipts, private artifact paths, descriptors and SDK payloads stay outside Git. The ledger/report hashes were independently reconciled by the Runtime integration chat.

The request fixture and launch-intent derivative are explicitly synthetic. They contain no session UUIDs, observed account data or reusable historical consent. The original sealed intent remains unchanged outside the capsule. The synthetic derivative intentionally fails the legacy gateway CLI seal check before transport; the host test expects that refusal.

A real launch still requires Florian's budget decision, a fresh account/authentication check and the exact launch approval. Provider availability, actual subscription consumption and the 24 trial outcomes remain unverified.

## Portability correction after native qualification

[The correction receipt](qualification/portability-correction.json) records a conversion of the same nine permission bits into the platform mode type. It removes a Linux Clippy warning without changing file permissions. Format, macOS workspace Clippy and 11 targeted tests passed locally, including post-stop promotion and the stopped-namespace prerequisite. The available isolated Linux toolchain lacks Clippy; no component was installed and no network was opened. The initial native evidence belongs to `2ea53a9c786403dd78b1e6cf2ed82bca3b1615c0`. The subsequent three-case replay and CI now qualify the corrected code at `1ab71472`; the historical correction receipt remains unchanged. The local CPU timing failure is retained separately.

## Publication boundary

The final evidence update changes documentation only. CI and native execution evidence above apply to code commit `1ab71472`; a later documentation commit is not presented as an executed native binary. The replay used an independent cache copy and fresh roots, preserved the original container and cache through read-only access, and stopped its own container after all three cleanup proofs were checked. Raw Runtime receipts, synthetic private patches and SDK streams are not published.

The host capsule manifest and its verification receipt retain their original frozen identity. Their pending native-evidence note is historical. Current native and CI status is recorded in `qualification/qualification.json` and the new replay record; the 141 host tests are not presented as newly executed.

## Recovered host delivery

The [five-file host snapshot and offline checker](host/README.md) preserve the separate production parent delivery from 18 October. Its 26 historical tests were replayed with coded responses on 19 October; the source hashes match the original delivery. This restoration does not change the qualification receipts above, reconcile the stopped campaign, or authorize another model request.
