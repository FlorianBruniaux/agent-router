# Bound asset contracts and gates

`router-asset-contracts` adds independent v1 documents for FR005/008/018. Existing
catalog v1, generated catalog v2, host observation v1 and retrieval metadata v2
bytes are unchanged. No provider, process, network, filesystem or executable
skill runner is invoked by this crate.

`AssetContractsDocument` binds the canonical policy catalog hash. Each model
declares its exact provider, supported host versions, maximum context, optional
features, sensitivity and provenance. Skills declare version, provenance,
license expression, body hash, instruction/executable kind, trust, compatible
host versions, tools, capabilities, mutations, sandboxes, sensitivity, context
budget and retrieval triggers/examples. Legacy skill capabilities cannot be
removed and trust cannot differ from the bound policy catalog.

`AssetAvailabilityDocument` binds a validated host snapshot. Each row binds
exact provider/model/host/version/efforts, authentication, features and context
accounting. Missing information is unknown. Positive native claims require
native evidence references matching snapshot source hashes and timestamps;
a Codex model cache cannot supply native evidence. Authenticated access requires
a native runtime reference. Explicit no-auth providers require an operator
review reference. These are evidence links, not parsers of the original native
stream or independent authentication of the claims.

The parent must establish document authority outside these JSON documents and
supply independently pinned expected hashes to `ValidatedContracts::from_json`
and `ValidatedAvailability::from_json`. Reading `content_hash` from an untrusted
document and passing it back as the expected hash does not establish authority.
`seal_contracts` and `seal_availability` are producer operations that compute
integrity only. All library errors have fixed codes and omit input text.

Inputs use strict JSON with unknown fields, duplicate keys and duplicate array
members rejected. Documents are limited to 4 MiB, 64 models/availability rows,
256 skills and 64 evidence entries. Labels, collections, timestamps and numeric
limits are bounded. License expressions have bounded boolean-expression syntax
and must exactly match the caller's approved license allowlist. The library does
not determine legal compatibility or infer a license from a repository URL.

## Review and approval

After metadata selection, `review_skill` checks the exact body hash and returns
an opaque `PendingSkillReview`. Its `projection()` exposes the declared source,
revision, version, license, body/manifest hashes, trust, kind, tools, permissions,
sandbox, budget and scanner findings. Raw body and matched scanner text are
absent. The review is intended for the approving operator; source identities
are visible and it is not a redacted audit bundle.

The scanner reads at most 256 KiB of UTF-8 text and emits at most 64 fixed-code
findings with byte offsets. It flags credential markers, instruction overrides,
network references, process syntax and encoded payload markers. Truncation
blocks approval. Credential markers cannot be waived through the policy.
Ambiguous findings require an explicit policy disposition and a fresh review.
No match establishes safety; `safety_not_established` always remains true.

The parent calls `approve_review` only after an authorized review action matching
the exact displayed digest and an independently established scope digest.
`SkillApproval` cannot be deserialized. Its binding includes the catalog-bound
contracts, complete manifest, body, scanner, policy and authority scope. Changes
invalidate prior approval. Source authentication remains explicitly unknown in
the projection even when the catalog declares a trusted tier: this crate does
not verify publisher signatures or manufacture provenance from hashes.

## Two gates

`preflight` rejects incompatible metadata before ranking. Its
`EligibleAssetPlan` is not an execution permit. It verifies the exact provider,
host version and effort, fresh auth/evidence, feature support on both model and
host, restrictive policy, skill type/license/trust, tools, capabilities,
mutations, sandbox, sensitivity and context reservation. Model/host/policy
limits intersect. Native permission and phase authorization remain the phase
router's responsibility; this crate adds constraints rather than replacing it.

`gate` repeats preflight at the supplied current time, checks the complete set of
selected bodies and opaque approvals, scans the current bytes, checks each
skill's context budget and measures the actual final input. The caller supplies
the exact prepared bytes that will be transmitted, including skills and native
wrappers; checking only `ExecutionRequest.prompt` would be insufficient. The
runtime must also ensure those bytes came from its validated adapter request.

