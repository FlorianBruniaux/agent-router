# Changelog

All notable changes to this project are documented in this file.

The format follows Keep a Changelog 1.1.0:
https://keepachangelog.com/en/1.1.0/

This project has no published release yet. Every entry below is under
`[Unreleased]`, and no version number or release date has been assigned.

## [Unreleased]

### Breaking

- **BREAKING:** Signed catalogs whose provenance source is `owner/repo`, `scheme:id` or a local path are now invalid. A source must be a lowercase `https://` URL (ASCII host, optional port 1 to 65535, path segments limited to `A-Za-z0-9._~-`, no dot segments, percent escapes, credentials, query strings or fragments) or a bare identifier without `/` or `:`. `verify` and sealing fail with a hard `Invalid` error before the publisher lookup, with no `Quarantined` state and no `schema_version` bump (it stays 1). A catalog already signed with a refused source must have its source fixed, then be re-sealed and re-signed. The repository's signed catalog fixtures still pass with the new rule (workspace tests on `e456214`). (#46)
- **BREAKING:** The unsigned catalog document, `ModelSpec`, `SkillSpec`, `RouteRequest`, `TaskContext`, `HostSnapshot`, `SkillCandidate`, `EffectiveRestrictions` and the BM25 candidate document now reject unknown keys (`deny_unknown_fields`). Producers that added extra keys must remove them; a per-candidate `path` stays accepted and ignored in the candidate document. Unsigned catalogs also apply the signed label and text rules: ids in `[A-Za-z0-9-_.:/]`, no control or bidirectional characters in the version, model source or skill description.
- **BREAKING:** `public_source` is hardened: loopback, private, link-local, `0/8`, `100.64/10`, multicast, `localhost` and `*.local` literal hosts, integer or hexadecimal address spellings, and bare identifiers that do not start and end with a letter or digit are refused. Signed manifests that used such a source now fail sealing and verification and must be re-sealed and re-signed.
- **BREAKING:** Importer report paths escape control and bidirectional characters as `\u{..}` and a backslash as `\\`, and `source_locator` is checked by the signed verifier predicate.
- Retrieval queries accept at most 8 distinct quoted phrases; more is refused with `Limit("query phrases")`.

### Added

Routing, policy and catalogs

- `public_source` is now public in `router-catalog`, so the importer and other producers share the signed verifier's provenance check.
- Validated routing and offline evaluation foundation: passive host probing with explicit snapshots, provenance-bearing catalogs and aliases, local BM25 metadata retrieval, immutable phase routing with receipts, and replay and shadow evaluation. (#14)
- Contextual retrieval and evidence-aware task handling, with risk floors and human-readable CLI output. (#20)
- Validation of generated plans and an Omnigent routing bridge. (#21)
- Approved standalone escalation from the CLI. (#22)
- Escalation of coordinated workflow attempts in the runtime. (#25)
- Persistent storage of approved current signed catalogs. (#23)
- Enforcement of signed catalog constraints before scoring. (#27)
- Revalidation of signed catalog authority around preparation assembly. (#31)
- Inert importer that turns a local directory of Markdown definitions into an unsigned candidate catalog manifest. Library only: it signs nothing, writes nothing to the catalog store and activates nothing. (#47)
- Local ranking from caller-supplied vectors. (#51)
- Opt-in advisory decision cache in a new `router-advisory-cache` library. Library only, no CLI wiring: it stores authenticated backend proposals and never becomes authority. (#50)
- Review-edge restriction to distinct declared providers, as a pure function in `router-policy`. Library only: no CLI command, runtime gate or contract field uses it yet. (#53)

Execution, workflows and runtime

- Approved isolated execution with private artifacts. (#15)
- Coordination of approved workflow steps with process cleanup proof. (#16)
- Execution of signed backend decisions over pinned optional host transports. (#19)
- Model-proposed plan review without approval. Library only, no CLI entry point. (#55)
- Typed structured validation contracts in a new `router-validation` crate. Library only: no I/O, no process spawning, no command runner. (#52)
- Typed multimodal input and JSON output contracts in `router-asset-contracts`. (#56)
- Inert validation of adapter descriptors in `router-adapter-api`. Descriptors are data, never authority, and `activation` must be `none`. (#54)

CLI and integrations

- Route-only local API, available through `agent-router serve` only when built with `--features local-api`. It returns routing decisions and cannot execute, record feedback, activate a backend or read the catalog store. (#48)
- Read-only `doctor` inspection of coherent owned SQLite snapshots. (#26)
- Standard sidecar client with bounded ranking and consented exports. Optional outbound client, absent from the default build. (#28)
- Cross-host Rust project tooling for Codex and Claude Code development. (#13)
- Comparison of cohort quality and cost tradeoffs through `agent-router eval tradeoffs`. Descriptive only: it does not certify that a compared policy produced an executed plan. (#44)
- Integration candidate combining guided task intake and export, `doctor` inspection of runtime storage without recovery, shared persistent admission quotas, shared conservative session budgets, exact cohort cost budget and calibration metrics in evaluation, publisher policy read guards with review reconstruction, and reproducible local native binary exports. (#43)

Release and supply chain

- Verifiable local release recipe: `scripts/release-local.py` binds source and binary archives to a manifest, verifies an externally produced signature and renders a Homebrew formula. It never generates a key, signs, publishes or installs, and it does not mark any candidate as final. (#49)
- CI verification of complete dependency license evidence for distribution exports. (#29)

### Changed

- README redesigned, with a reproducible routing demo. (#18)
- Documentation shows the delivered sidecar, signed preparation and dependency gates. (#33)

### Fixed

- Preparation authority and quarantine are retained until cleanup is proven, and probe cancellation is preserved during final output collection. (#30)
- Two environment-dependent workspace test failures. The competing-processes budget test now writes its result through a staging file and rename, so the parent no longer reads an empty file under load. (#45)
- Test-only: stabilized historical expiry setup in runtime tests. (#17)
- Test-only: added diagnostics for native version cancellation failures in runtime tests. (#24)

### Security

- CI audit of locked dependencies against fresh RustSec data. (#32)

## Before the pull request workflow

The first commits landed directly on `main` before pull requests were used (2026-08-28 to 2026-08-30). They define the core design and plan, the deterministic routing contracts, catalog validation, hard routing constraints, a versioned BM25 candidate boundary, the `route` and `explain` commands, a deterministic routing fixture matrix, and bounds on routing input, catalog size and combined routing work. Individual commits are listed in the Git history.
