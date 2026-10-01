import unittest

from pilot_lib.semver import compare


class SemverTests(unittest.TestCase):
    def test_core_numbers_are_numeric(self):
        self.assertLess(compare("1.9.0", "1.10.0"), 0)

    def test_build_metadata_does_not_change_precedence(self):
        self.assertEqual(compare("2.0.0+one", "2.0.0+two"), 0)

    def test_release_outranks_prerelease(self):
        self.assertGreater(compare("1.0.0", "1.0.0-rc.1"), 0)

    def test_prerelease_numeric_and_prefix_order(self):
        self.assertLess(compare("1.0.0-alpha.2", "1.0.0-alpha.10"), 0)
        self.assertLess(compare("1.0.0-alpha", "1.0.0-alpha.1"), 0)

    def test_rejects_leading_zero_numeric_identifier(self):
        with self.assertRaises(ValueError):
            compare("1.0.0-01", "1.0.0-1")
