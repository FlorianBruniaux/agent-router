import unittest
from pilot_lib.text import slugify
class T(unittest.TestCase):
    def test_basic(self):
        self.assertEqual(slugify("Hello World"), "hello-world")
    def test_accents(self):
        self.assertEqual(slugify("Crème brûlée à Paris"), "creme-brulee-a-paris")
    def test_runs_and_edges(self):
        self.assertEqual(slugify("  --Rust & Go!! 2026--  "), "rust-go-2026")
    def test_empty(self):
        self.assertEqual(slugify("!!!"), "")
