# Native hook failure observations, 6 October 2026

Codex 0.159.2 resumes the parent after all five tested unexpected hook failures.
**Do not dispatch child work from this hook when the requirement is zero parent
inference after a hook failure.** This result applies to the pinned macOS binary;
it does not establish another version's behavior.

The [sanitized record](result.json) preserves the eight fresh-session observations.
The [diagnostic](README.md) explains reproduction, trust and cleanup. The hook
was trusted and then disabled through the actual native `/hooks` interface in
an isolated disposable profile. No global hook was installed.

| Fixed case | Hook status | Parent HTTP requests | Observation |
| --- | --- | --- | --- |
| Valid JSON block | blocked | 0 | Intended block works |
| Exit 1 | failed | 1 | Parent continues |
| Malformed JSON | failed | 1 | Parent continues |
| Timeout | failed | 1 | Parent continues after native timeout |
| SIGTERM | failed | 1 | Parent continues |
| Exit 2 with stderr reason | blocked | 0 | Explicit blocking convention works |
| Exit 2 without reason | failed | 1 | Parent continues |
| Disabled through native UI | absent | 1 | Fresh session does not invoke the hook |

All six parent HTTP requests reached the rejecting loopback provider and received
HTTP 503. There was no inference and no router child. The parent turns failed
because of that fixed provider refusal; those failed turns are not evidence that
the hook stopped them. Every case has complete correlated output and verified
owned-process/provider cleanup. Both UI sessions exited normally within their
180-second and 2 MiB capture bounds. Preparation recorded zero provider contacts.

For timeout, native evidence reports `hook timed out after 1s`, 1,004 ms, and no
late-return journal row. The classifier conservatively leaves
`timeout_failure_refusal` unknown; the directly observed parent request suffices
to reject a safe-refusal claim in this case.

No duplicate execution occurred in this experiment. The demonstrated failure
boundary allows a parent request after a hook failure; if a hook had already
dispatched a child, that continuation could duplicate work. Catching ordinary
exceptions does not cover every interruption of the hook process.

The native hook run ID also repeats across these distinct sessions. Any future
deduplication must include session/thread, turn and event, with content binding.
The hook run ID alone is not an occurrence identifier.

## Next useful step

For applied routing, extend the explicit U06 handoff with an entry point that
owns submission: user task, CLI decision, one native session, correlated result
and measurement. No parent turn has already started on that path. Before real
inference, prove atomic occurrence reservation, no automatic retry after an
uncertain result, decision/result identity, bounded provider sending and the
[personal trial inputs](../calibration-v2/personal-trial.md). This entry point is
a proposal, not an implemented or admitted replacement for U07.

A hook that only displays a local recommendation remains a distinct option. It
must disclose that the recommendation is not applied and must never launch child
work. Its failure would leave the user's ordinary parent workflow in place;
it would not prove automatic model selection or savings.

## Evidence limits

There is one observation per case, with a pinned local host and no real provider.
No Agent Router runtime receipt, full workflow, quality result, cost saving,
desktop integration, Claude integration or public release is established.
The native UI may check for updates; this experiment claims zero inference,
not OS-wide egress isolation or zero network traffic. Raw profiles, protocol
messages and terminal captures remain in a verified private archive; its hash
accompanies the sanitized record. The disposable profile was removed after
a final process-absence check, successful provider-port rebind and archive
content verification.
