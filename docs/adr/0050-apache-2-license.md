---
adr: "0050"
title: License the project under Apache-2.0
status: proposed
created: 2026-10-28
last_modified: 2026-10-28
scope: "Project license, NOTICE, SPDX headers and the SPDX header check"
amended_by: []
supersedes: []
superseded_by: []
related: []
---

# ADR 0050: License the project under Apache-2.0

## Context

The repository was published under `FSL-1.1-ALv2` (Functional Source License,
Apache 2.0 future license) and became public on 2026-10-28. The owner's
priorities are attribution and adoption by companies. Several points weigh
against keeping FSL:

- FSL is not an OSI-approved license, and some companies refuse to adopt or
  contribute to such code.
- Each FSL version converts to Apache 2.0 two years after publication anyway,
  so the restriction on competing use is temporary.
- Apache-2.0 carries an express patent grant, which FSL covers only for its
  permitted uses.

## Decision

From the commit that merges this ADR, new versions are licensed under
`Apache-2.0`:

- `LICENSE.md` holds the unmodified Apache License 2.0 text. The file name is
  unchanged because the packaging scripts and the Homebrew formula install it
  by that name.
- `NOTICE` keeps the copyright line and asks that redistributions carry it, as
  section 4(d) of the license requires of derivative works that distribute.
- The workspace declares `license = "Apache-2.0"`.
- Rust, Python, C and shell sources carry an SPDX header. A `REUSE.toml` with
  REUSE-style annotations covers files that cannot carry one, frozen evidence
  under `docs/bench/`, and files whose bytes a test or `xtask` hashes or embeds.
- CI runs `scripts/check-spdx-headers.py`, which fails when a tracked `.rs`,
  `.py`, `.c` or `.sh` file has neither the header nor an annotation.

The project does not claim REUSE compliance and does not run `reuse lint`. That
tool requires a `LICENSES/` directory, which collides with the existing
`licenses/` directory (dependency license evidence, part of the archive layout)
on case-insensitive file systems such as the default macOS one.

## Consequences

- The change is irreversible for published code. Versions already obtained
  under Apache-2.0 stay Apache-2.0 whatever the owner decides later.
- Anyone may fork and sell the software, including in a competing commercial
  product, provided they keep the license text and `NOTICE`.
- Historical records (dated ADR text, `docs/bench/`, earlier changelog entries)
  still describe FSL as it was at the time.
- Dependency license evidence is unchanged and remains separate from the project
  license.

## Rejected options

- **Keep FSL-1.1-ALv2.** It protects against competing products for two years
  only, and costs adoption among companies that accept OSI licenses only.
- **AGPL-3.0.** It would deter the company adoption this change aims at.
- **MIT.** It has no patent grant and no NOTICE mechanism, so it gives weaker
  attribution than Apache-2.0.
