# Retrieval catalog binding

Metadata v2 binds an index to the full content digest of the catalog used by its
caller. A catalog version string alone cannot detect a policy or asset change
published under the same version.

The wire envelope contains four fields:

| Field | Meaning |
| --- | --- |
| `schema_version` | String `"2"` |
| `catalog_content_sha256` | Digest supplied from the independently loaded catalog |
| `metadata_content_sha256` | SHA-256 of the canonical nested metadata document |
| `metadata` | Existing schema 1 metadata document, including catalog version and assets |

`MetadataEnvelope::new(metadata, catalog_hash)` validates and canonicalizes the
metadata, then calculates its digest. `canonical_json()` serializes the validated
envelope. `Bm25Index::build_bound` performs the same construction in memory;
`Bm25Index::from_json` validates an existing envelope. Missing or malformed
digests, duplicate JSON fields, unknown fields, future versions and a changed
metadata payload under its old digest are refused.

The caller obtains the expected digest from its authoritative `Catalog`, for
example `Catalog::content_hash()` or a validated phase context's
`catalog_hash()`. The retrieval crate accepts that digest as an input; it does
not load a catalog, authenticate an author, or infer authority from the digest
inside the metadata itself.

Before ranking, call `search_with_catalog(query, expected_hash)` or
`skill_candidates_with_catalog(query, expected_hash)`. To check both the current
metadata source and current catalog, call
`search_current_with_catalog(source, query, expected_hash)`. Before reading a
selected body, call `load_body_with_catalog(root, kind, id, expected_hash)`.
Every operation checks the expected digest again. Catalog mismatch is returned
before scoring, asset selection or filesystem access. A matching digest still
requires the ordinary trust, path and body-content checks.

`catalog_hash()` returns `None` for legacy v1 data. `validate_catalog_hash`
returns `UnboundCatalog` for such data, `CatalogMismatch` for changed content,
and `Invalid` for a malformed expected digest. Legacy operations on a v2 index
return `CatalogBindingRequired`. Metadata-only inspection through `asset`
grants no permission and reads no body; execution preparation must separately
validate the catalog binding and compare asset/body hashes with its registry.

V1 JSON and its ordinary search API remain compatible and explicitly unbound.
V2 search results use schema `"2"` and include the catalog digest. Existing
metadata files are never rewritten or upgraded automatically. To migrate,
review the metadata against the intended catalog, construct a v2 envelope with
that catalog's digest, and explicitly save the resulting bytes to a chosen file.
A changed catalog requires a newly reviewed binding even when its version label
is unchanged. Rehashing metadata provides byte identity, never permission.

Validation uses synthetic metadata and caller-owned catalog examples:

```sh
cargo test -p router-retrieval --locked --offline
cargo clippy -p router-retrieval --all-targets --all-features --locked --offline -- -D warnings
```

## Version 3: five families and contextual evidence

`AssetKind::Model` adds model metadata to agent, skill, prompt and tool metadata.
A model entry cannot have an executable body. Retrieval identifies candidates;
its metadata does not prove runtime model availability, authentication or effort
support. Execution still uses the host snapshot and policy constraints.

`MetadataEnvelopeV3::new(metadata, signals, catalog_hash)` constructs schema `"3"`.
Its `metadata_content_sha256` covers the canonical pair `(metadata, signals)`;
the source digest covers the entire envelope, including the catalog digest.
There must be exactly one `AssetSignals` record per canonical `(kind, id)`.
The envelope validates counts, provenance, field bounds and duplicate identities
before indexing. `Bm25Index::from_json` accepts all three versions;
`Bm25Index::build_v3` accepts a typed envelope. V1/v2 structs and canonical hashes
remain unchanged. V3 uses a separate index algorithm version.

Each signal is `Signal::Unknown` or `Signal::Observed { value, provenance }`.
`SignalProvenance` records a source identifier, source version, source SHA-256 and
observation time. Languages and required tools are canonical bounded lists.
`SuccessHistory` records task kind, optional language, attempts, verified task
successes and transport successes. Transport success never contributes to the
verified-success score. Counts are imported attestations: the digest identifies
the submitted evidence and does not authenticate a reviewer or establish causality.
The caller must obtain these counts from its accepted outcome records.

