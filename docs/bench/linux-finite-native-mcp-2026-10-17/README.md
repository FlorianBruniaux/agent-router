# Native finite MCP qualification, 2026-10-17

This slice connects the pinned native SDK to the finite MCP owner under the existing process supervisor. The broker supplies written Responses, with no model, account, paid API or external network. **Runtime Tools admission remains disabled.** The legacy Linux composition also refuses the new startup configuration.

The preliminary replay passed all six public packets: t02 H0/H1 returned 3/4 real tool results, t05 H0/H1 returned 3/4, and t08 H0/H1 returned 5/4. These are 23 returned results, not 23 model calls. Each namespace stopped and its fresh private home was disposed under the exact retained lease. t05 created its initially absent review.json. H0 scratch sources were compiled and executed across calls. H1 used the supplied immutable feedback script and observed RED before the approved correction and GREEN afterwards.

The coordinating session independently checked 101 exported files and all six frozen acceptance criteria, including compilation and rejection of three t08 mutants. Its first oracle invocation was refused by the outer development sandbox; the independently confined replay outside that sandbox passed. These are separate provenances. The [final redacted reconciliation](evidence/independent-summary.json) confirms all six candidates are byte-identical to the independently evaluated inputs and revalidates the 23 native results. Final-source reproduction and local quality receipts identify their exact source snapshots below.

## Final-source receipts

The [redacted component receipt](evidence/component-summary.json) records six successful native dialogues, 23 returned tool results, 35 passing component tests and ten passing guard tests. Source verification checked 422 Rust sources/manifests with no mismatches; the explicit overlay is pinned in [source-overlay.json](evidence/source-overlay.json). The kernel retained 8589934592 bytes and zero swap. Raw requests, prompts, boot data, terminal data and SDK output remain local; only narrow counts and receipt hashes are published. These prove component behavior, with Runtime admission closed.

## Local quality

[The local quality receipt](evidence/quality-summary.json) records 33/34 passing checks on an unchanged source snapshot: full workspace tests, both Clippy profiles, format, harness, Python checks and Gitleaks pass. The sole failure is the pre-existing release-local suite: this host has OpenSSL 3.5.9 while the suite pins 3.6.5. The release sources match the base control that reproduces this failure. Neither the version pin nor the memory bound was relaxed. This is not a full passing quality verdict, Linux Clippy or remote CI equivalence.

## Native compatibility fixes

The pinned musl SDK requires an anonymous Unix SEQPACKET pair for subprocess startup. The original filter refused syscall199 with arguments 1,524293,0. The updated native-MCP filter admits that pair only; it still refuses addressed SEQPACKET, all candidate addressed sockets and all legacy/candidate SEQPACKET pairs. Ten kernel canaries check these rules, protected files, 8 GiB, actual feedback and detached descendants under timeout/parent EOF.

The native CLI requires explicit per-tool approval settings for this noninteractive MCP path. Those settings exist only in the fresh temporary home, for the three finite tools. Optional native tools, agents, goals, shell and apps are disabled. No global setting, permission list, credential or personal directory is imported.

The real raw catalogue has four top-level entries: three native discovery wrappers and the finite namespace. The broker adapter pins the complete schema, then projects a copy solely for the unchanged authored validator. Raw SDK requests remain available. The native success envelope has a timing text part and a JSON ToolResult part. The adapter rejects native errors and malformed, duplicate, truncated or falsified values. No discovery-wrapper call is emitted by the scripts. This does not prove that the SDK advertises only three tools.

## Reproduce without a model

The replay requires the existing private development image/cache, the public pinned Linux SDK and the exact six-packet/validator exports from the coordinating session. It never pulls an image, installs software or imports auth.json. It verifies every Rust source/manifest against the checkout before compiling offline. These prerequisites do not establish public release reproducibility.

```sh
python3 -B docs/bench/linux-finite-native-mcp-2026-10-17/reproduce.py \
  --output "$NEW_EVIDENCE_DIRECTORY" \
  --inputs "$FROZEN_PUBLIC_PACKETS" \
  --adapter "$FROZEN_AUTHORED_VALIDATOR" \
  --native "$PINNED_PUBLIC_LINUX_CODEX"
```

The outer container has no network, personal mounts, capabilities or swap, and memory.max is 8589934592. The loopback-only namespace uses a fresh sysfs view and the nonprivileged UID mapping. Setup capabilities are dropped before the test process. The worker receives one candidate, fresh state and the pinned programs; controller scripts, validation data and the build cache stay outside its mounts.

The replay also runs the real global-session deadline regression: a 500 ms session stops a longer exec and its detached writer, with stable heartbeat afterwards. Existing policy already passed that control; no timeout relaxation was applied. The terminal validator separately retains the preparation inode and immutable-input checks.

## Remaining work

The Runtime entrance still refuses MCP startup before registering a run. The next boundary is an explicit public-only staging profile, fresh Tools/WorkspaceWrite effects, a retained terminal descriptor and the exact namespace fence, followed by a bounded one-file transfer into the private artifact view and existing patch capture. The proposed scratch check remains intact through capture. Automatic approval refused applying that boundary without more specific approval; the proposal is stored separately and is not executed by this branch.

Real provider/subscription access, live model metadata and cumulative accounting are unqualified here. Native usage 0/0 without a received usage event is UNKNOWN, not observed zero consumption. The 24 model trials are separate and remain unlaunched by this slice.

See [ADR0041](../../adr/0041-native-finite-mcp-component.md) for the component and authority boundary.
