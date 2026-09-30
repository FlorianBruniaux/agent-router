#!/usr/bin/env python3
"""Validate ADR frontmatter, headings, cross references and the ADR index.

Stdlib only. Does not read git: commit dates cannot be verified on a shallow
clone, so `created` and `last_modified` are checked for format and order only.
"""

from __future__ import annotations

import argparse
import datetime
import json
from pathlib import Path
import re
import sys

STATUSES = ("proposed", "accepted", "implemented", "amended", "superseded", "rejected")
FIELDS = (
    "adr",
    "title",
    "status",
    "created",
    "last_modified",
    "scope",
    "amended_by",
    "supersedes",
    "superseded_by",
    "related",
)
LIST_FIELDS = ("amended_by", "supersedes", "superseded_by", "related")
FILE_RE = re.compile(r"^(\d{4})-[a-z0-9-]+\.md$")
ROW_RE = re.compile(r"^\|\s*\[(\d{4})\]\(([^)]*)\)\s*\|(.*)\|\s*$")
MAX_BYTES = 1_000_000


def parse_value(raw: str):
    raw = raw.strip()
    if raw.startswith('"'):
        return json.loads(raw)
    if raw.startswith("["):
        value = json.loads(raw)
        if not isinstance(value, list) or not all(isinstance(v, str) for v in value):
            raise ValueError("list must contain only strings")
        return value
    return raw


def parse_frontmatter(text: str):
    """Return (fields, body_after_frontmatter) or raise ValueError."""
    lines = text.split("\n")
    if not lines or lines[0] != "---":
        raise ValueError("missing frontmatter opening '---' on line 1")
    try:
        end = lines.index("---", 1)
    except ValueError:
        raise ValueError("missing frontmatter closing '---'") from None
    fields = {}
    for number, line in enumerate(lines[1:end], start=2):
        if not line.strip():
            continue
        key, sep, raw = line.partition(":")
        if not sep or not re.fullmatch(r"[a-z_]+", key):
            raise ValueError(f"line {number}: expected 'key: value'")
        if key in fields:
            raise ValueError(f"line {number}: duplicate field {key}")
        try:
            fields[key] = parse_value(raw)
        except ValueError as error:
            raise ValueError(f"line {number}: bad value for {key}: {error}") from None
    return fields, "\n".join(lines[end + 1 :])


def parse_date(value: str):
    if not re.fullmatch(r"\d{4}-\d{2}-\d{2}", value):
        return None
    try:
        return datetime.date.fromisoformat(value)
    except ValueError:
        return None


def load_adrs(adr_dir: Path, errors: list[str]):
    adrs = {}
    for path in sorted(adr_dir.glob("*.md")):
        match = FILE_RE.match(path.name)
        if not match:
            continue
        name = path.name
        if path.stat().st_size > MAX_BYTES:
            errors.append(f"{name}: file exceeds {MAX_BYTES} bytes")
            continue
        try:
            fields, body = parse_frontmatter(path.read_text(encoding="utf-8"))
        except (ValueError, UnicodeDecodeError) as error:
            errors.append(f"{name}: {error}")
            continue
        missing = [key for key in FIELDS if key not in fields]
        unknown = [key for key in fields if key not in FIELDS]
        if missing:
            errors.append(f"{name}: missing frontmatter fields: {', '.join(missing)}")
        if unknown:
            errors.append(f"{name}: unknown frontmatter fields: {', '.join(unknown)}")
        if missing:
            continue
        for key in LIST_FIELDS:
            if not isinstance(fields[key], list):
                errors.append(f"{name}: {key} must be a list")
        for key in ("adr", "title", "status", "created", "last_modified", "scope"):
            if not isinstance(fields[key], str) or not fields[key].strip():
                errors.append(f"{name}: {key} must be a non-empty string")
        if any(not isinstance(fields[k], str) for k in FIELDS if k not in LIST_FIELDS) or any(
            not isinstance(fields[k], list) for k in LIST_FIELDS
        ):
            continue
        fields["_file"] = name
        fields["_body"] = body
        adrs[match.group(1)] = fields
    return adrs


