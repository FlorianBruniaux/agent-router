---
name: ar-session-handoff
description: "Agent Router session handoff: record scope, branch, owned changes, observed checks and the exact next action without asserting runtime activation."
---

# ar-session-handoff

## Triggers

Ending or transferring a bounded development task, or writing a reviewer handoff.

## Non-triggers

Publishing a release, sending messages without authorization, creating global memories.

## Inputs

Issue, workspace path, branch, base and current SHA, changed paths, check results and remaining blockers.

## Steps

1. Read current status and diff without staging. Done when owned and unrelated files are distinguished.
2. Record check commands, results and unverified host behaviors separately. Done when every success has observed evidence.
3. Write a handoff with scope, paths, branch/SHA, risks and one exact next action. Done when another agent can resume without inferring permissions.

## Checks

Run from the repository root, using `rtk` when available.

```sh
git status --short
git branch --show-current
git rev-parse HEAD
git diff --check
cargo run -p xtask --locked -- check
```

## Stop

Do not send or publish the handoff without task authorization. Never describe installed, discovered, invoked and verified as equivalent.

This is a textual skill, not an executable extension. Host permissions still apply.
