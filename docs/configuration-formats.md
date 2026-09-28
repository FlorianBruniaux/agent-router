# Configuration layer formats

`agent-router config --layers FILE` accepts explicit local `.json`, `.toml`,
`.yaml` and `.yml` files. The extension is case-sensitive and selects the parser;
the CLI does not guess the format from the content. Add `--json` for the stable
machine-readable result. Human output renders that same result.

```sh
agent-router config --layers config.toml --json
agent-router config --layers config.yaml
```

This command reads a `ConfigurationLayers` document and prints effective
restrictions. It does not discover a user or project configuration automatically,
rewrite a file, activate a policy or launch a host. A URL passed as `FILE` is
treated as a local path and is never fetched. Environment interpolation and file
includes are unsupported.

The format selection applies only to `config --layers`. Task input, phase
policies, catalogs, signed artifacts, asset contracts, preparation manifests and
execution definitions retain their existing JSON contracts. This command does
not implement the separate TOML/YAML routing-policy example in technical
specification §7 or an interactive task-entry flow.

## Document and precedence

The root requires `schema_version = 1` and a `defaults` layer. The optional layers
are `active_offline_policy`, `preset`, `project`, `organization` and `task`.
Each supplied layer requires a `name` and accepts these optional fields:

| Field | Type |
| --- | --- |
| `allowed_models` | Set of model identifiers, at most 64 per layer |
| `allowed_capabilities` | Set of capability enum values |
| `allowed_mutations` | Set of mutation enum values |
| `max_cost_microusd`, `max_tokens` | Unsigned integer |
| `max_concurrency` | Integer from 1 to 64 |
| `preferred_model` | Model identifier |

Sets intersect across all layers, and budget or concurrency ceilings take the
minimum. An explicit empty set removes every member of that set. An omitted
optional restriction contributes no additional limit; it grants no runtime
permission. JSON and YAML also accept `null` for optional values. In TOML, omit
the field because TOML has no null value.

Preferences use the last supplied value in the order `defaults`,
`active_offline_policy`, `preset`, `project`, `organization`, `task`. A preference
outside the final permitted model set is an error. The result includes ordered
source names and a deterministic `policy_hash`; neither constitutes an execution
approval or evidence of host capability.

These two files resolve to the same restrictions. The task's mutation list
cannot restore the network permission removed by the organization layer.

```toml
schema_version = 1

[defaults]
name = "base"
allowed_mutations = ["network", "workspace_write"]
max_tokens = 1000

[organization]
name = "org"
allowed_mutations = []

[task]
name = "task"
allowed_mutations = ["network"]
max_tokens = 250
```

```yaml
schema_version: 1
defaults:
  name: base
  allowed_mutations: [network, workspace_write]
  max_tokens: 1000
organization:
  name: org
  allowed_mutations: []
task:
  name: task
  allowed_mutations: [network]
  max_tokens: 250
```

## Parsing and refusal rules

All formats reject unknown fields, duplicate keys and duplicate set members,
including duplicates that become equal after string escape decoding. Wrong
types fail; a quoted number or a floating-point value cannot become an integer
budget by coercion or rounding. TOML dates and times are rejected. For values
portable across all three formats, use nonnegative decimal integers no larger
than `9223372036854775807`, TOML's signed integer maximum. JSON and YAML integer
budget fields can additionally represent the remainder of the `u64` range.

YAML accepts one document, mappings with string keys and arrays of strings for
sets. `---` and `...` may delimit that single document. Anchors, aliases, merge
keys, explicit tags and directives are rejected, including unused anchors and
reserved directives. Quoted `!include`, `&anchor`, `*alias` and `${NAME}` are
literal string content. YAML plain scalars recognize null and boolean forms,
decimal integers, `0x`/`0o` integers and finite floats; the typed schema still
determines which values are allowed. Other plain scalars remain strings. Quote
identifiers that resemble numbers or booleans.

The input must be UTF-8 and at most 262,144 bytes, inclusive. Tree depth is limited
to 16 below the root, strings and keys to 8,192 UTF-8 bytes, and traversed nodes
to 4,096. YAML also has a 16,384-token scan ceiling and counts mapping-key events
toward its node limit. TOML retains its parser recursion guard before the tighter
application tree checks. These are acceptance limits, not measurements of peak
memory or parser latency.

On Unix, the input's final component cannot be a symlink, and the opened
descriptor must be a regular file. Nonblocking opening lets FIFOs fail without
waiting for a writer. Parent path components may resolve through symlinks; this
loader does not claim descriptor-relative containment of the complete path.

Failures exit with code 2 and emit a fixed error code on stderr, without source
text, parser excerpts or the input path. Format, read, parse and resource errors
use `configuration.format`, `configuration.read`, `configuration.invalid` and
`configuration.limit`. Valid syntax that violates a workflow constraint retains
the appropriate `workflow.*` error. A successful configuration resolution exits
with code 0 and leaves the source file unchanged.

The CLI behavior suite is
[`config_formats.rs`](../crates/agent-router-cli/tests/config_formats.rs):

```sh
CARGO_BUILD_JOBS=4 cargo +1.85 test -p agent-router-cli --test config_formats --all-features --locked --offline
```
