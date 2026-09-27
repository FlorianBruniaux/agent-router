# Bounded metadata retrieval

`Bm25Index` indexes versioned metadata for agents, skills, prompts and tools.
The index is in memory and rebuildable from `MetadataDocument`; it performs no
network, provider, agent, hook or model execution. Body contents are never read
by index construction or queries. This implementation uses the workspace's
existing dependencies instead of introducing a persistent search engine.

```rust
use router_retrieval::{Bm25Index, SearchQuery};
# fn query(bytes: &[u8]) -> Result<(), router_retrieval::RetrievalError> {
let index = Bm25Index::from_json(bytes)?;
let query = SearchQuery::new("rust regression fixtures");
let result = index.search(&query)?;
// Each candidate has field scores, term evidence and an explicit trust tier.
let policy_candidates = index.skill_candidates(&query)?;
# Ok(())
# }
```

The JSON envelope uses `schema_version: "1"`, a nonempty `catalog_version`, and
an `assets` array. Each asset has `id`, `kind`, `trust` and `description`, plus
optional `task_kinds`, `triggers`, `examples`, `tags`, `paths`, `anti_triggers`,
and `body: { "path": "relative/path.md", "sha256": "<64 lowercase hex digits>" }`.
Kinds are `agent`, `skill`, `prompt`, and `tool`. Trust uses the existing
`router_types::SkillTrust` contract. Unknown envelope/metadata fields are rejected.

Identifiers are validated; assets and metadata lists are sorted before hashing.
`source_hash()` records the canonical metadata, including catalog version,
trust and body references. `index_hash()` additionally includes the algorithm
version and parameters. `is_current` detects changed metadata;
`search_current` refuses stale metadata rather than silently using it.
Bodies changing without metadata changes are detected when their hash is checked
at selection time. No filesystem watcher or persistent cache is implied.

The scorer sums field-specific BM25 contributions, with `k1=1.2`, `b=0.75`
and positive IDF `ln(1 + (N-df+0.5)/(df+0.5))`. Boosts are identifier 8,
task kinds 6, triggers 5, description 3, examples 2.5, tags 2 and paths 2.
Each contribution is rounded to integer millionths before addition; ordered
maps and canonical asset keys make ties deterministic. These formulas follow
the [BM25 term-frequency and document-length model](https://nlp.stanford.edu/IR-book/html/htmledition/okapi-bm25-a-non-binary-model-1.html),
using the positive smoothed IDF variant. Raw scores are relevance evidence,
not correctness probabilities.

The tokenizer retains Unicode words and full technical tokens such as `C++`,
`foo::bar` and `src/lib.rs`. Quoted phrases require contiguous words in a
metadata field; phrase evidence is reported separately. Anti-triggers match
contiguous normalized whole words and exclude the asset. There is no stemming,
semantic inference or negation interpretation. Unclosed quotes are interpreted
as a phrase extending to the end of the query.

Trust is a hard boundary. Blocked assets have no postings. Untrusted assets have
separate corpus statistics and zero route utility. Trusted assets rank first,
so untrusted assets cannot displace trusted hits at a top-k boundary or alter
their scores. Per-kind corpus statistics also prevent tool metadata from changing
skill scores. `skill_candidates` fixes the kind to skill, drops untrusted assets
and returns nothing on abstention. The policy layer must still check the
authoritative catalog: metadata trust claims cannot grant permissions.

Abstention distinguishes empty query, no matches, untrusted-only results, weak
matches and ambiguity. Default policy requires raw score >= 0.5, coverage >=
25%, and a relative lead above 10% over the next trusted match. These are visible,
configurable starting policies, with no human calibration or recall claim.
Ambiguity is checked before top-k truncation. The JSON fixture corpus carries
synthetic labels for engineering tests; human validation is `NOT_DONE`.

Limits are 20,000 assets, 32 MiB serialized metadata, 32 KiB per asset, 8 KiB per
field, 32 entries per list, two million indexed term occurrences, 4 KiB and 64
terms/words per query, two million postings visited per query, and 50 returned
candidates/exclusions. Exclusion counts retain the full total. Limit violations
return typed errors, never a silently truncated passing result.

`load_body(root, kind, id)` requires explicit selection of a trusted asset. It
rejects absolute paths, traversal, symlink components, nonregular files, content
above 1 MiB and hash mismatches. Unix opens use `O_NOFOLLOW | O_NONBLOCK` and
verify the opened descriptor is a regular file. The caller supplies an authorized
root and must prevent hostile concurrent ancestor replacement. These checks do
not establish an OS sandbox, and loaded text is data rather than permission.

Validation and a reproducible 10,000-asset capacity sample:

```sh
cargo test -p router-retrieval --locked
cargo clippy -p router-retrieval --all-targets --locked -- -D warnings
cargo run -p router-retrieval --release --locked --example benchmark -- 10000
```

The benchmark prints measured build and selective/broad p50/p95 query times,
metadata bytes and postings visited. Its synthetic data measures capacity only;
it cannot establish retrieval recall or a human-validated acceptance threshold.
