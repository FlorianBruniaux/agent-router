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

To produce the two pinned documents, `assets seal --kind contracts --document
contracts.json --catalog catalog.json --json` or `assets seal --kind
availability --document availability.json --host-snapshot snapshot.json --json`
recomputes `content_hash` from an unsealed document and prints the sealed
document with its hash. It writes no file, edits no manifest and approves
nothing; the operator copies the hash into `expected_hash` after checking it
independently.

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

For `programs.codex`, configure the canonical path of the native executable,
not the npm `codex.js` entry. Both macOS and Linux reject Codex programs whose
first two bytes are `#!` with `runtime.program.script_unsupported` (exit 2),
before sandbox probes or dispatch. The runtime does not search for Node or
widen `PATH=/usr/bin:/bin`. Script contracts for other host adapters are unchanged.
This check does not implement ADR 0021's private-copy execution batch.

Limits require every field: `cpu_seconds`, `address_space_bytes`,
`max_open_files`, `max_processes`, and `max_file_bytes`. The memory field is
required even when explicitly null. macOS cannot enforce RLIMIT_AS on the
verified host; a requested memory limit fails closed. Explicit null is reported
as `not_requested`, never as memory enforcement.

Codex and Claude receive a fresh 0700 host-state directory inside the store for
each attempt, writable even with a read-only workspace. Caller-provided
`CODEX_HOME` or `CLAUDE_CONFIG_DIR` is refused. The runtime sets the matching
variable and removes state after completion or preparation failure. No personal
config, keychain or OAuth token is copied. An approved `OPENAI_API_KEY` seeds a
0600 Codex `auth.json`; Claude uses the approved `ANTHROPIC_API_KEY` environment
value. Missing API keys leave state empty. OAuth login is not supported by this
bootstrap. Store roots and ancestors must stay outside the child read/write
roots. A killed router may leave private orphan state, never reused by a run.

Only named environment variables (plus runtime-owned host-state paths) are passed. Their values are not included in
the preview. Binaries, helper and native sandbox wrapper are hashed in the
approval and rechecked by the supervisor. Codex and Claude execute a private
copy of their approved entry file for each attempt and version probe. Its
0700 directory is outside the store and writable host state; its file is 0500
and the child receives no write grant covering either. The verified copy stays
alive through the attempt and is removed during cleanup. Original-file
replacement cannot change the selected entry bytes. Other adapters, helper and
wrapper paths retain the hash-to-exec boundary in [runtime.md](runtime.md).
Interpreters and dynamically loaded modules are not copied or bound by this
change. A Claude script that relies on files next to its original path may fail
from a copy; Codex shebang entries remain refused. Another unsandboxed process
running as the same user remains outside this protection.

The approval also binds each named variable as read by the invoking process.
`HOME`, `CODEX_HOME`, `CLAUDE_CONFIG_DIR` and `SSL_CERT_FILE` are bound by
value. Every other name, API keys included, is bound only by an HMAC-SHA256,
keyed by the runtime store's approval key, of
`agent-router.approval.environment-hmac.v1`, a zero byte, the name, a zero
byte and the value. Neither the value nor its HMAC appears in the output or an
error. An unset variable is bound as absent, distinct from an empty value.
Review and `--approve` must therefore run with the same values: any change,
removal or addition refuses the launch with
`runtime.approval.required_or_changed`. This binding is approval format v7
(`runtime-cli-approval-v7`, `-v7-backend`, `-v7-escalation`), which also binds the
private host-state policy, Codex-only macOS preference reads and execution from
a private verified Codex/Claude entry copy. Digests
produced by versions v1 to v6 are stale: review the run again for a new
digest. The same applies to `coordinate` review, wave and verification digests.

The approval key is 32 random bytes in `approval.key` inside the runtime store
(`--store` for `run`, `STORE/runtime` for `coordinate`). It exists because the
approval digest is meant to be displayed and pasted and all its other inputs
are known: with a plain hash, whoever sees the digest could confirm a guessed
API key offline. The key is created by the first review or approval that binds
a secret value, as a private file (mode 0600, created exclusively, never through a
symlink) published atomically, and it is read again at every approval. After
publication the store directory is synced; if that sync fails, the command
stops with `runtime.approval_key.published_durability_unknown` (exit `6`): the
key exists but may not survive a crash, so check that `approval.key` is present
before approving. A key that is not a regular private file of the current
user, or a store directory that is not private, is refused with
`runtime.approval_key.unsafe` (exit `2`).

A key of the wrong length, for example an empty or truncated file, is refused
with `runtime.approval_key.integrity` (exit `7`) at every review and approval
that binds a secret value, until the file is deleted by hand. Nothing repairs
or replaces it automatically. Deleting or replacing the key makes every pending
approval that binds a secret value stale: the next review or approval creates
or reads another key, so review again for a new digest. A digest that binds no
secret value never reads the key.

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

