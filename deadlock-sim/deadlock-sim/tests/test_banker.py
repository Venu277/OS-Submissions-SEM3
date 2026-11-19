import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "src"))
from banker import BankerState  # noqa: E402


def textbook_state():
    # Silberschatz example: 5 processes, 3 resource types
    return BankerState(
        available=[3, 3, 2],
        maximum=[[7, 5, 3], [3, 2, 2], [9, 0, 2], [2, 2, 2], [4, 3, 3]],
        allocation=[[0, 1, 0], [2, 0, 0], [3, 0, 2], [2, 1, 1], [0, 0, 2]],
    )


class TestBanker(unittest.TestCase):
    def test_safe_sequence_matches_textbook(self):
        self.assertEqual(textbook_state().safe_sequence(), [1, 3, 4, 0, 2])

    def test_request_granted_when_safe(self):
        s = textbook_state()
        self.assertTrue(s.request(1, [1, 0, 2]))
        self.assertEqual(s.available, [2, 3, 0])

    def test_request_denied_when_unsafe_and_state_restored(self):
        s = textbook_state()
        s.request(1, [1, 0, 2])                  # safe
        before_avail = list(s.available)
        before_alloc = [list(r) for r in s.allocation]
        self.assertFalse(s.request(0, [0, 2, 0]))  # unsafe
        self.assertEqual(s.available, before_avail)
        self.assertEqual(s.allocation, before_alloc)

    def test_request_denied_when_not_enough_available(self):
        s = textbook_state()
        s.request(1, [1, 0, 2])
        self.assertFalse(s.request(4, [3, 3, 0]))

    def test_release_returns_resources(self):
        s = textbook_state()
        s.release(2)
        self.assertEqual(s.available, [6, 3, 4])
        self.assertEqual(s.allocation[2], [0, 0, 0])


if __name__ == "__main__":
    unittest.main()
