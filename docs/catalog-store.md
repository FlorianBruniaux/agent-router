# Persistent current catalog authority

`router-catalog-store` adds a local current-selection boundary to the existing
signed-catalog verifier. It installs no publisher key, body, skill or host, makes
no network call and executes nothing. CLI, routing, preparation and runtime
integration remain separate work. A legacy policy-catalog projection is still a
prior, not signed execution authority.

## Identity and external authority

Create one store per catalog ID with
`CatalogStore::create(canonical_absolute_path, store_id, catalog_id)`. Creation
requires an existing parent. Supply a unique store ID for
each new store incarnation; there is no global ID registry. The store returns its public
`StoreIdentity` and `config_hash()`. Store that expected digest independently of
the directory. `create` no longer requires a new directory: it adopts an
existing directory that is owned, `0700` and holds nothing but `catalog.lock`
and at most one `state.pending`, so an empty private `0700` directory is
accepted and a `create` that failed part way can be run again. A directory with
a `state.json`, any other entry or other permissions still returns `Exists`.
`open(path, expected_config_hash, checkpoint)` never takes its
expected identity from the document it is supposed to validate.

The identity binds schema, caller-provided store ID, catalog ID and SHA-256 of the
canonical absolute locator. Public status contains the locator digest, never its
machine path. Relative paths, symlink traversal and alternate noncanonical
spellings are rejected. Copying a store to another path does not preserve its
approved identity. An already opened store also detects replacement of its
named directory or lock inode before acquiring another guard. Every held guard
also retains both descriptors and checks their current names again on access.

`TrustedCatalogPublishers` comes from external application configuration. Its
full fingerprint includes publisher key bytes, IDs, catalog scope, asset
families, declared-license grants and trust ceilings. Every active acquisition,
review, approval application and current projection receives these external
roots. A document cannot supply its own publisher approval.

## Review, approval and changes

`review_activate(bytes, publishers, now, ttl_seconds)` validates the complete
signed document, all validity windows and grants. It returns an opaque
`CatalogReview`. Unknown publishers remain refused; no quarantined document is
installed. `review_revoke` returns the same kind of review for clearing the
current selection. There is no separate semantic promotion protocol.

The review's inspectable `ReviewProjection` and approval digest bind the exact
store identity, expected head and revision, signed document digest and binding,
external grant fingerprint, action, review time and expiration. Pass the exact
`approval_digest()` to `apply(review, approval, publishers, now)` after explicit
operator approval. The library compares that value; it does not attest who
supplied it or automate a human decision. Reconstructing a review DTO cannot
construct the opaque reviewed object.

Applying a change takes an exclusive nonblocking interprocess lock, rereads and
validates the current snapshot, checks compare-and-swap against the reviewed
head and revalidates signature, current time and external grants. Stale approval,
changed roots, expiry or a different store leaves state unchanged. Updates use
write, file fsync, atomic rename and directory fsync, with an exact preimage
comparison before publication. If the final directory fsync fails after the
rename, `apply` returns `StoreError::PublishedDurabilityUnknown` (code
`published_durability_unknown`): the new snapshot is visible but its durability
is UNKNOWN. In `apply`, `Io` is a failure before publication; `Io` also covers
reads and `create`.

Never retry blindly after that error: read `status()` and re-review from the
observed head. Apply revalidates the named directory and lock identity again
just before publishing. An interrupted staging file is never read as committed state. A later
approved update can replace only that verified private staging file under the
exclusive lock.

Each accepted activation or revocation appends a head containing the store and
catalog IDs, locator, revision, predecessor, document and catalog binding,
grants fingerprint, application time and exact approval digest. Revisions grow
monotonically. Returning A to service after A→B is another activation: A→B→A has
three different heads even when A's signed document and catalog binding are
identical. Old queued work must retain and compare the head, not only A's binding.
Referenced canonical signed documents are immutable in the bounded snapshot.
Removal, replacement or an unreferenced document refuses validation.

The caps bound live rows, not lifetime changes. When a change would exceed 128
retained events, 16 retained documents or the byte budget, the oldest events and
the documents only they reference are pruned. The head of the last pruned event
is stored as the snapshot `anchor`; retained events chain from it, and the
anchor must be a sealed head of this store. Revisions keep growing across
pruning. A pruned head can no longer be passed to `historical`, and a trusted
checkpoint older than the retained window, including the genesis head once
history is pruned, is refused with `Conflict`. Advance the external checkpoint
at least once per 128 changes.