After verification, `artifacts --definition ... --store ... review-publication
--json` returns the exact diff, regular-file blob preimages/postimages and Git modes,
source, resulting tree/commit, verifier record and a separate approval digest.
Review builds unreferenced Git objects on a private index; it changes no ref or
user working file. Save the inner `review` object to use `check-publication-review
--review FILE --json`: it validates the entire saved payload against the current
candidate before returning the diff. Replaced diff bytes, unknown keys, unsupported
versions and oversized input are refused. A digest beside a display file is not a
substitute for this validation.

`artifacts ... --config SANDBOX.json publish --approve DIGEST --json` requires current capability
evidence and explicit Integrate authority, then publishes that exact result under
the returned `target_ref`. It changes no working files, index, HEAD or current
branch. Use `git show REF` on that returned ref to inspect the complete revision.
Its creation is not a human acceptance verdict or a feedback applied fact.

`artifacts ... publication-status --json` observes the journal and current ref.
`published` requires matching terminal evidence and a direct ref to the expected
revision. `unknown` returns exit 3, retains the consumed intent and never retries or
rolls back user changes. A crash after ref creation cannot be repaired by treating
the ref alone as proof of complete publication. `not_attempted` returns exit 0.
These commands also exist under `coordinate artifacts`.

When capability claims exist, both `publish` and legacy `apply` require the
existing `--config` sandbox configuration to reconstruct current executable,
adapter and scope pins. Historical reopen alone cannot authorize mutation.
Publication rebuilds these bindings during verification and immediately before
its Git effect; this does not exclude unrelated writers changing files after the
last check. Promisor/partial clones, filters, binary patches, rename/copy headers
and pre-existing symbolic result refs are refused. Source and result blob budgets
are checked separately, as specified in ADR 0027.

From the repository root, this self-contained simulated task checks the faulty
source, the candidate and the code extracted from the published revision:

```sh
cargo test -p router-runtime --locked --test runtime \
  simulated_useful_fix_passes_independent_checks_on_the_published_revision -- --exact
```

It requires no provider account and does not establish native-host execution or
model savings.

Legacy `preview` followed by `apply --approve DIGEST` still modifies the source
working copy. Application authority is now one-shot even if a user restores the
original source. Its multi-file working-copy writes do not share publication's
single visibility point. See [ADR 0027](adr/0027-verified-patch-publication.md) for
evidence limits, including Git-metadata races and native qualification. The
[application and recovery qualification](bench/application-recovery-2026-10-23/README.md)
checks exact revision bytes, a useful fixture correction and interrupted owners.
An interrupted intent is not replayed or automatically rolled back; partial file
writes remain possible.

`keep` preserves the worktree; `discard --approve RUN_APPROVAL_DIGEST` removes only
that owned worktree after a terminal state.

`artifacts ... outputs --json` lists retained output metadata. `purge-outputs
--attempt 1 --json` deletes only the registered private files for that attempt.
Expiration requires a caller to invoke the bounded purge API; no background
scheduler or secure storage erasure is implied.

If host-state cleanup fails (for example, the child removes directory
permissions), a completed attempt is recorded as failed and the CLI reports
`runtime.cleanup.unknown` (exit 6). A version probe cannot succeed with failed
state cleanup. Inspect the private orphan before retrying; no automatic OAuth
recovery or credential write-back is attempted.

On macOS, only the Codex program receives `user-preference-read` for the
`com.openai.codex` domain and shared-memory reads for names prefixed
`apple.cfprefs.`. No preference writes or other preference domains are granted.
The shared-memory namespace is the cfprefs service namespace, not a
per-domain memory restriction. Linux has no macOS preference service and its
bubblewrap profile is unchanged. A local Codex 0.159.2 startup with disposable
state, a synthetic invalid API key and networking denied passed preference
synchronization with the combined rules and reached connection errors. The
shared-memory rule alone also reached turn startup; the domain rule alone did
not. This does not establish that both rules are independently necessary.
Authenticated provider execution remains unverified.

## Resume a private finite application

Use [`finite-resume`](reference/finite-application-resume.md) to inspect a new
context-bound application intent after controller interruption. It reopens the
original admission and requires fresh approval in the same process before a
single bounded write. Partial files, later edits and already consumed resumes
remain preserved. This command starts no worker and grants no new host access.

### Finite candidate presentation before application

Before the `accept_apply` prompt, `entry.candidate_review` now carries `diff`
alongside its existing `review`. The runtime reconstructs the candidate from the
captured artifact and rereads the exact bounded patch. The decoded UTF-8 bytes
hash to `review.artifact_sha256`; that hash is already bound into the approval.
The CLI revalidates the displayed payload after approval and before application.
Changed text, artifact, candidate, evidence or source refuses the write.

The display accepts text up to 131,072 bytes, without lossy conversion or binary
patches. JSON escaping preserves bytes; clients must render patch text as data.
The existing 1 MiB output-line bound and output deadline still apply. A failed
review write prevents the approval prompt and application. This exposes the same
private captured artifact already present in the final result, earlier in the
same controller stream; it grants no new filesystem access.

`finite-resume` provides the same inline text with the original review and a
fresh resume review; see [the resume protocol](reference/finite-application-resume.md).
Local protocol tests do not replace Linux qualification of the changed binary.
