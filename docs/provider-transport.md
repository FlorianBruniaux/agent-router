# Provider transport (loopback)

`router-bridge-contract` can send a verified `PreparedProviderRequest` to an
approved loopback endpoint and decode the stream. It is off by default. See
[ADR 0014](adr/0014-loopback-provider-transport.md).

## Enabling

```toml
router-bridge-contract = { path = "integrations/bridge-contract", features = ["loopback-transport"] }
```

Without the feature, the `transport` module does not exist and no socket code is
compiled.

## Usage

```rust
use router_bridge_contract::transport::*;

let endpoint = ApprovedEndpoint::parse(prepared.signed_route.envelope.provider, "127.0.0.1:8123")?;
let transport = ProviderTransport::new(endpoint, Secret::new(api_key)?);
let cancel = CancelToken::new();
let evidence = transport.execute(&adapter, &prepared, &trusted_key, &validated, run_id, &cancel)?;
```

- `trusted_key`, `validated` and `run_id` come from the supervising runtime, as
  for `verify_provider_request`.
- The endpoint comes from the runtime's own configuration. The signed envelope
  binds the provider host label, not an address.
- Call `cancel.cancel()` from another thread to stop a running call. The call
  returns `Cancelled` at the next socket poll. Every socket call waits at most
  25 ms, and connection attempts are retried in 25 ms slices, so a listener
  whose accept backlog is full cannot hold the call past the token or the
  deadline. The tests assert a return within 3 s, not a tighter bound.

The result is `RuntimeEvidence` from the stream decoder. Provider text is
discarded. A terminal provider error event is evidence (`HostError`), not a
`TransportError`.

## Trust boundary: what "loopback only" does not give

Loopback restricts where the bytes go on the network. It does not authenticate
who listens. The prompt and the credential are sent in clear text to whatever
accepts connections on the approved address:

- another local user or process that bound the port first;
- a forwarder such as `ssh -L`, `socat` or a container port-forward, which can
  carry the bytes to a remote host;
- a listener that replaced the intended one after approval.

There is no peer authentication and no check of who owns the port. The runtime
that approves the address is responsible for choosing a port it controls.

## What is checked, in order

1. The adapter provider equals the envelope provider, and the endpoint was
   approved for that provider.
2. `verify_provider_request` passes.
3. The body part of the bytes to write hashes to the signed payload digest.
4. The connection is opened, the request is written and the response read.

Steps 1 to 3 run before any connection. A refusal opens no socket.

## Errors

| Error | Cause |
| --- | --- |
| `Refused` | endpoint not loopback, port 0, host name, or approved for another provider |
| `Credential` | credential shorter than 8 bytes, longer than 4096, or not visible ASCII |
| `Envelope` | verification failed, or adapter and envelope providers differ |
| `Unauthorized` | HTTP 401 or 403 |
| `RateLimited { retry_after_secs }` | HTTP 429. `Some` only if `Retry-After` is all digits, capped at 3600 |
| `ServerError { status }` | HTTP 5xx |
| `Rejected { status }` | other HTTP 4xx |
| `Timeout` | deadline reached during connect, write or read |
| `Cancelled` | token set |
| `Truncated` | body shorter than `Content-Length`, cut chunk, stream without a terminal event, or a last frame not ended by a blank line |
| `Malformed` | bad status line or header, redirect, wrong content type, content encoding, both `Content-Length` and `Transfer-Encoding`, forged or ill-formed event |
| `Limit` | headers over 16 KiB, body over `max_output_bytes`, too many events |
| `SecretEcho` | the response contains the credential in a covered form |
| `Io` | other socket failure |

### Status lines and framing

