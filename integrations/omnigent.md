# Omnigent external routing contract

`router_bridge_contract::omnigent` translates the documented
[`omnigent.api.routing.v1` contract](https://omnigent.ai/docs/reference/routing-api)
offline. This is the first part of roadmap O1. It does not start an HTTP server,
install Omnigent, run a host, or grant execution authority.

## Parent responsibilities

The caller constructs a `ValidatedPhaseContext` from its local task, policy,
catalog and validated host observation. Remote text does not supply permissions,
budget, capability evidence or approval. `HarnessBinding` explicitly maps that
one local harness to its Omnigent name. An omitted remote harness requires a
separate explicit binding; no native-harness detection is inferred.

Parse bytes through `RoutingRequest::from_json`, then call `select_route` with
the router, context and binding. The task prompt must exactly match the local
normalized objective. A prompt truncated upstream cannot authorize a longer
task by prefix matching. The caller must establish a suitable local context
for each request; no automatic prompt classifier is part of this translation.

The supported strategy is `agent-router-phase-v1`. Unknown strategies and
nonempty strategy configuration are refused. Model IDs are exact: prefix
stripping/restoration belongs to the upstream caller and is never guessed.

## Selection and output

Selection applies the existing deterministic phase router to the intersection
of offered model/harness pairs and locally eligible candidates. The immutable
fallback receipt marks unoffered candidates as unavailable, meaning absent
from this offer, not an observed provider outage. The complete receipt stays
local and is reproducible against the original context. Ties, missing required
evidence and policy thresholds retain the core router's refusal semantics.

Only the selected offered pair and a static rationale are serialized in the
wire response. It does not export the local prompt, history, approval, effort
or asset authority. The caller must separately enforce any selected effort or
assets; this pair-only protocol cannot prove they were applied. Session history
is parsed as a bounded advisory object and discarded. Its schema remains
unspecified in the public reference used here.

Inputs are limited to 64 KiB, 64 unique offers, 4,000 prompt characters and
256 ASCII identifier bytes. Struct fields are strict; malformed JSON, duplicate
known fields, extra authority fields and invalid identifiers are rejected.
Errors do not echo input. There is no network, credential or subprocess path.

## Enforcement boundary and checks

Omnigent documents a fallback to the session's default harness after routing
errors or empty/invalid responses. Returning `RoutingError::Refused` here is
therefore not proof of blocked execution in Omnigent. An HTTP integration must
test and constrain that caller behavior before it can be used as a mandatory
policy gate. Its server is not implemented by this module.

`cargo test -p router-bridge-contract --test omnigent --locked` covers offered
alternatives, unknown models, host mapping, local permission/budget refusals,
exact task binding, missing harness, order stability, redaction and limits.
Fixtures are synthetic and make no provider calls. No native compatibility or
model-quality claim follows from these tests.
