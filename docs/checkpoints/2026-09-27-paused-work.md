# Paused development checkpoint, 2026-09-27

This branch preserves the local work at the user-requested stop. Its base is
main at `5525b1205d2db2a36d5b8fa6312f66a6756dbc9d` (PR #20). PRs #14 through #20
are already merged. This checkpoint is not a completed or merge-ready release.

## Preserved work

- Durable escalation: attempt accounting, explicit parent bounds, single-use
  reservation, approval and native sandbox tests. The separately reviewed R05
  snapshot passed 60 ordinary tests and four native macOS canaries. A helper-path
  correction also passed those four canaries with a separate build directory.
- Signed catalogs: seven families, externally approved publisher keys, scoped
  grants and guarded projections. The last focused suite passed 38 tests; an
  independent regression confirmed that unsupported signed mutation ceilings
  must be refused by the legacy phase-registry projection.
- Advanced plan generation: bounded generator, distinct generated execution
  decisions, CLI and runtime integration, and workflow-level proposal pins.
  Integration and SQLite schema-7 migration tests were still in progress.
- TOML/YAML configuration: parser, CLI wiring and test cases preserved. Full
  behavioral validation had not completed at the stop.
- Contextual learning: dependency edges prepared; implementation is not present
  in this checkpoint.

The test results above apply to their tested subsets and source versions. The
combined checkpoint has not passed the complete workspace or native CI suites.
Its formatting and integration may still need correction. Development agents
were stopped, and no automatic continuation was created.

## Resume

Review the current diff, complete generation/migration and configuration tests,
then run formatting, strict workspace Clippy, all-feature workspace tests,
harness parity, secret scanning and the applicable native macOS/Linux canaries.
Do not infer provider identity, pricing, human quality or objective success from
synthetic workers. Rebuild distribution artifacts for the eventual final SHA.

The full remaining scope and external evidence requirements are tracked in
[remaining acceptance](../remaining-acceptance.md) and the
[completion ledger](../completion.md). Issues #5, #8 and #11 remain open at this
checkpoint; closing them requires their actual evidence.
