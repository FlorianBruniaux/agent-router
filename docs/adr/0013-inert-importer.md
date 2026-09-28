# 0013: Import definitions as an unsigned, untrusted candidate

Status: accepted for the `router-import` library. No CLI subcommand.

## Context

Phase 7 requires importers with provenance, licence, quarantine and idempotent
output (R23). Third-party agent and skill definitions carry frontmatter keys such
as `permissions`, `tools` and `model`, and may ship scripts. An importer that
mapped those keys to catalog fields, or that signed its own output, would turn
untrusted text into authority.

## Decision

`router-import` reads an explicitly passed local directory (`agents/`, `skills/`)
and returns an unsigned `SignedCatalogManifest` built through the existing
`seal_signed_manifest`, plus a report. It has no signing key, grant, catalog-store
or activation code.

- Licence, authors, validity window, revision and public source locator come from
  the operator. Missing values refuse the import. Frontmatter is read for `name`
  and `description` only; every other key is listed in the report and discarded.
- Every entry is `untrusted`, permission `UNKNOWN`, with no tools, capabilities,
  mutations or host bindings and a read-only sandbox at public sensitivity.
- Symlinks, non-regular files, unsafe names, oversized or non-UTF-8 files,
  unsupported extensions, executable files and script extensions are refused per
  file and reported. Request errors, a bad source directory, I/O errors, limits
  (including 16 MiB of bodies), an empty result, a `seal_signed_manifest`
  rejection (256 skills, 512 assets) and an existing output path cancel the whole
  import; `docs/importer.md` lists them. Scripts point to R08. A shared name refuses every file that
  uses it. YAML anchors, aliases, tags and directives are refused.
- Output is deterministic and contains no source path. Files are created with
  create-new semantics, mode 0600, and never replaced.
- The locator check is stricter than the catalog verifier's `public_source`,
  which still accepts relative paths and `FILE:` (recorded in the importer doc).
- Source concurrency: no `openat` walk. The final file is opened with
  `O_NOFOLLOW | O_NONBLOCK` and directories are checked with `lstat`, so the
  source is documented as non-concurrent. Rationale: the importer is inert, its
  output carries digests and descriptions only, and an attacker able to modify
  the source can already choose its content.

## Consequences

An unknown publisher key gives `Quarantined`. Verification and grants stay with
the external publisher process and the R06 authority CLI, which this change does
not open. A signature over the candidate still attests only the declarations of
its publisher, not the safety of the bodies.
