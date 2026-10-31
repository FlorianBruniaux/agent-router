# Architecture decision records

An ADR records one structural decision: what was decided, why, and what it costs. Write one when a change fixes a contract, a trust boundary, a storage format or a crate dependency that later work has to respect (see `CLAUDE.md`: "Record structural decisions in `docs/adr/`"). Routine changes that follow an existing decision do not need one.

Start from [TEMPLATE.md](TEMPLATE.md), take the next free number and name the file `NNNN-short-title.md`. Never rename an ADR file, because source code cites these paths.

## Index

| ADR | Title | Status | Created | Modified | Amended by |
| --- | --- | --- | --- | --- | --- |
| [0001](0001-project-harness.md) | One project harness, two host projections | implemented | 2026-09-26 | 2026-09-26 | - |
| [0002](0002-bounded-routing-pipeline.md) | Bounded routing and execution boundaries | implemented | 2026-09-27 | 2026-09-27 | - |
| [0003](0003-bound-asset-contracts.md) | Bind asset constraints separately from legacy wire documents | implemented | 2026-09-27 | 2026-09-27 | - |
| [0004](0004-signed-backend-promotion.md) | Separate backend activation proof from deterministic receipts | implemented | 2026-09-27 | 2026-09-27 | - |
| [0005](0005-durable-backend-activation.md) | Pin durable backend authority through dispatch | implemented | 2026-09-27 | 2026-09-27 | - |
| [0006](0006-current-signed-catalog-authority.md) | Persistent current signed-catalog selection | amended | 2026-09-28 | 2026-09-29 | [0017](0017-bounded-retention-and-publish-durability.md) |
| [0007](0007-durable-preparation-cleanup.md) | Durable cleanup evidence for preparation | implemented | 2026-09-28 | 2026-09-28 | - |
| [0008](0008-shared-admission.md) | Explicit shared admission with opaque release | amended | 2026-09-28 | 2026-09-29 | [0009](0009-conservative-session-allocations.md), [0017](0017-bounded-retention-and-publish-durability.md) |
| [0009](0009-conservative-session-allocations.md) | Conservative shared session allocations | implemented | 2026-09-28 | 2026-09-29 | - |
| [0010](0010-passive-runtime-diagnostics.md) | Inspect runtime storage without application recovery | implemented | 2026-09-28 | 2026-09-28 | - |
| [0011](0011-advisory-decision-cache.md) | Authenticated advisory cache of backend proposals | amended | 2026-09-29 | 2026-09-30 | [0017](0017-bounded-retention-and-publish-durability.md) |
| [0012](0012-local-route-api.md) | Route-only local API on an owner-private Unix socket | implemented | 2026-09-29 | 2026-09-29 | - |
| [0013](0013-inert-importer.md) | Import definitions as an unsigned, untrusted candidate | implemented | 2026-09-29 | 2026-09-29 | - |
| [0014](0014-loopback-provider-transport.md) | Loopback-only provider transport behind a Cargo feature | accepted | 2026-09-29 | 2026-09-29 | - |
| [0015](0015-structured-validation.md) | Structured validation as a pure library | accepted | 2026-09-29 | 2026-09-29 | - |
| [0016](0016-adapter-descriptor-registry.md) | Validate adapter descriptors as inert data | implemented | 2026-09-29 | 2026-09-29 | - |
| [0017](0017-bounded-retention-and-publish-durability.md) | Bounded retention and publish durability for stores | accepted | 2026-09-29 | 2026-09-30 | - |
| [0018](0018-incremental-event-chain-verification.md) | Incremental event chain verification by the store owner | implemented | 2026-09-29 | 2026-09-29 | - |
| [0019](0019-outcome-measurement.md) | Outcome measurement | implemented | 2026-09-30 | 2026-09-30 | - |
| [0020](0020-cross-host-routing.md) | Cross-host phase routing | proposed | 2026-09-30 | 2026-09-30 | - |
| [0021](0021-host-capability-evidence.md) | Host capability evidence from supervised canaries | amended | 2026-09-30 | 2026-10-25 | [0022](0022-provider-egress.md) |
| [0022](0022-provider-egress.md) | Provider egress requires an external tool boundary | accepted | 2026-10-01 | 2026-10-01 | - |

| [0023](0023-owned-task-entry.md) | Owned task entry stops before native activation | amended | 2026-10-07 | 2026-10-08 | [0026](0026-owned-client-simulated-entry.md) |
| [0024](0024-accepted-revision-accounting.md) | Accepted revision accounting | implemented | 2026-10-07 | 2026-10-07 | - |
| [0025](0025-run-observation.md) | Bounded passive run observation | implemented | 2026-10-07 | 2026-10-08 | - |

| [0026](0026-owned-client-simulated-entry.md) | Dedicated Codex client for simulated patch work | accepted | 2026-10-08 | 2026-10-08 | - |
| [0027](0027-verified-patch-publication.md) | Publish verified patches through a dedicated Git revision | implemented | 2026-10-08 | 2026-10-22 | - |

| [0028](0028-finite-client-preparation.md) | Bind finite-client preparation to an owned broker | implemented | 2026-10-09 | 2026-10-09 | - |

| [0029](0029-finite-client-owned-assets.md) | Retain reviewed finite-client asset copies | amended | 2026-10-11 | 2026-10-11 | [0032](0032-finite-client-owned-startup.md) |
| [0030](0030-qualified-local-capture-timing.md) | Qualify local supervisor capture intervals | implemented | 2026-10-11 | 2026-10-11 | - |

