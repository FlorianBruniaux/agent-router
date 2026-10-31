#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Florian Bruniaux
# SPDX-License-Identifier: Apache-2.0
import importlib.util
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location("spdx", Path(__file__).with_name("check-spdx-headers.py"))
spdx = importlib.util.module_from_spec(spec)
spec.loader.exec_module(spdx)

GOOD = "// SPDX-FileCopyrightText: 2026 A\n// SPDX-License-Identifier: Apache-2.0\n"
TOML = '[[annotations]]\npath = ["docs/bench/**", "xtask/src/lib.rs", "**/*.json"]\n'


class Check(unittest.TestCase):
    def run_check(self, files):
        return spdx.missing(list(files), files.get, spdx.annotation_patterns(TOML))

    def test_header_passes(self):
        self.assertEqual(self.run_check({"a/b.rs": GOOD + "fn x(){}"}), [])

    def test_bare_source_fails(self):
        self.assertEqual(self.run_check({"a/b.rs": "fn x(){}"}), ["a/b.rs"])

    def test_one_tag_only_fails(self):
        self.assertEqual(self.run_check({"b.py": "# SPDX-License-Identifier: Apache-2.0\n"}), ["b.py"])

    def test_other_license_fails(self):
        head = "// SPDX-FileCopyrightText: x\n// SPDX-License-Identifier: MIT\n"
        self.assertEqual(self.run_check({"b.rs": head}), ["b.rs"])

    def test_annotation_covers_bare_file(self):
        files = {"docs/bench/x/y.py": "", "xtask/src/lib.rs": "", "xtask/src/main.rs": ""}
        self.assertEqual(self.run_check(files), ["xtask/src/main.rs"])

    def test_non_source_files_are_ignored(self):
        self.assertEqual(self.run_check({"README.md": "", "a/b.json": ""}), [])

    def test_star_does_not_cross_directories(self):
        pats = spdx.annotation_patterns('[[annotations]]\npath = "src/*.rs"\n')
        self.assertEqual(spdx.missing(["src/a/b.rs"], lambda n: "", pats), ["src/a/b.rs"])


if __name__ == "__main__":
    unittest.main()
