# Recovered subscription host and offline replay

The five Python files in this directory reproduce the host delivery from 18 October 2026 byte for byte. They were recovered from recorded text edits in the originating Codex session after the temporary delivery directories could no longer be found. No historical command was executed during recovery. [recovery.json](recovery.json) records all five historical SHA-256 values and the replayed text edits.

The snapshot contains two production host modules and three test modules: 12 launcher tests, 10 parent tests and four initial-deadline tests. [verification.json](verification.json) records the fresh offline replay of all 26 tests on 19 October 2026. The replay also checks the seven pinned controller dependencies and the sealed public packet manifest already versioned in this branch.

## Run the 26 tests

From the repository root, with Python 3 on Linux or macOS:

```sh
python3 -B docs/bench/finite-subscription-relay-2026-10-18/host/test-offline.py
```

No dependency installation, Docker, credentials or model access is needed. The checker writes only disposable fixtures and prints its JSON verification report to stdout. CI runs this same command on Linux and macOS.

The historical test files reference one former worktree and a temporary packet manifest. The checker verifies their original hashes first, then rebinds those paths in disposable test copies to the versioned controller and public packets. It also redirects each test's temporary directory into its disposable fixture root. The two production modules remain byte-identical throughout. No historical source file in this directory is rewritten.

Each test interpreter and its simulated Runtime children load a Python audit guard. The guard rejects socket operations, credential-file opens, opens of a live `results.json`, writes outside the fixture root and subprocesses other than Python fixture scripts. Four preflight probes check these refusals before the suites run. This guard is a regression-test boundary, not the production OS sandbox. The parent tests still exercise real anonymous pipes and child Python processes with coded provider responses.

## What this preserves

The host constructs the private channel and controller from one loaded Session, preserving its strict class-ownership check. Preparation before `runtime_ready` has a 300-second deadline; `PrivateRuntimeIO.timeout` stays at 60 seconds and the qualified controller restores that 60-second window at READY. The tests cover READY after 90 seconds, absent READY at 300 and 301 seconds, rejection before READY, and expiry exactly 60 seconds after READY.

The historical launch configuration remains fixed at 24 assignments, one attempt per assignment, 6,000 observed tokens per trial and a reactive stop threshold of 48 million equivalent microcredits. That threshold is not a hard provider billing cap. Unknown consumption, missing cleanup, an invalid binding or a failed parent stops advancement. The production modules do not contain the coded sender used by the tests.

## Activation and evidence limits

This recovery neither approves a launch nor repairs the historical `usage_unknown` transport failure. The last campaign report received on 19 October described one transport attempt, no verified model response, unknown consumption and 23 assignments not run. That report was not reopened or reconciled as part of this recovery. No real campaign registry was read, reset, copied or modified.

The original consent-document digest remains part of the recovered source; the consent document itself, account metadata, credentials, live specifications, launch approvals, Runtime receipts and campaign registry are excluded. Moving the code changes destination-bound review material. Historical approvals are not reusable, and passing the offline tests authorizes no model request.

This directory supplements the [native campaign launcher](../../finite-campaign-launcher-2026-10-18/README.md). It preserves the missing host source and its offline checks; native qualification, actual provider availability, subscription consumption and task acceptance remain separate evidence.

A separate [diagnostic host candidate](../diagnostic-host/README.md) retains bounded, redacted transport evidence for future requests. It preserves this snapshot and the qualified controller pins; it neither reconciles the historical attempt nor authorizes a launch.
