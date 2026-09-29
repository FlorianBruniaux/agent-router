# 0016: Validate adapter descriptors as inert data

Status: accepted for the descriptor validator in `router-adapter-api`.

## Context

`integrations/codex/adapter.json` and `integrations/claude-code/adapter.json`
describe packaged adapters. No Rust code parsed them. A registry that accepted
them loosely could be read as an activation or trust mechanism.

## Decision

A descriptor is data, never authority. `AdapterDescriptor::parse` and
`AdapterRegistry::from_documents` accept caller-supplied bytes only. They do not
read the file system, load or resolve `entry_point`, install anything or verify
a signature.

- Parsing is strict and bounded: unknown fields and repeated JSON keys are
  refused, a document is at most 16384 bytes and a set at most 32 documents.
- `activation` must be exactly `none`.
- `native_plugin_manifest` must be `false`. Native plugin proof is a separate
  criterion and is not accepted by this slice.
- `requires` must contain at least `validated-phase-receipt`,
  `bound-authorization` and `supervised-sandbox`, which are the gates of the
  execution path. A descriptor may add entries but cannot lower the minimum.
  The field remains declarative: nothing here enforces it at run time.
- The capability matrix is complete and UNKNOWN. A descriptor may restate
  `unknown`; any `supported` or `unsupported` claim is refused, because support
  is evidence, not declaration.
- `entry_point` is text of the form `crate_name::Item`. Its first segment must
  equal the declared `crate` with `-` replaced by `_`. The grammar cannot express
  a path, so absolute paths, `..` and separators are refused.

### Reserved names

The reserved names (`RESERVED_NAMES`) and prefixes (`router_`, `agent_router`)
are a choice of this slice. They are not derived from an external registry and
can be extended by a later ADR. They apply to both `host` and `crate`.

### Normalization

Cargo treats `-` and `_` as equivalent in a library name. Reserved checks and
conflict checks (host, crate) therefore compare the form with `-` replaced by
`_`. `adapter-codex` and `adapter_codex` conflict, and `router_types` is as
reserved as `router-types`. Because the entry point is bound to the crate, a
crate conflict also covers entry-point conflicts.

### `host` is not an identity

`host` is a label chosen by the descriptor author. Without a signature, nothing
proves that a descriptor for `codex` was produced by the Codex adapter. Signing
by publisher key belongs to the same trust-authority family as the `catalog-active`
CLI, which remains closed (`docs/catalog-active-cli.md`, "Delivery boundary"), and
is out of this slice.

## Out of slice

- Signature or publisher-key verification.
- Installation, activation or any file system access by the validator.
- Native plugin proof.
- Resolving, loading or executing `entry_point`.
- Enforcing `requires` at execution time.

## Consequences

A validated descriptor grants nothing. Any later consumer must still pass the
receipt, authorization and sandbox gates described in `integrations/README.md`.
