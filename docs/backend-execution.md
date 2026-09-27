# Execute a signed backend decision

A deterministic baseline and an effective execution decision are distinct
objects. The baseline remains unchanged even when a promoted backend chooses a
different admissible candidate or resolves a deterministic abstention. A backend
cannot add a candidate, capability, permission, asset or budget.

## Establish authority

Use [backend activation](backend-store.md) to create a private store, review a
signed promotion and explicitly activate it. Retain the expected configuration
hash outside that store. Private signing keys and human evaluation labels are
supplied by the operator; Agent Router does not generate production approval or
reviewer identity.

Execution authority is scoped to the exact source checkout and writable/readable
scope. Compute the scope hash from a prepared request:

```sh
agent-router backend-activation scope-hash --request prepared/request.json --json
agent-router backend-activation config-hash --config backend-config.json --json
```

Use the first hash as `StoreConfig.scope_sha256`. The canonical scope input is the
JSON tuple `("agent-router:backend-execution-scope:v1", workspace, source_state,
scope)` serialized by the Rust contract and hashed with SHA-256. The source state
and ordered scope must match the actual execution request. Both commands are
read-only projections with `approved: false`; printing a digest does not make
its source trustworthy.

## Bind a decision to preparation

An externally managed backend supplies a signed proposal for the current
activation head and the exact bounded routing context. Retain a private JSON
manifest with these fields:

| Field | Meaning |
| --- | --- |
| `schema_version` | `1` |
| `store` | Absolute store path, or path relative to this manifest. |
| `expected_config_hash` | Independently retained authority configuration hash. |
| `expected_head` | Optional independently retained checkpoint, or `null`; signed successors are allowed. |
| `activation_state_hash` | Exact historical activation head used for this decision. |
| `decided_at_unix_seconds` | Original decision time within that head's verified interval. Future times are refused. |
| `proposal` | Complete `SignedBackendProposal`, or `null` for an explicitly permitted deterministic fallback. |

Add `"backend_decision": "backend-decision.json"` to the preparation manifest.
`prepare` reconstructs the signed state, verifies the proposal, preserves the
baseline receipt and uses the effective selection for assets and prompt
construction. It also checks current authority before producing output. The
output definition retains the absolute backend manifest path. Keep that document
and its signed inputs immutable while retaining the run.

A prepared request's `receipt_id` is the effective execution decision ID. The
baseline receipt file still contains its original deterministic ID. The decision
binding exposes both identities, the effective candidate, activation head,
authority roots, promotion and proposal hashes. A backend-selected candidate can
be valid while the deterministic baseline abstains; the original abstention is
not rewritten as a selection.

## Review, dispatch and revoke

The usual `run` and `coordinate` interfaces consume this definition. Their outer
approval includes the canonical backend store path, independently pinned config
and checkpoint, and the canonical manifest hash from the **same loaded object**
used to reconstruct the decision. Changing a locator to a copied store with an
identical history requires a new approval. Definitions without a backend keep
the original approval domain and serialized shape.

At dispatch, the runtime reacquires the current store under a shared interprocess
lock, revalidates the selected candidate and fresh promotion/proposal against
current roots, then holds the lock through the spawn attempt. Activation and
rollback require the corresponding exclusive lock. The guard is released after
spawn, rather than held for the entire model call. Rollback, expiry or changed
roots while a run is queued blocks launch; it does not silently substitute the
baseline under the existing approval.

Native version probes are subject to cancellation before a model worker starts.
A durable cancellation observed during preparation waits for cleanup, records
`Cancelled` and launches no worker. See [other-host transports](other-hosts.md).

## History and feedback

After rollback or expiry, `verify`, artifact controls and coordinator service can
reconstruct the exact historical decision at its original head/time. Historical
reconstruction is separate from current dispatch authority. An old decision
cannot use historical authority to launch another attempt.

Consented feedback records the actual selected candidate and effective receipt
ID. For semantic decisions, its provenance hash also binds the immutable baseline
and decision binding. Transport completion, independent validation and objective
success remain separate. Raw proposals, private signatures and prompts are not
added to the ordinary runtime/feedback projection.

Tests use explicitly synthetic signed labels and fake hosts under real native
sandboxes. These establish protocol, authority and lifecycle behavior, not human
quality improvement or a production semantic promotion.
