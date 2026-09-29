# 0010: Inspect runtime storage without application recovery

Status: accepted for the passive diagnostic API and CLI.

## Context

Opening `Runtime` creates directories, migrates its schema and recovers interrupted
runs. A doctor command must be able to inspect the existing storage during or
after execution without invoking these operations. Cancellation writers do not
acquire the owner lock, so that lock cannot justify SQLite `immutable=1`.

## Decision

Open an existing operator-controlled private database with normal read-only
SQLite semantics and capture a bounded, committed in-memory copy. Perform SQLite
integrity, known read-layout checks, journal consistency and aggregate queries
on that same copy. A trusted linked Rust callback extends the existing snapshot
API; it preserves SQL limits, counts queries and returns no rows in the CLI report.
The serialized copy must retain its fingerprint and have no open transaction
when the callback returns. This is not a sandbox against hostile Rust callbacks.

Inspect owner and lease locks separately, using nonblocking shared locks. Keep
recorded cleanup evidence distinct from these later observations. A pending or
cleanup-marked lease never becomes an opaque cleanup proof. A missing or free
owner lock never proves that a process is orphaned. No migration, application
recovery, repair, checkpoint, reservation release or host invocation is requested.

Use directory-relative metadata checks for SQLite files. Auxiliary open/close
cycles on the same inode would interfere with process-wide POSIX locks held by
another SQLite connection. Do not replace normal SQLite coherence with an
immutable reader. SQLite may coordinate or reconstruct its WAL-index; zero
filesystem writes are not promised.

## Consequences

The report can pass its scoped storage checks while process liveness, journal
authenticity and cleanup authority remain unknown. Database and lease observations
are explicitly non-atomic. Older schemas need migration; future or unrecognized
read layouts are refused. Schema compatibility must be updated when runtime
migrations are added. The integrated schema-9 layout includes admission binding
and intent tables; shared admission accounting remains unknown because its
separate registry is outside this diagnostic.

The private root and its ancestors remain within the operator trust boundary.
Mode, owner, link count and inode checks do not defeat malicious same-user path
replacement or certify platform ACLs. Input, SQL and materialized-row limits
bound the diagnostic work; native execution permission is never derived from it.
