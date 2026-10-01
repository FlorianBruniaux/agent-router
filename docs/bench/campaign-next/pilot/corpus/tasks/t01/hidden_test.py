import unittest

from pilot_lib.pagination import collect_pages


class PaginationTests(unittest.TestCase):
    def test_preserves_order_and_stops_at_terminal_cursor(self):
        pages = {None: ([1, 2], "a"), "a": ([3], None)}
        self.assertEqual(collect_pages(lambda cursor: pages[cursor]), [1, 2, 3])

    def test_rejects_cursor_cycle(self):
        with self.assertRaises(ValueError):
            collect_pages(lambda cursor: ([], "again"), max_pages=5)

    def test_rejects_limit_exhaustion(self):
        with self.assertRaises(ValueError):
            collect_pages(lambda cursor: ([], f"next-{cursor}"), max_pages=2)

    def test_rejects_nonpositive_limit(self):
        with self.assertRaises(ValueError):
            collect_pages(lambda cursor: ([], None), max_pages=0)
