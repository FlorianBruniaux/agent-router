# Bounded plan generation

The `router-policy` generator ranks complete assignments of eligible phase
candidates to an existing, validated workflow. It implements the top-k join,
bounded beam search, dominance, diversity and scoring stages in technical
specification §6. Its additive schema version 1 preserves existing routing and
workflow wire formats. The library does not launch a host, load skill bodies or
change a parent model.

The CLI connects generation to preparation and per-node execution through an
explicit generated-plan document. A generated JSON proposal alone is not
permission to execute. Phase asset gates, workflow ownership, runtime approval
and sandbox checks remain separate requirements. Generated decisions and signed
semantic backend decisions cannot be composed in the same execution definition.

## Inputs and compatibility

`PlanGenerationInput` takes the validated workflow plan and its execution
contract, one `GenerationNode` per node, declared edge compatibility, the versioned
`PlanGenerationPolicy` and an explicit planning timestamp. Each node contains a
`PhaseRouter`, its opaque `ValidatedPhaseContext`, ordered dimension lists and
optional soft evidence. The task and phase must exactly match the workflow.
Every selected composite passes `PhaseRouter::validate_selection` before scoring.

The dimensions are exact model and effort pairs, optional agent identities,
skill sets, tool sets with their policy, and sandbox modes. Each ordered list
must contain only distinct values already present in eligible composites.
Top-k restricts each list independently, then joins it against those existing
composites. It never creates a new model, agent, skill combination or permission
from a Cartesian product. `GenerationNode::from_context` supplies initial ranks
from validated local utility with canonical candidate IDs as tie breakers.
Callers can supply different explicit ranks, subject to the same eligibility
checks. Empty dimensions produce `no_eligible_join`.

Every workflow dependency needs a `GenerationEdgeCompatibility` declaration:

```json
{
  "source": "execute",
  "target": "verify",
  "allowed_pairs": [["primary", "primary"], ["primary", "secondary"]]
}
```

Pairs refer to exact eligible candidate IDs at the source and target. Duplicate
edges, nonexistent DAG edges and ineligible IDs are errors. A missing declaration
or `allowed_pairs: null` is UNKNOWN and produces `unknown_compatibility`.
An empty set is a known incompatibility and can produce `no_feasible_plan`.
The caller is responsible for the provenance of this compatibility assertion;
a hash does not authenticate it.

## Resource accounting and ranking

Every estimate remains an estimate. Costs, tokens and elapsed time add across
all assigned nodes. Coordination overhead adds once per dependency edge.
Changing the exact model ID on an edge adds a second configurable overhead.
An effort change on the same model is not counted as a model switch in version 1.
Overhead is charged to the consuming node and to the global ledger. The global
cost and token ceilings are the minimum of the task budget and all effective
restriction ceilings. Each node must also remain within its allocated budget.
Arithmetic overflow is an error.

`elapsed_ms` is the conservative sum of candidate and overhead durations. The
separate `critical_path_elapsed_ms` accounts for DAG dependencies, including all
incoming overhead at a consumer. It does not presume that the host can actually
run independent nodes concurrently. The cumulative duration drives the hard
budget and latency score. No parallel speedup is claimed.

The policy contains a complete set of weights for each task class that it uses.
All fractional signals and weights are bounded to 0 through 1,000,000. Quality
normalizes the already weighted phase quality through the explicit
`phase_quality_scale`; the default matches the stable phase policy. Relevance,
history, cache, context pressure and residual risk average over assigned nodes.
Cost, elapsed time and coordination time normalize against their global budget;
model switch count normalizes against the number of dependency edges.

Utility adds quality, relevance, history and cache contributions, then subtracts
cost, latency, context, risk, coordination and model switch penalties. The report
retains every component. A favorable score cannot restore an excluded composite
or exceed a hard budget. Weights are configuration, not measured probabilities.
The report does not claim calibrated confidence or global optimality.

Optional history, cache, context, risk, family and transport observations include
an evidence digest, exact context and execution contract hashes, and a timestamp.
Invalid bindings or out-of-range fractions are errors. Future or expired evidence
is `stale`. Unknown and stale history/cache provide zero benefit; unknown and
stale context/risk use the conservative maximum penalty. Status remains explicit
in each choice. Fresh observations are caller attestations, not verified human
labels or a claim that a provider cache exists. A prepared opaque proof rechecks
freshness before its execution binding is used.

Beam states retain partial choices, cumulative resources, DAG finish times and
score features. The canonical workflow wave order and candidate ID order make
ties deterministic. Dominance removes a state only when all positive features
are no lower and all penalty resources/features are no higher, with at least one
strict improvement. Candidates on the frontier must be identical: even the same
model can have different allowed continuations. Diversity labels at the current
node must also match before dominance is considered.

Per-family and per-transport quotas apply at each beam extension, including the
terminal node. Unknown and stale labels share a single UNKNOWN bucket. They do
not create artificial diversity. The search keeps at most `beam_width` states and
returns one selected plan plus at most `max_alternatives` retained alternatives.
Top-k, dominance, diversity and beam width can exclude a globally optimal path;
the returned result is the best retained feasible assignment under that policy.

## Work and memory boundaries