| [0031](0031-macos-physical-memory-observation.md) | Separate macOS physical-memory observation from hard limits | proposed | 2026-10-11 | 2026-10-11 | - |

| [0032](0032-finite-client-owned-startup.md) | Bind producer-owned finite-client startup state | implemented | 2026-10-11 | 2026-10-13 | - |

| [0033](0033-host-state-writer-exclusion.md) | Require writer exclusion for host-state disposal | implemented | 2026-10-15 | 2026-10-15 | - |
| [0034](0034-offline-evaluation-evidence.md) | Keep evaluator qualification and supplied usage accounting separate | implemented | 2026-10-15 | 2026-10-21 | - |

| [0035](0035-finite-client-namespace-ownership.md) | Transfer finite-client resources to the namespace lifecycle | implemented | 2026-10-15 | 2026-10-15 | - |
| [0036](0036-finite-client-runtime-approval.md) | Bind finite-client preparation to durable runtime approval | amended | 2026-10-16 | 2026-10-16 | [0037](0037-qualified-local-finite-dispatch.md) |
| [0037](0037-qualified-local-finite-dispatch.md) | Execute approved finite clients through a qualified local Linux backend | implemented | 2026-10-16 | 2026-10-16 | - |

| [0038](0038-inert-finite-tools-and-output-creation.md) | Prepare finite output creation and inert local tool primitives | implemented | 2026-10-16 | 2026-10-16 | - |

| [0039](0039-finite-tool-guard-canaries.md) | Test approved finite tool guard modes before runtime integration | implemented | 2026-10-16 | 2026-10-16 | - |
| [0040](0040-supervised-finite-mcp-workers.md) | Reuse runtime supervision for finite MCP workers | accepted | 2026-10-17 | 2026-10-17 | - |
| [0041](0041-native-finite-mcp-component.md) | Qualify the native finite MCP component | accepted | 2026-10-17 | 2026-10-17 | - |

| [0042](0042-owned-subscription-relay.md) | Bind subscription transport and observations to the finite Runtime owner | proposed | 2026-10-18 | 2026-10-18 | - |

| [0043](0043-finite-artifact-verification.md) | Explicit verification of a finite private artifact | accepted | 2026-10-21 | 2026-10-22 | - |

| [0044](0044-finite-context-profile.md) | Bind finite context policy to a distinct native profile | proposed | 2026-10-21 | 2026-10-21 | - |
| [0045](0045-fresh-finite-entry.md) | Fresh entry through the existing finite Runtime | accepted | 2026-10-22 | 2026-10-22 | - |
| [0046](0046-finite-controller-application.md) | Explicit controller acceptance and private candidate application | proposed | 2026-10-23 | 2026-10-23 | - |
| [0047](0047-explicit-finite-application-resume.md) | Explicit finite application resume on an intact preimage | proposed | 2026-10-24 | 2026-10-24 | - |
| [0051](0051-advisory-task-route.md) | Advisory route for one normalized task | implemented | 2026-10-28 | 2026-10-28 | - |


## Frontmatter fields

Every ADR starts with a YAML frontmatter block. GitHub renders it as a table.

| Field | Meaning |
| --- | --- |
| `adr` | Four-digit number, quoted, equal to the file name prefix. |
| `title` | Title in sentence case. The H1 is `# ADR NNNN: <title>`. |
| `status` | One value from the status list below. |
| `created` | ISO date (`YYYY-MM-DD`) of the commit that added the file. |
| `last_modified` | ISO date of the last change to the text of the decision. |
| `scope` | One short phrase: what the decision covers and any limit on its status. |
| `amended_by` | ADR numbers that amend this one. |
| `supersedes` | ADR numbers this one replaces. |
| `superseded_by` | ADR numbers that replace this one. |
| `related` | Other ADR numbers cited in the body. |

Cross references are reciprocal. If A lists B in `supersedes`, B lists A in `superseded_by`. If A lists B in `amended_by`, B lists A in `related`.

## Status values

| Status | Use |
| --- | --- |
| `proposed` | Written, not yet accepted. |
| `accepted` | Decided. Code may be partial, see `scope`. |
| `implemented` | Decided and fully implemented. |
| `amended` | Still in force, with parts changed by later ADRs listed in `amended_by`. |
| `superseded` | Replaced. `superseded_by` is not empty. |
| `rejected` | Considered and not adopted. |

## Rule for `last_modified`

`last_modified` is the date of the last change to the text of the decision. A change of format only (frontmatter, heading style, index, typo fixes that leave the meaning unchanged) does not move it. The 2026-09-30 normalization that added the frontmatter kept the dates taken from `git log --follow`.

## Checking the ADRs

```sh
python3 scripts/check-adrs.py
python3 scripts/test-check-adrs.py
```

`check-adrs.py` exits 0 when everything passes and 1 otherwise, with one message per problem on stderr. It checks:

- the frontmatter is complete, with an `adr` equal to the file number;
- `status` belongs to the enum, dates are ISO and `created` is not after `last_modified`;
- the H1 matches `adr` and `title`;
- referenced ADRs exist and the cross references are reciprocal;
- every ADR appears in the index above with the same title, status, dates and amendments.

The script does not read git. Commit dates cannot be verified in CI with a shallow clone, so the dates are checked for format and order only. `TEMPLATE.md` and this file are excluded from the ADR checks.
