import unittest
from pilot_lib.roman import roman_to_int
class T(unittest.TestCase):
    def test_values(self):
        for numeral, value in [("III", 3), ("IV", 4), ("IX", 9), ("LVIII", 58), ("MCMXCIV", 1994), ("MMXXVI", 2026)]:
            self.assertEqual(roman_to_int(numeral), value, numeral)
    def test_errors(self):
        for bad in ["", "ABC", "xiv"]:
            with self.assertRaises(ValueError, msg=bad):
                roman_to_int(bad)
