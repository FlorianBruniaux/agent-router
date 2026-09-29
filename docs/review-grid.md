# Review grid

Checklist for a broad review of the workspace. The `rust-code-reviewer` and
`rust-security-reviewer` instructions point to it when the parent asks for more
than a diff review; nothing enforces its use.
Each finding cites `file:line`, is classified as blocking, should-fix or note,
and states whether it was verified by reading the code path or only inferred.
Each item ends as PASS, FAIL or UNKNOWN; a static reading never yields PASS for
runtime behavior.

## Errors and panics

1. No `unwrap`, `expect`, `panic!`, `unreachable!`, `todo!` or unchecked indexing in fallible library paths (tests and proven infallible constants excepted, and named).
2. Library errors are typed `thiserror` enums; CLI boundaries add operation context, and `anyhow` appears there only if already needed.
3. No swallowed failure: `.ok()`, `unwrap_or_default()`, `let _ =`, `Err(_) => {}` on a fallible result must be justified or replaced by a typed error or explicit UNKNOWN.
4. Missing evidence is reported as UNKNOWN, never defaulted to a value that looks like a real result.
5. Every `?` crossing a module or I/O boundary keeps enough context to identify the failing operation without leaking secrets.

## Determinism and bounds

6. Output order is deterministic: no `HashMap`/`HashSet` iteration, timestamps, randomness or locale dependence in serialized output without explicit sorting.
7. Every read of untrusted input has a size cap (files, stdin, subprocess output, JSON depth and length).
8. Every loop, recursion and collection fed by external input has an explicit work limit.
9. Slicing of untrusted text uses char-safe APIs (`get(..)`, `char_indices`), not raw byte offsets.
10. Arithmetic on external values uses checked or saturating operations; truncating `as` casts are justified.

## Security

11. No shell string execution; subprocesses use `Command::new(program).args(...)`, an allowlisted program and a scrubbed or explicit environment.
12. Paths from input are validated, symlinks rejected where required (TOCTOU considered), never joined blindly with `..`.
13. No secret, token or raw provider response in logs, `Debug` output, errors or test snapshots.
14. No permission escalation: a request never widens scope; approval digests and capability evidence are checked for freshness before use.
15. `unsafe` is absent or has an adjacent SAFETY comment and a test; no `transmute` or `from_raw_parts` on external lengths.

## Contracts and types

16. Serialized contracts follow the trust and compatibility rules of `docs/reference/json-contracts.md`; external input rejects unknown fields where the contract is strict.
17. Invalid states are unrepresentable where cheap: newtypes for identifiers, enums instead of stringly-typed modes.
18. Public APIs do not expose foreign error types needlessly.
19. Crate boundaries match CLAUDE.md: no product code depends on `xtask`; feature evidence formats stay with their validating library.

## Concurrency

20. No `std::sync` lock guard held across `.await`; no blocking I/O or sleep inside async code.
21. Shared mutable state is justified, lock poisoning handled without a panic; file locks and SQLite transactions cover the whole read-modify-write.

## Tests and evidence

22. Each public behavior has negative and boundary tests (empty, oversized, malformed, non-UTF-8).
23. Tests assert observable behavior; no tautological assertion (`is_ok() || is_err()`), no test that passes when the code under test is deleted.
24. Fixtures are real or contract-derived, separate from product fixtures, without real credentials.
25. Docs and comments do not claim a guarantee the code does not enforce (docstring vs code contradiction).
26. The verdict rests on executed evidence run by the parent: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`, `cargo test --workspace --all-features --locked` and `cargo run -p xtask --locked -- check`.

## Simplicity

27. No speculative abstraction: a trait, generic or registry with a single implementer, or a dependency replaceable by std, is a finding.
28. Clones, allocations and `collect` in bounded/hot paths are justified; dead code and duplicated logic across crates are reported.
