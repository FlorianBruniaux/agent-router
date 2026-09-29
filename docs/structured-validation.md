# Structured validation

`router-validation` evaluates a bounded, typed contract against a view supplied
by the caller. It performs no I/O, spawns no process and has no command runner.

## Contract v1

```json
{
  "version": 1,
  "scope": ["src", "docs/x.md"],
  "assertions": [
    {"kind": "changed_paths_within_scope"},
    {"kind": "path_present", "path": "src/lib.rs"},
    {"kind": "path_absent", "path": "src/old.rs"},
    {"kind": "sha256", "path": "Cargo.lock", "sha256": "<64 lowercase hex>"},
    {"kind": "json_pointer_equals", "path": "a.json", "pointer": "/k/0", "value": 1},
    {"kind": "max_size", "path": "a.json", "max_bytes": 4096}
  ]
}
```

Unknown fields, unknown kinds and versions other than 1 are rejected.
`changed_paths_within_scope` requires a non-empty `scope`; a path is in scope
when it equals a scope entry or lies below it on a `/` boundary.

## Bounds

| Limit | Value |
| --- | --- |
| Contract bytes | 64 KiB |
| Assertions / scope entries | 64 / 64 |
| Path / pointer bytes | 512 / 512 |
| JSON nesting (contract, expected value, inspected file) | 32 |
| View: changed paths / files | 4096 / 1024 |
| File bytes in the view, `max_bytes` | 4 MiB |

Paths are relative and `/`-separated: no empty, `.` or `..` component, no
leading or trailing `/`, no backslash, no NUL. Nesting depth is checked on the
raw text before any recursive parse, ignoring brackets inside strings.

## Evaluation and report

`parse_contract` returns a `ValidatedContract` and the sha256 of its exact
bytes. `evaluate` takes it with a `ValidationView` (changed paths and file
contents; a missing file key means the path is absent) and returns a
`ValidationReport`.

- Assertion failures (missing file, invalid or too-deep JSON in the view,
  mismatch) are recorded as failed results with a `FailureCode`, not as errors.
- A malformed contract or view is an `Err`.
- `validation` is `passed` only if every assertion passed.
- `objective` is always `unknown`: satisfying a structural contract is not
  evidence that the task goal was met. The type has a single variant.
- The report holds no file content. It carries `contract_sha256`, `view_sha256`
  (over sorted changed paths and per-file sha256) and `evidence_sha256`, the
  sha256 of the serialized report body. `evidence_sha256` has the format
  expected by `VerificationRecord.evidence_sha256` (64 lowercase hex).
  `record_verification` does not recompute it and additionally requires
  `artifact_sha256` and an explicit verifier, which the report does not carry
  (R13-b). Serialization is deterministic. Independence from input order holds
  for the view only: assertion order follows the contract.

## Limit

The evaluator proves a statement about the view only. Linking the view to the
real artifact (who read the files, at which revision) is done at the R13-b
junction and is not part of this crate. Until then a passing report attests
nothing about the working tree. `json_pointer_equals` uses `serde_json::Value` equality, so `1` and `1.0`
differ. A path such as `C:/x` passes path validation; R13-b must handle it.
Not covered: command execution, glob scopes,
JSON pointer comparison beyond value equality.
