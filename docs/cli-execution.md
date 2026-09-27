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

An explicit policy file overrides a policy store. When the store supplies the
policy, the generated definition preserves its artifact identity and checks that
it remains active before fresh execution. Policy source changes invalidate the
CLI approval. Historical inspection reconstructs the exact existing definition.

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
Retries are explicit with `run --retry` and require a new approval digest.
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
