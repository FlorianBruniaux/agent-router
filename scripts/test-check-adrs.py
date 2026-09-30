#!/usr/bin/env python3
"""Adversarial checks for the ADR validator. Fixtures only, no git, no network."""

from __future__ import annotations

import contextlib
import importlib.util
import io
from pathlib import Path
import tempfile
import unittest

sys_path = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location("check_adrs", sys_path / "check-adrs.py")
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)

ADR_ONE = """---
adr: "0001"
title: First decision
status: amended
created: 2026-09-01
last_modified: 2026-09-02
scope: "First scope."
amended_by: ["0002"]
supersedes: []
superseded_by: []
related: []
---

# ADR 0001: First decision

## Context

Body one.
"""

ADR_TWO = """---
adr: "0002"
title: Second decision
status: accepted
created: 2026-09-03
last_modified: 2026-09-03
scope: "Second scope."
amended_by: []
supersedes: []
superseded_by: []
related: ["0001"]
---

# ADR 0002: Second decision

## Context

Body two.
"""

README = """# Architecture decision records

| ADR | Title | Status | Created | Modified | Amended by |
| --- | --- | --- | --- | --- | --- |
| [0001](0001-first.md) | First decision | amended | 2026-09-01 | 2026-09-02 | [0002](0002-second.md) |
| [0002](0002-second.md) | Second decision | accepted | 2026-09-03 | 2026-09-03 | - |
"""


class CheckAdrTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="agent-router-adr-test-")
        self.dir = Path(self.temp.name)
        self.write("0001-first.md", ADR_ONE)
        self.write("0002-second.md", ADR_TWO)
        self.write("README.md", README)
        self.write("TEMPLATE.md", "not an ADR at all\n")

    def tearDown(self):
        self.temp.cleanup()

    def write(self, name, text):
        (self.dir / name).write_text(text, encoding="utf-8")

    def edit(self, name, old, new):
        path = self.dir / name
        text = path.read_text(encoding="utf-8")
        self.assertIn(old, text, "fixture edit must hit")
        path.write_text(text.replace(old, new, 1), encoding="utf-8")

    def errors(self):
        return CHECK.check(self.dir)

    def assertError(self, fragment):
        errors = self.errors()
        self.assertTrue(any(fragment in error for error in errors), f"{fragment!r} not in {errors}")

    def test_valid_fixture_passes_and_template_is_ignored(self):
        self.assertEqual(self.errors(), [])

    def test_missing_frontmatter(self):
        self.write("0002-second.md", "# ADR 0002: Second decision\n")
        self.assertError("missing frontmatter opening")

    def test_unterminated_frontmatter(self):
        self.write("0002-second.md", "---\nadr: \"0002\"\n")
        self.assertError("missing frontmatter closing")

    def test_missing_field(self):
        self.edit("0002-second.md", 'scope: "Second scope."\n', "")
        self.assertError("missing frontmatter fields: scope")

    def test_unknown_field(self):
        self.edit("0002-second.md", "related:", 'owner: "x"\nrelated:')
        self.assertError("unknown frontmatter fields: owner")

    def test_number_must_match_file_name(self):
        self.edit("0002-second.md", 'adr: "0002"', 'adr: "0003"')
        self.assertError("does not match file number 0002")

    def test_status_outside_enum(self):
        self.edit("0002-second.md", "status: accepted", "status: done")
        self.assertError("status 'done' not in")

    def test_date_not_iso(self):
        self.edit("0002-second.md", "created: 2026-09-03", "created: 03/09/2026")
        self.assertError("created '03/09/2026' is not an ISO date")

    def test_impossible_date(self):
        self.edit("0002-second.md", "last_modified: 2026-09-03", "last_modified: 2026-02-30")
        self.assertError("is not an ISO date")

    def test_created_after_modified(self):
        self.edit("0002-second.md", "created: 2026-09-03", "created: 2026-09-05")
        self.assertError("is after last_modified")

    def test_heading_must_match_title(self):
        self.edit("0002-second.md", "# ADR 0002: Second decision", "# ADR 0002: Other words")
        self.assertError("first heading")

    def test_heading_must_match_number(self):
        self.edit("0002-second.md", "# ADR 0002: Second decision", "# 0002: Second decision")
        self.assertError("first heading")

    def test_leftover_status_line(self):
        self.edit("0002-second.md", "## Context", "Status: accepted\n\n## Context")
        self.assertError("still contains a 'Status:' line")

    def test_reference_to_unknown_adr(self):
        self.edit("0002-second.md", 'related: ["0001"]', 'related: ["0001", "0099"]')
        self.assertError("references unknown ADR 0099")

    def test_reference_to_itself(self):
        self.edit("0002-second.md", 'related: ["0001"]', 'related: ["0001", "0002"]')
        self.assertError("references itself")

    def test_amended_by_needs_related_back(self):
        self.edit("0002-second.md", 'related: ["0001"]', "related: []")
        self.assertError("amended_by 0002 but ADR 0002 does not list 0001 in related")

    def test_supersedes_needs_superseded_by_back(self):
        self.edit("0002-second.md", "supersedes: []", 'supersedes: ["0001"]')
        self.assertError("supersedes 0001 but ADR 0001 does not list 0002 in superseded_by")

    def test_superseded_by_needs_supersedes_back(self):
        self.edit("0001-first.md", "superseded_by: []", 'superseded_by: ["0002"]')
        self.assertError("superseded_by 0002 but ADR 0002 does not list 0001 in supersedes")

    def test_superseded_by_requires_superseded_status(self):
        self.edit("0001-first.md", "superseded_by: []", 'superseded_by: ["0002"]')
        self.edit("0002-second.md", "supersedes: []", 'supersedes: ["0001"]')
        self.assertError("has superseded_by but status is 'amended'")

    def test_amended_by_requires_amended_status(self):
        self.edit("0001-first.md", "status: amended", "status: accepted")
        self.assertError("has amended_by but status is 'accepted'")

    def test_adr_missing_from_index(self):
        self.edit("README.md", "| [0002](0002-second.md) | Second decision | accepted | 2026-09-03 | 2026-09-03 | - |\n", "")
        self.assertError("0002-second.md is not in the index")

    def test_index_row_without_file(self):
        self.edit("README.md", "| [0002](0002-second.md) | Second", "| [0009](0009-x.md) | X | accepted | 2026-09-03 | 2026-09-03 | - |\n| [0002](0002-second.md) | Second")
        self.assertError("row 0009 has no matching ADR file")

    def test_index_title_differs(self):
        self.edit("README.md", "Second decision", "Second choice")
        self.assertError("row 0002 title")

    def test_index_status_differs(self):
        self.edit("README.md", "| accepted |", "| implemented |")
        self.assertError("row 0002 status")

    def test_index_created_differs(self):
        self.edit("README.md", "| 2026-09-01 |", "| 2026-08-01 |")
        self.assertError("row 0001 created")

    def test_index_modified_differs(self):
        self.edit("README.md", "| 2026-09-02 |", "| 2026-09-04 |")
        self.assertError("row 0001 last_modified")

    def test_index_amended_by_differs(self):
        self.edit("README.md", "| [0002](0002-second.md) |\n", "| - |\n")
        self.assertError("row 0001 amended by")

    def test_missing_index(self):
        (self.dir / "README.md").unlink()
        self.assertError("missing ADR index")

    def test_main_exit_codes(self):
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(CHECK.main(["--dir", str(self.dir)]), 0)
        self.edit("0002-second.md", "status: accepted", "status: done")
        with contextlib.redirect_stderr(io.StringIO()) as err:
            self.assertEqual(CHECK.main(["--dir", str(self.dir)]), 1)
        self.assertIn("error:", err.getvalue())


class RepositoryTests(unittest.TestCase):
    def test_repository_adrs_pass(self):
        self.assertEqual(CHECK.check(sys_path.parent / "docs" / "adr"), [])


if __name__ == "__main__":
    unittest.main()
