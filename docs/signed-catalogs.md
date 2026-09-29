# Signed common catalogs

`router-catalog` adds an explicit signed catalog format alongside the
unchanged v1 policy catalog and v2 generated catalog. The new document uses numeric
schema version `3` and covers models, agents, skills, prompts, tools, hosts and
presets. Existing parsers do not silently interpret it as a legacy catalog.

The implementation authenticates declarations made by an approved publisher. It
does not establish native availability, the safety of a body, legal compatibility,
human task success or permission to execute. No keys are installed, processes
started or network requests made by catalog verification.

## Manifest and identities

`SignedCatalogDocument` contains a schema 1 `SignedCatalogManifest` and a
`CatalogPublisherSignature`. Each asset has a canonical `(kind, id)` key, version,
source/revision/source digest, declared licence expression, authors, validity
window, declared trust, capabilities, requirements, restrictions, context-cost
estimate and optional explicit replacement. The payload definition has its own
SHA-256 digest. Search metadata and typed signals are included in the signed
manifest for the five retrieval families.

Authors and licences are **signed declarations**. The verifier does not identify
an author independently or issue a legal compatibility opinion. A source is either a
`https://` URL (lowercase scheme) or a bare identifier, at most 512 bytes.

- URL authority: `host[:port]`. The host is ASCII `[A-Za-z0-9.-]` in any case, not
  empty and not only dots. The port, if present, is 1 to 5 digits between 1 and 65535.
- URL path: segments of `[A-Za-z0-9._~-]`. `.`/`..` segments, empty segments
  (`//`), percent escapes, spaces, non-ASCII and bidi characters are refused. One
  trailing slash is allowed.
