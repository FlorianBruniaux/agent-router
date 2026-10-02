# Claude local provider and MCP investigation

Status on 2026-10-02: **INCONCLUSIVE for native Claude admission**. The archived
2026-10-01 runs used driver v1, which recorded only successfully parsed JSON
POSTs. Their empty `provider_requests` arrays mean no valid JSON POST was
recorded. They did not count accepted TCP connections, malformed HTTP or
unsupported methods, so transport activity in those runs remains UNKNOWN.
The updated driver reports accepted TCP connections, bounded HTTP response
events and valid JSON POSTs separately. Its bounded v2 inventory run used
Claude Code 2.1.287 and reached the local listener: one TCP connection sent a
`HEAD` request to an unclassified route, which received HTTP 501. The process
then timed out after 25 seconds without a valid JSON POST. The tools inventory,
forced built-in refusal and native MCP round trip remain UNKNOWN. The debug log
reports that the system managed-settings file was missing; behavior when that
file is present remains UNKNOWN. No personal HOME/configuration or OAuth token
was imported, and no paid provider was used. Provider spend: USD 0.

The independent MCP fixture works under its own network-denying Seatbelt
profile. Applying that profile from inside the controller's sandbox fails.
These results support investigating a supervisor-launched sibling executor;
they do not admit the native host or justify broadening its permissions.

## Measured boundaries

The archived 2026-10-01 v1 runs used Claude Code `2.1.286`, SHA-256
`75e3016e9d2570767b08e43a7467d4817a4f149232c169ca295f2c95fef21433`, on
macOS 26.5.1 (25F80). The 2026-10-02 v2 inventory used Claude Code `2.1.287`,
SHA-256 `6eab8333fe2121553100d8f40bfada384a3e989b94f947e18ba6677a6fcb41ea`.
Linux was not tested.

Every controller run used fresh owner-only `HOME`, `CLAUDE_CONFIG_DIR` and
workspace directories, an explicit synthetic API key and `ANTHROPIC_BASE_URL`
pointing to `127.0.0.1` on an ephemeral port. The environment was constructed
from a fixed dictionary, not inherited. The Seatbelt profile used deny by
default and allowed outbound sockets only to `localhost:<provider-port>`.
No `(allow network*)` rule was added. A positive socket control reached that
port; the same process received EPERM on a second local port.

The profile retains the runtime's broad `mach-lookup` grant. Socket denial is
not proof of absence of every DNS or system-service information channel.
No external destination was contacted by the test driver, and no external
socket destination was granted to the child. There was no packet capture.

| Result | Observed outcome |
| --- | --- |
| [Initial syntax diagnostic](results/inventory-initial.json) | Seatbelt rejected `127.0.0.1:port` in the profile before executing the child; use `localhost:port`. |
| [Temporary-directory diagnostic](results/inventory-localhost.json) | Socket control passed; Claude exited 1 opening `/tmp/claude-501`. Driver v1 recorded no valid JSON POST; TCP and other HTTP activity were not counted. |
| [Three intended cases](results/local-v1.json) | After setting `CLAUDE_CODE_TMPDIR` to private state, inventory, forced Bash and MCP nonce each timed out at 25 seconds. Driver v1 recorded no valid JSON POST; TCP and other HTTP activity were not counted. |
| [Startup debug](results/inventory-debug.json) | 15-second timeout. Debug reached MDM load, certificate initialization and private `.claude.json` writes. |
| [Without restricted mode](results/inventory-without-restricted.json) | Removing only `--restricted` still timed out at 15 seconds. Debug also reached ToolSearch configuration for the local base URL. |
| [Bounded stack sample](results/inventory-sampled.json) | 45-second timeout, no valid JSON POST recorded by driver v1. TCP and other HTTP activity were not counted. Sample shows native/JSC frames and signal handling; symbols do not identify a demonstrated root cause. |
| [Instrumented v2 inventory](results/inventory-accounting-v2.json) | Claude Code 2.1.287: one accepted local TCP connection, one `HEAD` response event for `/api/<other>` with status 501, no valid JSON POST, 25-second timeout. The remainder of the route is redacted. Preflight reached only the allowed port; the second loopback port was denied with EPERM. |
| [Nested MCP control](results/mcp-protocol-control.json) | Exit 71: `sandbox-exec: sandbox_apply: Operation not permitted`. The MCP worker did not run. |
| [Standalone MCP control](results/mcp-standalone-control.json) | Exit 0. Initialize, tools/list and tools/call completed; exact nonce written. Provider port, second port and controller-state read all returned EPERM. Credential variables absent in worker. |

