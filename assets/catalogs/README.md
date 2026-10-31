# Codex policy catalogs

`codex-core-v2.json` covers the nine model identifiers and effort sets in the
sanitized October 2026 discovery fixture. Its JSON schema is still 1; `v2` names
this policy revision. Use it explicitly for current discovery:

```sh
agent-router inspect --host codex --model-cache /path/to/selected-models-cache.json \
  --observed-version 0.163.0-alpha.10.2 --json > observation.json
agent-router catalog refresh --host-snapshot observation.json \
  --prior assets/catalogs/codex-core-v2.json --json > catalog.json
```

The version above is an example. Supply the independently observed full version
of the selected native executable. These commands do not run that executable,
contact a provider, initialize a proof store or issue an attestation.

The three GPT-5.6 entries retain their existing policy scores and declared
capabilities, with expanded effort sets. The six newly listed entries use
`builtin-unrated-policy-prior`: quality 0, cost and latency 1,000,000, and no
declared capabilities. These are conservative policy bounds, not measured
quality, prices or timing. They add discovery coverage without preferring an
unrated model or assuming tool support. Any later rating needs a separately
reviewed policy revision and evidence for its interpretation.

The generated catalog intersects prior capabilities with authenticated host
support. A cache-only observation therefore produces empty model capabilities
and keeps `tools` unknown. A newly observed identifier or effort outside the
prior remains an error; the catalog never guesses an alias or new rating.

`codex-core-v1.json` remains the three-model legacy prior for existing fixtures
and historical examples. Neither catalog authenticates a host, grants native
permissions, establishes provider access or authorizes model execution.

# Claude Code policy catalog

`claude-core-v1.json` is an operator-declared catalog for advisory routing in
Claude Code. Its JSON schema is 1. Every entry uses `operator-declared-unrated-prior`:
quality 0, cost and latency 1,000,000, the declared capability `tools`, efforts
`low` through `max`, and all three stages. Nothing is rated, priced or timed. The
router therefore never prefers one entry for quality, and with equal scores it
picks by identifier order, which is a tie-break and not a recommendation.

| Entry | Basis for the identifier |
| --- | --- |
| `claude-fable-5-1` | Current Claude model id list given to the coding agent by its runtime environment on 2026-10-31 (Fable 5.1). Operator-declared, not observed in a stream. |
| `claude-haiku-4-5-20251001` | Dated id observed in a Claude stream (`crates/adapter-claude/tests/conformance.rs:648`, `docs/adr/0021-host-capability-evidence.md`). |
| `claude-haiku-5-5` | Same runtime list, 2026-10-31 (Haiku 5.5). Operator-declared, not observed in a stream. |
| `claude-opus-5-5` | Pilot pool (`docs/bench/campaign-next/pilot/catalogs/claude-pool.json:6`). |
| `claude-sonnet-5-5` | Pilot pool (`docs/bench/campaign-next/pilot/catalogs/claude-pool.json:25`). |

These identifiers are not proof that an account offers them. The operator must
confirm each one, and add or remove entries, before relying on a plan. Aliases
such as `opus` or `sonnet` are not accepted as model identifiers, and no alias
is resolved or date suffix stripped. The efforts follow the values the Claude
adapter can pass to `--effort` (`crates/adapter-claude/src/execution.rs:52-58`);
`ultra` is not listed. Whether a given model accepts a given effort is not
established here, so the request's host snapshot must name the efforts it
supports.

Every entry declares `tools`. That is a model-side prior only. In
`CapabilitiesSatisfied` (`crates/router-policy/src/constraint.rs:172-217`) a
required capability is satisfied only when the request's capability knowledge
says `Supported`, the host snapshot lists it (`constraint.rs:192`) and the model
lists it (`constraint.rs:202`). Without explicit knowledge, the capability counts
as supported only if the host snapshot itself lists it
(`crates/router-policy/src/preset.rs:161-168`).

The declaration therefore establishes only that the catalog does not exclude
these models for `tools`. It does NOT establish that a host supports tools, that
the account's Claude Code session has tools enabled, or that any model accepts
them in a given configuration. A host snapshot that does not list `tools` still
gets `host.capability.missing`, and `unknown` knowledge still yields an unknown
verdict, so such a task gets no route. The host snapshot must declare or
discover `tools` itself. Other capabilities stay UNKNOWN: no entry declares them.

## Discovery

No passive discovery of offered Claude models or efforts exists yet, so the
catalog stays operator-declared. `agent-router inspect --host claude
--native-stream <file> --observed-at <unix-seconds>` reads an explicitly
supplied native stream and lists only models seen in root assistant messages,
with no efforts and all capabilities unknown. `catalog refresh --host-snapshot`
accepts that snapshot but refuses to generate a catalog (`catalog.invalid`),
because a refreshed model needs observed efforts. Refresh is therefore not
available for Claude Code today.

Like the Codex catalogs, this one authenticates no host, grants no native
permission, establishes no provider access and authorizes no model execution.