- Bare identifier: `[A-Za-z0-9._-]`, no `/` or `:`, not `.` or `..`.
- Refused: credentials, query strings, fragments, backslashes, any other scheme in
  any case (`file:`, `HTTP:`), absolute or relative paths, drive paths (`C:\`,
  `C:/`) and `owner/repo` forms, since a relative path cannot be told apart from
  `owner/repo`.

Compatibility break: a catalog already signed with a source such as `owner/repo`,
`scheme:id` or a path now fails with a hard `Invalid` error, both when sealing and
when verifying, before the publisher lookup. There is no `Quarantined` state and no
schema version bump (`schema_version` stays 1). Fix the source, re-seal and re-sign.

No machine configuration, environment
variable, credential store or private key is read to populate a manifest.

Payloads reuse existing model/skill policy and agent/tool registry types. Host
payloads declare harness/version compatibility and capabilities; they never
produce a `HostObservation`. Presets declare an existing workflow preset and
bounded planning settings. Prompt payloads describe typed variables, compatibility,
rendering limits and the output contract. Prompt text remains in an external body.
Agent and skill instruction bodies likewise remain external. A body's raw-byte
SHA-256 is distinct from the canonical payload-definition digest.

`seal_signed_manifest(candidate)` canonicalizes and computes hashes. It grants no
trust and creates no signature. `catalog_signing_message(key_id, manifest)` returns
the domain-separated Ed25519 preimage. Signing remains outside the library and
requires the publisher's private key. Verification uses Ed25519 strict verification
and rejects weak public keys, malformed signatures and altered content even if an
attacker recomputes every unsigned hash.

Canonical asset and author ordering is stable. Retrieval metadata/signals use the
same canonicalization as the retrieval crate. Duplicate keys, duplicate set
members, unknown JSON fields, invalid nested definitions, duplicate asset IDs,
unknown host/model references, replacement cycles and cross-family replacements
are refused. A replacement is explicit; the verifier never follows it silently to
choose a different asset. Replaced assets become blocked in active projections.

Limits include 8 MiB per signed document, 512 assets, 64 models, 256 skills,
128 approved publisher keys, 16 authors per asset and the existing retrieval and
prompt bounds. Validity windows are at most 366 days and use `[valid_from,
valid_until)`. Every asset's window must lie within the manifest's window. An
expired or future asset refuses the entire active catalog instead of silently
changing its effective membership.

## External publisher grants and quarantine

`TrustedCatalogPublishers::new` takes `CatalogPublisherGrant` values from trusted
application configuration. Each grant binds a key identifier to actual public-key
bytes, permitted catalog IDs, asset families, exact licence declarations and a
trust ceiling. This type has no `Deserialize` implementation. A catalog cannot
supply its own root key or broaden its grant.

An approved key's valid signature must also satisfy its whole grant. The effective
trust is capped by the external grant. A third-party publisher cannot self-label
its assets `builtin` to obtain builtin trust. Existing `untrusted` and `blocked`
labels remain restrictive. A signature does not replace a skill scanner, asset
review or host permission gate.

`SignedCatalogDocument::from_json(bytes)?.verify(publishers, now)` returns:

- `CatalogVerification::Verified(ValidatedSignedCatalog)`, an opaque immutable
  value with no deserializer, for valid content, signature, scope and time;
- `CatalogVerification::Quarantined(QuarantinedCatalog)` when the publisher key is
  not approved. This contains a bounded descriptive projection marked `untrusted`
  and has no operational projection methods;
- a typed error for malformed, expired, altered or incorrectly signed content, or
  an approved key operating outside its grant.

Unknown-key quarantine does not assert that its signature is authentic. Review of
that candidate is separate from approving or installing an external root.

## Current selection and cache invalidation

Every active projection takes a `CatalogAccess`:

```rust
CatalogAccess {
    publishers: &approved_publishers,
    now_unix_seconds: current_time,
    expected_binding_hash: current_catalog_selection,
}
```

`current_catalog_selection` comes from the application's independently maintained
selection of a verified catalog. A cached document must not supply its own
expected hash as evidence that it is still current. The verifier compares this
pin and the full publisher-grant fingerprint, rechecks signature/grants and checks
all validity windows on every operation. A cached value cannot retain access
after root revocation, root/grant rotation, expiry or a changed selection pin.

The binding digest covers the complete canonical manifest, publisher identity,
signature and external root/grant fingerprint. `content_hash()` identifies the
manifest itself; `binding_hash()` additionally identifies that authorization
context. Neither is merely a version label.

The projected v1 `Catalog` has a version derived from the complete binding. A
host-author change, preset change, body-reference change, skill deletion or trust
grant change therefore changes `Catalog::content_hash()` even when model rows and
the publisher's version label remain unchanged. Existing catalog-bound contexts,
receipts, retrieval indexes and caches consequently have a different key.

This verifier is not a filesystem watcher. The separate
[`router-catalog-store` library](catalog-store.md) provides persistent, explicitly
approved current selection, monotone revisions and a guard held through dispatch.
Applications must still provide current external roots and time, and revalidate
the guard at the final gate. Routing, preparation and runtime integration remain
separate work. Historical signed bytes can still be inspected through the plain
document DTO. Historical validity is not fresh execution authority.

## Operational projections

Only `ValidatedSignedCatalog` exposes these methods, each with current access:

| Method | Result and boundary |
| --- | --- |
| `catalog` | An existing `Catalog` of model/skill priors, with effective trust and a complete binding-derived version. Native availability remains separate. An active model is required by the existing v1 contract. |
| `retrieval` | `MetadataEnvelopeV3` for the five search families, bound to that exact policy-catalog hash. No body is read. |
| `validate_retrieval` | Compares a supplied index's complete source/index identity with the projection derived from the signed manifest. A rehashed index with the correct catalog hash but different metadata is rejected. |
| `generation_config` | An existing `GenerationConfig` of prior/availability references for `GeneratedCatalog::generate`; the latter still requires an independently validated host observation. Body/definition references are retained. This legacy DTO cannot carry all signed restrictions and is not signed execution authority. |
| `phase_registry` | An existing agent/tool registry for an explicit declared host/version. Incompatible assets are excluded. Non-representable mutation ceilings or tool requirements refuse the projection. Permission is `UNKNOWN` unless supplied by a separate parent permission map. Publisher documents must contain `UNKNOWN` in that field. |
| `host` | A selected, trusted host compatibility declaration, never a probe or authentication result. |
| `preset` / `build_plan` | A selected preset or an opaque `SignedPlanProjection` containing a real validated `WorkflowPlan`, catalog binding and full projection hash. |
| `validate_selected_body` / `load_selected_body` | Exact-byte validation or bounded reading of one explicitly selected body under a caller-supplied root. |
| `validate_prompt` | Validates one selected body's digest, strict JSON, nested prompt digest and exact signed template metadata, returning the existing typed `PromptTemplate`. |

Model/skill policy catalogs and generation configurations remain **priors**.
Their legacy DTOs cannot retain all common manifest restrictions. Applications
must not feed them into a production route as evidence of signed execution
authority. A registry or metadata index alone is not a skill activation.

The signed `allowed_mutations` set is an upper bound on the effects of an
execution selecting the asset. It differs from `required_mutations`, which lists
minimum requirements. `PhaseRegisteredAsset` has no upper-bound field. Accordingly,
`phase_registry` refuses any compatible active agent/tool unless its signed
mutation set includes all four current mutations: `workspace_write`, `network`,
`execute_skills` and `integrate`. It preserves the separate sandbox and sensitivity
restrictions. Even a parent permission of `supported` cannot bypass this refusal.
The full set does not grant a mutation: the existing task, policy, permission,
snapshot and sandbox gates still apply.

Supporting restrictive signed ceilings in active routing requires a future
projection that binds the complete request and all selected asset restrictions,
then revalidates that binding at selection, preparation and dispatch. This module
does not implement that integration. Keeping a signed hash alongside a mutable
legacy DTO is insufficient. No current projection authorizes execution by itself.

Planning only narrows the caller's allowed mutations and budget, raises the risk
floor when required, and adds prerequisite capabilities without granting them.
It checks the selected host declaration, sensitivity, required mutation authority,
allowed capability set and each node mode against permitted sandboxes. Preset
tool requirements cannot be represented by the current workflow planner and are
explicitly refused. Tool availability must not be guessed. Agent/tool requirements
that the current phase registry cannot represent likewise refuse that projection
instead of being discarded. The returned plan is not approved or executed. Its
full projection hash binds the catalog/preset even if two definitions produce the
same underlying workflow shape; callers must retain this binding alongside the
plan.

For fresh execution or use of cached projections, repeat current catalog access
and the existing execution checks. Do not serialize the opaque wrapper and later
treat an arbitrary reconstructed DTO as proof of validation.

## Selected body boundary

Search, hashing, signature verification, generation and planning do not open body
files. `load_selected_body` requires an explicit root and selected `(kind, id)`.
It reuses retrieval's relative-path, regular-file, symlink, one-MiB, UTF-8 and
SHA-256 checks. It writes nothing. Supplying an expired/revoked/stale access fails
before filesystem access. `validate_prompt` can instead consume bytes already
loaded through a trusted caller's selected-body path.

The underlying retrieval path checks are not an OS sandbox against an attacker
concurrently replacing ancestor directories. The caller must provide an isolated
or otherwise protected body root; execution still needs the native sandbox and
sealed artifact boundary. No stronger filesystem guarantee is claimed here.

## Verification

The synthetic test keys are test fixtures, never installed publisher roots.
Behavior tests cover all seven families through real downstream components,
unknown-key quarantine, forged signatures despite recomputed hashes, exact TTL
boundaries, grant/key rotation, trust ceilings, canonical ordering, strict nested
schemas, body integrity/path refusal, deprecation and metadata/cache invalidation.
They do not claim real publisher approval or native host availability.

```sh
cargo test -p router-catalog --locked --offline
cargo clippy -p router-catalog --all-targets --all-features --locked --offline -- -D warnings
```
