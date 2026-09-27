# ADR 0004: separate backend activation proof from deterministic receipts

Status: accepted for the promotion library. Runtime integration must explicitly
consume its opaque proof before enabling semantic execution.

A backend proposal is untrusted selection data. Promoting it by marking a better
candidate unavailable or by rewriting a deterministic receipt would confuse the
selection source and undermine replay. Instead, preserve the deterministic
receipt and return a separate opaque `ActivatedBackendDecision` after signed
promotion and activation checks.

The promotion library depends on decision, evaluation and policy. Policy does
not depend on the promotion library, avoiding a dependency cycle. The decision
layer exposes only a checked read-only lookup into its closed eligible pool.
Applications consume the activation proof together with existing phase, asset,
source and sandbox gates; no public deserializable approval token is introduced.

Every promotion reruns held-out replay gates with signed independent labels and
preregistered criteria. External shadow/canary claims remain signed attestations,
not fabricated observations. Distinct external Ed25519 roles, exact proposal
lifetimes and a canonical trust-root fingerprint bind all opaque proofs. Signed
state transitions use compare-and-swap hashes, nonces and explicit rollback.

This requires callers to retain a trusted history head, supply current keys and
clock, and validate the proof again at dispatch. It deliberately leaves storage,
key rotation, remote execution and real production activation outside this
library. Unknown evidence remains ineligible; synthetic positive tests establish
implementation behavior only.
