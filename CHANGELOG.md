# Changelog

All notable changes to this project are documented in this file.

The format follows Keep a Changelog 1.1.0:
https://keepachangelog.com/en/1.1.0/

`0.0.1` is the first tagged version. It is a source tag for evaluation: nothing
is published, signed or installed by it, and it is not a final release. Public
release is still gated by license, security and native host evidence (see
[remaining acceptance](docs/remaining-acceptance.md)). The CI workflows did not
run for this tag (the GitHub Actions jobs were not started for billing
reasons); every change was verified locally on macOS.

## [Unreleased]

### Added

- 2026-10-15: add offline [evaluator qualification](docs/evaluator-qualification.md) with required-case comparisons, explicit inconclusive outcomes and retained verdict history (#174). Supplied oracle claims do not establish independent qualification or authorize adoption.
- 2026-10-15: bridge supplied physical-call usage to qualified pricing and existing accepted-revision accounting, with a separately allowlisted [portable pricing projection](docs/supplied-usage-pricing.md) (#120/#5). Incomplete totals remain unknown; no collector or provider call is added.
- 2026-10-15: reconstruct [retained M4 acceptance evidence](docs/bench/accepted-evidence-2026-10-15/README.md) using the existing calculator: three attempts, one subsequently accepted Router revision, unknown monetary totals. No trial is rerun and no savings are inferred.

### Fixed

- 2026-10-15: bound concurrent startup of the four synthetic finite-client resource-probe fixtures with a shared test slot. Require the malformed-protocol fixture to prove helper execution so a startup timeout cannot masquerade as protocol rejection. Preserve the production three-second probe deadline and resource limits.

- 2026-10-15: require [writer exclusion before private host-state disposal](docs/adr/0033-host-state-writer-exclusion.md). Retain exposed homes/program copies on ambiguous cleanup or async abandonment, reject group-only evidence, and keep filesystem obligations quarantined across restart. HostState on macOS now refuses group-only cleanup; 8 GiB and F12a NO_GO remain unchanged.

- 2026-10-13: bind [private host-state cleanup](docs/bench/host-state-cleanup-2026-10-13.md) to retained home/store identities. Refuse substituted or missing paths and changed modes, avoid Drop retries after explicit failure, and preserve independent program-copy cleanup. Keep 8 GiB and F12a NO_GO.

- 2026-10-11: bind [producer-owned finite-client startup](docs/bench/linux-owned-startup-2026-10-11/README.md) before review, including private installation state and SQLite location. Reject changed startup identities and prior owned v2/v3 proofs; preserve the 8 GiB limit and production admission refusal.

- 2026-10-11: integrate the accepted M4 report-cost correction. Invalid attempts retain their list/product costs in totals and cost-per-accepted-change ratios; unknown costs remain unknown. Add four regressions and record [the subsequent acceptance](docs/bench/m4-fix-integration-2026-10-11.md), preserving frozen trial evidence and its measurement limits.

### Documentation

- 2026-10-15: add a [shared autonomous handoff template](assets/prompts/autonomous-handoff.md) and extend the development handoff skill with resume revalidation of revisions, concurrent edits, evidence and current authority. Cover bounded rendering and changed-material recovery refusal; no provider trial or host activation is implied.

- 2026-10-11: add [copied-client Linux qualification](docs/bench/linux-copied-client-2026-10-11/README.md) using the existing producer, pinned real Codex, Landlock/seccomp and an external namespace owner at unchanged 8 GiB. Preserve protected-copy, broker and detached-child observations, lifecycle receipts and negative evidence tests; production broker-only execution remains refused and F12a stays NO_GO.

- 2026-10-11: test the [real resource helper in Linux isolation](docs/bench/linux-helper-memory-2026-10-11/README.md) with the unchanged 8 GiB bound. Retain an offline locked build, direct/fork/exec allocation refusal, four supervised cleanup cases and negative evidence tests. Product sources and native admission remain unchanged; F12a stays NO_GO.

- 2026-10-11: retain the [strict 8 GiB startup follow-up](docs/bench/macos-strict-memory-startup-2026-10-11/README.md). Baseline/private/avoid/baseline loader controls all refuse the exact bound. The measured no-access region matches pinned XNU GPU-carveout geometry; installed-kernel attribution remains UNKNOWN. No bound, production environment or admission rule is changed.

- 2026-10-11: test a [macOS physical-footprint observer](docs/bench/macos-memory-protection-2026-10-11/README.md) on one inert child. Five observer cases meet their oracles; the self-only physical-limit API is refused. Preserve measured overshoot, the initial collector failure, unchanged production defaults and F12a NO_GO. Propose the distinct monitoring contract in [ADR 0031](docs/adr/0031-macos-physical-memory-observation.md).

- 2026-10-11: publish a [minimal macOS memory diagnostic](docs/bench/macos-memory-followup-2026-10-11/README.md). Both a C fixture and the unchanged product helper refuse the exact 8 GiB address-space request outside the outer sandbox. Keep virtual reservations distinct from resident RAM; retain NO_GO and the unresolved memory-contract decision. Refresh both README status summaries and the execution plan.

- 2026-10-11: publish the [approved M4 v2 result](docs/bench/measurement-m4-v2-result-2026-10-11/README.md): all three single-response configurations complete with coherent host counters, totaling 156,434 tokens of the 300,000-token envelope. Router and Luna fixed produce identical code and each pass four public cases plus 17 existing tests; Sol fixed is rejected because its exact patch is inapplicable. Preserve all attempts, exact patch bytes and usage evidence. Human acceptance is pending; no savings, native qualification or production fix is claimed. The earlier proposal entry below is historical.

- 2026-10-11: publish the [M4 stop report](docs/bench/measurement-m4-result-2026-10-10/README.md): 279,563 host tokens spent on coordination, no worker assignment started, and no savings demonstrated. Prepare [M4 v2](docs/bench/measurement-m4-v2/README.md) as three single-response patch attempts with local verification, a proposed 300,000-token envelope and a tested offline admission ledger. Approval is pending; no v2 model call is launched.

- 2026-10-11: explain skill suggestions in both READMEs and the [skill selection guide](docs/skill-selection.md): explicit caller, metadata search, catalog/host/model compatibility, abstention and separate body preparation. Add Mermaid flowcharts and a synthetic offline CLI example; distinguish v1/v2 route retrieval from contextual v3 search and personal hooks.

- 2026-10-10: seal the [M4 assisted diagnostic proposal](docs/bench/measurement-m4/README.md) after #158 merged: one reproducible report-cost defect, three fixed configurations and a 600,000 host-token admission envelope. Record that Router selects the lightweight baseline, that the threshold is not an enforced in-flight cap, and that approval and live trace binding remain pending. No trial or task solution is launched.

- 2026-10-10: record the [M1–M3 measurement preparation](docs/bench/measurement-m123/README.md) after #157 merged. Preserve the distinction between assisted Codex comparison, native qualification and measured savings; real trials still require a sealed manifest and budget approval.

- 2026-10-10: retain the [B1b self-dogfood trial](docs/bench/self-dogfood-b1b-2026-10-10/README.md), including the sealed advisory routing inputs, requested Codex worker configuration, review corrections and local measurement limits. The unchanged catalog exposes only one eligible delegation model; no comparative savings or native execution qualification is claimed.

- 2026-10-08: document opt-in observation v2 in both README entry points, the guide, ADR 0025, JSON contracts and the current test plan. Keep historical v1 verification separate from the new per-attempt projection and retain the native admission and measurement limits.

- 2026-10-08: refresh the [current status](docs/bench/status-and-action-plan-2026-10-05.md) and [execution order](docs/bench/finalization-plan-2026-10-07.md#ordre-dexécution-à-partir-du-8-octobre) after PR #139 merged. Separate delivered observation and local diagnostics from the next dedicated Codex client qualification, parallel measurement work, useful-patch trial and later comparison. Link the prepared personal task; preserve local test scope, blocked remote CI and the remaining real-provider admission requirements.

- 2026-10-07: add the [passive observation guide](docs/run-observation.md) and [ADR 0025](docs/adr/0025-run-observation.md): F04a reads one bounded, redacted snapshot of an existing runtime store; native fields remain unknown, and F04b and the full timeline stay open. Index the [F03a diagnostic](docs/bench/physical-send-feasibility-2026-10-07/README.md), retaining its overall NO-GO and timeout limitation; reconcile the six issue updates without closing them.

- 2026-10-07: add the [finalization plan](docs/bench/finalization-plan-2026-10-07.md) with dependency-aware delivery units, personal-trial measurements, release gates and a disposition for every open issue. Link the current plan from both READMEs, documentation indexes and the bench; no native activation or publication is implied.

- 2026-10-07: retain the [two-pair native build investigation](docs/bench/reproducibility-investigation-2026-10-07.md). Four historical-source builds match exactly; the original mismatch remains unexplained and no reproducibility fix is claimed.

- 2026-10-07: record [ADR 0023](docs/adr/0023-owned-task-entry.md), the early NO-GO for unmodified direct app-server and ACP under the physical-send contract, pinned schema/source evidence and an independent broker diagnostic frontier. No native activation or complete behavioral parity is claimed.

- 2026-10-07: document pinned Git Cargo installation and the complete two-binary Homebrew channel, standalone fixture smoke checks, scoped removal and [current distribution qualification](docs/bench/distribution-qualification-2026-10-07.md). The earlier core-only trial remains historical; no public tap, crates.io package or release asset is announced.

- 2026-10-07: add the [experimental offline onboarding](docs/experimental-release.md), with a reproducible routing example, explicit synthetic-fixture provenance and separate host-integration limits. The source-available candidate retains FSL-1.1-ALv2; this documentation does not publish the repository or establish daily-use savings.

- 2026-10-05: add a calibration v2 preparation protocol and incomplete manifest template, linked from the documentation indexes and bench playbook. No real campaign is authorized by these documents.

### Breaking

- Binary recipe, observations and run report advance to v2 with both `agent-router` and `router-sandbox-helper` hashes. Release attestation uses `agent-router-release-v2` and its matching signature domain, requires exact CLI features `claude` and `codex`, and refuses old v1 or single-binary artifacts. Rebuild and externally re-sign rather than relabel an old archive.

- The local Homebrew formula installs both binaries, refuses OS/CPU mismatches and tests synthetic route/explain plus helper argument refusal. The release verifier can render an exact canonical GitHub HTTPS archive URL without requesting or publishing that asset.

- `feedback report` now emits schema version 3 with per-run evidence and attempt cost coverage; `feedback summary` emits version 2. Historical report versions 1/2 and summary version 1 remain valid. Hash domains follow these output versions. Runtime and external native evidence remain explicitly unknown.

### Fixed

- Verified patch publication refuses repositories that could fetch missing Git objects implicitly, pre-existing symbolic result refs and unsupported or oversized patch inputs before applying them. Publication rechecks current capability and configuration bindings at its effect boundary; the result remains a dedicated Git revision, not a user-file rewrite or human acceptance.

- The local app-server diagnostic verifies descendant termination after signalling its process group. Closed output pipes and an exited leader no longer suffice; Linux live or unreadable descendants retain a bounded cleanup failure.

- The local API's test-only server helper now shuts down and joins its accept loop when the client panics, then propagates the original panic. A bounded regression test covers the previously stranded test loop; production request handling is unchanged.

- Binary packaging persists a private JSON report for each completed build before comparing hashes. The qualification retains an initial refused mismatch and a later matching pair; the initial cause remains `UNKNOWN`, with no general reproducibility fix claimed.

- Local release attestation accepts real multiline Cargo/Rust compiler version observations while validating the complete bounded output and retaining its hash. The manifest keeps a single-line version label. Regression tests preserve refusal of private paths and invalid controls; the [release qualification](docs/bench/release-qualification-2026-10-07.md) records the real-archive result and Homebrew limit.

- Feedback pricing retains distinct recorded attempts while deduplicating their snapshots. Missing ordinals, ambiguous legacy retries or conflicting usage keep the complete total unknown, with a separate known subtotal. This does not establish complete task cost across escalations or dispatches without feedback.

- The native integration diagnostic now counts refused HTTP requests and connections without a request. Its negative control requires the exact expected RPC error and zero provider contacts; historical probe results remain available.

- Codex execution usage preserves a declared `cache_write_input_tokens` count, including zero. Missing, null or invalid counts stay unknown; input totals are not incremented again.

### Breaking

- Non-unknown host capability claims now require an authenticated proof verifier. A handwritten `host_runtime` source no longer passes shared snapshot validation. Proof-consuming phase receipts use schema 5 and include compact evidence metadata; exported replay without its proof store reports `unverified` and grants no execution authority.

- Native Codex and Claude execute a verified private copy of the approved entry file per attempt, outside writable host state. Approval format v7 requires a new preview; other adapters, interpreters, modules and sandbox/helper binaries retain their existing boundaries.

- Native Codex configuration rejects shebang launchers such as `codex.js` with `runtime.program.script_unsupported` (exit 2), on macOS and Linux. Configure the canonical native binary path; no Node lookup or PATH expansion is performed.

- The macOS Codex sandbox reads the `com.openai.codex` preference domain and shared memory named `apple.cfprefs.*`. Approval format v6 binds this permission change; v1-v5 approvals require a new preview. No preference writes or Linux profile change is added.

- Native Codex and Claude runs use disposable private host state. Configured `CODEX_HOME` and `CLAUDE_CONFIG_DIR` are refused; OAuth state is never copied. Approval format v5 requires a fresh preview for the API-key-only bootstrap and host-state grants.

- **BREAKING:** `prepare` no longer reports `prepare.invalid_or_denied` or `prepare.final_input.invalid_or_denied`. The assembly step returns one fixed code per failure class: `prepare.decision.authority_unavailable`, `prepare.decision.authority_required`, `prepare.decision.context`, `prepare.decision.abstained`, `prepare.decision.stale`, `prepare.decision.serialization` (exit 4), `prepare.workflow.invalid`, `prepare.route.invalid`, `prepare.retrieval.invalid`, `prepare.adapter.invalid`, `prepare.snapshot.invalid_or_stale`, `prepare.route.abstained`, `prepare.binding.mismatch`, `prepare.input.limit` and `prepare.encoding.json` (exit 4). All other codes exit 2. Scripts that matched the old codes must match the new list, which is in `docs/cli-output.md`.
- **BREAKING:** The Claude stream probe proves the model from root assistant `message.model` values only. The `init` model is a label: it is no longer required among observed models and never enters `supported_models`. A stream whose `init` announces an alias while the assistant message carries a dated id is now accepted and lists only the dated id; a stream with no root assistant model still fails with `model not observed`. No alias or date suffix is resolved, so a candidate must name the dated id. A consumer must not assume `init` and `supported_models` agree.

### Added

- 2026-10-11: accept complete v3 observations in `eval attempt-correlation`, validating bounded timing rows and their attempt/event bindings before the unchanged B1 projection. Keep v2 support, explicit unknowns and the original v3 export; do not add a duration aggregate or infer provider usage. See the [correlation guide](docs/attempt-correlation.md).

- 2026-10-11: add a reviewed-resource preflight for the owned finite-client helper copy, preserving exact limits and source/copy freshness checks. Publish [the F12a resource stop](docs/bench/finite-client-qualification-2026-10-11/README.md) and [independent macOS component evidence](docs/bench/macos-component-qualification-2026-10-11/README.md); the 8 GiB request remains refused and native admission stays NO_GO. Test historical M3 fixtures against their declared Git base while retaining current report regressions and the initial failed run.

- 2026-10-11: retain reviewed finite-client asset copies and identity-bound preparation previews (PR #163, ADR 0029). Independent post-fix review and local macOS/Linux checks support preparation-only integration; copied native launch composition remains F12a NO_GO. See the [review receipt](docs/bench/verification-finite-client-assets-review-2026-10-11.json).

- 2026-10-11: add opt-in `observe --schema-version 3` with qualified local supervisor capture durations. Persist a bounded timing record atomically with its exact attempt evidence; missing legacy records remain unknown. Preserve v1/v2 formats and native/provider duration unknowns. See [ADR 0030](docs/adr/0030-qualified-local-capture-timing.md); broader phase, cleanup and end-to-end timing under #117 remains open.

- 2026-10-10: add [measurement preparation M1–M3](docs/bench/measurement-m123/README.md): a separate Codex campaign catalog, a bounded importer for retained host usage exports, and an 18-assignment fake-only comparison rehearsal using the existing accepted-change calculator. Register their Python suites in the local quality runner. Costs and overlapping usage remain unknown; no real campaign or native authority is enabled.

- 2026-10-10: add `eval attempt-correlation` with direct B1 input and complete `observe --schema-version 2 --json` projection modes. Bounded strict inputs retain missing physical claims as unknown, refuse identity collisions and keep the report `supplied_unverified`. No provider, runtime-store access, usage collection or execution-authority change. See the [CLI guide](docs/attempt-correlation.md).

- 2026-10-10: add `router_feedback::attempt_correlation` v1, an offline computation over supplied evidence (#120, F06a minimal correlation). It joins v2 attempt keys to physical observation keys `(authority, physical)` and refuses conflicting observations of one key, claims to absent attempts, physical keys under the runtime authority and attempt shapes v2 never emits. A repeated identical observation counts once; distinct keys sharing an external request id stay two calls. An attempt with no claimed call reports `physical_sends: unknown`, never zero. The report is `supplied_unverified`: snapshot, runtime authority and revision are not re-verified, usage, duration, thread and turn are not joined, and `observed_tokens`/`observed_cost_microusd` are not read. No runtime, `router-types`, journal or schema change; the CLI command and observation projection were deferred to the B1b entry above. Design notes: [measurement binding contract](docs/superpowers/plans/2026-10-10-measurement-binding-contract.md). The secret scan excludes only the exact synthetic run and attempt digest values.

- 2026-10-10: the loopback provider tests check the meaning of the request the fake upstream received, for OpenAI Responses and Anthropic Messages (#152). Model, effort, token ceiling (distinct from `max_output_bytes`), streaming, storage, role, exact root keys and a hand-written multi-line Unicode prompt with one instruction are compared with the authorized request. A verifier test corrupts each covered field of a really received body and expects that field to be named. See [received request semantics](docs/provider-transport.md#received-request-semantics). Native Codex client compatibility and real provider behavior are not verified; images and conversation state are not representable in `ExecutionRequest`.

- `observe --schema-version 2` adds bounded per-attempt identities, requested model/effort, supervisor termination/exit code and source hashes from the existing passive snapshot. Missing outcomes stay visible; incomplete capture preserves both output-limit and reader-deadline causes. Default and explicit v1 output remain byte-compatible. Native execution, usage, duration and objective remain unknown; the published schema supports both versions.

- Add an explicit native supervision diagnostic that records broker-only refusal, finite-input behavior and OS/resource limits without a model turn. Reconcile its cleanup tests with the stronger publication-hardening stop contract; F12a remains NO_GO. See the [diagnostic and limits](docs/bench/owned-supervision-feasibility-2026-10-08/README.md).


- `artifacts review-publication`, `check-publication-review`, `publish` and `publication-status` expose approval-bound verified patch revisions through the existing runtime. The simulated useful correction and one-shot publication are described in [ADR 0027](docs/adr/0027-verified-patch-publication.md). Native composition, direct multi-file working-copy safety and real-provider admission remain separate qualifications.

- Dedicated Codex client diagnostic with task/thread/turn/model/effort binding, strict bounded JSON, refusal of permission callbacks and reroutes, broker request validation and controlled cancellation. Three local native scenarios qualify the simulated entry under [ADR 0026](docs/adr/0026-owned-client-simulated-entry.md); full ACP comparison, real-provider and OS admission remain open. The local runner now includes the client, physical-send and startup-outage unit suites without implicitly launching native Codex.

- `schema run-observation` publishes the existing v1 passive observation shape, including explicit unknown fields and finite bounds. The shared schema/version registry lists it; real CLI output and hostile mutations exercise the contract. No runtime output, native evidence or execution authority changes.

- `observe --store ROOT --run-id ID [--json]` reads a coherent passive run projection from an existing runtime store. Human output shows the last 20 state/sequence references; JSON keeps the complete bounded snapshot. Missing stores are not created; no recovery, migration, execution or permit release occurs, although SQLite may coordinate WAL/SHM. Native model/effort, outcomes, timing and liveness remain unknown.

- `eval accepted-changes --input FILE --json` adds a standalone offline schema for exact revision acceptance, revocation and cost frontiers. All assignments and physical work remain visible, duplicate observations count once, and incomplete costs retain null totals. Existing feedback formats and stores are unchanged; supplied evidence is not authenticated and no native usage gain is claimed.

- Explicit local `capability attest` and `capability revoke` commands authenticate operator declarations without executing a host. Snapshot schema 2 keeps these references separate from UNKNOWN capabilities; policy schema 3 requires explicit capability opt-in and consuming receipts use schema 6. Network attestation remains refused pending provider-egress admission. The capability store migrates atomically only on explicit issuance, and legacy wire formats remain compatible.

- ADR 0021 batch 6 adds bounded, append-only MAC capability records in the private runtime store. `inspect`, `route-phase`, catalog/asset and retrieval commands accept `--store` with `--execution-config`; generated-plan nodes take the equivalent manifest pair and coordinator waves validate each step against their private runtime proof store; readers never create the proof database or launch a host. Runtime dispatch rechecks live proof validity after version probing, while historical reopen uses its recorded decision clock and sequence horizon. Synthetic fixtures validate the plumbing; this adds no production canary issuer and establishes no real host capability.

- `assets seal --kind contracts|availability --document FILE (--catalog FILE | --host-snapshot FILE) --json` prints a sealed contracts or availability document and its `content_hash`. It writes nothing, edits no manifest and approves nothing: the operator pins the hash independently in the asset-authority manifest. New codes: `assets.seal.input_required`, `assets.seal.document_invalid`.
- `agent-router feedback summary`: one global total over every group of `feedback report` (same snapshot and classification), with the accepted rate and its Jeffreys interval, computed cost and cost per accepted change (null with a reason while any run is unpriced), time to acceptance p50 and p90, UNKNOWN counts and `attribution: not_established`. Human view by default, `--json` for the versioned result.
- `agent-router schema <name>` and `schema --list`: hand-written JSON Schemas (draft 2020-12) embedded in the binary for `route-decision`, `feedback-report`, `feedback-summary`, `error` and `version`. No schema library was added.
- `agent-router version --json`: crate version, target triple, enabled features and the `schema_version` values supported per output contract. `--version` is unchanged.
- `agent-router completions <bash|zsh|fish>`: completion scripts generated with `clap_complete` 4.6.9 (clap project, `MIT OR Apache-2.0`, no dependency beyond `clap`), recorded in the license inventory produced by `scripts/package-core.py`.

## [0.0.1] - 2026-09-30

### Breaking

- **BREAKING:** The CLI exits with distinct classes instead of folding most failures into 2: 2 invalid input or refusal (default), 3 valid but not final (unchanged), 4 internal (`route.internal`, `*.storage`, `*.json`, including an unreadable host output line), 5 transient contention (`*.busy`, `*.locked`; SQLite BUSY or LOCKED now surface as `runtime.busy` or `coordinator.busy`), 6 outcome unknown, inspect first (`backend_store.published_durability_unknown`, `runtime.cleanup.unknown`, `runtime.approval_key.published_durability_unknown`), 7 integrity (`*.integrity`, `*.storage_corrupt`, `policy.store.invalid`, `feedback.policy.signature`, `assets.review.trust_mismatch`, `backend_store.anchor`, `feedback.store.invalid`), 130 interrupted. Scripts must handle 4 to 7: retry on 5, inspect before retrying on 6, stop and investigate on 7. The table is in `docs/cli-output.md` and `docs/reference/json-contracts.md`. (#75)
- **BREAKING:** Approval digests move to v4 and bind the values of the environment passed to the host: `HOME`, `CODEX_HOME`, `CLAUDE_CONFIG_DIR` and `SSL_CERT_FILE` by value, every other allowed name (API keys included) by HMAC-SHA256 with a random 32-byte key kept in the runtime store (`approval.key`, created once, mode 0600, never replaced). Changing a bound value after approval refuses the launch. Approvals of versions 1 to 3 are no longer valid, and deleting or replacing `approval.key` voids pending approvals: preview again and approve the new digest. (#75)
- **BREAKING:** `explain` refuses a decision whose `schema_version` is not `"1"` (`decision.schema.unsupported`); an execution definition naming both a policy file and a policy store is refused (`policy.composition.denied`) instead of ignoring the store; an empty or oversized trusted key set reports `feedback.invalid` instead of `feedback.policy.signature`; library codes are aligned with the CLI (`runtime.cleanup.unknown`, `coordinator.json`); a second `agent-router serve` on a locked state directory exits 5 instead of 2. (#75)
- **BREAKING:** The runtime doctor report (`doctor --runtime-root`) is `schema_version` 2. Over the 4 MiB inspection cap it returns a partial report with `overall` unknown and exit 3, instead of `doctor.input.limit` with exit 2 and no report: `partial_reason: database_exceeds_inspection_limit`, `schema_layout: not_inspected`, `journal_integrity: unknown`, and `database`, `storage_schema_version`, `counts` and `leases` null. A complete report keeps every v1 field with the same meaning; a v1 consumer must accept the nullable fields and new values. (#78)
- **BREAKING:** With `--json` anywhere before `--`, every error, parse errors included, is one line on stderr, `{"schema_version":1,"error":"<code>"}`, with the same exit status; scripts that parsed `error: <code>` under `--json` must parse JSON. The local API error body uses the same field order (it sorted keys before). (#82)
- **BREAKING:** `run --retry` is a read-only preview that persists nothing and prints the digest to approve; `run --retry --approve <digest>` performs the retry and refuses with no write if the run changed since the preview. A plain `run --approve` on a failed or timed-out run is refused with `runtime.retry.use_retry_approve`. (#67)
- **BREAKING:** Every command exits 4 with `route.internal` when writing its result to stdout fails (for example `| head`), instead of panicking with exit 101. (#67)
- **BREAKING:** The unsigned catalog document, `ModelSpec`, `SkillSpec`, `RouteRequest`, `TaskContext`, `HostSnapshot`, `SkillCandidate`, `EffectiveRestrictions` and the BM25 candidate document reject unknown keys (`deny_unknown_fields`); a per-candidate `path` stays accepted and ignored. Unsigned catalogs also apply the signed label and text rules (ids in `[A-Za-z0-9-_.:/]`, no control or bidirectional characters, ids and version up to 128 bytes, source 512, description 8192). (#64)
- **BREAKING:** `public_source` refuses loopback, private, link-local, `0/8`, `100.64/10`, multicast, `localhost` and `*.local` literal hosts, integer or hexadecimal address spellings, and bare identifiers that do not start and end with a letter or digit; signed manifests that used such a source must be re-sealed and re-signed. DNS names resolving to private ranges are not detected. Importer report paths escape control and bidirectional characters and backslashes. (#64)
- **BREAKING:** Signed catalogs whose provenance source is `owner/repo`, `scheme:id` or a local path are invalid. A source must be a lowercase `https://` URL without credentials, query, fragment or dot segments, or a bare identifier without `/` or `:`. Re-seal and re-sign a catalog with a refused source. (#46)
- **BREAKING:** The catalog store prunes history behind a sealed `anchor` instead of dead-ending at 128 events (ADR 0017). A checkpoint older than the retained window, genesis included once pruned, is refused with `Conflict`, so an operator must advance the external checkpoint at least once per 128 changes. `StoreError::Io` and `StoreError::Path` carry the operation and errno. (#63)
- **BREAKING:** Promotion blocks a regression on any metric, even one the criteria do not list; only an explicit `maximum_regression_micros` waives it. (#68)
- **BREAKING:** Execution scopes with an empty, `.` or `..` segment are refused with `Invalid("scope")`, and a workflow whose summed producer output bounds exceed the consumer's `max_input_bytes` is refused with `Invalid("input bytes")`. (#62)
- **BREAKING:** The runtime admission registry moves to schema version 3 through an explicit migration; an older binary on a v3 registry fails closed. A persisted `writer_locks` row containing `..` makes every new writer lock fail with `Integrity("writer lock workspace")`. (#65)
- **BREAKING:** New serialized `RunFailure::Unclassified` (`unclassified`), non-retryable and mapped to `Unknown` feedback; an older binary cannot read it. (#66)

### Added

Routing, policy and catalogs

- Validated routing and offline evaluation foundation: passive host probing with explicit snapshots, provenance-bearing catalogs and aliases, local BM25 metadata retrieval, immutable phase routing with receipts, and replay and shadow evaluation. (#14)
- Contextual retrieval and evidence-aware task handling, with risk floors and human-readable CLI output. (#20)
- Validation of generated plans and an Omnigent routing bridge. (#21)
- Approved standalone escalation from the CLI. (#22)
- Escalation of coordinated workflow attempts in the runtime. (#25)
- Persistent storage of approved current signed catalogs. (#23)
- Enforcement of signed catalog constraints before scoring. (#27)
- Revalidation of signed catalog authority around preparation assembly. (#31)
- Inert importer that turns a local directory of Markdown definitions into an unsigned candidate catalog manifest. Library only: it signs nothing, writes nothing to the catalog store and activates nothing. (#47)
- Local ranking from caller-supplied vectors. (#51)
- Opt-in advisory decision cache in a new `router-advisory-cache` library. Library only, no CLI wiring: it stores authenticated backend proposals and never becomes authority. (#50)
- Review-edge restriction to distinct declared providers, as a pure function in `router-policy`. Library only. (#53)
- `public_source` is public in `router-catalog`, so the importer shares the signed verifier's provenance check. Retrieval queries accept at most 8 distinct quoted phrases (`Limit("query phrases")`). (#64)
- `StoreError::PublishedDurabilityUnknown` in the catalog store and `backend_store.published_durability_unknown` in the backend store, returned when the directory fsync fails after the publishing rename; do not retry blindly. The catalog store `create` resumes after an interruption. (#63, #68)

Execution, workflows and runtime

- Approved isolated execution with private artifacts. (#15)
- Coordination of approved workflow steps with process cleanup proof. (#16)
- Execution of signed backend decisions over pinned optional host transports. (#19)
- Model-proposed plan review without approval. Library only, no CLI entry point. (#55)
- Typed structured validation contracts in a new `router-validation` crate. Library only. (#52)
- Typed multimodal input and JSON output contracts in `router-asset-contracts`. (#56)
- Inert validation of adapter descriptors in `router-adapter-api`; `activation` must be `none`. (#54)
- Supervised loopback provider transport in `router-bridge-contract`, behind the default-off `loopback-transport` feature ([ADR 0014](docs/adr/0014-loopback-provider-transport.md)). Library only: not wired into the CLI or the runtime, no TLS, no credential broker, no real provider canary; supported on single-user hosts only. (#58, #71)
- `probe_dns_resolution()` for the Seatbelt sandbox, run only on explicit call; `network:false` is documented as not proving the absence of DNS or LaunchServices channels. Bubblewrap remounts a `.git` at a writable scope root read-only. (#66)
- Journal events `attempt.lease_orphan_reclaimed` and `admission.release_failed`. (#73)
- `runtime.workspace.nested_git`: a produced worktree with any `.git` entry other than the root's own is refused before the first host `git` command. (#79)

CLI and integrations

- Route-only local API, available through `agent-router serve` only when built with `--features local-api`. It cannot execute, record feedback, activate a backend or read the catalog store. (#48)
- Read-only `doctor` inspection of coherent owned SQLite snapshots. (#26)
- Standard sidecar client with bounded ranking and consented exports. Optional, absent from the default build. (#28)
- Cross-host Rust project tooling for Codex and Claude Code development. (#13)
- `agent-router eval tradeoffs` compares cohort quality and cost. Descriptive only. (#44)
- Integration candidate combining guided task intake and export, `doctor` inspection of runtime storage without recovery, shared persistent admission quotas, shared conservative session budgets, cohort cost budget and calibration metrics, publisher policy read guards and reproducible local native binary exports. (#43)
- `strict_json_limited(bytes, limit)` in `router-adapter-api`; `audit verify` accepts a valid 5 MiB bundle. A CI step checks that no `local-api` command exists in the default build. (#67)
- `DoctorError::DatabaseTooLarge` (`doctor.database.too_large`). (#78)
- Usable CLI: a usage error keeps `error: cli.invalid` as its first line, then the parser's indented explanation and `Run 'agent-router <command> --help' for usage.`; every subcommand and common option has a help text, with examples for `route`, `explain`, `plan`, `run` and `doctor`; the human view starts with a summary (status, selection or abstention reason, approval digest) and groups hashes at the end. `--json` output is unchanged. In text mode the explanation quotes the offending argument as typed. (#82)

Release and supply chain

- Verifiable local release recipe: `scripts/release-local.py` binds source and binary archives to a manifest, verifies an externally produced signature and renders a Homebrew formula. It never generates a key, signs, publishes or installs. (#49)
- CI verification of complete dependency license evidence for distribution exports. (#29)
- The workspace version is `0.0.1`, set once in `[workspace.package]` and inherited by every crate.

### Changed

- Performance: the runtime owner verifies only the new tail of its append-only event chain instead of replaying it on every register, plan-pin check and escalation (ADR 0018); on one macOS machine, release build, 100 `verify_plan_pin` calls at 100 000 events went from 11.5 s to 2.4 ms, then 1.2 ms in a second measurement. Opening the store, observers and `verify_integrity` still replay the whole chain, and a full replay must extend the remembered trust point. A rewrite of an old row by a same-UID process during the owner's lifetime is found at the next open or `verify_integrity`. (#81)
- Performance, coordinator: a sealed workflow definition is validated once per wave instead of on every 20 ms tick (55 validations during a one-second wave before, 1 after); `request_control` no longer holds the coordinator write lock while it observes the runtime; cancellation visits every run and returns the first error. (#76)
- Performance, shared admission: waiting tickets compare a cached policy fingerprint, decide on a read transaction first and back off 20 to 100 ms; a hand-off between waiters or a cancellation can be observed up to 100 ms plus one poll later (20 ms before). (#77)
- Backend store: `review` refuses what `apply` would refuse, and every activation keeps room for the rollback that must revoke it; the snapshot format is unchanged. (#74)
- Runtime admission retention: released tickets beyond the 512 most recent are pruned inside `enqueue`, committed reservations are folded into per-session totals first, and `MAX_TICKETS` bounds live tickets. The budget event journal keeps its lifetime cap of 16384. (#65)
- Runtime output handling: an exited process whose output reader did not finish ends `Failed`/`unclassified`, is not decoded, and its retained artifacts are marked `truncated`. (#66)
- Backend store promotions are not pruned; the promotion cap is 64 and the lifetime cap of 128 events remains until a signed checkpoint compaction (deferred, ADR 0017). (#68)
- Routing: a definite `Unsupported` capability is no longer hidden by an earlier `Unknown`; the below-execute exclusion allocates much less on large pools. (#62)
- Refactor with no intended behavior change: shared helpers replace duplicated code in stream parsers, digest checks, bidi checks and event lookups. (#70)
- Documentation: README sections fold into `<details>`, with infographics, a French `README.fr.md`, a "Why it exists" section and diagram, a full bilingual documentation index (`docs/README.md`, `docs/README.fr.md`), a French installation guide, a review grid for contributors, exit-class references in the guides, and ADR status lines aligned with the code. (#59, #60, #69, #71, #80)
- README redesigned, with a reproducible routing demo. (#18)
- Documentation shows the delivered sidecar, signed preparation and dependency gates. (#33)

### Fixed

- A signed catalog (8 MiB bound) or backend promotion document (16 MiB bound) larger than 4 MiB was rejected as `Invalid`, and a sealed catalog over 4 MiB could not be read back. (#72)
- Runtime state: an orphan lease file left by a crash no longer blocks a retry forever; `record_verification` is atomic and a conclusive record is never replaced; a failed admission release no longer discards the run outcome; a long-lived `Runtime` no longer hits `Limit` after 1024 standalone runs; recovery order is deterministic. The release-failure path has no test. (#73)
- Sandboxes: Seatbelt refuses a working directory or write path reached through a symbolic link, as Bubblewrap already did. (#79)
- Host adapter and loopback stream decoders decide a truncated tail by position (`bad\nbad` is a protocol error), and a stream ending in a space or a lone CR is not complete. (#61, #62)
- Loopback transport: linear redaction scan that observes the deadline and cancellation, bounded frames (256 KiB) and paths, provider failure events become `TransportError::ProviderFailed`, socket errors before any byte are `Io`, IPv6 scope ids refused. Decision client: a legacy entropy `confidence` no longer stands in for the answer probability, `Content-Length` takes digits only. (#61)
- Contracts: `ChangedPathsWithinScope` no longer accepts extra fields, duplicate keys inside validation values are refused, and quadratic duplicate and deprecation checks are linear. (#64)
- Claude probe and transport decoding ignore subagent events, so subagent models are no longer attributed to the root worker. (#62)
- Coordinator controls: a re-requested rejected control replays full validation; terminal steps settle pending controls and release material; one failing run no longer aborts the others. (#62)
- Runtime store: a refused budget dispatch is refunded, cancel is idempotent, writer locks compare normalized paths, storage diagnostics no longer refuse healthy stores. (#65)
- Process supervision: truncated output can no longer pass a check or a run, adapter classification errors no longer blame the model, `git diff --no-renames` scope-checks a rename's source path. (#66)
- CLI: `explain` and human output escape terminal control and format characters; documented read bounds are reachable. (#67)
- Evaluation and feedback: a full advisory cache sweeps expired entries instead of refusing forever; only unknown pricing yields an unknown cost; a reentrant export lock returns `Locked` instead of deadlocking. (#68)
- `xtask generate` recreates a missing owned file; a stale `fuzz/Cargo.lock` is refreshed. (#61, #62)
- Preparation authority and quarantine are retained until cleanup is proven, and probe cancellation is preserved during final output collection. (#30)
- Two environment-dependent workspace test failures. (#45)
- Test-only: stabilized historical expiry setup and native version cancellation diagnostics in runtime tests. (#17, #24)

### Security

- Environment-bound approvals: a displayed approval digest no longer lets anyone confirm a guessed API key offline. (#75)
- Nested `.git` written by a worker under Bubblewrap no longer reaches the host's `git add` and `git diff`, which run outside the sandbox; under Seatbelt a detached descendant is not proven terminated before those commands (documented). (#79)
- The event chain owner detects a self-consistent rewrite of its last verified row and a total truncation, which the previous full replay accepted. (#81)
- CI audit of locked dependencies against fresh RustSec data. (#32)

## Before the pull request workflow

The first commits landed directly on `main` before pull requests were used (2026-08-28 to 2026-08-30). They define the core design and plan, the deterministic routing contracts, catalog validation, hard routing constraints, a versioned BM25 candidate boundary, the `route` and `explain` commands, a deterministic routing fixture matrix, and bounds on routing input, catalog size and combined routing work. Individual commits are listed in the Git history.

[Unreleased]: https://github.com/FlorianBruniaux/agent-router/compare/v0.0.1...HEAD
[0.0.1]: https://github.com/FlorianBruniaux/agent-router/tree/v0.0.1
