# CLI preparation and execution

`prepare` assembles one workflow node from already constrained inputs. It loads
only the selected bodies and renders a typed prompt. `run` reviews that concrete
request, then requires the returned digest before starting a native host.

## Preparation manifest

Paths are relative to the manifest directory unless absolute. The options file
uses `PreparationOptions` from `router-prepare`; the CLI replaces its clock with
the current time and refuses a snapshot age allowance above 300 seconds.

```json
{
  "schema_version": 1,
  "run_id": "review-001",
  "context": "phase-context.json",
  "host_snapshot": "snapshot.json",
  "catalog": "catalog.json",
  "policy": null,
  "policy_store": null,
  "asset_authority": "asset-authority.json",
  "plan": "plan.json",
  "node_id": "execute",
  "metadata": "bound-metadata.json",
  "body_root": "assets",
  "template": "prompt.json",
  "variables": "variables.json",
  "options": "preparation-options.json"
}
```

Create bound metadata with `index --metadata metadata.json --catalog catalog.json
--json`. Search that envelope with `search --metadata bound-metadata.json
--catalog catalog.json --query 'Rust parser' --json`. A catalog content change
invalidates the binding even when its version string stays the same.

```sh
agent-router prepare --manifest preparation.json --json
agent-router prepare --manifest preparation.json --output-directory prepared --json
```

The first command emits a redacted preview. The second creates a new 0700
directory with 0600 request, context, snapshot, policy, catalog, plan, preview and
receipt and definition files. It refuses an existing output directory. An I/O failure can
leave a private partial directory; `definition.json` is written last and no
process is started. Request files contain the assembled private prompt, so treat
the bundle as sensitive.

In a preparation manifest, an explicit policy file overrides a policy store.
When the store supplies the policy, the generated definition preserves its
artifact identity and checks that it remains active before fresh execution.
The generated definition names exactly one policy source. An execution
definition that names both `policy` and `policy_store`/`policy_artifact_id` is
refused with `policy.composition.denied`, instead of silently ignoring the store.
Policy source changes invalidate the CLI approval. Historical inspection
reconstructs the exact existing definition.

An optional `receipt` path selects a previously computed fallback receipt. The
entire receipt is reproduced from the immutable context, including the explicit
unavailable candidate set. It does not attest that a provider actually failed.
The resulting bundle preserves that receipt; changing or removing it cannot
silently restore the original winner.

For linked steps, supply `workflow_contract` and `generation` together in the
preparation manifest or execution definition. The contract binds a separate
`instance_id`, plan hash, workspace, source commit, explicit phase per node and
typed dependencies. Its hash is part of every approval. Generations start at 0;
a new generation is an explicit replacement, subject to retry and budget gates.
Without a contract, the legacy path is limited to a node without dependencies.

## Asset authority

New execution approval requires `asset_authority` in the definition and a v3
receipt. Historical v2 review and inspection remain available. The authority
manifest is parent-owned configuration with independently pinned contract and
availability hashes; a child's self-reported hash cannot establish authority.
The same optional path is accepted by `prepare` and `route-phase
--asset-authority`. Preparation preserves its absolute reference, so retain the
manifest and its exact referenced inputs with the private bundle.

```json
{
  "schema_version": 1,
  "contracts": {"path": "contracts.json", "expected_hash": "PINNED_CONTRACT_HASH"},
  "availability": {"path": "availability.json", "expected_hash": "PINNED_AVAILABILITY_HASH"},
  "policy": "asset-policy.json",
  "requests": "candidate-asset-requests.json",
  "execution_scope": {"workspace": "/absolute/repository", "source_state": "COMMIT_HASH", "scope": ["src"]},
  "approved_review_digests": {},
  "metadata": "bound-metadata.json",
  "body_root": "assets"
}
```

The placeholders must be replaced with independently established values. Asset
requests map every candidate ID to its exact typed provider, model, host,
capabilities and context requirements. Routing reads metadata only. To inspect
selected skill bodies explicitly, use `assets review --manifest
asset-authority.json --context phase-context.json --host-snapshot snapshot.json
--catalog catalog.json --skill SKILL_ID --json`. Repeat `--skill` for up to 16
IDs. The result contains review projections and digests, never raw bodies, and
does not add approvals or execute skills. Put an authorized review digest in
`approved_review_digests` only after that review. The runtime checks the actual
prepared bytes and fresh evidence again before dispatch. See
[asset-contracts.md](asset-contracts.md) for evidence and tokenizer limits.

