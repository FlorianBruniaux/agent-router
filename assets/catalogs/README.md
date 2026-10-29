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
quality 0, cost and latency 1,000,000, no declared capabilities, efforts `low`
through `max`, and all three stages. Nothing is rated, priced or timed. The
router therefore never prefers one entry for quality, and with equal scores it
picks by identifier order, which is a tie-break and not a recommendation.

| Entry | Basis for the identifier |
| --- | --- |
| `claude-haiku-4-5-20251001` | Dated id observed in a Claude stream (`crates/adapter-claude/tests/conformance.rs:648`, `docs/adr/0021-host-capability-evidence.md`). |
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

`tools` and every other capability stay UNKNOWN: entries declare none, and a task
that requires `tools` gets no route without separate host evidence.

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
