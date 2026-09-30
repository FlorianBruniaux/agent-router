import unittest

import pilot_lib


class SmokeTest(unittest.TestCase):
    def test_package_imports(self):
        self.assertTrue(pilot_lib.__doc__)


if __name__ == "__main__":
    unittest.main()