`ContextSearchQuery::new(query, provenance, now)` supplies explicit time and
query provenance. The caller can add languages, task-required tools, observed tool
availability, task kind, age limits, sample minimum and retriever weights.
No host discovery occurs in this crate. `search_contextual(query, catalog_hash)`
returns schema `"3"`, the query/index/source hashes, candidates with per-source
scores and provenance, and bounded exclusions with a complete exclusion count.
`search_contextual_current_with_catalog(source, query, catalog_hash)` also
compares the current metadata source before ranking. `search_families` returns
separate top-k results in agent, skill, prompt, tool, model order.

Missing, unknown, future or expired required-tool evidence excludes a candidate.
An explicit unsupported tool is reported separately from an unknown tool.
Both the asset's dependencies and the task's required tools must have fresh
`Supported` observations. Missing language/history evidence contributes zero and
retains an `Unknown` status; stale evidence also contributes zero. History must
match task kind and language and meet the configured sample minimum. Defaults
are five samples and an evidence age of one day, with a maximum of thirty days.
These are visible starting policies, without a human-calibrated recall claim.

A v3 index refuses the legacy search and skill-candidate methods with
`ContextRequired`, so no caller can accidentally omit its tool gates. Body reads
remain explicit and require the catalog digest. Search, incremental updates and
fusion never read body contents. The execution preparation layer independently
checks the selected body's trust, hash and current asset contract.

## Ranked sources and deterministic fusion

Contextual retrieval combines BM25, languages, required tools and verified
history. Language/tool matches contribute two million score units each; history
uses `verified_successes * 1_000_000 / attempts`. These source-specific values
order each source; RRF uses ranks. Each source contributes at most 200 candidates,
then final results contain at most 50. Blocked/untrusted assets, anti-triggers,
required phrases and tool exclusions apply before every source ranking.
A query may hold at most `MAX_QUERY_PHRASES` (8) distinct quoted phrases;
repeats collapse to their first occurrence. Queries with 9 or more distinct
quoted phrases are refused with `Limit("query phrases")` (`retrieval.limit: query
phrases`); this tightens the earlier behavior, which accepted up to the 64-word
query limit.

The index hash folds in the BM25 parameters and per-field boosts, derived from
the constants the scorer uses, so changing one changes the hash.

`fuse_rankings(sources, RrfConfig)` is also a standalone pure function. It accepts
up to five `RankedSource` lists and deduplicates each canonical `(kind, id)` at its
first occurrence within a source. Different families remain distinct. Source
identifiers must be unique; source ordering cannot change the result. The default
`k` is 60, and weights are configurable integer millionths in `1..=1_000_000`.
Contextual query weights additionally allow zero to disable a source.

Each contribution is `floor(weight_micros * 1000 / (k + rank))`, with a one-based
rank among unique entries. Results include every contribution and original source
score. Fused ties use canonical asset ordering. Lexical-only results keep the
existing BM25 confidence policy; fused results use the separate
`fusion_ambiguity_margin_micros`, initially zero. Identical source-score profiles
remain ambiguous even when canonical tie ordering assigned different ranks.
Selection checks ambiguity before applying top-k. Fusion is advisory ranking,
not a correctness probability or permission grant.

## Optional externally supplied semantic ranking

The Cargo feature `semantic-ranking` is disabled by default. It enables validation
of a caller-supplied `SemanticRanking`; it includes no embedding model, provider,
installation, process execution or network request. The runtime query also starts
with `SemanticPolicy::Disabled`, even when the feature is compiled in.

Call `search_contextual_with_semantic(query, catalog_hash, Some(ranking))` after
explicitly choosing `Required` or `LexicalFallback`. `Required` refuses unavailable
or invalid evidence. `LexicalFallback` reports `UnavailableFallback` or
`InvalidFallback` and retains the local sources. Supplying a semantic list while
disabled is an error. A valid list binds the exact index hash, canonical query hash
and fresh provenance, and contains at most 200 hits. Unknown identifiers or a
wrong requested family invalidate the list; known but ineligible candidates
remain excluded. The caller owns any external embedding computation and must
separately authorize its data policy. No automatic semantic promotion occurs.

