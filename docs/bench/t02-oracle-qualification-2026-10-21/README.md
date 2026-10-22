# Frozen t02 oracle qualification, 2026-10-21

The retained t02 oracle is **unqualified**: it accepts a solution that loses
unsigned sum overflows. This is a new diagnostic verdict about the exact frozen
oracle, not a change to a historical task verdict or an accepted correction.

| Control | Independent public-contract label | 13 public tests | 4 frozen oracle tests |
| --- | --- | --- | --- |
| `correct_i128` | Correct | Pass | Pass |
| `correct_u128` | Correct | Pass | Pass |
| `sum_u64` | Known defect: wrapping sum | Fail | **Pass: missed defect** |
| `round_each` | Known defect: rounding before sum | Fail | Fail |
| `omit_bounds` | Known defect: missing work bound | Fail | Fail |
| `truncate_final` | Known defect: truncating final result | Fail | Fail |

The decisive public counterexample is
`score(0, &[u64::MAX, 1], &[], &[]) == Ok(-4611686018427387904)`.
The sum is `2^64`, so dividing by four gives `2^62`. Wrapping the sum in `u64`
produces zero instead. Equivalent latency and skill witnesses are included.
Expectations are literal values derived from the public formula, not copied
from another implementation's output.

## Evidence and independence

- [labels.json](labels.json) records a separate agent's mathematical review of
  the exact two correct controls, four defects and 13 public tests (14 assertions).
  That reviewer saw neither the hidden oracle nor its results before assigning
  labels and verifying input digests. This is agent review, not human acceptance
  or authenticated external provenance. Control selection targeted weaknesses
  after the author inspected the frozen oracle; the separate label reviewer
  remained blind to it. This supports a counterexample, not an unbiased adequacy estimate.
- [controls.json](controls.json) pins the public sources. Case names use `qa174_`;
  this is qualification material, not held-out decision evidence. Different IDs
  do not establish semantic hold-out independence.
- [result.json](result.json) records actual Linux executions, compiler identity,
  input/runner digests, bounded outputs' digests and outcomes. Two deliberate
  infrastructure failures (missing compiler and compilation refusal) yield
  `unknown`, never functional failure.
- [isolation.json](isolation.json) records the cell: 8,589,934,592 bytes, zero
  swap, one CPU, 256 PIDs, non-root UID, read-only root, no network, no host mounts,
  all capabilities dropped and no-new-privileges. Private inputs were copied
  through stdin into a private disposable directory. No access policy or original
  file was changed. Per-child limits bound CPU, wall time, address space and
  output further. In `result.json`, `child_output_bytes: 65536` is the capture
  and classification threshold, not an emission limit. The temporary output file
  is separately capped at 16 MiB by `RLIMIT_FSIZE`; excess capture yields unknown.
  The cell is stopped after retaining evidence.
- [provenance.json](provenance.json) reconstructs acceptance binding
  `072e26ddcfb71bfd284cbced9acdd3dc3c04ae72960d48f6676f7bc339224563`
  in the [t02 campaign packets](../../../crates/router-runtime/examples/campaign/packets.json).
  The retained R2 preparation seal and its input/dependency hashes were checked,
  along with the corpus-manifest digest and saved oracle entry. This establishes
  correspondence of retained local bytes, not provenance of every historical
  corpus file or a human review. The older R1 preparation is not this lineage.

Frozen oracle SHA-256:
`dcf8247d51256f888681b604b29503304eaa7fca89a9be97c3c6f1373c92b821`.
Its private source is absent from this directory. Historical corpus files and
verdicts remain unchanged. No model, provider or campaign registry was invoked.
`oracle_authenticity` stays `supplied_unverified`, `human_acceptance` is null and
`adoption_authorized` is false.

[validation.json](validation.json) records the complete local gates: formatting,
warnings-denied workspace Clippy, 1,934 reported passing Rust results with zero
failures and 43 ignored results (nested helper summaries included, not unique
tests), harness parity, ten Python tests on macOS and Linux, and Gitleaks.
The initial terminal-sandbox socket refusal and successful local API replay are
retained separately; no product policy was changed to make those tests pass.

## Recheck the published evidence

From this directory, without the private oracle or network:

```sh
python3 test_qualify.py
rustfmt --edition 2021 --check controls/*.rs public_witnesses.rs
```

Ten Python tests check result classification, bounded input refusal, missing
compiler handling, source/label/report bindings, recorded outcomes and limits.
They validate retained evidence consistency; they do not rerun the hidden oracle.

## Replay in the existing local Linux image

A replay requires the matching retained private `hidden.rs` and the already
installed image below. Missing private evidence or image means unavailable;
never fetch another oracle, substitute a model or pull an image. The runner
accepts only the pinned oracle and six fixed controls. It is not a generic
candidate evaluator.

Create a fresh cell with the same restrictions:

```sh
docker run --detach --name ar174-oracle-replay --pull=never --init \
  --network=none --read-only --user=65534:65534 --cap-drop=ALL \
  --security-opt=no-new-privileges --memory=8589934592 \
  --memory-swap=8589934592 --cpus=1 --pids-limit=256 \
  --tmpfs /tmp:rw,exec,nosuid,nodev,size=536870912,mode=1777 \
  --tmpfs /task:rw,exec,nosuid,nodev,size=2147483648,mode=1777 \
  sha256:a808cb44a27ee036c8f0d7bd8aeb6150ecc3fe83f8dac3cfbfcf4df19a88f71b \
  sleep 3600
```

Inspect the cell before execution. Copy this directory and the exact private
oracle through stdin, without a host mount, into `/task/probe` (mode 0700, files
0600). Place the oracle at `/task/probe/private/hidden.rs`. Then run:

```sh
docker exec ar174-oracle-replay python3 /task/probe/test_qualify.py
docker exec ar174-oracle-replay python3 /task/probe/qualify.py \
  --oracle /task/probe/private/hidden.rs --output /task/probe/replay.json
docker exec ar174-oracle-replay cat /task/probe/replay.json
docker stop ar174-oracle-replay
```

The runner refuses to overwrite prior evidence. Exit zero means all diagnostic
executions completed, including an `unqualified` verdict. Exit two means incomplete
evidence. Input or isolation refusal produces no new verdict. A complete Rust test
summary is required for a functional outcome; timeout, abnormal exit, missing or
partial summary, ignored/filtered tests and excessive output remain unknown.

## Remaining work for #174, #132 and F12a

This finite probe does not establish exhaustive adequacy or an independently
accepted useful correction. The frozen oracle cannot support that acceptance
claim. The next slice must create a separately versioned replacement evaluator,
qualify it on independent labels and reserve new held-out decision cases. Preserve
this negative result and all earlier verdicts.

Authenticated review/provenance, durable append-only verdict storage and runtime
useful-correction acceptance remain open. This diagnostic changes no product
admission, approval digest, private-access rule, resource limit or runtime
evaluator. F12a is not unblocked by a passing probe runner.
