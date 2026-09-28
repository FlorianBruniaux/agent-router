# Signed catalog routing, first integration boundary

`PhaseRouter::context_with_signed_catalog` constructs a routing context from a
`CurrentCatalogGuard`, externally supplied `TrustedCatalogPublishers`, a validated
host observation, and the exact signed metadata index. It does not execute a host,
read an asset body, install an asset, approve a run, or change a catalog store.

This is the first integration boundary of R06. Execution and global plan generation
are deliberately unavailable for these contexts until a current catalog guard is
retained through their final authorization checks. Existing unsigned v2 and v3
contexts keep their receipt bytes and behavior.

## API and authority

The caller supplies `SignedPhaseInputs` containing a host asset ID, preset asset
ID, an exact candidate-ID to prompt-ID mapping, and `PhaseExecutionScope`. These
are choices, not authority. The constructor requires the opaque current guard;
serialized heads, manifests, receipts, indexes, or matching hashes cannot replace
it. The catalog priors are derived from the signed document, rather than accepted
as an independent caller-provided catalog.

The guard revalidates its named directory and lock identity, external publisher
roots and grants, signature, validity window, and current head before projections
and once again after projection work. The existing store's external configuration
pin and optional checkpoint remain its opening boundary. Local history does not
protect against an owner able to rewrite all local authority and history.

The metadata index must match the full signed retrieval projection, including
its signals, body references, trust after publisher grants, catalog digest, source
digest, and index digest. A correctly rehashed but modified index is rejected.
Routing never resolves its body paths or reads their files.

Host compatibility requires one unambiguous observed native version from the
validated snapshot, excluding model-cache evidence. The signed host definition
must allow that exact harness, version and candidate model. Declaring a host or
model in a signed catalog does not prove its runtime availability, authentication,
or permissions. Those still come from the observation and parent policy.

## Constraints before scoring

Every selected model, agent, skill, tool, prompt, host and preset must be present,
trusted under the external grants, and not deprecated. Every selected entry's
exact host compatibility, required tools, mutation ceiling, sandbox ceiling,
sensitivity ceiling and context cost are enforced. Empty allowed sets are not
wildcards. Required capabilities and mutations from all selected entries are
combined with task, candidate, registry and optional v3 contract requirements.
Only candidates passing the complete combination reach scoring.

The model and host must support the complete capability set. The prompt must
allow the exact host and model. The preset must allow the capabilities, task risk,
budget and deterministic plan. If projecting the preset would silently narrow
the caller's normalized task, construction fails: the caller must explicitly
adopt the narrower task first. The execution scope remains bounded and confined
to the task's scope, using the existing v3 scope validation.

Signed agent and tool payloads cannot supply permission. Their operational
registry rows must match their signed definitions and effective trust; only the
parent permission value is preserved and it must be explicitly supported.
Native host tool policy is refused because its unenumerated tools cannot satisfy
this catalog boundary's exact tool constraints.

Optional v3 contracts remain independently bound to the catalog, snapshot and
execution scope. Their added capabilities and mutations are checked against all
signed ceilings after the v3 gate and before scoring. A v3 skill body digest,
version, provenance, license declaration or effective trust cannot replace the
corresponding signed identity. Additional restrictions remain effective.
Authors and licenses are signed declarations, not verified authorship or legal
compatibility conclusions.

## Identity and compatibility

Signed contexts and receipts use domain-separated v4 hashes. Their identity binds
all original request, observation, policy and catalog fields, the optional v3
contract binding, the complete store head, its monotone revision and predecessor,
store and catalog IDs, canonical locator digest, external grants binding, signed
choices, exact scope, deterministic preset plan digest, and metadata/index digests.
Both context and pool hashes change when this current authority changes.

An A to B to A rollback therefore has a new identity even when A's signed document
and content binding are unchanged. Changing publisher grants also requires a new
activation and changes the context identity. Fallback retains the same current
v4 context and cannot add candidates, assets, models or permissions. A v4 receipt
or execution decision cannot be replayed as a legacy v2/v3 receipt with otherwise
identical candidates and catalog priors.

`has_signed_catalog`, `signed_catalog_head`, `signed_catalog_authority_hash`,
`signed_plan` and `signed_prompt_id` are read-only inspection APIs. The returned
plan and routing snapshot are descriptive. They do not keep a store lock alive,
revalidate wall-clock freshness on every later read, or authorize current dispatch.
The caller remains responsible for obtaining a suitably fresh validated host
observation before constructing a context.

## Explicit remaining boundaries

`generate_plans` and `validate_generated_plan` reject signed contexts because their
external plan/node inputs are not yet bound to the exact signed preset plan.
`Runtime::review_decision` and `reopen_decision` reject them before any runtime
record lookup, Git source inspection or run registration. All public review and
reopen wrappers converge on these checks. `execute_decision` retains the same
refusal in defense; no approved signed run is constructible through the current
public review APIs.

Future integration must bind the exact signed plan and node to preparation,
revalidate the selected prompt template and only the selected body bytes, bind
current head/revision into the final approval, and retain the current catalog
read guard through spawn alongside any backend activation guard. It must recheck
publisher roots, validity, revocation and scope at that boundary. Removing these
refusals without that integration would violate the contract. No CLI, coordinator,
preparation, runtime dispatch, or historical signed-run reconstruction is claimed
by this first boundary.

## Evidence

The fixtures use synthetic Ed25519 publisher keys and public synthetic metadata.
They cover the seven families, stronger candidates rejected by signed ceilings,
UNKNOWN capability and permission, altered registry/metadata/body identity,
legacy receipt substitution, fallback, rollback ABA, changed grants, and refusal
of an external plan. A controlled removal of the signed pre-score gate makes the
mutation-ceiling regression select the forbidden primary candidate; restoring it
selects the permitted secondary candidate.

Legacy receipt byte fingerprints were captured using the unchanged policy sources
at `f2d4107` and are checked by `legacy_phase_bytes.rs` for v2 and v3. Runtime tests
exercise all eight public review/reopen entry points and verify an unchanged event
journal and absent run record. Their workspace deliberately is not a repository,
so the exact signed-authority refusal also precedes Git inspection. No native
process or model invocation is involved in these tests.