### Local ranking from caller-supplied vectors

With `semantic-ranking` enabled, `rank_semantic(index, query, asset_vectors, query_vector,
provenance)` computes a `SemanticRanking` locally from fixed-point integer vectors. It
performs no I/O, network access or provider call, and uses no floating point. The
caller owns embedding computation and its data policy.

- `AssetVectorSet` binds the exact `index_hash`; `QueryVector` binds
  `ContextSearchQuery::content_hash`. `rank_semantic` returns `SemanticInvalid` for:
  a schema version other than 1, another `index_hash` or `query_hash`, a dimension
  of 0 or above 1024, a vector length differing from `dimension`, a component out of
  range, a zero vector, and an unknown or duplicate asset. An error from
  `ContextSearchQuery::content_hash` (`Invalid` or `Limit`) is propagated as is, and
  more than `MAX_ASSETS` vectors return `Limit("asset vectors")`.
- Bounds: dimension `1..=1024`, components in `-32767..=32767`, at most `MAX_ASSETS`
  vectors. A dot product then stays below 2^40 and the scaled numerator below 2^120,
  so all arithmetic is exact in `u128`. Checked operations return
  `Limit("vector arithmetic")` as a defense that the bounds make unreachable.
- Score: `floor(cosine * 1_000_000)` computed as
  `isqrt(dot^2 * 10^12 / (|a|^2 * |b|^2))`. Non-positive cosines are omitted. Ties
  use canonical `AssetKey` order, so input order does not matter. At most 200 hits
  are kept, and a query restricted to one asset kind ignores other kinds.
- The result is an ordinary `SemanticRanking`. The provenance is supplied by the
  caller and is not validated by `rank_semantic`; `search_contextual_with_semantic`
  applies its existing provenance, freshness and eligibility validation.
- Without the feature, neither `rank_semantic` nor the vector types exist.

## Atomic incremental refresh

`refresh_from_json(successor, expected_source_hash)` accepts a complete successor
metadata document and checks the current canonical source digest as a precondition.
It computes the canonical asset diff, removes deleted or changed postings, updates
field statistics and inserts only changed metadata. Unaffected postings and token
maps remain intact. Signal-only changes update the source/index hashes without
retokenizing lexical metadata. `IndexUpdateReport` reports inserted, updated,
removed and retokenized asset counts plus the resulting hashes.

Validation and changes occur on a candidate index. An invalid successor, stale
precondition or term limit leaves the published in-memory index unchanged.
Canonical ordering/whitespace changes are no-ops. A schema migration requires an
explicit rebuild; refresh cannot silently discard catalog or contextual bindings.
The full successor still determines the hashes, so incremental and rebuilt indexes
produce identical hashes, candidates, scores, exclusions and abstentions. Refresh
can change a catalog binding only through the explicit successor document, after
which callers presenting the old catalog digest are refused.

There is no implicit filesystem watcher, durable index cache or background refresh.
The input owner supplies the current metadata source and independently expected
catalog digest. Per-source ranked-list limits and the signal/history bounds are
part of the public contract; held-out human recall evaluation remains separate.

The synthetic regression suite covers every family, score-changing signals,
unknown/stale evidence, transport versus verified success, source and catalog
mutation, incremental rebuild equivalence, RRF tie/dedup behavior and semantic
feature/fallback boundaries:

```sh
cargo test -p router-retrieval --locked --offline
cargo test -p router-retrieval --all-features --locked --offline
cargo clippy -p router-retrieval --all-targets --all-features --locked --offline -- -D warnings
```

## CLI context search and refresh

Legacy commands retain their flags. `search --query TEXT --kind model` adds the
fifth family to lexical v1/v2 search. For v3, supply a strict `ContextSearchQuery`
JSON file instead of `--query`, `--kind` or `--top-k`; those settings belong inside
`query` in the file. `--catalog` is required for context search. Inputs are bounded
and duplicate or unknown fields are refused.

