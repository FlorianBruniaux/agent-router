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
