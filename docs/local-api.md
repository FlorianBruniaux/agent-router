# Local route-only API

Opt-in and read-only. Build with `--features local-api`; without it there is no
`serve` subcommand. The API returns routing decisions. It cannot execute,
record feedback or outcomes, activate a backend, grant anything or read the
catalog store. See `docs/adr/0012-local-route-api.md`.

## Start

```sh
agent-router serve --state-dir /abs/path/state [--max-connections 16] [--request-deadline-ms 10000]
```

The state directory must be absolute, owned by you, mode 0700 and not a
symlink. It is created (0700) when missing, but its parents are not checked:
place it under a directory you control. The command prints one JSON line once
listening, then serves until SIGINT or SIGTERM, then removes the socket. A
second SIGINT or SIGTERM exits at once with status 130 and leaves the socket,
which the next start replaces because nothing accepts on it.

- `router.sock`: Unix socket, mode 0600. The mode is applied after `bind`, not
  with `fchmod` before `listen`, so the 0700 directory is what keeps other
  users out.
- `token`: bearer token, mode 0600. Created on first start from 32 random bytes
  in hex, written to a temporary file and published without replacing an
  existing token. That randomness is guaranteed only at creation: a later start
  accepts any existing owner-only file holding 32 to 256 visible ASCII
  characters. The token is never printed or logged.
- `lock`: held with a non-blocking `flock` for the life of the server. A second
  `serve` on the same directory fails with `local_api.state_dir.locked`.

## Requests

Send `Authorization: Bearer <token>` on every request.

| Method and path | Purpose |
| --- | --- |
| `GET /v1/health` | `{"schema_version":1,"status":"ok"}` |
| `GET /v1/capabilities` | API version, `route-only` authority and limits |
| `POST /v1/route` | Routing decision |

`POST /v1/route` takes `Content-Type: application/json` and a body:

```json
{"request": {}, "catalog": {}, "host_snapshot": {}}
```

`request` and `catalog` are required, `host_snapshot` is optional. Each is the
content of the file the matching `agent-router route` flag would read. The
response body is byte-identical to `agent-router route --json` stdout on the
same documents, for `selected` and `no_route` alike (HTTP 200 for both).
`--retrieval` has no API equivalent.

## Refusals

Errors are `{"schema_version":1,"error":"<code>"}`.

| Status | Cause |
| --- | --- |
| 400 | Malformed HTTP, `Transfer-Encoding`, duplicate `Content-Length`, duplicate JSON key, unknown top-level field, invalid documents (CLI codes such as `request.json.invalid`) |
| 401 | Missing or wrong token |
| 404 / 405 | Unknown path / wrong method (after authentication) |
| 408 | Whole-request deadline exceeded |
| 500 | Handler failure, including a panic (`local_api.handler_panic`), or a response that cannot be serialized (`local_api.internal`) |
| 411 / 413 / 415 | No length / body over 9 MiB / not `application/json` |
| 431 | Request line and headers over 16 KiB |
| 503 | Connection cap reached |
| 505 | Any `HTTP/` version other than 1.1 |

Per-document limits match the CLI: request 1 MiB, catalog 4 MiB, snapshot 4 MiB.

## Limits

- Whole-request deadline (default 10 s) for reading the request, then a fixed
  2 s to write the response and 200 ms to discard unread bytes. A slow peer
  cannot extend any of them, so the read, write and drain phases take at most
  about 12.2 s with the default deadline. `RouteHandler::route` runs between the
  read and the write, synchronously, and is not covered by that bound: the
  server does not interrupt a handler, so a connection slot is held for the
  handler's run time on top of it.
- JSON nesting depth is 64 in the API. The CLI parses with serde_json's default
  limit, documented as 128 and not re-measured here, so a document nested
  between 65 and 128 levels is accepted by `route` and refused by the API.
- At most 100,000 object members per body.
- Transient `accept` errors (`EMFILE`, `ENFILE`, `ECONNABORTED`, `ENOBUFS`,
  `ENOMEM`) are retried with a backoff of at most 500 ms; any other `accept`
  error stops the server.