```sh
agent-router index --metadata metadata.json --signals signals.json \
  --catalog catalog.json --json > index-v3.json
agent-router search --metadata index-v3.json --catalog catalog.json \
  --context-query context-query.json --json
```

`metadata.json` remains a schema `"1"` `MetadataDocument`. `signals.json` is an
array with exactly one `AssetSignals` per `(kind, id)`, for example:

```json
[
  {
    "asset": { "kind": "model", "id": "example-model" },
    "languages": { "status": "unknown" },
    "required_tools": { "status": "unknown" },
    "history": { "status": "unknown" }
  }
]
```

Unknown tool requirements exclude this example from selection. An observed empty
list means the source explicitly declares no tool dependencies; it requires its
own provenance. Omitting `--signals` keeps index output at v2. Both index forms
are validated by constructing the bounded in-memory index before output.

A context query contains these fields; the timestamps and digest below are
synthetic fixture values, not observations of a host:

```json
{
  "query": {
    "text": "database migration",
    "kind": "model",
    "top_k": 5,
    "policy": {
      "minimum_score_micros": 500000,
      "minimum_coverage_micros": 250000,
      "ambiguity_margin_micros": 100000
    }
  },
  "languages": ["rust"],
  "required_tools": [],
  "available_tools": [],
  "task_kind": "implement",
  "provenance": {
    "source_id": "synthetic-example",
    "source_version": "1",
    "source_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    "observed_at_unix_seconds": 100
  },
  "now_unix_seconds": 110,
  "max_signal_age_seconds": 86400,
  "minimum_history_samples": 5,
  "rrf_k": 60,
  "semantic_policy": "disabled"
}
```

The CLI replaces `now_unix_seconds` with the actual invocation clock. A timestamp
inside a query cannot keep old evidence fresh. The existing explicit `--now 110`
flag selects a reproducible replay clock for fixtures; it is not runtime evidence.
Weights and the separate fused ambiguity margin can be supplied or use their
library defaults. Exit code `3` means abstention, `2` means invalid or unavailable
input, `7` means an integrity failure (`retrieval.integrity`), and `0` means a
candidate was selected. Selection is metadata advice and
never authorizes model or tool execution.

To validate an externally prepared semantic ranking, compile with CLI feature
`semantic-retrieval` and pass `--semantic-ranking ranking.json` together with
`--context-query`. The context must explicitly select `required` or
`lexical_fallback`. The ranking's `query_hash` must bind the effective query,
including the invocation clock. For reproducible file-based experiments, prepare
both files for the same explicit `--now` value. The default build does not enable
this feature. No command downloads embeddings, starts a provider or sends text
to a network service.

Incremental refresh takes an old index document and a complete successor of the
same schema. Obtain `source_hash` from the previous search result or library
index; a raw file digest is different from this canonical source digest.

```sh
agent-router refresh --metadata index-v3.json --successor successor-v3.json \
  --expected-source-hash "$PREVIOUS_SOURCE_HASH" --catalog catalog.json \
  --output next-index-v3.json --json
```

The command checks the old source digest, applies and validates the metadata diff,
then checks the successor against the independently loaded catalog. Stdout
contains `schema_version: 1`, `update` counters and the canonical `document`.
Without `--output`, no file changes. With it, the source is written to a private
adjacent temporary file, synced, then published atomically to a **new** destination
without overwriting an existing file or symlink. The old source is never modified.
The command refuses in-place replacement. Directory syncing is used on Unix;
reported I/O failures are not a promise that publication never occurred.

`canonical_source_json()` provides the same schema-preserving serialization to
library callers. A restart still rebuilds postings from metadata; refresh is an
incremental in-memory update, not a persistent posting cache or filesystem watcher.
Deep doctor validates v2 and v3 catalog/body bindings while keeping runtime
capabilities `UNKNOWN`; valid signal syntax is not proof of its provenance.
