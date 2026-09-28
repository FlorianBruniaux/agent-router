# Read-only diagnostics and audit

`router-doctor::diagnose(&DoctorRequest, now)` inspects an explicitly supplied
root and list of artifacts. It never discovers home directories, reads the
environment, launches a host, sends a health request, authenticates, activates a
hook, creates a database or repairs files. Reports count these effects as zero.
The CLI layer owns command-line exposure and output persistence.

Each check separates configuration from runtime. Missing categories are
`unknown`; a clean static check never produces an overall runtime success.
The report contains fixed codes, artifact kinds and hashes of supplied IDs.
Paths, user labels and file contents do not appear in diagnostics or errors.

| Artifact | Evidence checked | Runtime limit |
| --- | --- | --- |
| JSON configuration | Duplicate-free object and schema version 1 | Component-specific configuration semantics require its own validator |
| Catalog | Real `router-catalog` schema validation | Availability is separately probed |
| Host snapshot | Hash, timestamp, age, model/effort structure and provenance shape | A caller-supplied hash is not source authentication |
| Retrieval index metadata | Real BM25 metadata validation and index construction | No asset body is loaded |
| Managed file | Bounded content hash against optional expected preimage | Presence does not mean active |
| Hook or plugin | Bounded content, JSON structure for plugin, optional expected hash | Hook invocation/trust and plugin loading remain unknown |
| Authentication marker | File presence and private permissions only; content is never read or hashed | Credentials are not tested |
| Executable | Regular-file presence and Unix executable permission | No `--version` process is launched; host versions may be supplied in snapshots |
| Database | SQLite header only | Integrity, migrations, locks and live health remain unknown |
| Lease | PID shape, heartbeat timestamp and bounded expiry | Staleness does not prove an orphan; no process is killed |
| Backend status | Bounded supplied JSON | Health and liveness remain unknown without an independent observation |
| Sandbox evidence | Bounded supplied JSON | A JSON claim does not establish enforcement |

Reads are limited to 128 artifacts, 4 MiB per file and 16 MiB cumulatively.
Unix traversal opens every directory relative to an existing directory
descriptor with `O_NOFOLLOW`, then opens the final regular file with the same
rule. Symlinks, traversal and nonregular files are rejected; concurrent parent
renames cannot redirect the descriptor traversal. Other platforms return an
explicit unsupported error. Pass a canonical root such as `/private/tmp` on
macOS; `/tmp` itself is a symlink and is rejected.
The crate forbids unsafe code and uses rustix's safe `openat`/owned descriptor
APIs. Reads retain `O_CLOEXEC` and `O_NONBLOCK`; a FIFO is rejected without waiting
for a writer. Descriptor tests replace already-open roots and intermediate
directories with symlinks and verify that reads retain the original directory.

Sensitive-key and token-pattern detection is heuristic. A match produces a
warning without the matched value. No match does not establish that a file is
free of secrets. Authorization-marker contents bypass this scanner because
they are never read. File mode checks are metadata evidence, not proof of an
effective sandbox or credential validity.

## Explicit deep diagnostics

`diagnose_deep(&DeepDoctorRequest, now)` adds local checks to the existing
read-only pass. `agent-router doctor --deep --manifest deep.json` selects this
contract explicitly. Its schema version is 1 and its fields are `base` (the
unchanged `DoctorRequest`), `schemas` and `retrieval_bindings`. The result has
`base`, additional `checks`, `overall`, `examined_body_count` and
`database_queries`. Existing doctor manifests and audit bundles keep their
formats. All host runtime states remain `unknown`; local SQLite integrity is a
configuration result, not evidence of a running service.

Each schema binding names an existing JSON configuration or managed-file
artifact by `artifact_id`. `schema` selects the real component validator:
`configuration_layers`, `effective_restrictions`, `phase_policy`,
`normalized_task`, `workflow_plan` or `prompt_template`. Validation checks
restriction hashes, policy bounds, task contracts, DAG invariants and prompt
seals as applicable. It never renders a prompt or applies permissions.
Unknown and duplicate fields are rejected, including fields tolerated by an
older component DTO. A generic configuration without an explicit schema gets
`component_schema_required`, not a semantic validation claim. Authentication
markers cannot be reclassified as configuration and their contents remain
unread.

Each retrieval binding names `index_artifact_id` and `catalog_artifact_id`.
The current validated catalog's policy projection supplies the expected
content hash, exactly as phase routing and preparation do. The metadata must
use the bound v2 envelope and agree on both content hash and catalog version.
A generated catalog also supplies the explicit registry: declared assets and
body hashes must match it. Skill trust must agree with the policy catalog;
metadata cannot raise that trust. A legacy v1 catalog has no agent, prompt or
tool registry, so those assets yield `catalog_asset_registry_unavailable`.
An unbound legacy index yields `catalog_binding_required`.