Only an explicitly established `Utf8ByteUpperBound` accounting profile is
accepted. The counter uses UTF-8 byte length, not Unicode character count. This
is a conservative text-token bound for a tokenizer/host combination whose
profile the parent has independently established. It is not universal tokenizer
detection or measured token usage. Host overhead must be known and referenced
to native evidence. Input reservation plus output reservation plus host overhead
must fit the minimum model/host/policy context window; actual bytes must fit the
input reservation. Output reservation is a planning constraint, not a claim
that a provider consumed or enforced that token amount.

The resulting `GatedAssets` binds contracts, availability, request, policy,
approvals and exact final input. `verify_prompt` detects subsequent byte changes;
it does not refresh evidence. The runtime must repeat the gate with its current
clock before dispatch and independently enforce approvals and sandbox policy.
An old successful gate or a serialized hash cannot authorize a new execution.

Image/audio/file/citation/JSON capabilities have explicit dimensions. A JSON
schema requirement also binds the schema digest. This does not add transports
to existing adapters: a text-only adapter must reject a requirement it cannot
represent even if an input availability document declares support. Reasoning
replay remains explicitly unsupported. Executable skills remain explicitly
unsupported, including when policy permits `execute_skills`; they require a
separate reviewed runner and cannot be relabeled as instructions.

Tests cover exact bindings, rehash without a pinned root, catalog drift at the
same version, duplicates/unknown fields, stale/unknown auth, wrong native source,
feature intersections, context boundaries, multibyte input, license/mutation
restrictions, executable rejection, changed approvals, scanner canaries and
exhaustive monotonicity across feature-policy subsets. Fixtures are synthetic;
they establish no live provider or host capabilities.

```sh
cargo test -p router-asset-contracts --locked --offline
cargo clippy -p router-asset-contracts --all-targets --all-features --locked --offline -- -D warnings
```

## Phase routing and preparation

`PhaseRouter::context_with_assets` accepts the unchanged v2 phase request plus
`PhaseAssetInputs` and produces a v3 receipt. The ordinary `context` method and
v2 receipt validation retain their historical behavior. V3 binds both complete
documents, the asset gate policy, every candidate's requirements, authorized
review digests and an exact `PhaseExecutionScope` in the context hash. A caller
cannot downgrade a v3 receipt to the legacy context and retain validation.

`asset_authority_scope_digest(request, execution_scope)` binds the whole phase
request and prospective workspace, source revision and step scope. Scope paths
must be relative, unique and covered by the normalized task. Per-candidate
requirements must match the candidate model, effort, skills, tools, sandbox and
sensitivity. Capabilities and mutations must include every effect introduced by
the manifests. Existing phase permissions, restrictions and budgets still apply;
the asset policy cannot regrant them. All metadata gates run before scoring.

`approved_review_digests` comes from separately authorized parent configuration.
It does not deserialize a proof. Routing reads no skill bodies. After selection,
`validate_execution_assets` reconstructs opaque reviews from the selected bytes,
compares the exact authorized digests and applies the final gate. Missing or
stale review authorization fails. Extra prompts supplied by a coordinator may be
included before the normal runtime review; the final gate measures those bytes
too. A derived worktree uses the original approved request for source authority
and its actual prepared stdin for the byte gate; the runtime must separately
bind and validate the derived workspace.

`router-prepare` verifies instruction kind, body and trust against the v3
manifest. It gates the complete serialized request plus a conservative framing
allowance before returning a preview. That bound is not observed token usage.
The runtime must call `validate_execution_assets` again on the actual adapter
stdin immediately before dispatch, using the current time. The method returns
`None` only for the explicitly historical v2 path. Text-only execution rejects
all requested multimodal or structured-output features until an adapter input
contract represents them.

Synthetic fixture construction shared by policy and preparation tests lives in
`crates/router-policy/tests/support/asset_fixture.rs`. It is test authority only,
never a source of live authentication, context-window or provider evidence.
