# Standard sidecar client v1

R11 adds an optional outbound client. It starts no server, downloads no model,
changes no policy and never sends an event after a run automatically. The default
build has no sidecar transport. Build with `--features local-sidecar` to expose
it. `backend --backend deterministic` remains independent of any server.

## Protocol and identity

The standard client supports four fixed endpoints:

| Request | Meaning |
| --- | --- |
| `GET /v1/capabilities` | Negotiate protocol version, artifact identity, operations and bounds |
| `POST /v1/rank` | Suggest a choice from the supplied immutable redacted pool |
| `POST /v1/outcome` | Explicitly submit one retained automated observation |
| `POST /v1/feedback` | Explicitly submit one separately recorded human verdict |

Version negotiation lives in capabilities; there is no additional `/version`
endpoint. Every message has `schema_version: 1` and `protocol_version: 1`.
`backend_artifact_sha256` must match the operator's explicit expected digest.
This pin detects changed claims; it is not attestation of deployed server code.
The existing nested `BackendRequest.artifact_digest` identifies the routing
context, and is a separate identity from the backend artifact.

A capabilities response contains exactly these fields:

```json
{
  "schema_version": 1,
  "protocol_version": 1,
  "backend_artifact_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "operations": ["rank", "outcome", "feedback"],
  "max_request_bytes": 32768,
  "max_response_bytes": 65536,
  "max_candidates": 20,
  "idempotent_telemetry": true
}
```

This digest is a synthetic example. Operations must be unique, known and
nonempty. Versions must match exactly. Advertised bounds must be positive. Each
operation uses the minimum of the local and advertised request, response and
candidate limits. Larger server limits are compatible and never expand local
budgets; smaller limits further restrict messages. Capability output preserves
the server's declaration rather than relabeling it as an effective local limit.
Unsupported operations fail before their POST. Capabilities are negotiated for
each exchange, with no persistent cache and no fallback to another protocol.

`SidecarRankRequest` wraps the existing `BackendRequest` in the version/identity
envelope. `SidecarRankResponse` wraps `BackendProposal` in the same envelope.
The request carries at most 20 opaque candidate IDs and numerical features,
with task kind, risk and phase. It contains no objective, workspace, model name,
prompt or asset body. The response cannot introduce candidates, permissions,
tools or capabilities. A nonempty probability distribution must sum to exactly
1,000,000. Confidence is only a server claim.

All standard messages reject duplicate JSON keys, unknown fields and malformed
or excessive values. The selected ID and distribution are checked against the
original pool and context digest. The caller still applies its confidence gate.
The CLI `backend` command keeps the deterministic receipt and reports only a
shadow choice. HTTP negotiation or acknowledgement never activates a backend.
Existing signed promotion and runtime authority remain separate.

## Explicit commands

```text
agent-router sidecar capabilities --endpoint 127.0.0.1:8000 \
  --expected-backend-artifact <sha256> --json

agent-router backend --backend standard-v1 --endpoint 127.0.0.1:8000 \
  --expected-backend-artifact <sha256> --allow-redacted-metadata \
  --context <phase.json> --host-snapshot <snapshot.json> --catalog <catalog.json> \
  --timeout-ms 500 --on-error fallback --json

agent-router sidecar outcome --endpoint 127.0.0.1:8000 \
  --expected-backend-artifact <sha256> --store <private-feedback-store> \
  --event-sequence <sequence> --expected-event-hash <sha256> \
  --expected-consent-revision <revision> --allow-sidecar-export --json
```

Use `sidecar feedback` with the same export options for a human event. Obtain the
sequence, event hash and revision from an explicit local feedback export. These
commands require a server already configured by the operator. The implementation
ships no matching server. `--bearer-env NAME` reads only that explicitly selected
variable; the value is never printed in diagnostics or persisted by this client.

Jev and Laya remain explicit legacy choices: `--backend jev` and `--backend laya`
continue to use `/v1/systemone`. They do not negotiate this protocol, and the
standard client never retries through them after a refusal.

## Consent and redaction

`--allow-redacted-metadata` permits ranking metadata only. It does not authorize
telemetry. `--allow-sidecar-export` separately authorizes exactly the selected
retained event and destination. Collection and learning must both be enabled in
the local store, the expected current consent revision must match, and the event
must have been collected under that revision and remain within retention. A later
consent revision does not silently reauthorize old retained events.

`FeedbackStore::authorize_sidecar_export` validates the selected row and its hash,
then returns a non-serializable guard holding the store mutex. The CLI retains it
through the network exchange. The CLI also caps its entire exchange deadline at
the event retention boundary, including negotiation time. Configure/purge through that store is serialized
with this operation. Revocation completed before acquisition prevents the send;
a revocation requested during an authorized send waits for the guard. Cooperative
processes also respect the existing store owner lock. This is not protection
against an arbitrary same-user process rewriting SQLite or ignoring that lock.

The projection contains typed verdicts, bounded counters, optional measurements,
and hashes. Model/harness/context identities are hashed; free-form identifiers,
producer strings, comments, paths and output bodies are excluded. Hashes preserve
correlation and are not anonymization. Missing values remain unknown. Automated
transport success does not become objective success or a human label. An event
hash binds a local claim; it does not authenticate the run or reviewer.

Library callers must separately authorize a destination and keep the live consent
guard for the complete exchange and respect its `valid_until_unix_seconds`
retention boundary. `SidecarExportAuthorization::for_event` is an
explicit grant for that event hash and revision, not a replacement for the store
or an authority deserialized from a server response.

## Bounds and delivery

Only numeric loopback addresses with a nonzero port are accepted. DNS, proxy
configuration, redirects and alternate endpoints are not used. Requests are
limited to 32 KiB, responses to 64 KiB and headers to 16 KiB. HTTP responses require
status 200, JSON content and one bounded Content-Length; chunked and compressed
responses are refused. Negotiation and operation share one deadline, at most
10 seconds. There are zero automatic retries, including rank inference.

Telemetry requires advertised idempotency support. Its stable message digest
binds the backend artifact, operation and complete payload. The ACK must repeat
that digest, operation and protocol/artifact identity, with status `accepted` or
`duplicate`. It attests only the server's response. CLI output explicitly retains
`learning_effect: "unknown"`.

After any POST bytes have been emitted, a missing, malformed, mismatched or late
ACK returns `backend.delivery.unknown`. It is not proof that the server did not
receive the event. The client never retries automatically or replays a native run.
An explicit repeat uses the same message digest. A server claiming idempotency
must implement it; this client cannot prove the server's actual retention,
deduplication or learning behavior. Local purge cannot retract bytes already
sent and does not claim remote deletion. There is no background queue or sender.

## Verification scope

Synthetic loopback tests exercise exact paths and bodies, incompatible negotiation,
pool/context/digest checks, strict schemas, shared timeouts, bounds, denied consent,
revocation and ambiguous delivery. Tests also preserve the legacy adapters.
No installed model, real learning effect or provider is exercised.

R19 is separate: the inbound daemon, Unix socket/TCP authentication, SSE,
execution/events APIs and Omnigent `/routes:select` endpoint are not implemented
here. The existing offline Omnigent translation does not prove that Omnigent's
fallback applies an Agent Router refusal.
