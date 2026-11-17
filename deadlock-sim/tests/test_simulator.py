import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "src"))
from simulator import simulate          # noqa: E402
from workload import generate_workload  # noqa: E402


class TestSimulator(unittest.TestCase):
    def setUp(self):
        self.wl = generate_workload(5, [10, 10, 10], 6, seed=7)

    def test_avoidance_never_deadlocks(self):
        for seed in range(10):
            wl = generate_workload(6, [8, 8, 8], 6, seed=seed)
            r = simulate(wl, "avoidance")
            self.assertEqual(r["deadlocks"], 0)
            self.assertEqual(r["aborts"], 0)

    def test_avoidance_completes_low_contention(self):
        r = simulate(self.wl, "avoidance")
        self.assertEqual(r["unfinished"], 0)

    def test_detection_accounts_for_every_process(self):
        r = simulate(self.wl, "detection")
        self.assertEqual(r["completed"] + r["unfinished"], 5)

    def test_same_seed_gives_same_result(self):
        a = simulate(generate_workload(6, [8, 8, 8], 6, seed=3), "avoidance")
        b = simulate(generate_workload(6, [8, 8, 8], 6, seed=3), "avoidance")
        self.assertEqual(a, b)

    def test_invalid_strategy_rejected(self):
        with self.assertRaises(ValueError):
            simulate(self.wl, "bogus")


if __name__ == "__main__":
    unittest.main()