| Boundary | Maximum |
| --- | ---: |
| Workflow nodes | 64 |
| Candidate rows across contexts | 2,048 |
| Ranked values per dimension per node | 64 |
| Declared compatibility pairs across edges | 4,096 |
| Beam width | 64 |
| Returned alternatives | 15 |
| Serialized input projection or report | 8 MiB |
| Global work units | 20,000,000 |

The default work limit is 1,000,000. One meter covers serialized input bytes,
dimension joins, state expansions, dependency checks, frontier construction,
sorting comparisons and dominance checks. Exhausting it returns
`generation.work_limit` without a partial recommendation. These are deterministic
work units, not CPU instructions or a wall-clock performance guarantee.
Serialization uses a capped writer, and state storage contains bounded choice
indices and numerical ledgers rather than duplicated contexts or asset bodies.
No provider call, filesystem read or embedding computation occurs here.

## Validation before preparation or dispatch

`validate_generated_plan(input, proposal)` reruns generation against the supplied
current inputs and accepts only an exact retained selected plan or alternative.
Editing a proposal and recomputing its hash cannot create the opaque
`ValidatedGeneratedPlan`. The input digest covers workflow and execution contract
identity, node contexts, dimension order, soft observations, compatibility,
policy and planning timestamp. Changing the workspace, source state, scope,
policy or context therefore invalidates the original proposal.

The opaque object exposes `proposal()`, `selection_for(node_id)` and
`validate_execution(node_id, request, now)`. The last check binds the exact
candidate, model, effort, phase, host, workspace, source state, node scope,
sandbox, agent, tool policy, required capabilities, selected skill IDs and node
timeout. It also rechecks the freshness of observations used by the selected
assignment. It is deliberately not deserializable and implements no runtime
activation authority.

The caller must still enforce the effective execution decision and receipt,
current parent authority, selected skill content hashes and approvals, actual
prepared input bytes, adapter capability checks, sandbox and workflow generation.
The generator does not authorize prompt content, executable skills, arbitrary
children or scope expansion. Reconstructing a planning proof is not a substitute
for those checks at dispatch.

## Behavioral verification

`cargo test -p router-policy --test generator --locked` covers deterministic
ordering, alternatives, top-k joins, unsupported composites, UNKNOWN edges,
cumulative restrictions, every overhead resource, conservative DAG latency,
switch and coordination costs changing the selected path, safe frontier dominance,
class weights, stale/future signals, diversity, global work exhaustion, bounded
serialization, strict schemas, proposal tampering, exact execution bindings and
expiration between planning and execution. All fixture host evidence and history
are explicitly synthetic; no model, network call or real cache is involved.


## CLI and durable workflow binding

`generate-plan --manifest generation.json --json` writes a report with its exact
input manifest identity, source digest and planning timestamp. A preparation
manifest can name that document with `generated_plan`, alongside the workflow
contract and node generation. `prepare --manifest prepare.json
--output-directory bundle --json` reconstructs the whole report, including the
unselected nodes, validates freshness and writes a private reviewable bundle.
It does not approve or start a worker. The subsequent `run --definition
bundle/definition.json --store /absolute/private/store --sandbox sandbox.json
--json` returns the exact approval digest. Repeating it with `--approve DIGEST`
requires the same generated document, manifest sources, v3 asset authority,
workflow contract, request and sandbox configuration.

The generated decision ID binds the retained global proposal and exact node
selection. It occupies the runtime receipt identity; the deterministic baseline
receipt remains separately inspectable. The runtime pins the global proposal
for the workflow instance, so another node cannot mix a legacy decision or a
different retained proposal into the same instance. The initial generated pin,
instance and contract are recorded in `workflow.plan.authority.v1` within the
registration transaction. Registration, dispatch and reopening compare the
mutable lookup table against that event and the event hash chain. Removing a
pin or replacing it with another valid proposal's pin is rejected. This is local
consistency evidence, not protection against an actor able to rewrite the whole
database and recompute its history.

The SQLite v6-to-v7 migration preserves existing runs, events and contracts and
marks existing instances `legacy`. It runs transactionally and reopening is
idempotent. A generated v7 pin without its original authority event is refused;
it is never promoted from an unanchored lookup value. Expired proposals remain
readable for history, but cannot start another attempt.

The runtime `generated` suite exercises actual SQLite migration and rollback,
reopening, unchanged history, mixed decisions, missing and substituted pins,
source/scope/capability changes and expiry before dispatch. The CLI `generation`
suite checks report recomputation, changed manifests and unselected-node inputs,
preparation boundaries and refusal to publish a bundle after invalid evidence.

An explicitly ignored native canary,
`native_generated_plan_prepare_approve_and_execute_preserves_v3_authority`, uses
a fake Claude protocol under the real OS sandbox. It generates and prepares a
complete two-node assignment, then approves and executes only its execution
node. Its phase baseline chooses `primary` (`fixture-best`), while the global
compatibility constraint selects `secondary` (`fixture-second`) for execution.
The fake host requires the explicit model and effort flags and writes their
received values into the isolated worktree. Assertions cover those exact
arguments, the generated decision identity, refusal without v3 authority, exact
retained output and writes confined to that worktree. Its
availability and model metadata are synthetic. Transport completion leaves task
validation and objective success UNKNOWN; it does not prove live provider
availability or an entire verified workflow. The Linux native workflow invokes
this canary explicitly instead of treating an ignored test as a passing check.
