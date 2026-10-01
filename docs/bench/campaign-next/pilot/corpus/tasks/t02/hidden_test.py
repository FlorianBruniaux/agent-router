import unittest

from pilot_lib.intervals import merge_half_open


class IntervalTests(unittest.TestCase):
    def test_sorts_merges_overlap_and_ignores_empty(self):
        self.assertEqual(merge_half_open([(5, 8), (1, 3), (2, 6), (9, 9)]), [(1, 8)])

    def test_touching_is_configurable(self):
        intervals = [(1, 3), (3, 5)]
        self.assertEqual(merge_half_open(intervals), intervals)
        self.assertEqual(merge_half_open(intervals, merge_touching=True), [(1, 5)])

    def test_rejects_reversed_intervals(self):
        with self.assertRaises(ValueError):
            merge_half_open([(4, 2)])

    def test_does_not_mutate_input(self):
        intervals = [(5, 8), (1, 3)]
        merge_half_open(intervals)
        self.assertEqual(intervals, [(5, 8), (1, 3)])
