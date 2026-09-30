import unittest
from pilot_lib.text import word_wrap
class T(unittest.TestCase):
    def test_wrap(self):
        self.assertEqual(word_wrap("the quick brown fox jumps", 10), ["the quick", "brown fox", "jumps"])
    def test_long_word(self):
        self.assertEqual(word_wrap("a extraordinarily b", 5), ["a", "extraordinarily", "b"])
    def test_blank_and_invalid(self):
        self.assertEqual(word_wrap("   ", 4), [])
        with self.assertRaises(ValueError):
            word_wrap("x", 0)