An absent forbidden-Bash marker in a run that never reaches the provider does
not prove built-in refusal. Likewise, the standalone worker's successful
nonce does not prove a Claude-issued MCP call. No grandchild, sibling IPC,
broker-authentication, cancellation-during-tool or Linux test passed here.

## Reproduce the bounded cases

Run from the repository root on macOS. The driver requires permission to bind
a local listener and launch `sandbox-exec`; a surrounding tool sandbox may
deny those operations before this test's own sandbox is installed. Python 3.9
or newer and the native Claude executable are required.

```sh
python3 docs/bench/egress-admission/claude_local.py \
  --case inventory --timeout 15 --output /private/tmp/claude-inventory.json

python3 docs/bench/egress-admission/claude_local.py \
  --case inventory --omit-restricted --timeout 15 \
  --output /private/tmp/claude-inventory-without-restricted.json

python3 docs/bench/egress-admission/claude_local.py \
  --case inventory --sample-startup --timeout 45 \
  --output /private/tmp/claude-startup-sample.json

python3 docs/bench/egress-admission/claude_local.py \
  --case mcp_protocol_control --timeout 5 \
  --output /private/tmp/claude-mcp-nested.json

python3 docs/bench/egress-admission/claude_local.py \
  --case mcp_standalone_control --timeout 5 \
  --output /private/tmp/claude-mcp-standalone.json
```

`--binary` selects an explicit native executable; otherwise the driver resolves
`~/.local/bin/claude`. `--case all` runs inventory, forced built-in and native
MCP cases, each with its own deadline. The two initial diagnostics describe
superseded fixture mistakes; the current script includes their corrections.
Each historical result retains its generating script digest. Ephemeral ports,
process identifiers and timestamps vary on rerun.

The native argv uses `--bare`, `--restricted`, `--tools ""`,
`--strict-mcp-config`, an explicit MCP document, empty `--setting-sources`,
an empty settings document, `--permission-prompts none`,
`--no-session-persistence`, `--max-turns 3`, print mode and stream JSON.
The MCP case adds only `--allowedTools mcp__admission__write_nonce`.
Results retain the actual argv, profiles and environment with paths and the
synthetic key redacted. The private temporary override follows the
[Claude environment-variable reference](https://code.claude.com/docs/en/env-vars).

The driver bounds process time and captured output, terminates each process
group, closes listeners and removes its temporary tree. The optional sample
targets only the fixture process and writes to a supervisor directory outside
both child profiles' write grants. Its parent directory is also protected
against child renaming. It never launches a login flow or copies
personal state. Signals or a killed driver can still leave temporary files;
no crash-recovery guarantee is claimed. Captured native stdout, stderr and
debug excerpts concern only this synthetic run; full HTTP prompts and auth
headers are not retained.

Artifact reads pin the parent directory and regular-file descriptor, reject
symlinks and FIFOs, and enforce byte limits before exporting any content.
The artifact regression suite uses only synthetic files, including an external
canary that the old sample destination overwrote through a workspace symlink:

```sh
python3 -B -m unittest discover -s docs/bench/egress-admission -p 'test_artifact_reads.py' -v

python3 -B -m unittest discover -s docs/bench/egress-admission -p 'test_provider_accounting.py' -v
```

Ten artifact tests pass on macOS, including a real Seatbelt child denied
attempts to write, unlink, create a symlink or rename the supervisor sample
directory. The sample invocation itself is simulated in the overwrite
regression. `test_provider_accounting.py` covers raw accepted connections,
malformed and unsupported HTTP, bounded body handling, preflight exclusion,
and body/header redaction. It must run outside a surrounding socket sandbox.
See `results/sample-write-regression.json` for RED/GREEN provenance and the
final standalone MCP result. Earlier results retain their historical driver
hashes. These fixture checks do not establish native CLI admission.

## Stop condition and next evidence

Stop retrying these variants without new evidence about the startup failure.
The next investigation must explain that failure while preserving the private
state and local-only network profile. A working controller then needs its
real outbound tool inventory and a forced forbidden-tool rejection. The
direct MCP control justifies reusing this worker to test a supervisor-launched
sibling and a bounded relay, rather than retrying nested `sandbox-exec` under
the failed profile.

No explicit real API keys were available in the coordinating task. The current
design permits a separately approved API key held by a broker; it does not
authorize copying subscription OAuth state. Real credentials would not fix
the demonstrated pre-request startup failure or establish tool isolation.
The global USD 5 budget is centralized by the coordinating task; this fixture
cannot spend it because it always supplies a synthetic key and local URL.
