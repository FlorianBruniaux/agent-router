import unittest
from pilot_lib.semver import semver_compare
class T(unittest.TestCase):
    def test_core(self):
        self.assertEqual(semver_compare("1.10.0", "1.9.0"), 1)
        self.assertEqual(semver_compare("1.0.0", "1.0.0"), 0)
    def test_prerelease_chain(self):
        chain = ["1.0.0-alpha", "1.0.0-alpha.1", "1.0.0-alpha.beta", "1.0.0-beta", "1.0.0-beta.2", "1.0.0-beta.11", "1.0.0-rc.1", "1.0.0"]
        for low, high in zip(chain, chain[1:]):
            self.assertEqual(semver_compare(low, high), -1, (low, high))
            self.assertEqual(semver_compare(high, low), 1, (high, low))
    def test_build_ignored(self):
        self.assertEqual(semver_compare("1.0.0+build.5", "1.0.0+other"), 0)
