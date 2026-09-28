# ADR 0006: Persistent current signed-catalog selection

Status: Accepted for the isolated library boundary; application integration pending.

## Problem

A signed catalog can authenticate a publisher while still being stale. Supplying
its own binding hash as the application's expected current pin makes rollback,
revocation and replacement ineffective. The existing signed-catalog verifier
therefore requires an independently maintained current selection.

## Decision

Use a separate `router-catalog-store` crate, with no dependency on execution
receipts, semantic backend promotion, the runtime or the CLI. Require an external
expected store configuration digest and external publisher grants. Bind the
canonical locator and store/catalog IDs into reviews and current heads. Retain
canonical signed documents and a bounded monotone head chain in a private,
atomically replaced snapshot.

Activation and revocation require an opaque validated review, its exact approval
digest and CAS against the reviewed head. Returning to a previous document makes
a new revision. Acquire a non-deserializable current-catalog guard under the same
interprocess lock that updates acquire exclusively. Keep it alive through future
final dispatch integration; every signed projection also checks current external
roots and time. Retain directory and lock descriptors in the guard and recheck
their named identities on every access and final validation. A replacement inode
cannot make a guard on the old lock continue granting current access. Expose
historical reconstruction as a different inspection-only
type that cannot acquire current authority.

Use the existing Unix nofollow, owner/mode/hardlink checks and fsync/rename
publication pattern. Reject unsupported platforms. Explicit finite limits refuse
further changes instead of pruning history or replacing documents implicitly.

## Consequences and limits

Stale queued pins, expired catalogs, changed grants, copied paths and ABA
transitions can be rejected independently of the future source of a routing
decision. Old records remain inspectable using explicit historical roots.
Callers must retain their external configuration digest, current root source and
any anti-rollback checkpoint outside the store. A local hash chain cannot stop
an owner who can rewrite the whole directory. A checkpoint only covers rollback
before that checkpoint. Operator approval digest matching is not proof of a
human signer's identity.

This crate does not propagate signed per-asset ceilings into routing or enable
signed catalogs in the CLI/runtime. Those integrations must retain the exact
head and guard, preserve all restrictions and prove dispatch behavior separately.
