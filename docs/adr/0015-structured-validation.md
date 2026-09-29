# 0015: Structured validation as a pure library

Status: accepted for `router-validation`.

## Context

`WorkflowNode.verification` is a list of free strings and `record_verification`
stores an external attestation. Nothing yet turns a structural check into
bounded evidence.

## Decision

Add a crate with no I/O that evaluates a versioned, `deny_unknown_fields`
contract of typed assertions against a caller-supplied view. Every input is
bounded (see `docs/structured-validation.md`). The report is deterministic and
carries its own digest in the format of `VerificationRecord.evidence_sha256`.
`record_verification` does not recompute it and also requires `artifact_sha256`
and an explicit verifier, which R13-b must supply.

`objective` is a single-variant enum fixed at UNKNOWN: a structural contract
cannot establish the task goal. No command runner is added.

## Consequences

- The report is only as trustworthy as the view. Binding the view to the actual
  artifact belongs to R13-b, and a report must not be treated as an attestation
  of the working tree before that.
- Only equality on a JSON pointer is supported. Richer predicates need a new
  contract version.
