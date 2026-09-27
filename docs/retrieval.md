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
