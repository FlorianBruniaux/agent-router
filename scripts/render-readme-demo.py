#!/usr/bin/env python3
"""Capture real offline CLI results and render a small, readable README demo.

Requires Pillow and ffmpeg, already installed by the caller. Installs nothing.
All screen output is an exact, line-numbered excerpt of captured stdout.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tempfile

from PIL import Image, ImageDraw, ImageFont

WIDTH, HEIGHT = 1280, 800
DURATION_MS = 8000
BG, PANEL, BORDER = "#0b1120", "#111b2e", "#26364d"
TEXT, MUTED, GREEN, BLUE, AMBER = "#e4edf9", "#9aacbf", "#73e2b0", "#9ec5ff", "#ffd38a"


def sha256(path: Path) -> str:
    with path.open("rb") as handle:
        digest = hashlib.file_digest(handle, "sha256")
    return digest.hexdigest()


def public_text(value: str) -> None:
    for forbidden in ("/Users/", "/private/", "/home/", "florian", "TOKEN=", "API_KEY="):
        if forbidden in value:
            raise ValueError("Unexpected private text in a demo artifact; capture was not published")


def capture(binary: Path, repo: Path, args: list[str], display_args: list[str], expected: int) -> dict:
    result = subprocess.run([str(binary), *args], cwd=repo, capture_output=True, timeout=10, check=False)
    if len(result.stdout) + len(result.stderr) > 256 * 1024:
        raise ValueError("Demo output exceeded its 256 KiB capture limit")
    stdout, stderr = result.stdout.decode("utf-8"), result.stderr.decode("utf-8")
    public_text(stdout)
    public_text(stderr)
    if result.returncode != expected or stderr:
        raise ValueError(f"Unexpected CLI result for {args[0]}: exit {result.returncode}")
    return {"argv": ["agent-router", *display_args], "exit_code": result.returncode,
            "stdout": stdout, "stderr": stderr,
            "stdout_sha256": hashlib.sha256(result.stdout).hexdigest()}


def unavailable_snapshot(repo: Path, destination: Path) -> None:
    observation = json.loads((repo / "fixtures/examples/phase-snapshot.json").read_text())
    observation["host"]["supported_models"] = []
    observation["host"]["supported_efforts"] = {}
    observation.pop("snapshot_hash")
    # Preserve the public fixture's Rust-serialized field and enum-map order.
    payload = json.dumps(observation, separators=(",", ":"), ensure_ascii=False).encode()
    observation["snapshot_hash"] = hashlib.sha256(b"agent-router:host-observation:v1\0" + payload).hexdigest()
    destination.write_text(json.dumps(observation, indent=2) + "\n")


def selected_lines(stdout: str, kind: str) -> list[int]:
    lines = stdout.splitlines()
    if kind == "route":
        # The first selected plan only; alternatives are retained in the transcript.
        stop = next(i for i, line in enumerate(lines) if '"alternatives":' in line)
        keys = ('"preset":', '"kind":', '"model":', '"effort":', '"utility_micros":')
        return [i + 1 for i, line in enumerate(lines[:stop]) if line.strip().startswith(keys)]
    if kind == "explain":
        return list(range(1, len(lines) + 1))
    indices = {i for i, line in enumerate(lines) if line.strip().startswith(('"selected":', '"abstention":'))}
    for i, line in enumerate(lines):
        if '"code": "model.host.unsupported"' in line:
            indices.update((i - 1, i, i + 1))
    return [i + 1 for i in sorted(indices)]


def render_screen(font_path: Path, title: str, command: list[str], record: dict, numbers: list[int], caption: str) -> Image.Image:
    image = Image.new("RGB", (WIDTH, HEIGHT), BG)
    draw = ImageDraw.Draw(image)
    mono = ImageFont.truetype(str(font_path), 20)
    small = ImageFont.truetype(str(font_path), 16)
    heading = ImageFont.truetype(str(font_path), 28)
    draw.text((36, 25), "AGENT ROUTER", font=heading, fill=TEXT)
    draw.text((36, 66), title, font=mono, fill=BLUE)
    draw.text((790, 35), "OFFLINE / PUBLIC FIXTURES", font=small, fill=GREEN)
    draw.rounded_rectangle((32, 111, 1248, 737), radius=14, fill=PANEL, outline=BORDER, width=2)
    for x, color in [(57, "#fb7185"), (81, "#fbbf24"), (105, "#34d399")]:
        draw.ellipse((x, 130, x + 10, 140), fill=color)
    draw.text((134, 124), "agent-router", font=small, fill=MUTED)
    code = record["exit_code"]
    draw.text((1030, 124), f"exit {code}", font=small, fill=GREEN if code == 0 else AMBER)
    draw.line((33, 153, 1247, 153), fill=BORDER, width=1)
    y = 170
    for line in command:
        if draw.textlength(line, font=mono) > 1154:
            raise ValueError("Command does not fit; reflow explicitly before rendering")
        draw.text((57, y), line, font=mono, fill=GREEN)
        y += 27
    y += 12
    draw.text((57, y), "stdout excerpt (original line numbers):", font=small, fill=MUTED)
    y += 27
    original = record["stdout"].splitlines()
    for number in numbers:
        line = original[number - 1]
        if draw.textlength(line, font=mono) > 1100:
            raise ValueError("Output does not fit; never crop CLI text silently")
        if y + 26 > 701:
            raise ValueError("Output does not fit vertically")
        draw.text((51, y), f"{number:>3}", font=small, fill=MUTED)
        color = AMBER if ('null' in line or 'unsupported' in line or 'no_eligible' in line) else TEXT
        draw.text((99, y - 2), line, font=mono, fill=color)
        y += 27
    draw.text((57, 706), caption, font=small, fill=MUTED)
    draw.text((36, 761), "Real CLI output / synthetic inputs / edited pacing, not a performance measurement", font=small, fill=MUTED)
    return image


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--font", type=Path, required=True)
    parser.add_argument("--output-directory", type=Path, default=Path("docs/media"))
    parser.add_argument("--ffmpeg", default=shutil.which("ffmpeg"))
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[1]
    binary, font = args.binary.resolve(), args.font.resolve()
    out = args.output_directory.resolve()
    if not binary.is_file() or not font.is_file() or not args.ffmpeg:
        parser.error("A built CLI, a readable monospace font, Pillow and ffmpeg are required")
    out.mkdir(parents=True, exist_ok=True)
    (out / "fixtures").mkdir(exist_ok=True)
    (out / "frames").mkdir(exist_ok=True)
    unavailable = out / "fixtures/phase-unavailable.json"
    unavailable_snapshot(repo, unavailable)
    route_args = ["route", "--request", "tests/fixtures/requests/codex-implementation.json", "--catalog", "assets/catalogs/codex-core-v1.json", "--json"]
    route = capture(binary, repo, route_args, route_args, 0)
    decision = json.loads(route["stdout"])
    stages = decision["decision"]["selected"]["plan"]["stages"]
    if [stage["kind"] for stage in stages] != ["plan", "execute", "verify"]:
        raise ValueError("Unexpected public routing fixture; review demo before regenerating")
    decision_path = out / "route-decision.json"
    decision_path.write_text(route["stdout"])
    explain_args = ["explain", "--decision", str(decision_path)]
    explain = capture(binary, repo, explain_args, ["explain", "--decision", "docs/media/route-decision.json"], 0)
    if decision["decision_id"] not in explain["stdout"]:
        raise ValueError("Explain did not consume the captured decision")
    phase_args = ["route-phase", "--context", "fixtures/examples/phase-request.json", "--host-snapshot", str(unavailable), "--catalog", "fixtures/examples/phase-catalog.json", "--now", "100", "--json"]
    phase_display = phase_args.copy()
    phase_display[4] = "docs/media/fixtures/phase-unavailable.json"
    refusal = capture(binary, repo, phase_args, phase_display, 3)
    receipt = json.loads(refusal["stdout"])
    if receipt["selected"] is not None or receipt["abstention"] != "no_eligible_candidate":
        raise ValueError("Unavailable fixture was not refused")
    if not any(row["code"] == "model.host.unsupported" for row in receipt["exclusions"]):
        raise ValueError("Expected model availability exclusion is missing")
    version = capture(binary, repo, ["--version"], ["--version"], 0)
    specifications = [
        ("route", "01 / Select a plan, execution and verification model", [
            "$ agent-router route \\",
            "    --request tests/fixtures/requests/codex-implementation.json \\",
            "    --catalog assets/catalogs/codex-core-v1.json --json"], route,
            "Selected stages only. Alternatives and exclusions are in the full transcript."),
        ("explain", "02 / Explain the exact saved decision", [
            "$ agent-router explain \\",
            "    --decision docs/media/route-decision.json"], explain,
            "Reads the decision captured in step 01. No model is called."),
        ("unavailable", "03 / No available model: an explicit refusal", [
            "$ agent-router route-phase \\",
            "    --context fixtures/examples/phase-request.json \\",
            "    --host-snapshot docs/media/fixtures/phase-unavailable.json \\",
            "    --catalog fixtures/examples/phase-catalog.json --now 100 --json"], refusal,
            "Valid synthetic snapshot. Empty model availability. Exit 3 is expected."),
    ]
    frames, manifest = [], []
    for index, (name, title, command, record, caption) in enumerate(specifications, 1):
        numbers = selected_lines(record["stdout"], name)
        frame = render_screen(font, title, command, record, numbers, caption)
        filename = f"{index:02}-{name}.png"
        frame.save(out / "frames" / filename)
        frames.append(frame)
        record.update({"name": name, "shown_stdout_lines": numbers, "screen_command": command})
        manifest.append({"file": "frames/" + filename, "duration_ms": DURATION_MS})
    frames[0].save(out / "routing-demo-poster.png")
    palette_frames = [frame.quantize(colors=128, method=Image.Quantize.MEDIANCUT, dither=Image.Dither.NONE) for frame in frames]
    gif = out / "routing-demo.gif"
    palette_frames[0].save(gif, save_all=True, append_images=palette_frames[1:], duration=DURATION_MS, loop=0, disposal=2, optimize=True)
    if gif.stat().st_size > 4 * 1024 * 1024:
        raise ValueError("GIF exceeded the 4 MiB publication budget")
    with tempfile.TemporaryDirectory(prefix="agent-router-demo-") as tmp:
        tmp_path = Path(tmp)
        # Repeat only three frames; their 8-second holds are editing, not latency.
        frame_list = tmp_path / "frames.txt"
        lines = []
        for index, frame in enumerate(frames):
            path = tmp_path / f"{index}.png"
            frame.save(path)
            lines.extend([f"file '{path.as_posix()}'", f"duration {DURATION_MS / 1000}"])
        lines.append(f"file '{(tmp_path / '2.png').as_posix()}'")
        frame_list.write_text("\n".join(lines) + "\n")
        subprocess.run([args.ffmpeg, "-hide_banner", "-loglevel", "error", "-y", "-f", "concat", "-safe", "0", "-i", str(frame_list), "-t", "24", "-vf", "fps=12,format=yuv420p", "-c:v", "libx264", "-preset", "slow", "-crf", "18", "-movflags", "+faststart", "-map_metadata", "-1", str(out / "routing-demo.mp4")], check=True, timeout=60, capture_output=True)
    inputs = ["tests/fixtures/requests/codex-implementation.json", "assets/catalogs/codex-core-v1.json", "fixtures/examples/phase-request.json", "fixtures/examples/phase-snapshot.json", "fixtures/examples/phase-catalog.json"]
    transcript = {"schema_version": 1, "source": "actual CLI stdout captured by render-readme-demo.py",
        "binary_sha256": sha256(binary), "binary_version_stdout": version["stdout"],
        "fixture_kind": "synthetic public engineering inputs; not provider availability",
        "path_mapping": "binary displayed as agent-router; output directory displayed as docs/media",
        "timing": {"kind": "editorial holds, not measured CLI latency", "duration_ms": 24000},
        "inputs_sha256": {path: sha256(repo / path) for path in inputs},
        "generated_snapshot_sha256": sha256(unavailable), "frames": manifest,
        "commands": [route, explain, refusal]}
    encoded = json.dumps(transcript, indent=2, ensure_ascii=False) + "\n"
    public_text(encoded)
    (out / "routing-demo-transcript.json").write_text(encoded)
    with Image.open(gif) as verify:
        if verify.n_frames != 3 or verify.size != (WIDTH, HEIGHT):
            raise ValueError("Unexpected GIF frame inventory")
        durations = []
        for i in range(verify.n_frames):
            verify.seek(i)
            durations.append(verify.info.get("duration", 0))
        if sum(durations) != 24000:
            raise ValueError("GIF duration differs from its transcript")
    print(f"Captured 3 real CLI commands; rendered 24s GIF ({gif.stat().st_size} bytes) and MP4.")


if __name__ == "__main__":
    main()
