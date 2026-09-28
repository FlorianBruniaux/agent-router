# Signed catalog authority CLI slice

This slice defines the trust boundary for a future `catalog-active` command. It does not grant runtime dispatch or process execution permission. Signed documents cannot provide publisher roots, and `CatalogPublisherGrant` remains constructed only after the CLI decodes a separate parent-owned policy and verifies its independently retained fingerprint.

## Publisher policy guard

`router-catalog-store::PublisherPolicyAuthority` reads an explicitly named, private policy file under a stable sibling directory named `<policy-file>.authority`. Its `catalog.lock` is separate from the policy file that a writer may atomically replace. Reads are bounded, reject symlinked path components and non-private files, and hold a shared interprocess lock. `validate_current` checks the named lock identity and rereads the bytes. Callers acquire this guard before the catalog store lock and retain it through the operation.

Policy writers must take an exclusive flock on `<policy-file>.authority/catalog.lock` before replacing the policy file, and must keep the lock inode stable. This is a cooperative advisory-lock protocol: a writer that ignores the lock is outside the guarantee. The reader detects changed bytes at validation, while the independently supplied `TrustedCatalogPublishers` fingerprint detects changed grants. The library currently exposes the read guard only; it does not install or replace parent policy.

## Review identity

Review output consists of a `ReviewProjection` and its approval digest. Application must reconstruct the opaque review from that projection, the exact signed document bytes, current parent roots and real current time. `reopen_activate` and `reopen_revoke` reject changed roots, expired or future review times, stale head/revision and altered change projections. The resulting review remains subject to the store's compare-and-swap in `apply`.

## Delivery boundary

The library guard and review reconstruction are implemented and tested. CLI `status`, `history`, `review`, `create` and `apply` commands are not delivered in this slice; the CLI entry point remains closed pending explicit approval for this trust-authority command family. No signed dispatch, runtime wiring, activation hook or global installation is included.