An activation must leave 1 MiB of the snapshot budget free, so an active
catalog can always be revoked. `review_*` runs the same staging on a scratch
copy and decides on the exact encoded length, so it refuses with `Limit` what
`apply` would refuse. There is no watcher, installation or permission grant.
See ADR 0017.

## Current guard and historical reconstruction

`acquire_current(expected_head_hash, publishers, now)` returns an opaque
`CurrentCatalogGuard` only for the exact current, valid and approved head. It
holds a shared lock until dropped. Updates return `Busy` while any cooperating
reader retains its guard, including in another process. There is no unbounded
wait or retry loop. Keep the guard through the final dispatch operation so an
approved update cannot occur between the current check and dispatch.

The guard exposes `head()` and `catalog()`. To use the signed catalog, obtain
`access(current_publishers, now)` and pass it to the existing signed projection
methods. Access rechecks the named directory and lock against its retained file
descriptors, including owner, modes and hard-link count, then fingerprint,
signature and time. Replacing either name while a guard is held invalidates its
next access, including when a newly opened handle has already revoked the
catalog using the replacement lock. It does not silently reuse roots saved at
acquisition. Call `access` or `validate_publishers` again immediately at the final
dispatch gate; a previously returned access value is not a new current check. A guard held past document expiry cannot
project at the later timestamp. The trusted application supplies a real clock
and current external roots, and must synchronize external root changes with its
own final dispatch protocol. This library cannot lock arbitrary external
configuration providers. No generic user-implementable authority trait or
constructor for a current guard is exposed.

`historical(head_hash, publishers_at_activation)` verifies retained bytes with
explicit historical roots at the recorded application time. It works after
expiration, replacement or revocation. The returned `HistoricalCatalog` exposes
only its head and descriptive document DTO. It cannot become a current guard or
an operational validated-catalog wrapper. Historical readability never grants
a new execution, and current checks cannot accept an earlier head after rollback.

An optional external checkpoint on `open` requires that exact historical head
or a verified successor. It detects restoration to a snapshot before that
checkpoint. Snapshot hashes and the local chain provide consistency checks,
not publisher signatures over the store's decisions and not protection against
a same-owner attacker rewriting the entire store and recomputing its chain.
Named-inode rechecks detect observed replacements; they do not make advisory
locks prevent filesystem mutation by the owner after a final check.
Without a later external checkpoint, restoring a complete old valid snapshot is
not distinguishable from that original state. The catalog's Ed25519 signature
still authenticates its publisher declarations; it does not authenticate the
local operator's activation history.

## Bounds and filesystem contract

| Bound | Maximum |
| --- | ---: |
| Encoded snapshot, checked during reads and serialization | 64 MiB |
| Retained distinct signed documents (older ones pruned) | 16 |
| Signed document | Existing 8 MiB verifier limit |
| Retained changes (older ones pruned behind the anchor) | 128 |
| Free bytes reserved for revocation | 1 MiB |
| Review lifetime | 300 seconds, expiration exclusive |
| Store/catalog identifier | 128 ASCII letters, digits, dot, underscore or hyphen |
| Canonical absolute path | 4096 bytes |

The local store requires Unix descriptor-relative nofollow operations. Unsupported
platforms fail explicitly. Directories are owner-only `0700`; state, lock and
staging files are current-owner regular files with exact `0600`, one hard link
and no symlink. Reads and changes use the same `catalog.lock`. Full snapshot
validation checks strict JSON keys, canonical signed-document digests, all
references, event ordering, approval preimages, head digests and the stored tip.
No body path is opened. Active selection additionally verifies Ed25519 and grants;
historical inspection verifies them with caller-supplied historical roots.

## Evidence

Synthetic-key tests cover exact approval, CAS and restart; A→B→A; independent
checkpoint rollback refusal; changed keys and grants; revocation and expiry;
historical inspection; copied and replaced directories; held-guard refusal after
lock/directory replacement and revocation through a new handle; altered or missing
entries; duplicate JSON; private modes, symlinks and hardlinks; unpublished
staging recovery; retention pruning with an anchored chain and revocation
after more than 128 changes; exact byte-budget boundaries; anchor validation;
resumable `create`; the durability-unknown error; and a real subprocess
contending with a reader's shared lock. These are local library tests, not CLI
integration, host execution or real publisher approval.

```sh
cargo test -p router-catalog-store --locked --offline
cargo clippy -p router-catalog-store --all-targets --all-features --locked --offline -- -D warnings
```
