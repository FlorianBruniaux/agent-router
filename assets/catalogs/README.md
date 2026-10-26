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
