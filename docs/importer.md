# Inert importer

`router-import` turns a local directory of Markdown definitions into an
**unsigned candidate** catalog manifest and an import report. It is a library only.
It signs nothing, grants nothing, writes nothing to the catalog store and
activates nothing. See [ADR 0013](adr/0013-inert-importer.md).

## Input

The caller passes `ImportRequest` with an explicit `source_dir`. Nothing scans
`~/.claude`, `~/.codex` or any default location. The directory must be a real
directory (not a symlink) and its path must not contain `..`.

Files are read from `agents/` and `skills/` (depth at most 4). Each `.md` file
starts with `---` YAML frontmatter containing `name` (letters, digits, `-`, `_`,
`.`; no separators, no leading dot or `..`) and `description`.

The operator supplies, never the files:

- `catalog_id`, `catalog_version`, `revision`;
- `license_expression` and `authors`;
- `valid_from_unix_seconds` and `valid_until_unix_seconds`;
- `source_locator`, either a lowercase `https://` URL with a non-empty host and
  no `.` or `..` segment, or a bare identifier (`A-Za-z0-9`, `.`, `-`, `_`; no
  `/` and no `:`). Absolute or relative paths, `file:` in any case, drive paths,
  other schemes, credentials, queries, fragments and whitespace are refused.

## Bounds

- 256 KiB per file, 16 MiB of accepted bodies in total, 16 KiB frontmatter;
- 512 entries per directory, 4096 entries in total, depth 4, 512 bytes per
  relative path (names with control or bidirectional characters are refused);
- at most 256 skills and 512 assets, enforced by `seal_signed_manifest`.

YAML anchors, aliases, tags, directives, duplicate keys, more than 512 nodes and
nesting deeper than 8 are refused.

## Output

`import_directory` returns `ImportOutput { manifest, report }`.

- `manifest`: sealed by `seal_signed_manifest`, `SkillTrust::Untrusted`,
  permission `UNKNOWN`, empty tools, capabilities, mutations and host bindings,
  read-only sandbox, public sensitivity. The body SHA-256 (raw bytes) is in the
  search body reference, the agent `content_sha256` and the entry provenance.
  Bodies stay external.
- `report`: imported assets with the frontmatter keys that were ignored, refused
  files with a reason code, and name conflicts.

Refusal is per file for the reasons below; everything else stops the whole import
(see "Errors that cancel the import").

Refusal reasons: `symlink`, `not_regular`, `unsafe_path`,
`too_large`, `not_utf8`, `unreadable`, `unsupported_file`,
`executable_script_see_r08`, `invalid_frontmatter`, `missing_name`,
`invalid_name`, `missing_description`, `invalid_description`, `duplicate_id`.
A shared name refuses all files using it. If nothing is accepted the import fails
with `import.no_assets`.

Descriptions containing control or bidirectional characters are refused
(`invalid_description`). Script detection uses the last extension, case
insensitively, plus the executable bit: `RUN.SH` is refused, while a
non-executable `x.sh.md` is a Markdown definition and is imported.

## Errors that cancel the import

- an invalid request: missing or blank `license_expression`, `authors`,
  `revision`, `catalog_id`, `catalog_version` or `source_locator`, a duplicate
  author, a non-public locator, an empty validity window;
- a source directory that is a symlink, not a directory, or contains `..`;
- an I/O error while reading the tree;
- a limit: entries per directory, total entries, depth or total bytes;
- no acceptable asset (`import.no_assets`);
- a rejection by `seal_signed_manifest`, for example more than 256 skills;
- an output path that already exists (`import.output_exists`).

`ImportOutput::write_new(manifest_path, report_path)` uses create-new semantics
with mode 0600 and returns `import.output_exists` rather than replacing a file.
If the report cannot be written, the manifest is removed only when the path still
holds the file this process created (same device and inode). Serialization is
deterministic and contains no absolute source path.

## Trust boundary

A candidate loaded without an approved publisher key verifies as `Quarantined`.
Signing needs the publisher's private key and stays outside this library.
Executable skills are not decided here (R08).

## Limits

- **Non-concurrent source.** Each directory level is checked with `lstat` and the
  final file is opened with `O_NOFOLLOW | O_NONBLOCK` (a FIFO swapped in after the
  check is refused without blocking). A writer who replaces an intermediate
  directory by a link between the check and the read is not defended against; no
  `openat` walk is implemented. The importer therefore assumes the source is not
  modified during an import. Impact is bounded: the output holds only digests,
  descriptions and relative names, and a `.md` file with valid frontmatter is
  needed to be read at all.
- **Known defect in the verifier, to fix separately.** `public_source` in
  `crates/router-catalog/src/signed.rs` still accepts `../../home/me/pack`,
  `Users/me/pack`, `FILE:///etc` and `https:///x` as provenance sources
  (reproduced with `seal_signed_manifest` on 2026-09-29; lowercase `file:///etc` is
  refused). The importer applies a stricter check before sealing and does not
  modify the verifier. Other producers of manifests are not protected.
