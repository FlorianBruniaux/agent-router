import unittest
import os, tempfile
from pilot_lib.tables import csv_sum
class T(unittest.TestCase):
    def write(self, text):
        handle = tempfile.NamedTemporaryFile("w", suffix=".csv", delete=False)
        handle.write(text); handle.close()
        self.addCleanup(os.unlink, handle.name)
        return handle.name
    def test_sum(self):
        self.assertEqual(csv_sum(self.write("a,b\n1,2\n3,4.5\n"), "b"), 6.5)
    def test_missing_cells(self):
        self.assertEqual(csv_sum(self.write("a,b\n1,\n2, \n3,1\n"), "b"), 1.0)
    def test_unknown_column(self):
        with self.assertRaises(KeyError):
            csv_sum(self.write("a\n1\n"), "z")
