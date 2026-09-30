import unittest
from pilot_lib.durations import parse_duration
class T(unittest.TestCase):
    def test_values(self):
        self.assertEqual(parse_duration("1h30m"), 5400)
        self.assertEqual(parse_duration("45s"), 45)
        self.assertEqual(parse_duration("2m5s"), 125)
        self.assertEqual(parse_duration(" 1h1s "), 3601)
    def test_errors(self):
        for bad in ["", "10", "5d", "h", "1h30"]:
            with self.assertRaises(ValueError, msg=bad):
                parse_duration(bad)
