# Offline routing demo

[routing-demo.gif](routing-demo.gif) and [routing-demo.mp4](routing-demo.mp4)
show three real invocations of the built Agent Router CLI. Both are 1280 × 800,
last 24 seconds and use eight-second editorial holds. Their playback timing is
not a startup or routing benchmark. The MP4 uses H.264, 12 fps and no audio.

1. `route` selects the plan, execute and verify stages from a public request and
   catalog. It exits with code 0.
2. `explain` reads the exact decision captured in the first step. It exits with
   code 0.
3. `route-phase` receives a valid synthetic snapshot with no available models.
   It returns `no_eligible_candidate` and exits with code 3. This is a routing
   refusal, not a malformed-input error.

All inputs are public engineering fixtures. These commands do not call models,
install tools, contact providers or execute the selected plan. The synthetic
snapshot's clock is fixed at Unix second 100. It must never be used as current
production evidence.

The screens show exact, noncontiguous stdout lines with their original line
numbers. No result text is invented or rewritten. Colors, titles and captions
are presentation annotations. The complete stdout, stderr, exit codes, input
hashes and binary hash are retained in
[routing-demo-transcript.json](routing-demo-transcript.json). The executable
path is displayed portably as `agent-router`, and the selected output directory
is displayed as `docs/media`; no private machine paths are recorded. Nothing
was redacted from the successful captured stdout or stderr. The renderer rejects
unexpected private path or credential markers instead of publishing them.

`route-decision.json` is the captured first result. The derived snapshot in
`fixtures/phase-unavailable.json` clears `supported_models` and
`supported_efforts` from the public phase snapshot and recomputes its integrity
hash. All remaining synthetic provenance stays unchanged. Acceptance by
`route-phase`, followed by an exit-3 receipt, checks that the snapshot is valid.

## Reproduce

Use Python 3.11 or later with Pillow, ffmpeg and a monospace TrueType font already
installed. The script does not install dependencies. Run from the repository
root with a built CLI:

```sh
cargo build -p agent-router-cli --locked
python3 scripts/render-readme-demo.py \
  --binary target/debug/agent-router \
  --font /System/Library/Fonts/Menlo.ttc \
  --output-directory docs/media
```

On Linux, pass an installed font such as
`/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf`. If ffmpeg is not on `PATH`,
pass its location with `--ffmpeg`. A different font or ffmpeg version may change
the media bytes. The command results and their hashes remain the source of
truth. Run `--help` for the supported arguments.

The script captures each command with a ten-second timeout and a 256 KiB output
limit, checks the selected phases and expected refusal, and verifies that the
explanation references the captured decision. It also checks text layout bounds,
GIF frame inventory and duration, and enforces a 4 MiB GIF budget. The three PNGs
in `frames/` are the complete inventory of visual states. `routing-demo-poster.png`
is the first frame for a video poster or static fallback.

The generated media and all three source frames were visually inspected for
readability. The published MP4 was checked to be 24 seconds at 1280 × 800. This
confirms presentation and capture consistency, not native host execution or
performance.
