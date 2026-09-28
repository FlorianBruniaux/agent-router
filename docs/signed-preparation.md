# Signed preparation

`router-prepare::prepare_signed_with_receipt` creates a bounded, point-in-time
preview from the deterministic signed-catalog route. It accepts the validated
phase context, the workflow execution contract and node, the retrieval index,
the local body root, prompt variables, public preparation options, a validated
phase receipt, a live current-catalog read guard, and trusted publisher roots.
It does not accept a caller-supplied plan, prompt template, or execution
decision. The plan and candidate-specific prompt are derived from the signed
context.

The entry requires both the signed catalog authority and V3 asset contracts.
It revalidates the current head, publisher grants, catalog/index identity, the
whole deterministic receipt, V3 contract and availability evidence, and the
snapshot before reading bodies. It loads only bodies selected by that receipt,
checks the exact loaded bytes against signed catalog entries, and assembles the
request from those same bytes. It repeats current-authority, V3, snapshot, and
expiry checks after assembly. The public options contain no caller clock; the
operation records its own start and finish time and rejects clock errors or
backward movement.

The result, `SignedPreparedExecution`, is opaque and non-deserializable. Its
review preview excludes prompt text, variable values, and body contents. The
explicit `canonical_request_json` export contains the assembled private
request. Its preparation hash binds that request and its preparation evidence
to the current catalog head, authority, routing context, receipt, plan, node,
scope, index, and timing. Preparation marks approval as required and execution
as not started.

The four legacy preparation and request-validation entries reject signed
contexts before body I/O. Runtime review, reopen, execution, and generator
paths still require their existing authority and refuse signed-catalog
contexts. A signed preparation preview is not dispatch authority or runtime
approval.

Tests use synthetic local catalogs, publisher keys, snapshots, and body files.
They cover deterministic fallback, legacy entry refusal before body reads,
selected-body integrity, freshness during preparation, and runtime refusal of
a real signed prepared result without journal changes. No provider or native
process is invoked.
