import unittest
from pilot_lib.retry import retry
class T(unittest.TestCase):
    def flaky(self, failures):
        state = {"calls": 0}
        def fn():
            state["calls"] += 1
            if state["calls"] <= failures:
                raise RuntimeError(state["calls"])
            return "ok"
        return fn, state
    def test_success_after_failures(self):
        fn, state = self.flaky(2)
        sleeps = []
        self.assertEqual(retry(fn, 3, backoff=1.0, sleep=sleeps.append), "ok")
        self.assertEqual(state["calls"], 3)
        self.assertEqual(sleeps, [1.0, 2.0])
    def test_exhausted(self):
        fn, state = self.flaky(5)
        sleeps = []
        with self.assertRaises(RuntimeError):
            retry(fn, 3, backoff=0.5, sleep=sleeps.append)
        self.assertEqual(state["calls"], 3)
        self.assertEqual(sleeps, [0.5, 1.0])
    def test_invalid(self):
        with self.assertRaises(ValueError):
            retry(lambda: 1, 0)