## Workflow coordination commands

`coordinate` adds declared result handoff and explicit waves over the one-node
runtime. A version 1 workflow definition contains `plan`, `execution`, one
`artifacts` declaration per node, `failure_policy`, `max_input_bytes`, and
`definition_sha256`. The execution contract must already be sealed. Every
declared dependency input references an actual producer output; a patch can
only be exposed as a read-only workspace to an independent Verify node.

```sh
agent-router coordinate seal --definition proposed-workflow.json --json
agent-router coordinate review --definition workflow.json \
  --source-definition prepared/definition.json --sandbox sandbox.json \
  --node-id execute --generation 0 --store /absolute/private-workflow \
  --output-directory reviewed-step --json
agent-router coordinate run-wave --definition workflow.json --wave wave.json \
  --store /absolute/private-workflow --json
agent-router coordinate status --definition workflow.json --store /absolute/private-workflow --json
agent-router coordinate cancel --definition workflow.json --store /absolute/private-workflow --json
```

Save the `definition` returned by `seal` as `workflow.json`. Sealing grants no
approval. Review creates a new private directory containing the final request,
its exact artifact grants and `bundle.json`. It includes dependency data before
computing the approval. The wave manifest is `{"schema_version":1,"steps":[{
"bundle":"reviewed-step/bundle.json","approval":"DIGEST_FROM_REVIEW"}]}`.
Paths in the wave are relative to that manifest. Each step has its own explicit
approval; the bounded wave cannot select new models, invent later approvals or
turn model output into workflow authority. Every submitted run appears in the
wave completions or typed errors. A mixed or cancelled wave exits unsuccessfully.

`coordinate verify --bundle reviewed-step/bundle.json --store ... --evidence
verification.json --approval DIGEST_FROM_REVIEW --json` queues an explicit
verification control, including while a wave is running. `queued: true` is not
a verification verdict. After a restart, run `coordinate service --definition
workflow.json --bundle reviewed-step/bundle.json --store ... --json` to restore
the exact private material and process queued controls. Repeat `--bundle` for
the steps whose material is needed. No confidential prompt is reconstructed
from logs or model output.

`coordinate artifacts --bundle reviewed-step/bundle.json --store ... --json
preview` returns the integration approval digest. `apply --approve DIGEST` and
`discard --approve DIGEST` require that exact digest. `declared` lists result
metadata, and `purge-declared --id ARTIFACT_ID` explicitly removes an owned
result. Diagnostic `outputs` and `purge-outputs --attempt N` remain separate.
Original source files change only through explicitly approved integration.

`workflow --plan plan.json --execution-contract contract.json --seal-contract
--json` validates and hashes a proposed contract; it grants no approval. Omit
`--seal-contract` to verify an existing hash. Nodes must match the complete plan.

## Sandbox and privacy configuration

The sandbox file contains `schema_version: 1`, `programs` (native program name to
absolute binary path), `read_roots`, `environment_names`, an absolute resource
`helper` path, `bubblewrap` (Linux path, null on macOS), and `limits`.

Limits require every field: `cpu_seconds`, `address_space_bytes`,
`max_open_files`, `max_processes`, and `max_file_bytes`. The memory field is
required even when explicitly null. macOS cannot enforce RLIMIT_AS on the
verified host; a requested memory limit fails closed. Explicit null is reported
as `not_requested`, never as memory enforcement.

Only named environment variables are passed. Their values are not included in
the preview. Binaries, helper and native sandbox wrapper are hashed in the
approval and rechecked by the supervisor. Paths remain subject to the documented
hash-to-exec race boundary in [runtime.md](runtime.md).

