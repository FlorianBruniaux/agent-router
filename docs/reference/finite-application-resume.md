# Resume an interrupted finite application

`finite-resume` supports existing private output stores created with the durable
controller context introduced in ADR 0047. It starts no worker or model. An
older intent without that context is refused. It does not resume generation.

Inspect with:

```sh
agent-router finite-resume --config resume.json --run-id entry --json
```

The config contains `schema_version: 1`, `root` (the existing absolute runtime
store), and `admission` (the exact original AdmissionConfig, including its
registry path, expected_policy_sha256, runtime_id and optional session_budget).
Unknown fields, changed admission binding, missing stores, and noncanonical
paths are refused. Never substitute a new admission registry to reopen a store.
Opening can perform ordinary store recovery; inspection does not change the
source or append an application resume intent.

The `resume.review` JSON line includes `review`, `original` (the persisted
application review), and `diff` (the exact captured UTF-8 patch). Hash the decoded
UTF-8 bytes to compare with `original.artifact_sha256`; JSON escaping is transport
encoding, not part of the hash. Render this untrusted text as data. The candidate
hash and original approval bind the original review to the fresh resume review.
This diff describes the original candidate, not a delta computed from the current
possibly divergent file. A state without an approval digest permits no write.

The display is limited to 131,072 bytes. Missing, changed, symlinked, non-UTF-8 or
binary artifacts refuse presentation and interactive application. No external
display-file path is accepted. This may refuse display even when the original
state-only library review remains available. The CLI revalidates the whole
presentation before using the existing application checks. Persisted context,
original review and approval digest formats are unchanged.

The `review.state` has these meanings:

| state | Meaning | Approval |
| --- | --- | --- |
| `preimage_intact` | Source, metadata, Git and acceptance still match | Fresh digest |
| `candidate_observed` | Candidate bytes present without a completion record | None |
| `divergent` | Source differs, including partial writes or later edits | None |
| `completed` | An application completion is already recorded | None |
| `consumed` | A previous resume intent has no completion record | None |

Some unsafe metadata, Git or context changes cause an explicit error before a
review is produced. `observed_sha256` reports current bytes when a review is
available. A historical `completed` record does not certify unchanged current
bytes. Candidate acceptance and application completion remain distinct.

To apply an eligible candidate, start a **new** interactive invocation:

```sh
agent-router finite-resume --config resume.json --run-id entry --json --approve
```

Read that process's `resume.review`, then send one JSON line on its stdin within
30 seconds:

```json
{"schema_version":1,"kind":"resume.approve","approval_digest":"DIGEST_FROM_THIS_PROCESS"}
```

The inspection process's digest, the original application approval, and a digest
from another handle are invalid. A successful write emits `resume.applied`.
Input, output and authority lifetimes are bounded. A failed or interrupted
resume may have consumed its only intent; rerunning cannot replay it.

The command preserves original attestations and integration history. It never
repairs a partial file, rolls back later edits, publishes a ref, or starts a new
execution. The existing exclusive-controller ownership requirement still applies.

See [ADR 0047](../adr/0047-explicit-finite-application-resume.md) for contracts and
bounds. Unit tests and native qualification provide separate evidence.