| Response | Behavior |
| --- | --- |
| HTTP/1.1 or HTTP/1.0 status line | accepted. HTTP/1.0 without a length is read until close |
| any other version (for example `HTTP/2`) | `Malformed` |
| 1xx | `Malformed` |
| 2xx other than 200 (201, 202, 204, 206) | `Malformed`, even with a valid stream |
| 3xx (301, 302, 303, 307, 308) | `Malformed`. A redirect is never followed and its `Location` is ignored |
| chunk extension, trailer, non-hex or signed size, empty size line | `Malformed` |
| chunk size line of 9 hex digits or more | `Malformed` |
| chunk size of 8 hex digits above the response limit | `Limit` |
| missing CRLF after chunk data, bytes after the last chunk | `Malformed` |

Error messages are fixed strings. Error bodies are never read into an error.

## Redaction

Redaction is a refusal: a response containing the credential is not returned.
The body is fully buffered before the scan, so a split across TCP chunks does
not matter. The scan covers the credential as:

- the raw value;
- JSON string escapes: quote and backslash escaped, ASCII with non-ASCII as
  `\uXXXX` (lower and upper hex), every character as `\uXXXX`;
- every JSON string and key of each `data:` payload after decoding, which
  covers any mix of escaped and plain characters;
- the concatenation of the strings found at the same JSON path across events,
  which covers a value split between two deltas;
- base64, standard and URL-safe alphabets, at each of the three byte alignments,
  without padding;
- percent encoding (unreserved characters kept, or every byte), upper and lower
  hex;
- hex, lower and upper case.

Matching is a byte substring test. Every pattern except base64 ignores ASCII
case, so upper, lower and mixed case hex, percent encoding and the raw value
are covered. The scan also runs on the text with ASCII whitespace removed
(base64 wrapped by MIME line breaks, a value cut by a line break), and on the
lines of one event joined without a separator (a value cut between two `data:`
lines).

Exact limits:

- Not detected: rot13, insertion of text inside the value, compression, a value
  split across strings at different JSON paths, two-step encodings, and case
  variants of non-ASCII characters.
- Base64 matches exactly, so a base64 form whose case was changed is not seen.
- The scan looks at the response body only. Response headers are not scanned;
  they never leave the transport today (only the status and a parsed
  `Retry-After` integer are used).
- A credential under 8 bytes is refused at construction, because its pattern
  would match ordinary protocol text.
- Only the model identifier and event kinds leave the transport. A model
  identifier comes from one JSON string and is covered by the scan.

Copies of the credential in memory, none of them zeroized:

- the `Secret` string, and the caller's own copy passed to `Secret::new`;
- the clone made for a header whose name is not `Authorization`
  (`transport.rs`, `self.credential.0.clone()`), and the `format!("Bearer ...")`
  string for `Authorization`;
- the `head` string built by `wire`, which the `format!` of each header line
  also copies into;
- the `wire` byte vector, dropped after the write but not wiped, and freed
  buffers left behind by a reallocation;
- in `Redactor::new`, the `plain` and `exact` pattern vectors before the
  lower-casing pass, then the final patterns (fewer than 30 encoded forms);
- the intermediate strings that `forms` builds with `format!("{b:02x}")` and
  the other encoders;
- kernel socket buffers, outside the process's control.

## False positives

The refusal is fail closed, and a short, low-entropy credential can trigger it
on a legitimate response. Case folding, whitespace removal and concatenation by
JSON path all widen what matches. Examples that are refused:

- a credential such as `deadbeef` or a common word of 8 bytes, which then
  appears in ordinary text in any case;
- a legitimate text that spells the credential with a space or a line break
  inside it;
- consecutive deltas at one JSON path whose concatenation spells the credential.

Fragments alone do not trigger it: the tests show a response that carries
pieces of the credential in another case, or in different JSON paths, accepted.
`MIN_SECRET_BYTES` stays at 8. Raising it would refuse real short local tokens
and would not help with a guessable value, which is the caller's choice; a
random credential of 8 or more characters is unlikely to match by accident.

## Not covered

No TLS, no non-loopback endpoint, no real provider canary, no credential
broker, no cumulative budget. The tests use fake TCP servers on `127.0.0.1`;
the IPv6 test binds `[::1]` and skips itself with a message if the host cannot.