def check_each(adrs, errors):
    for number, adr in adrs.items():
        name = adr["_file"]
        if adr["adr"] != number:
            errors.append(f"{name}: adr {adr['adr']!r} does not match file number {number}")
        if adr["status"] not in STATUSES:
            errors.append(f"{name}: status {adr['status']!r} not in {', '.join(STATUSES)}")
        created = parse_date(adr["created"])
        modified = parse_date(adr["last_modified"])
        if created is None:
            errors.append(f"{name}: created {adr['created']!r} is not an ISO date")
        if modified is None:
            errors.append(f"{name}: last_modified {adr['last_modified']!r} is not an ISO date")
        if created and modified and created > modified:
            errors.append(f"{name}: created {adr['created']} is after last_modified {adr['last_modified']}")
        heading = next((line for line in adr["_body"].split("\n") if line.strip()), "")
        expected = f"# ADR {number}: {adr['title']}"
        if heading != expected:
            errors.append(f"{name}: first heading {heading!r} does not match {expected!r}")
        if re.search(r"^Status:", adr["_body"], re.MULTILINE):
            errors.append(f"{name}: body still contains a 'Status:' line; use the frontmatter")
        for key in LIST_FIELDS:
            for target in adr[key]:
                if target == number:
                    errors.append(f"{name}: {key} references itself")
                elif target not in adrs:
                    errors.append(f"{name}: {key} references unknown ADR {target}")


def check_reciprocity(adrs, errors):
    for number, adr in adrs.items():
        name = adr["_file"]
        for target in adr["superseded_by"]:
            if target in adrs and number not in adrs[target]["supersedes"]:
                errors.append(f"{name}: superseded_by {target} but ADR {target} does not list {number} in supersedes")
        for target in adr["supersedes"]:
            if target in adrs and number not in adrs[target]["superseded_by"]:
                errors.append(f"{name}: supersedes {target} but ADR {target} does not list {number} in superseded_by")
        for target in adr["amended_by"]:
            if target in adrs and number not in adrs[target]["related"]:
                errors.append(f"{name}: amended_by {target} but ADR {target} does not list {number} in related")
        if adr["superseded_by"] and adr["status"] != "superseded":
            errors.append(f"{name}: has superseded_by but status is {adr['status']!r}")
        if adr["status"] == "superseded" and not adr["superseded_by"]:
            errors.append(f"{name}: status is superseded but superseded_by is empty")
        if adr["amended_by"] and adr["status"] not in ("amended", "superseded"):
            errors.append(f"{name}: has amended_by but status is {adr['status']!r}")


def link_numbers(cell: str):
    return re.findall(r"\[(\d{4})\]\(", cell)


def check_index(adr_dir: Path, adrs, errors):
    readme = adr_dir / "README.md"
    if not readme.is_file():
        errors.append("README.md: missing ADR index")
        return
    rows = {}
    for line in readme.read_text(encoding="utf-8").split("\n"):
        match = ROW_RE.match(line)
        if not match:
            continue
        number, target, rest = match.groups()
        cells = [cell.strip() for cell in rest.split("|")]
        if number in rows:
            errors.append(f"README.md: ADR {number} listed twice")
        rows[number] = (target, cells)
    for number, adr in adrs.items():
        if number not in rows:
            errors.append(f"README.md: {adr['_file']} is not in the index")
            continue
        target, cells = rows[number]
        if len(cells) != 5:
            errors.append(f"README.md: row {number} must have 6 columns")
            continue
        title, status, created, modified, amended = cells
        for label, got, want in (
            ("link", target, adr["_file"]),
            ("title", title, adr["title"]),
            ("status", status, adr["status"]),
            ("created", created, adr["created"]),
            ("last_modified", modified, adr["last_modified"]),
        ):
            if got != want:
                errors.append(f"README.md: row {number} {label} {got!r} differs from {want!r}")
        if sorted(link_numbers(amended)) != sorted(adr["amended_by"]):
            errors.append(f"README.md: row {number} amended by {amended!r} differs from {adr['amended_by']}")
    for number in rows:
        if number not in adrs:
            errors.append(f"README.md: row {number} has no matching ADR file")


def check(adr_dir: Path) -> list[str]:
    errors: list[str] = []
    adrs = load_adrs(adr_dir, errors)
    if not adrs and not errors:
        errors.append(f"{adr_dir}: no ADR files found")
    check_each(adrs, errors)
    check_reciprocity(adrs, errors)
    check_index(adr_dir, adrs, errors)
    return errors


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    default = Path(__file__).resolve().parent.parent / "docs" / "adr"
    parser.add_argument("--dir", type=Path, default=default, help="ADR directory (default: docs/adr)")
    args = parser.parse_args(argv)
    errors = check(args.dir)
    for error in errors:
        print(f"error: {error}", file=sys.stderr)
    if errors:
        print(f"{len(errors)} ADR problem(s)", file=sys.stderr)
        return 1
    print("ADR check passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
