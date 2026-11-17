import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "src"))
from detector import find_deadlocked  # noqa: E402
from recovery import choose_victim    # noqa: E402


class TestDetector(unittest.TestCase):
    def test_circular_wait_is_detected(self):
        dead = find_deadlocked(
            available=[0, 0],
            allocation=[[1, 0], [0, 1]],
            requests=[[0, 1], [1, 0]],
        )
        self.assertEqual(dead, [0, 1])

    def test_no_deadlock_when_resources_can_be_freed(self):
        dead = find_deadlocked(
            available=[1, 0],
            allocation=[[1, 0], [0, 1]],
            requests=[[0, 1], [0, 0]],
        )
        self.assertEqual(dead, [])

    def test_victim_is_process_holding_fewest_units(self):
        self.assertEqual(choose_victim([0, 2], [[1, 1], [0, 0], [0, 1]]), 2)


if __name__ == "__main__":
    unittest.main()