With `body_root: null`, no body is read. A supplied `body_root` is a relative
directory beneath `base.root`, without traversal or symlinks. It requests all
bodies declared by that index, not a sample. Each must exist, be trusted and
match its declared hash. Missing bodies, blocked/untrusted assets, registry
mismatches and excessive counts are explicit failures. Reads use the same
descriptor-relative traversal as ordinary doctor checks; they do not use the
retrieval loader's pathname traversal. Bodies never confer executable/tool
authority, and neither bodies nor metadata labels appear in the report.

Deep checking adds at most 16 MiB of reads to the base pass's 16 MiB budget,
including repeated preimage comparisons. Files remain limited to 4 MiB,
bodies to 1 MiB, body reads to 128 and retrieval bindings to 16. A limit is a
failed check, never a silently truncated success. Between-pass hash changes
are reported as `input_changed`.

Database artifacts additionally receive SQLite `integrity_check(1)` and
`foreign_key_check`. Opening uses an already validated descriptor with
`mode=ro`, `immutable=1`, `SQLITE_OPEN_READ_ONLY` and `SQLITE_OPEN_NOFOLLOW`.
The connection confirms its read-only state; it has no create flag, extension
loading, migrations, writes or checkpoint step. Trusted schema, views and
triggers are disabled; temporary storage is memory-only. The report counts
three connection-local PRAGMAs and up to two integrity queries. No SQL error
text, table name, row content or database version is exported.

This descriptor technique is verified on macOS fdescfs, including replacement
of the original pathname after opening. On platforms where `/dev/fd` resolves
through symlinks, including ordinary Linux procfs layouts, it returns
`database_read_only_unsupported` and does not reopen the original pathname.
That boundary preserves the no-follow guarantee rather than claiming portable
SQLite proof. SQL length, column count, expression depth, compiled operations,
attached databases and worker counts are bounded. A progress callback stops
after approximately one million VM operations or 250 ms, checked each 1,000
operations. This is cooperative work control, not an OS deadline or memory
sandbox.

WAL-mode headers or any `-wal`, `-shm` or `-journal` sidecar yield
`database_snapshot_required` before SQLite opens. A clean, externally produced
consistent export is required for those databases; doctor does not make that
export itself. Descriptor bytes, the named file and sidecar presence are checked
again after inspection. This detects observed changes but cannot establish an
atomic snapshot against an adversarial concurrent writer. A passing local
result is not a migration compatibility, lock availability, liveness or future
health claim.

### Coherent snapshots from an owned connection

`diagnose_database_snapshot(&Connection)` is a separate, explicit library API.
`Runtime::diagnose_storage()` uses it under the mutex of its already owned SQLite
connection. It accepts no pathname, reopens no file, starts no process and creates
no on-disk export. It neither opens a new runtime nor runs migrations. The existing
file-based deep diagnostic retains its WAL and unsafe `/dev/fd` refusals.

