"""
detection.py - Deadlock detection and recovery.

Uses the multiple-instance detection algorithm (Silberschatz & Galvin).
State format matches bankers.py:
  available  : vector of length m
  allocation : n x m matrix
  request    : n x m matrix of outstanding requests

Recovery strategies:
  abort_one : abort the deadlocked process with the LEAST WORK DONE
              (ties broken by lowest pid), re-run detection, and repeat
              until no deadlock remains.
  abort_all : abort every deadlocked process at once.

Work lost is recorded two ways:
  units_lost : resource units held by the aborted processes
  steps_lost : time steps the aborted processes had already run
"""

from dataclasses import dataclass, field
from typing import List


@dataclass
class RecoveryResult:
    strategy: str
    deadlocked: List[int]                              # deadlocked before recovery
    victims: List[int] = field(default_factory=list)   # aborted pids, in order
    rounds: int = 0                                    # detection passes that found a deadlock
    units_lost: int = 0
    steps_lost: int = 0


class DetectionSystem:
    def __init__(self, available, allocation, request, work_done=None):
        self.n = len(allocation)
        self.m = len(available)
        self.available = list(available)
        self.allocation = [list(row) for row in allocation]
        self.request = [list(row) for row in request]
        # work_done[i] = time steps process i has run; the simulator updates this.
        self.work_done = list(work_done) if work_done is not None else [0] * self.n
        self.aborted = [False] * self.n
        self._validate()

    def _validate(self):
        if len(self.allocation) != self.n or len(self.request) != self.n:
            raise ValueError("Allocation and request must have the same number of rows")
        if len(self.work_done) != self.n:
            raise ValueError("work_done must have one entry per process")
        for row in self.allocation + self.request:
            if len(row) != self.m:
                raise ValueError("Each row must have m entries")
        if any(x < 0 for row in self.allocation + self.request for x in row):
            raise ValueError("Negative values are not allowed")

    def detect(self):
        """
        Detection algorithm.
        Returns the list of deadlocked process ids (empty if none).
        """
        work = list(self.available)
        # A process holding nothing cannot be part of a deadlock.
        # Aborted processes are already gone, so treat them as finished.
        finish = [
            self.aborted[i] or all(x == 0 for x in self.allocation[i])
            for i in range(self.n)
        ]

        progress = True
        while progress:
            progress = False
            for i in range(self.n):
                if not finish[i] and all(
                    self.request[i][j] <= work[j] for j in range(self.m)
                ):
                    for j in range(self.m):
                        work[j] += self.allocation[i][j]
                    finish[i] = True
                    progress = True

        return [i for i in range(self.n) if not finish[i]]

    def abort(self, pid):
        """
        Abort a process: release its resources and clear its request.
        Returns (units_lost, steps_lost) for this process.
        """
        units = sum(self.allocation[pid])
        steps = self.work_done[pid]
        for j in range(self.m):
            self.available[j] += self.allocation[pid][j]
            self.allocation[pid][j] = 0
            self.request[pid][j] = 0
        self.aborted[pid] = True
        return units, steps

    def recover_abort_one(self):
        """
        Abort one victim at a time: the deadlocked process with the least
        work done (ties go to the lowest pid). Re-run detection after each
        abort, until no deadlock remains.
        """
        dl = self.detect()
        result = RecoveryResult("abort_one", deadlocked=list(dl))

        while dl:
            # Least work done is the victim rule. Lowest pid breaks ties.
            victim = min(dl, key=lambda i: (self.work_done[i], i))
            units, steps = self.abort(victim)

            result.victims.append(victim)
            result.rounds += 1
            result.units_lost += units
            result.steps_lost += steps

            dl = self.detect()

        return result

    def recover_abort_all(self):
        """Abort every deadlocked process at once."""
        dl = self.detect()
        result = RecoveryResult("abort_all", deadlocked=list(dl))

        if dl:
            result.rounds = 1
        for pid in dl:
            units, steps = self.abort(pid)
            result.victims.append(pid)
            result.units_lost += units
            result.steps_lost += steps

        return result


def _textbook_state(p2_request=(0, 0, 0)):
    """
    Silberschatz detection example: 5 processes, 3 resource types.
    With p2_request=(0,0,0) the system is not deadlocked.
    With p2_request=(0,0,1) processes P1-P4 are deadlocked.
    """
    available = [0, 0, 0]
    allocation = [
        [0, 1, 0],
        [2, 0, 0],
        [3, 0, 3],
        [2, 1, 1],
        [0, 0, 2],
    ]
    request = [
        [0, 0, 0],
        [2, 0, 2],
        list(p2_request),
        [1, 0, 0],
        [0, 0, 2],
    ]
    return available, allocation, request


def _run_tests():
    # Case 1: no deadlock.
    a, al, r = _textbook_state()
    dl = DetectionSystem(a, al, r).detect()
    print("No-deadlock case, deadlocked:", dl)
    assert dl == []

    # Case 2: deadlock among P1-P4.
    work_done = [5, 3, 8, 2, 4]
    a, al, r = _textbook_state(p2_request=(0, 0, 1))
    dl = DetectionSystem(a, al, r, work_done).detect()
    print("Deadlock case, deadlocked:", dl)
    assert dl == [1, 2, 3, 4]

    # Case 3: abort-one recovery.
    # Least work among P1-P4 is P3 (2 steps), which holds 4 units.
    # After aborting P3, the system is no longer deadlocked.
    system = DetectionSystem(a, al, r, work_done)
    res = system.recover_abort_one()
    print("abort_one:", res)
    assert res.victims == [3]
    assert res.rounds == 1
    assert res.units_lost == 4
    assert res.steps_lost == 2
    assert system.detect() == []

    # Case 4: abort-all recovery.
    # Units: P1=2, P2=6, P3=4, P4=2 -> 14. Steps: 3+8+2+4 = 17.
    system = DetectionSystem(a, al, r, work_done)
    res = system.recover_abort_all()
    print("abort_all:", res)
    assert res.victims == [1, 2, 3, 4]
    assert res.units_lost == 14
    assert res.steps_lost == 17
    assert system.detect() == []

    print("All detection checks passed.")


if __name__ == "__main__":
    _run_tests()