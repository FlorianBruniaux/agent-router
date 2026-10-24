# Finite patch presentation before approval, 2026-10-24

The finite application and resume commands now present the exact captured text
before requesting an application approval. The runtime rereads the artifact,
validates its digest and compares the entire presentation against the current
review before entering the existing application checks. Approval domains and
persisted review/context formats are unchanged.

Source: [`71fcf6f`](https://github.com/FlorianBruniaux/agent-router/commit/71fcf6f12b5da49ffffb392c9e51a870228a0b28).
The three prerequisite PRs were merged in order: #208, #209 and #210. Each merged
tree equals its reviewed head, as recorded in [checks.json](checks.json).

## Evidence

- The full local workspace reports 2,010 passes, zero failures and 43 ignored
  results across 199 summaries, including nested helpers. Ignored tests are not
  successes. Formatting, warnings-denied Clippy, harness parity and Gitleaks pass.
- The final review changes only a conditional-compilation guard, a test name and
  one documentation clarification. Final formatting, default/all-feature Clippy,
  all five output protocol tests and Gitleaks pass. [source.json](source.json)
  distinguishes the full-suite snapshot from the final source by path/hash.
- Runtime tests reject changed display text even with a recomputed declared
  hash, changed receipt/candidate/run/source, missing/replaced/symlinked patches,
  and replacement after a resume presentation. Refusals preserve source bytes
  and journal history. Positive tests apply exactly once.
- The output test exercises the actual frame writer with 131,072 bytes requiring
  worst-case JSON escaping. Both frame types preserve the decoded artifact
  digest under the existing 1 MiB bound. It does not establish full native CLI
  ordering. Existing bounded-output and challenge tests also pass.

## Limits and next acceptance

No new Linux native run was performed for this changed binary. Earlier Linux
proofs remain bound to their original source and executables. No model, provider
account or real registry was used. The 8 GiB contract and private accesses are
unchanged. Local evidence is separate from billing-blocked remote CI.

The resume diff describes the original candidate, not a delta computed from a
currently divergent file. States without an approval digest still cannot write.
Display-only envelopes grant no worker, publication or filesystem authority.

#132 and #11 remain open. Next qualify the inline display and approval sequence
in the composed Linux lifecycle. Automatic host activation, the remaining native
lifecycle matrix, multi-file qualification and human acceptance are still
separate work. No savings or general completion claim follows from these tests.
