# claude-router regression matrix (issue #143)

This document assigns each scenario T01 to T14 of issue #143 a disposition and a
concrete test. It reuses scenarios and invariants from the pinned claude-router
analysis, not its JavaScript harness, thresholds or retry policy. Opening or
merging it does not implement parent routing, cache economics or hysteresis, and
a passing offline test is not native qualification.

Base: `bb2d71a`. Test references are `path::name` from the repository root.

## Dispositions

- `covered`: the cited test fails when the stated behavior breaks.
- `covered (partial)`: only the listed part is asserted; the rest is a separate
  row in "Open rows".
- `new`: added with #143, in a `claude_router_regression.rs` test file.
- `deferred`: Agent Router has no such behavior yet; the owner decides it first.
- `not applicable`: the scenario cannot occur in the current product.

## Matrix

| Row | Disposition | Tests and evidence scope |
| --- | --- | --- |
| T01 turn, continuations, repeated state read | deferred (#124) | No parent turn, pin or vote exists in `crates/*/src`. Nothing to assert. |
| T02 pending classification vs manual or newer choice | deferred (#124) | No pending classification state exists. The decision API is synchronous and returns one `BackendDecision`. Related but not equivalent: new `t04_success_after_the_deadline_cannot_apply_a_selection`. |
| T03 session clear, resume, compact, late result | deferred (#124) | No session lifecycle for classifier results. Finite session resume is a different contract (`docs/reference/finite-application-resume.md`). |
| T04 timeout, late success, zero retry | covered (partial) and new | New `crates/router-decision/tests/claude_router_regression.rs::t04_success_after_the_deadline_cannot_apply_a_selection` (a valid answer after the deadline gives `backend.timeout`, no shadow candidate, per failure policy). Existing `crates/router-decision/tests/standard_sidecar.rs::negotiation_timeout_stops_before_rank` and `::one_deadline_covers_both_negotiation_and_rank` (request count and shared deadline), `crates/router-decision/tests/backends.rs::timeout_oversize_duplicate_json_redirect_and_bad_answer_are_rejected`. Overlapping in-flight work and cancellation do not exist: the call blocks. Cleanup evidence stays with `crates/router-runtime` tests, not this row. |
| T05 refusal, absent dependency, auth failure, timeout, native fallback | new and covered | New `...::t05_t13_failures_are_coded_sent_once_and_never_echo_the_provider_body`: 401, 403, 429, 500 give `backend.transport`, a slow answer gives `backend.timeout`, a closed port gives `backend.transport`; each peer sees exactly one connection. Observed limit: HTTP 401, 403, 429 and 500 share one code, so auth failure is not a separate class at this boundary. Existing `crates/router-decision/tests/backends.rs::non_loopback_and_header_injection_are_rejected_before_connect` (no new endpoint), `...::every_backend_failure_obeys_explicit_fallback_policy`, `crates/router-adapter-api/tests/native_failures.rs::codex_messages_and_invented_fields_never_grant_a_native_class` and `crates/adapter-claude/tests/native_failures.rs::conflicting_or_unknown_terminal_http_status_cannot_inherit_an_older_cause` for native classes. |
| T06 parent vs subagent identity | new and covered | New `crates/adapter-claude/tests/claude_router_regression.rs::t06_subagent_models_do_not_overwrite_the_root_identity` (three subagent markers, plus subagent-only stream). Existing `crates/adapter-claude/tests/usage.rs::claude_subagent_usage_is_not_attributed_to_the_root_worker`, `::only_the_root_result_counts_when_a_subagent_result_is_mixed_in`, and `crates/adapter-claude/tests/conformance.rs::probe_ignores_subagent_models_and_caps_observed_models`. Main-metric overwrite by worker activity: no such metric surface found. |
| T07 requested vs served model, unsupported effort | new and covered (partial) | New `...::t07_requested_alias_is_not_promoted_to_the_served_model` and `...::t07_mixed_served_models_stay_separate_and_effort_stays_unknown`. Existing `crates/adapter-claude/tests/conformance.rs::probe_lists_only_the_dated_assistant_model_when_init_names_an_alias` and `crates/router-runtime/tests/finite_client.rs::changed_task_scope_model_effort_or_host_refuse` (the last one binds the approved request, it does not compare with an observed model). Not covered: false cache attribution (row T09), silent stronger route, continued rewriting (parent routing, #124). |
| T08 context fit, disallowed pin, final constraints | covered (partial), context-fit deferred | Hard constraints before selection: `crates/router-policy/tests/constraints.rs::unknown_high_risk_capability_closes_route`, `crates/router-policy/tests/phases.rs::every_risk_rejects_unknown_required_permissions_and_capabilities` and `::budgets_and_independence_are_hard_constraints` (refusal when no candidate is admissible). A backend cannot name a candidate outside the validated pool: `crates/router-decision/tests/backends.rs::every_backend_failure_obeys_explicit_fallback_policy` (mode `outside`). There is no context-window fit policy or final context-fit correction; #195 exact windows are not one. Deferred with #124 and #141. |
| T09 cache evidence, missing price or usage, mixed snapshots | covered (partial), cache deferred | Unknown stays unknown: `crates/router-eval/tests/replay.rs::unknown_prices_and_models_do_not_become_zero_cost`, `crates/adapter-claude/tests/usage.rs::streams_without_usage_have_no_usage_never_zero`, `::claude_usage_is_normalized_so_input_includes_cache_reads_and_writes`, `::codex_cached_input_is_never_counted_twice_in_the_total`. No prompt-cache freshness, warmth or shared-worker cache model exists. Deferred (#141). |
| T10 distribution mass, vote threshold, history bounds | new (mass), hysteresis deferred | New `...::t10_probability_mass_other_than_one_is_refused_before_selection` (mass 3, mass 0.3 and mass 1.5 refused with `backend.schema`, mass 1 accepted) and `...::t10_impossible_distribution_never_becomes_a_shadow_selection` (through `decide_shadow`). Existing `crates/router-decision/tests/backends.rs::digest_pool_and_probability_mass_cannot_be_forged` (mass 1.6). No vote threshold, retained history or hysteresis configuration exists, so the vote-threshold probe has no Agent Router equivalent. Deferred (#5). |
| T11 two identical tool errors | covered and new | `crates/router-runtime/tests/escalation.rs::one_real_attempt_with_two_errors_is_insufficient_and_metadata_stays_private` (two errors, one attempt, `EscalationError::Attempts`, no durable second attempt). New `crates/adapter-claude/tests/claude_router_regression.rs::t11_identical_tool_errors_are_not_host_errors` (decoder level). The multi-attempt native test `native_distinct_runs_count_only_within_the_same_instance_and_node` is `#[ignore]` (needs the native helper) and is not counted. |
| T12 stale or no-route projection, narrow rendering | covered (partial) | `crates/agent-router-cli/src/human.rs` unit test `preserves_unknown_and_exact_approval_while_neutralizing_terminal_controls` (UNKNOWN, null observed model, control characters), `crates/router-runtime/tests/finite_context_presentation.rs::projection_failure_does_not_rerun_the_tool` (no second tool call from rendering). Not covered: requested route vs observation in one rendered view, narrow-width rendering, provenance of stale projections (#118, #119). |
| T13 malformed, duplicate, unknown fields, sensitive provider error, oversize | new and covered | New `...::t13_malformed_unknown_and_duplicate_answers_fail_at_the_schema_boundary`, `...::t13_foreign_candidate_text_from_the_provider_never_reaches_the_decision` and the no-echo part of the T05 test. Existing `crates/router-decision/tests/backends.rs::timeout_oversize_duplicate_json_redirect_and_bad_answer_are_rejected`, `crates/router-decision/tests/standard_sidecar.rs::rank_rejects_foreign_identity_pool_duplicate_and_rounding`, `crates/router-runtime/tests/finite_client.rs::malformed_manifest_and_oversized_input_refuse_at_review`. Limit: the sidecar envelope (`JevResponse`) does not deny unknown top-level fields; only fields inside an answer and an action are denied. Not asserted either way. |
| T14 missing, repeated, mixed observations | covered (partial) | `crates/adapter-claude/tests/usage.rs::two_claude_root_results_are_ambiguous_and_give_no_usage`, `crates/router-eval/tests/metrics.rs::duplicate_missing_extra_or_swapped_case_records_are_rejected`, `crates/router-runtime/tests/observation_v2_adversarial.rs::rehashed_duplicate_durable_evidence_is_refused`. Continuation-only replay and cohort coverage stay with #5 and #116. |

## Upstream probe provenance

The three probes concern the pinned upstream code only. They are not defects of
Agent Router.

| Probe | Agent Router expectation | Test |
| --- | --- | --- |
| Accepted probability mass 3 | Refused before selection. | `t10_probability_mass_other_than_one_is_refused_before_selection` |
| Four-vote threshold with three retained votes | No such configuration exists. A future policy must reject it. | Deferred (#5). |
| Final context-fit selection bypasses the cost gate | No final context-fit stage exists. Candidates come only from the validated pool. | Pool membership above; context-fit deferred. |

## Open rows

- Turn and manual-choice races, session reset, late results: T01, T02, T03 (#124).
- Context-fit final constraints: T08 (#124, #141).
- Cache economics and warmth: T09 (#141).
- Hysteresis and vote or history bounds: T10 (#5).
- Rendering of requested vs observed route and narrow widths: T12 (#118, #119).
- Continuation-only replay and cohort coverage: T14 (#5, #116).
- Native qualification of any row: none attempted.

## Verification method

Every new test except one was checked against a temporary source mutation,
reverted afterwards (`git status` showed only the new files):

| Test | Mutation that makes it fail |
| --- | --- |
| `t10_*` | disable the probability-mass sum check in `validate_proposal` |
| `t04_*` | remove the post-deadline check in `decide_shadow` |
| `t05_t13_failures_*` | accept any HTTP status in `response_length` |
| `t13_malformed_*` | remove `deny_unknown_fields` on `ChoiceAnswer` |
| `t06_*` | drop the root-event check on the assistant model |
| `t07_requested_alias_*` | record the init event model as an executed model |
| `t07_mixed_*` | remove `effort.runtime_unknown` from `missing_evidence` |
| `t11_*` | map a Claude `user` tool result to `HostError` |

`t13_foreign_candidate_text_...` was not mutation-checked: the out-of-pool refusal
exists twice (`validate_proposal` and the alias lookup in `decide_shadow`), so one
mutation cannot fail it, and error variants carry no payload that could echo text.
Its value is as a guard if a variant ever gains one.

Existing tests cited as covering are checked by the mutation in the report for
`one_real_attempt_with_two_errors_...`, `claude_subagent_usage_is_not_attributed_...`
and `every_backend_failure_obeys_explicit_fallback_policy`, and by reading the
exact assertion for the others.
