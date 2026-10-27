# Finite context profile v4: T3 qualification

Date: 2026-10-21. Status: implemented contracts and authored fixtures; **native Linux ARM64 qualification NOT RUN on this macOS host**. No model, account, credential, provider request, remote machine or native SDK was launched during implementation.

[ADR 0044](../../adr/0044-finite-context-profile.md), [JSON contracts](../../reference/json-contracts.md#finite-context-tools-v4) and [coordinator preparation](../../coordinator.md#finite-context-preparation) describe the producer and Runtime boundaries. T1/T2 implement exact line windows, narrow SGR presentation and complete session-local raw recall.

## Explicit preparation

The parent selects a validated `FiniteContextPolicy` and creates a fresh home with `OwnedBroker::prepare_codex_context_tools_state`. It uses that same policy with `FiniteClientReview::with_mcp_context`, after the existing private output workspace, copied assets and Linux namespace are retained. The manifest profile must contain exactly `LINUX_FINITE_TOOLS_CONTEXT_PROFILE_V4` bytes. The request allowlist explicitly includes `mcp__finite__read_output` when presentation is enabled.

The startup, FilePolicy preparations, bootstrap and review bind the full policy and the actual deterministic MCP tools array. The parent rechecks the retained bootstrap hash and identities before dispatch. The owner validates policy and catalogue checksums before opening the session. A context terminal uses schema 2 with `context_policy_sha256` and `catalog_sha256`; the Runtime compares them against retained producer data after the exact namespace-stop fence. Terminal telemetry grants no access or launch authority.

Without policy, the old constructors, configuration, hash tuples, catalogue, profile v3 and schema-1 terminal bytes remain unchanged. A v3 profile cannot admit v4 policy, and a context terminal cannot be consumed by the v3 path. The local diagnostic receives an expected catalogue checksum, not a policy from environment variables. The owner alone chooses the policy from the sealed bootstrap.

## Authored dialogue and fixtures

[script.json](script.json) fixes five tool calls: exact second source line; one red diagnostic with an SGR view; complete raw recall; scratch counter `1`; approved output creation. [source.txt](source.txt) preserves CRLF and an unterminated tail. [feedback.py](feedback.py) increments the scratch counter once and exits 1 after writing a colored red diagnostic. It contains no inference code.

[context_responses.py](context_responses.py) validates raw SDK catalogue bytes, exact results, metadata, sequence and the two-part native output envelope before emitting the next written response. It emits no usage values. [fixtures.json](fixtures.json) pins these authored files and is included in the test executable. The [controller tests](test_context_responses.py) use synthetic catalogues; they do not establish the real SDK schema.

`native_context_windows_and_recall_without_model` compiles on this host and is ignored by default. It shares the legacy canary's helper, mounts, 8 GiB process limits, lease-bound namespace-stop proof and private-home disposal. It additionally requires Linux ARM64, isolated loopback, cgroup memory.max `8589934592`, swap.max `0`, the pinned native binary and fresh exclusive output paths. Guards, seccomp and Landlock are unchanged. This is component evidence, never a Runtime approval or a model trial.

## Offline checks on this host

```sh
rtk cargo test -p router-runtime --test finite_client --locked --offline
rtk cargo test -p router-runtime --lib finite_tools::context_tests --locked --offline
rtk cargo test -p router-runtime --lib finite_client::tools_state::context_tests --locked --offline
rtk cargo test -p router-runtime --bin finite-codex-client context_probe_requires_the_parent_catalog_digest --locked --offline
rtk proxy python3 -B -m unittest discover -s docs/bench/rtk-context-native-qualification -p 'test_*.py'
```

The final implementation receipt records the complete workspace, Clippy, formatting, harness, ADR and secret-scan results. Source checks, native invocation, OS interception and observed effects remain separate evidence.

## Linux replay and catalogue review

The existing [legacy replay](../linux-finite-native-mcp-2026-10-17/README.md#reproduce-without-a-model) supplies the pinned native SDK, compiled current-source client/helper/guard/namespace programs, compiler, private image/cache and the frozen six public packets. It does not pull images or install software. Use the same isolated loopback, UID mapping, read-only program mounts, 8 GiB and zero swap. Rebuild every changed Rust source and use the new `fixtures.json`; the historical overlay manifest does not describe T3.

The new canary needs `AR_MCP_PROGRAMS` and a fresh, absent `AR_CONTEXT_NATIVE_ROOT`. The native catalogue checksum is deliberately not guessed from the MCP schema: the SDK normalizes it. If `AR_CONTEXT_NATIVE_CATALOG_SHA256` is absent, the expected value is all zeroes and the canary records the first catalogue before refusing the dialogue. This run must fail and must retain its cleanup evidence. It is an observation run, not qualification.

Inspect `evidence/observed-native-catalog.json`, its computed checksum and the source/program receipts. Review all discovery wrappers, finite namespace names, parameters and defaults. Supply the reviewed observed checksum explicitly and run again into another fresh output directory. Never set the expected hash automatically from a failing request.

Inside the already qualified lab, using the current-source test binary:

```sh
AR_CONTEXT_NATIVE_ROOT="$NEW_EXCLUSIVE_OUTPUT" \
AR_MCP_PROGRAMS="$PINNED_CURRENT_PROGRAMS" \
AR_CONTEXT_NATIVE_CATALOG_SHA256="$REVIEWED_OBSERVED_NATIVE_CATALOG_SHA256" \
"$CURRENT_ROUTER_RUNTIME_TEST_BINARY" \
  native_context_windows_and_recall_without_model --ignored --nocapture --test-threads=1
```

Replay `native_sdk_returns_actual_mcp_results_for_six_public_packets` with its unchanged frozen inputs/validator as well. Export both cleanup receipts, exact source/program and profile hashes, MCP and raw native catalogue bytes, terminal bindings, scratch counter result and unchanged public input hashes. A green authored component canary does not establish provider availability, model quality, cost savings or campaign approval. Runtime v4 qualification remains unexecuted until the required Linux evidence exists.
