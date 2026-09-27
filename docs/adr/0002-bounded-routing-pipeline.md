# ADR 0002: Bounded routing and execution boundaries

Status: accepted for the product implementation.

## Context

The original deterministic router accepts declared host and skill information.
The broader product needs provenance, retrieval, phase-specific choices,
workflow structure and offline evaluation without allowing a candidate or
model response to become permission authority.

## Decision

Keep the version 1 router and serialized receipts compatible. New phase routes
use version 2 contexts with full snapshot, catalog, policy, pool and registry
hashes. Validation creates an immutable context before any utility comparison.
Runtime execution recomputes the route and binds the selected composite to an
approved workflow node; a deserialized receipt alone cannot authorize a run.

Shared wire contracts live in `router-types`. Feature import and storage formats
live with their validating crate: host evidence in `router-adapter-api`, catalog
projections in `router-catalog`, retrieval metadata in `router-retrieval`, and
replay and promotion formats in `router-eval`. No crate imports application I/O
from the CLI. The CLI exposes explicit files and stable error codes.

Use a bounded in-memory BM25 index for metadata. The current corpus ceiling is
20,000 assets and 32 MiB; query work, term count and top-k are separately capped.
This avoids a mandatory search service. Field contributions remain inspectable,
and untrusted documents cannot change trusted corpus statistics. Bodies are
loaded only after selection and hash validation. An embedding engine remains
optional and is not part of the baseline.

Normalize explicit task authority before planning. Eight workflow presets
produce acyclic plans with ownership, scope, budget and verification contracts.
Every required writer verification must be downstream of that writer and cover
its scope. Hash checks supplement semantic validation; a rehashed invalid DAG
still fails. Configuration roles have fixed preference precedence while their
restrictions intersect, including explicit task overrides.

Semantic backends receive only opaque IDs and reduced numerical features from
the validated pool. Their output is advisory until independent evaluation and
explicit policy activation. The reference route and fallback remain available.
HTTP adapters are optional and limited to a named loopback endpoint with a
total deadline and bounded request/response sizes.

Durable execution, feedback and policy state use private local SQLite stores.
Transport exit, command validation and task success are separate outcomes.
Native model metadata can establish an observed model, while an effort flag
only establishes what was requested. Unknown evidence is never successful
verification. Runtime storage and activation gates have their own behavioral
tests; a schema check does not prove sandbox enforcement.

## Consequences

Callers must supply explicit current evidence and budgets. Unknown capabilities
can cause abstention even if a CLI happens to be installed. Generated catalogs
may refuse a host model whose effort support is unknown. This is a visible
limitation, not an inferred permission grant.

Hashes establish byte identity and reproducibility, not source authenticity.
Review labels, prices, native evidence and signing keys need independent trust
at their ingestion boundary. Synthetic fixtures cannot satisfy human evaluation
or measured live-model improvement gates.
