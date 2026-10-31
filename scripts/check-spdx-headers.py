#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Florian Bruniaux
# SPDX-License-Identifier: Apache-2.0
"""Check that every tracked .rs, .py, .c and .sh file carries the SPDX header.

A file passes when its first lines contain both SPDX tags, or when a path
pattern of an annotation in REUSE.toml covers it. The check reads only Git's
file list and REUSE.toml; it does not run the `reuse` tool, whose LICENSES/
directory collides with the existing licenses/ directory on case-insensitive
file systems.
"""
import re
import subprocess
import sys
import tomllib
from pathlib import Path

SUFFIXES = (".rs", ".py", ".c", ".sh")
COPYRIGHT = "SPDX-FileCopyrightText:"
LICENSE = "SPDX-License-Identifier: Apache-2.0"
HEAD_BYTES = 600


def glob_regex(pattern):
    out, i = "", 0
    while i < len(pattern):
        if pattern.startswith("**/", i):
            out += "(?:.*/)?"
            i += 3
        elif pattern.startswith("**", i):
            out += ".*"
            i += 2
        elif pattern[i] == "*":
            out += "[^/]*"
            i += 1
        else:
            out += re.escape(pattern[i])
            i += 1
    return re.compile(out + r"\Z")


def annotation_patterns(toml_text):
    patterns = []
    for annotation in tomllib.loads(toml_text).get("annotations", []):
        path = annotation.get("path", [])
        patterns += [path] if isinstance(path, str) else path
    return [glob_regex(p) for p in patterns]


def missing(files, read_head, patterns):
    bad = []
    for name in files:
        if not name.endswith(SUFFIXES):
            continue
        head = read_head(name)
        if COPYRIGHT in head and LICENSE in head:
            continue
        if any(p.match(name) for p in patterns):
            continue
        bad.append(name)
    return bad


def main():
    root = Path(subprocess.check_output(["git", "rev-parse", "--show-toplevel"], text=True).strip())
    files = subprocess.check_output(["git", "ls-files", "-z"], cwd=root).decode().split("\0")
    patterns = annotation_patterns((root / "REUSE.toml").read_text())
    bad = missing(
        [f for f in files if f],
        lambda n: (root / n).read_bytes()[:HEAD_BYTES].decode("utf-8", "replace"),
        patterns,
    )
    for name in bad:
        print(f"missing SPDX header and no REUSE.toml annotation: {name}")
    if bad:
        return 1
    print("SPDX headers: PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
