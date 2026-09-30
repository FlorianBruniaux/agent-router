import unittest
from pilot_lib.intervals import merge_intervals
class T(unittest.TestCase):
    def test_merge(self):
        self.assertEqual(merge_intervals([(5, 7), (1, 3), (2, 4)]), [(1, 4), (5, 7)])
    def test_touching(self):
        self.assertEqual(merge_intervals([(1, 2), (2, 3)]), [(1, 3)])
    def test_nested_and_empty(self):
        self.assertEqual(merge_intervals([(1, 10), (2, 3)]), [(1, 10)])
        self.assertEqual(merge_intervals([]), [])
    def test_input_unchanged(self):
        data = [(3, 4), (1, 2)]
        merge_intervals(data)
        self.assertEqual(data, [(3, 4), (1, 2)])
