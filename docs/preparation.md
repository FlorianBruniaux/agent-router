# Preparing a workflow node

`router-prepare` assembles an `ExecutionRequest` from a validated routing context,
one node of an immutable workflow plan, explicitly selected asset metadata and a
typed prompt. It starts no process, grants no approval and creates no worktree.

The public entry point is `prepare(PreparationInput<'_>) -> Result<PreparedExecution>`.
Its input contains references to:

- `router: &PhaseRouter` and `context: &ValidatedPhaseContext`;
- `plan: &WorkflowPlan` and `node_id: &str`;
- `index: &Bm25Index` and `body_root: &Path`;
- `template: &PromptTemplate` and `variables: &BTreeMap<String, PromptValue>`;
- `options: &PreparationOptions`.

`PreparationOptions` is strict version 1 JSON with `workspace`, `source_state`,
`target`, `allow_degradation`, `timeout_ms`, `max_output_bytes`,
`max_input_bytes`, `now_unix_seconds` and `max_snapshot_age_seconds`. All fields
are explicit. The caller supplies the clock and source commit. Preparation
checks their shape and snapshot freshness; the runtime verifies the actual Git
state and rechecks authority immediately before execution.

## Asset contracts

The index must use the retrieval v2 envelope whose catalogue digest matches
`context.catalog_hash()`. An unbound v1 index or mismatched catalogue is refused
before body I/O. V1 retrieval remains available for its existing read-only uses.

The selected prompt is the explicit template ID. Its metadata body points to a
UTF-8 JSON serialization of the complete sealed `PromptTemplate`. Duplicate keys
are rejected. The loaded template must equal the supplied template, including
host/model compatibility, variables, output contract and content digest.
Different JSON whitespace is permitted when the body reference records the
actual bytes. Rendering checks the internal template digest and substitutes
typed values once.

The agent and skills come exclusively from the newly computed selected receipt.
The selected agent's body digest and trust must also match its declared phase
registry entry. Skill eligibility and catalogue trust were validated by the
phase router; the separate metadata entry must independently permit trusted body
loading. Metadata declarations and content digests do not authenticate their
publisher. The caller supplies the trusted metadata and authorized body root.

Only these selected bodies are read. Unselected, blocked or missing bodies are
not traversed. The loader rejects selected untrusted or blocked entries, hash
drift, invalid UTF-8, symlinks and unsafe relative paths. The caller must prevent
hostile concurrent replacement of the filesystem; these checks are not an OS
sandbox.

Loaded skills are instruction text. No executable skill is prepared. Tool body
files are never loaded: tool authority comes from the phase policy and registry.
`Restricted` preserves the exact allowlist or disables tools; `NativeHost`
preserves the explicit native-host tool policy. Agent text cannot select or add
tools, permissions or mutation rights. `PhaseRouter::execution_capabilities`
supplies the exact union checked during eligibility: task and candidate,
selected skill and registry requirements, tools, sandbox and mutation effects.
An asset's required network capability is retained in the request only after
the routing gates have established the corresponding authority.

## Request and review boundary

Preparation validates the full plan, requires its task to equal the routed task,
recomputes the receipt and independently validates the selected candidate. An
abstention produces no request. The selected sandbox must match the node mode;
its estimated cost, tokens and elapsed time must fit the node's budget. The
scope comes from that node and the timeout cannot exceed its budget.

The request prompt is a JSON document containing the immutable task, node
contract, optional agent instructions, rendered typed prompt and output
contract. Skill bodies remain separate `ExecutionSkill` entries for adapter
composition. The text boundary is not a prompt-injection defense. Permission
enforcement belongs to the reviewed request and OS sandbox.

Both composed prompt plus skill bytes and serialized request bytes are bounded
by 128 KiB. `max_input_bytes` can impose a lower limit on prompt plus skill
bodies. Output is bounded by the adapter's 4 MiB maximum. No tokenizer or model
is invoked, so observed input tokens remain unknown. Token estimates in the
route are policy inputs, not measured model usage.

`PreparedExecution` has private fields and cannot be deserialized. Its getters
are `request()`, `receipt()`, `preview()` and `preparation_hash()`. The digest
binds the complete request and receipt, plan/node identity, metadata and index
hashes, loaded asset hashes, prompt-template digest and preparation options.
It detects changed inputs; it is not a signature or execution approval.

The serializable preview includes routing identities, scope, tool policy,
content hashes, byte counts and `approval_required: true`. It excludes task
text, variable values and body contents. Debug output also redacts prompts and
skills. `canonical_request_json()` is an explicit private export containing
prompts and skill bodies. A CLI persisting it must use private file permissions
and obtain user intent for retaining those contents.

The resulting request uses `mode: execute` so it can enter the runtime review
path. Preparation only applies adapter input checks to an internal dry-run copy.
It never returns `ValidatedExecutionRequest`, an authorization digest or an
approved run. `Runtime::review`, fresh source/snapshot checks, supervisor probes
and the separate approval digest are still required.

This API handles one node. The caller supplies the phase context and explicit
prompt choice for that node. It does not schedule a whole DAG, transfer arbitrary
predecessor stdout, prove a native agent identity, verify task completion or
automatically integrate changes.

## Checks

```sh
cargo test -p router-prepare --locked --offline
cargo clippy -p router-prepare --all-targets --all-features --locked --offline -- -D warnings
```

The tests use synthetic catalogues, snapshots and local asset bodies. They cover
selection-only loading, privacy, hash and trust drift, catalogue binding,
snapshot freshness, typed variables, budgets, immutable inputs, explicit tool
policies and deterministic preparation. They invoke no native host or model.
