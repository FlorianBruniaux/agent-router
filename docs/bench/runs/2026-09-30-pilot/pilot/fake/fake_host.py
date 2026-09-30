#!/usr/bin/env python3
"""Synthetic host for testing the pilot driver. No model, no network.

Writes the reference solution for odd-numbered tasks in write mode only, so the
driver sees accepted and rejected runs. Emits Claude or Codex shaped JSONL with
fixed synthetic usage."""
import json
import re
import shutil
import sys
import uuid
from pathlib import Path

provider, argv = sys.argv[1], sys.argv[2:]
prompt = sys.stdin.read()
value = lambda flag: argv[argv.index(flag) + 1] if flag in argv else None
model = value("--model") or "fake-default"
if provider == "claude":
    write = value("--permission-mode") == "acceptEdits"
    cwd = Path.cwd()
else:
    write = value("--sandbox") == "workspace-write"
    cwd = Path(value("--cd"))
match = re.search(r"Task id: (t\d+)", prompt)
task = match.group(1) if match else None
if task and write and int(task[1:]) % 2 == 1:
    solutions = Path(__import__("os").environ["PILOT_FAKE_SOLUTIONS"])
    meta = json.loads((solutions.parent / "tasks" / task / "meta.json").read_text())
    shutil.copy(solutions / f"{task}.py", cwd / meta["target"])
text = "PASS\nfake summary" if "verify" in prompt.lower() else "fake plan"
size = len(prompt)
if provider == "claude":
    session = str(uuid.uuid4())
    for event in (
        {"type": "system", "subtype": "init", "session_id": session, "model": model},
        {"type": "assistant", "parent_tool_use_id": None, "message": {"model": f"{model}-20990101", "role": "assistant", "type": "message", "content": [{"type": "text", "text": text}]}},
        {"type": "result", "subtype": "success", "is_error": False, "result": text, "total_cost_usd": 0.01,
         "usage": {"input_tokens": size, "cache_creation_input_tokens": 1000 if "--safe-mode" in argv else 48000, "cache_read_input_tokens": 500, "output_tokens": 200,
                   "cache_creation": {"ephemeral_5m_input_tokens": 0, "ephemeral_1h_input_tokens": 1000 if "--safe-mode" in argv else 48000}}},
    ):
        print(json.dumps(event))
else:
    for event in (
        {"type": "thread.started", "thread_id": str(uuid.uuid4())},
        {"type": "turn.started"},
        {"type": "item.completed", "item": {"id": "i0", "type": "agent_message", "text": text}},
        {"type": "turn.completed", "usage": {"input_tokens": size + (1500 if "--ignore-user-config" in argv else 30000), "cached_input_tokens": 500, "cache_write_input_tokens": 0, "output_tokens": 200, "reasoning_output_tokens": 50}},
    ):
        print(json.dumps(event))
