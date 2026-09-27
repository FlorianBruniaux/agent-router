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

Sensitive-key and token-pattern detection is heuristic. A match produces a
warning without the matched value. No match does not establish that a file is
free of secrets. Authorization-marker contents bypass this scanner because
they are never read. File mode checks are metadata evidence, not proof of an
effective sandbox or credential validity.

`build_audit_bundle` accepts transient JSON artifacts classified as effective
config, catalog, decision, events, evidence or versions. It projects only fixed
metadata: schema version, entry count, input size, secret-pattern indicator and
content digest. All arbitrary strings and raw content are omitted by default,
including prompts, credentials, private paths and provider output. This is a
redacted inventory of those artifacts, not a recoverable copy of them.

Records form a deterministic SHA256 chain. `verify_audit_bundle` detects edits,
reordering and invalid structure. Passing an independently retained expected
root also detects truncation or complete replacement. Without that trusted
root, chain consistency proves integrity only: it does not prove provenance,
authenticate a writer or constitute a signature. The output explicitly reports
`raw_content_included: false` and `signature_verified: false`.

Tests cover redaction, determinism, no mutation, stale leases, false health and
sandbox claims, malformed/duplicate JSON, hash drift, path traversal, symlink
parents and leaves, private file permissions, bounds and audit tampering.

```sh
cargo test -p router-doctor --locked
```
