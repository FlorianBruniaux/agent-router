# 0014: Loopback-only provider transport behind a Cargo feature

Status: accepted for `router-bridge-contract` feature `loopback-transport` (R10-a).

## Context

`OpenAiResponsesAdapter` and `AnthropicMessagesAdapter` prepare a signed
`PreparedProviderRequest`, and `verify_provider_request` checks it. Nothing
sent that request. R10 asks for a supervised transport with bounded time and
size, normalized errors and no secret in output, exercised against fake servers.

Facts read in the code of this repository:

- `verify_provider_request` (`provider.rs`) checks the Ed25519 signature, the
  run id, receipt, model, effort, mode, authorization digest, context digest,
  the SHA-256 of `prepared.body`, the timeout and output size, the provider host
  label, the method, the path, the static headers and the credential header name.
- The envelope does not bind a socket address or a URL. Only the host label
  (`openai-api`, `anthropic-api`) is bound.
- `PreparedProviderRequest` carries the name of the credential header and never
  its value. The credential is supplied by the caller at send time.

## Decision

The transport lives in `router-bridge-contract`, module `transport`, compiled
only with the `loopback-transport` feature. The default build is unchanged.

- **Endpoint.** `ApprovedEndpoint` is built by the supervising runtime from its
  own trust store, because the envelope does not carry an address. It accepts a
  loopback IPv4 or IPv6 address with a non-zero port, parsed from `ip:port`.
  A host name does not parse. It is also bound to one provider kind.
- **Revalidation.** `execute` calls `verify_provider_request` immediately before
  connecting, then hashes the body part of the exact bytes about to be written
  against the signed payload digest. Any failure returns before a socket opens.
- **Bounds.** One deadline covers connect, write and read. It comes from the
  verified `timeout_ms`, capped at 600 s. The response limit is the verified
  `max_output_bytes`, capped at `MAX_TRANSPORT_BYTES`. Headers are limited to
  16 KiB. Event count and frame size are bounded by the existing decoder.
- **No thread, cooperative cancellation.** The client is single threaded.
  Every socket call, including each connection attempt, has a timeout of at
  most 25 ms. The deadline and the `CancelToken` are checked between calls.
  Nothing survives the call.
- **Errors.** `TransportError` has fixed messages. No response bytes and no
  header value reach an error. 401/403 map to `Unauthorized`, 429 to
  `RateLimited` (`Retry-After` kept only when it is all digits, capped at
  3600 s), 5xx to `ServerError`, other 4xx to `Rejected`, a cut body or stream
  to `Truncated`, a forged or ill-formed response to `Malformed`, size and
  event bounds to `Limit`. `SecretEcho`, `Credential`, `Envelope` and `Io`
  are additions to the list requested for R10-a.
- **Redaction by refusal.** A response that contains the credential in a
  covered form is refused with `SecretEcho`, not scrubbed. The whole body is
  buffered first, so a value split across TCP chunks is caught. The forms are
  listed in `docs/provider-transport.md`.
- **Output.** The transport returns `RuntimeEvidence` from the existing stream
  decoder. Provider text is discarded there. No tool runs and no workspace is
  involved. A terminal error event is returned as evidence, not as an error.

## Alternatives

Reuse `router-decision/src/http.rs`. Rejected for this lot:

- it is `pub(crate)` and tied to `BackendError`, so reuse means a public API in
  a crate that many others depend on, outside this lot's file ownership;
- it accepts only a 200 JSON body with `Content-Length`, has no streaming, no
  chunked decoding, no status normalization, no cancellation and no redaction;
- making `bridge-contract` depend on `router-decision` adds a heavy edge for one
  helper. There is no cycle today, but the dependency would be permanent.

The cost is a second small HTTP/1.1 reader. It duplicates the loopback check,
the global deadline idea and the header bound. A later lot may extract a shared
`loopback-http` crate once both sides stabilize.

## Limits

- "Loopback only" bounds where bytes travel, not who receives them. The prompt
  and the credential go in clear text to any listener on the approved address:
  another local user, a forwarder (`ssh -L`, `socat`, a container port-forward),
  a listener that replaced the intended one. There is no peer authentication and
  no port ownership check. The runtime must pick a port it controls.
- No TLS. A non-loopback endpoint cannot be approved, so no real provider is
  reachable through this code. No real canary was run.
- Redaction is a refusal on the listed encodings only. Other transformations
  (rot13, splitting with inserted text, compression, non-ASCII case variants,
  another cipher) are not detected. Text is not returned, so only the model identifier reaches output.
- The credential is copied several times in memory (the `Secret`, the header
  string, the request bytes, the redaction patterns) and none is zeroized. The
  list is in `docs/provider-transport.md`.
- A short, low-entropy credential can make a legitimate response match and be
  refused (fail closed). See the same document.
- Response headers are not scanned for the credential. They do not leave the
  transport.
- Status handling is closed: only 200 is a success. 1xx, other 2xx and all 3xx
  are `Malformed`, and redirects are never followed.
- The credential comes from the caller. There is no broker (O4) and no
  process isolation of the secret.
- No cumulative budget or counter (O3). One call has one timeout and one size.
- `RuntimeEvidence` reports `transport_success` from HTTP status 200, with an
  exit code of 0 by convention. It says nothing about the task result.
- Chunked bodies are decoded without trailers or chunk extensions. Redirects,
  compression and HTTP/2 are refused.

## Left for R10-b

- Runtime wiring, credential broker and revocation.
- A shared HTTP helper if the two transports converge.
- TLS and any non-loopback endpoint, with its own approval and evidence.
- A real canary with authorized credentials and cost.
