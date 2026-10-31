#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Florian Bruniaux
# SPDX-License-Identifier: Apache-2.0
"""Parse generated native syntax. This is not host discovery or hook trust."""
import json
from pathlib import Path
import tomllib

root = Path(__file__).resolve().parent.parent
for path in [root / ".codex/config.toml", *sorted((root / ".codex/agents").glob("*.toml"))]:
    with path.open("rb") as source:
        tomllib.load(source)
for path in [root / ".codex/hooks.json", root / ".claude/settings.json"]:
    with path.open() as source:
        json.load(source)
print("Native TOML/JSON syntax: PASS; runtime activation: UNKNOWN")
