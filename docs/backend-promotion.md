# Signed backend promotion

`router-backend-policy` validates an explicit promotion path for a semantic
backend. It performs no network request, model call, process launch, file write
or automatic activation. The application must integrate the resulting opaque
proof before a semantic choice can affect execution. The deterministic receipt
remains unchanged and is retained for fallback and audit.

`BackendArtifact` binds the backend identity/version, artifact and implementation
hashes, signing protocol, backend key, authors, routing policy, confidence,
data policy, failure policy and bounded proposal/evidence lifetimes. It cannot
add candidates, tools, capabilities or permissions. `BoundedDecisionContext`
resolves a signed proposal only within the phase router's already eligible pool.
A v3 context retains all asset gates and still needs the final byte-level gate.

The parent constructs `TrustedAuthorities` from externally supplied Ed25519
public keys. Documents cannot nominate their own keys. Roles are distinct for
the backend, human reviewer, criteria/trial owner and activation approver. The
same public key cannot occupy multiple entries or roles. Canonical signatures
include a domain, role and key identifier, preventing cross-role reuse.

Promotion reruns every frozen replay record. Criteria must be signed and
registered before evaluation, bind the exact dataset and artifact, and require
held-out families, independent signed human labels, no training-family leakage,
no hard-constraint violations, no route-accuracy regression and a demonstrated
predeclared gain. Baseline and semantic replay use the same phase policy. The
same signed regression limits apply separately to each held-out family;
aggregate gains cannot hide a family regression. A stable family needs no
individual gain. Typed family reports, including the exact cohort dataset hashes,
are included in the evaluation digest consumed by shadow and canary approvals.
The signed backend proposal binds the complete redacted request and evaluation case;
changing the context, pool, artifact, code, purpose or expiry invalidates it.
Unsigned `human_validated` fields and serialized success reports are insufficient.

Shadow and canary attestations are separately signed, sequenced after evaluation,
linked to its digest, bounded in case count and required to report zero hard
violations or subgroup regressions. Signatures authenticate those attestations;
they do not independently establish that an external trial happened or that its
observations are true. Actual human labels and trial evidence remain operator
inputs. Synthetic fixtures are never production evidence.

`BackendActivationState::apply` requires a signed compare-and-swap approval tied
to the current state hash and scope, with a fresh nonce and expiry. Activation
binds the validated promotion. Rollback targets the retained deterministic policy.
A changed artifact cannot reuse an already seen backend identity/version.
Restoring state rechecks signed events and requires an independently retained
expected head; a hash taken from untrusted history cannot detect truncation.
Persistence, atomic writes and protection of that expected head belong to the
application.

`decide_active` returns an opaque `ActivatedBackendDecision`. Invalid, absent or
expired proposals follow the artifact's explicit deterministic-fallback or
abstention policy, within the state's pinned baseline policy.
A different policy is rejected before a decision is constructed, including when
inactive or rolled back, and is rejected again by `validate_current`.
Proposals cannot fake an unavailable candidate to force fallback.
The audit evidence records the state, context, pool, unchanged baseline receipt,
selected candidate and exact signed proposal expiry.

Immediately before preparation or dispatch, the consumer calls
`validate_current(state, router, context, authorities, now)`. This repeats the
state/context/selection checks and checks the exact proposal expiry, rather than
extending a short proposal lifetime to the artifact's maximum. Expiry boundaries
are exclusive. A rollback or changed external trust-root fingerprint invalidates
an old proof. Evaluations, promotions and activation history also bind the full
canonical root fingerprint. Key rotation therefore requires explicit renewed
validation; it never silently reuses an old promotion.

The proof does not replace source-state checks, execution approval, sandbox
containment, v3 asset validation or independent outcome verification. No shipped
backend is activated by constructing this crate or by its synthetic test suite.