The approval also binds each named variable as read by the invoking process.
`HOME`, `CODEX_HOME`, `CLAUDE_CONFIG_DIR` and `SSL_CERT_FILE` are bound by
value. Every other name, API keys included, is bound only by a SHA-256 of
`agent-router.approval.environment-value.v1`, a zero byte, the name, a zero
byte and the value; the value itself never enters the digest input, the output
or an error. An unset variable is bound as absent, distinct from an empty value.
Review and `--approve` must therefore run with the same values: any change,
removal or addition refuses the launch with
`runtime.approval.required_or_changed`. This binding is approval format v4
(`runtime-cli-approval-v4`, `-v4-backend`, `-v4-escalation`). Digests produced
by earlier versions (v1, v2 and v3) are stale: review the run again for a new
digest. The same applies to `coordinate` review, wave and verification digests.

Two optional fields control private recording:

- `feedback_store`: an explicit existing feedback store. Collection requires its
  current consent; changed consent prevents the completion event from being stored.
- `output_retention`: defaults to `{"mode":"disabled"}`. Explicit
  `{"mode":"private_artifacts","retention_seconds":3600}` retains bounded
  stdout/stderr privately and binds that choice into the approval.

```sh
agent-router run --definition prepared/definition.json --store /absolute/private-runtime \
  --sandbox sandbox.json --json
agent-router run --definition prepared/definition.json --store /absolute/private-runtime \
  --sandbox sandbox.json --approve DIGEST_FROM_REVIEW --json
agent-router status --store /absolute/private-runtime --run-id review-001 --json
agent-router cancel --store /absolute/private-runtime --run-id review-001 --json
agent-router events --store /absolute/private-runtime --run-id review-001 --json
```

Approval includes the immutable request, plan, authority source, sandbox
configuration and executable hashes. A changed input requires a new review.
If writing the result to stdout fails (for example a closed pipe), `run` and the
other commands here exit `4` with `route.internal`; the persisted run state and
events are unaffected, so read them back with `status` or `events`.
Retries are explicit and require a new approval digest. `run --retry` alone is a
read-only preview: it prints the digest the retry would need and persists
nothing (state, events and control requests are unchanged). `run --retry
--approve DIGEST` checks `DIGEST` against that preview, performs the transition
and then executes. A plain `run --approve DIGEST` on a run in `failed` or
`timed_out` is refused with `runtime.retry.use_retry_approve`. Breaking change:
`--retry` used to persist the awaiting-approval state and could not be combined
with `--approve`.
Status separately reports `cleanup_evidence`: `not_started`,
`group_terminated`, `namespace_terminated`, or `unknown`. Group termination
does not prove termination of a descendant that created a separate session.
An unresolved attempt lease prevents recovery from releasing writer locks;
the CLI reports `runtime.cleanup.unknown` (exit `6`, inspect before any retry)
instead of authorizing a new run. SQLite contention on the runtime or
coordinator store is reported as `runtime.busy` or `coordinator.busy` (exit
`5`); the same command can be relaunched unchanged. See
[exit codes and error codes](cli-output.md#exit-codes-and-error-codes).
The runtime isolates dependencies by workflow instance and contract. A premature
step stays queued and can be cancelled or resumed with its existing exact
approval. Normal edges require `verified`; an explicitly independent verifier
may use `artifact_ready_for_verification` for a sealed write result still awaiting
verification. That readiness does not itself grant a view of the patch to the
consumer. These commands operate one node at a time; full artifact handoff and
workflow coordination have separate acceptance tests.

## Verification and artifacts

A zero exit status from the native process establishes transport only. Objective
and validation remain unknown until explicit verification evidence is supplied.
Timeout, cancellation and output overflow never become CLI success merely because
the child handles termination by exiting zero.

`verify --definition ... --store ... --evidence verification.json --json` records
an explicit verifier attestation. The record contains verifier identity, evidence
digest, artifact digest, validation and objective outcomes. A digest is an
integrity reference, not authentication of an external verifier.

After verification, use `artifacts --definition ... --store ... preview --json`,
then `apply --approve DIGEST --json` to integrate the reviewed patch. `keep`
preserves the worktree; `discard --approve RUN_APPROVAL_DIGEST` removes only that
owned worktree after a terminal state.

`artifacts ... outputs --json` lists retained output metadata. `purge-outputs
--attempt 1 --json` deletes only the registered private files for that attempt.
Expiration requires a caller to invoke the bounded purge API; no background
scheduler or secure storage erasure is implied.