The API explicitly starts a deferred read transaction, irrespective of the
caller's default transaction mode, checks the source page count and size, then
uses [SQLite's online backup API](https://sqlite.org/backup.html) to copy that read
view into a private in-memory connection. Committed WAL data is included; another
connection's uncommitted changes are excluded. The read transaction remains held
through all copy batches. A source already in a transaction is rejected without
rolling it back or consuming it. The API executes no source DML or checkpoint.
SQLite retains its ordinary read locks and shared-memory bookkeeping; this is not
a claim that SQLite reads never affect OS metadata.

Snapshots are limited to 4 MiB, 16 pages per copy batch and 513 batches. The copy
budget is checked between batches at 250 ms. The application must separately
bound source I/O and its busy handler; this cooperative check is not an OS deadline.
Busy/locked or incomplete copies return an unavailable/limit error, never a valid
report. Both success and failure release the transaction created by the diagnostic.
The temporary connection is then checked with the same defensive SQLite settings,
integrity queries and VM/time limits as deep mode. No source settings are changed.

The report contains a version, integrity code, SHA-256 of the serialized in-memory
snapshot, byte count and query count. It exports no row, schema, pathname or SQL
error text. A full inspection counts eight PRAGMA queries; transaction controls
and SQLite's internal backup operations are not query-count entries. The digest
identifies the captured bytes, not the current live database after the call.
Integrity does not authenticate the runtime journal, certify schema compatibility,
prove process liveness or authorize execution. `Runtime::verify_integrity()` and
host evidence remain separate checks. The passive runtime CLI below adds a
bounded journal and read-layout inspection. Active native host/hook diagnostics
remain outside these APIs.

Tests cover committed WAL foreign-key violations, uncommitted writes, preservation
of source permissions and active caller transactions, size/page-size limits,
copy-deadline refusal, lock contention, recovery after refusal and runtime ownership.
Linux evidence comes from the same tests in CI, not from the macOS result.

### Passive runtime storage diagnostics

```sh
agent-router doctor --runtime-root /absolute/private/runtime --json
```

`--runtime-root` is exclusive with `--request` and `--deep`. Without `--json`,
the same report is rendered for humans. Exit codes are 0 for passing storage
checks, 2 for a failed check or refusal, and 3 for warnings. This command never
calls `Runtime::open`, runs migrations, recovers interrupted runs, resolves a
lease, releases reservations, repairs a database or starts a host process.
It neither creates a missing runtime nor reads stored prompts into its output.

It opens the existing private database with SQLite's ordinary read-only flags
and takes the coherent memory snapshot described above. It deliberately avoids
`immutable=1`: cancellation can write through another connection even while the
runtime owner lock is available. SQLite may create or update its WAL-index and
read marks, including internal WAL-index recovery after a crash. This is **not**
a zero-filesystem-write guarantee. No application recovery or explicit checkpoint
is requested. See [SQLite read-only WAL](https://sqlite.org/wal.html#read_only_databases)
and [immutable semantics](https://sqlite.org/uri.html#uriimmutable).

The report keeps these observations separate:

| Field | Meaning | Limit |
| --- | --- | --- |
| `database` | Integrity, fingerprint and size of the committed copy | Does not authenticate its source |
| `schema_layout` | Expected version-9 tables and ordered columns are present | A recognized read layout does not prove every write constraint or authenticate schema SQL |
| `journal_integrity` | Event hash-chain and workflow authority-pin consistency on the same copy | A self-consistent replacement journal is not authenticated |
| `counts` | Runs, events, writer reservations, cleanup blocks, preparation cycles and recorded leases | Aggregates of persisted records, not proof of current execution |
| `owner_lock` | Shared-lock attempt was available, busy, missing or unknown | No PID lookup, orphan inference or recovery authority |
| `leases` | Later held, pending, cleanup-marked, missing or ambiguous observations | Markers are not opaque cleanup proofs; this observation is not atomic with the database copy |

Older schemas return `migration_required` without migration. Future versions or
missing expected tables/columns fail the check; counts and lease observations
are omitted for these layouts. A corrupted journal fails separately from SQLite
integrity. Cleanup blocks, pending/missing/ambiguous leases and missing owner
locks produce a warning. A busy owner is normal and does not prevent inspection.
`shared_admission_accounting`, `journal_authenticity`, `process_liveness` and
`cleanup_authority` remain `unknown`
on every successful report, including one whose scoped storage checks pass.
The runtime admission binding and intent tables are included in layout/size checks,
but the separate admission registry and session counters are not inspected.
In particular, a schema-8 runtime is reported as needing migration without opening
it for recovery or creating the two schema-9 tables.

The operator-selected runtime and its ancestors must remain under the operator's
control. The final directory is pinned, private and owned by the current user.
The database, existing SQLite sidecars and owner lock must be regular private
files with one link and the same owner. The main file is limited to 4 MiB; all
SQLite files together to 16 MiB. Identities are checked before and after capture;
new sidecars created by SQLite are checked too. Substitution or disappearance of
an initially present sidecar is refused. These checks do not protect against a
malicious actor with the same user privileges replacing ancestors between calls.
POSIX mode checks do not certify platform ACLs.

Database checks use `statat` without opening extra DB/SHM descriptors. Closing an
auxiliary descriptor can release SQLite's process-wide POSIX locks, even if that
descriptor was read-only. SQLite owns all its database descriptor lifetimes.
The passive lease reader uses a shared nonblocking lock, a 96-byte marker limit
and the recorded identity. It never changes a pending marker to recovered.

Inspection is limited to 4,096 rows per table and 64 KiB per field before journal
parsing or lease materialization. SQLite VM/time limits remain active on the
copy. The `diagnose_database_snapshot_with` callback is trusted linked Rust code, not a sandbox for arbitrary SQL or Rust. It counts its statements and
must preserve connection limits. A changed final fingerprint or an open
transaction is rejected. The checks cannot detect a trusted callback changing
and rolling back data before returning a misleading value, nor bound arbitrary
Rust computation. The historical snapshot wrapper and its JSON shape are unchanged.

Tests cover committed WAL observations, unchanged reservation and pending-lease
records, schema migration refusal, redaction, journal corruption, unsafe paths,
row bounds and a second process remaining blocked by an existing SQLite writer
lock after inspection. Native host activation and orphan cleanup are not covered
by these storage tests.

Deep mode launches no `--version`, authentication or status commands. Supplied
host snapshots retain their own source/freshness checks. An executable's name or
an argument such as `status` does not prove absence of network or disk effects.
Any future active diagnostic must use a separate supervised execution request
and approval bound to its exact binary, arguments, environment, permissions and
effects. `--deep` alone is not such authorization. Hook trust/invocation,
authentication, host sandbox enforcement and process ownership remain unknown
without separately observed evidence. No model call or active hook is used.

Tests cover all component validators, generated and legacy catalog bindings,
trust escalation, same-version content drift, changed bodies, absent databases,
foreign-key violations, WAL/journal refusal, descriptor replacement, symlinks,
bounds, redaction and unchanged database files. Existing audit v2 tests remain
part of the same package gate.

`build_audit_bundle` accepts transient JSON artifacts classified as effective
config, catalog, decision, events, evidence or versions. New exports use audit
schema version 2. Every record retains schema version, entry count, input size,
secret-pattern indicator and the SHA256 of the original bytes, plus a typed
projection with a fixed field allowlist:

| Input kind | Retained projection |
| --- | --- |
| Effective configuration | Hashed source and model identities, capability and mutation restrictions, preferred model hash, cost/token/time/concurrency bounds |
| Phase receipt, optionally wrapped in `receipt` | Selected/abstained/unknown verdict, phase, effort, sandbox and verification enums, required capabilities/mutations, utility and estimated budgets, counts of assets/alternatives/exclusions/unavailable candidates |
| Runtime events or completion | Run/event identity hashes, recognized run and transport event kinds, state, failure, attempts, supplied timestamp, transport termination/exit code/elapsed time, output byte counts and digest references, separate validation and objective outcomes |
| Runtime evidence, optionally wrapped in `runtime_evidence` | Transport outcome, transport success and stream completeness, separate validation and objective outcomes, hashes of executed models and their source evidence, observed effort, event count |
| Versions | Policy/backend/producer version identity hashes; numeric version components for strict one-to-three-part numeric versions; deterministic/Laya/JEV/unknown backend kind |
| Catalog | Version projection, model/skill/asset counts and catalog reference |

Receipt, context, catalog, policy, snapshot, pool, registry, execution definition,
approval, plan, node and artifact references are retained when recognized. Every
free-form identity or declared digest is hashed with the domain
`agent-router:audit-reference:v2\0` followed by its UTF-8 bytes. These values are
correlation keys. They cannot be compared directly to the original declared
digest without applying that transformation. All model identities are hashed;
this API has no independent proof that a catalog or model identity is public.
Hashing does not make low-entropy identities anonymous or prevent dictionary
guessing.

Prompts, credentials, private paths, reasons, provider output and arbitrary
strings never appear as plaintext in the projection. Unknown input fields are
ignored even when nested, and invalid enum values become unknown (`null` or an
explicit `unknown` variant). This is an allowlist, so secret-pattern detection is
not required to suppress raw values. The scanner's boolean remains a separate
heuristic. Model stdout cannot add an authenticated verdict: each projection
always declares `provenance: unverified_input`, regardless of input provenance
or signature claims. A projected `passed` or `selected` is a supplied claim,
not a validator result or execution authorization.

Projection lists have at most 64 entries. Runtime event arrays retain the first
64 and report the total and omitted counts. Overlong configuration lists are
unknown rather than partially represented restrictions. Nonnegative numeric
fields are bounded at 10^12; signed utility is bounded to ±10^18, and exit codes
must fit a signed 32-bit integer. Invalid values are omitted. Identity strings
above 16 KiB are omitted; strict numeric version components have at most four
digits. The input limits above still apply before JSON parsing. New exports
contain no recoverable copy of raw artifacts.

Records form a deterministic SHA256 chain. `verify_audit_bundle` detects edits,
reordering and invalid structure. Passing an independently retained expected
root also detects truncation or complete replacement. Without that trusted
root, chain consistency proves integrity only: it does not prove provenance,
authenticate a writer or constitute a signature. The output explicitly reports
`raw_content_included: false` and `signature_verified: false`.

The v2 chain covers the complete typed projection as well as record metadata.
Verification checks projection kind, digest shapes, numeric/list bounds and
event omission accounting. Deserialized bundles reject unknown output fields
or provenance enum values. `verify_audit_bundle` still accepts original v1
bundles, with their original hash algorithm and no projection. A projection
added to a v1 record, a missing v2 projection or a version-only downgrade is
rejected. No input or legacy bundle is rewritten.
The CLI permits audit bundles up to 32 MiB when verifying, separately from the
4 MiB source artifact limit, because typed projections can expand short input
values into digest references.

Tests cover redaction, determinism, no mutation, stale leases, false health and
sandbox claims, malformed/duplicate JSON, hash drift, path traversal, symlink
parents and leaves, private file permissions, bounds and audit tampering.
Audit tests also cover a fixed v1 fixture, v2 projection edits, downgrade
attempts, distinct transport/validation/objective outcomes, and secret canaries
in every string field of representative inputs, including digest-shaped
provenance values and nested unknown fields.

```sh
cargo test -p router-doctor --locked
```
