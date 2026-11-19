"""
bankers.py - Banker's Algorithm for deadlock avoidance.

Matrices are lists of lists:
  available  : vector of length m
  max_claim  : n x m matrix
  allocation : n x m matrix
  need       : n x m matrix, need = max_claim - allocation
"""

from copy import deepcopy


class BankersAlgorithm:
    def __init__(self, available, max_claim, allocation):
        self.n = len(allocation)
        self.m = len(available)
        self.available = list(available)
        self.max_claim = [list(row) for row in max_claim]
        self.allocation = [list(row) for row in allocation]
        self.need = [
            [self.max_claim[i][j] - self.allocation[i][j] for j in range(self.m)]
            for i in range(self.n)
        ]
        self._validate()

    def _validate(self):
        for i in range(self.n):
            for j in range(self.m):
                if self.allocation[i][j] > self.max_claim[i][j]:
                    raise ValueError(
                        f"Process {i} allocation exceeds max claim for resource {j}"
                    )
                if self.allocation[i][j] < 0 or self.max_claim[i][j] < 0:
                    raise ValueError("Negative values are not allowed")

    def is_safe(self):
        """
        Safety algorithm.
        Returns (True, safe_sequence) if the state is safe,
        or (False, None) if it is unsafe.
        """
        work = list(self.available)
        finish = [False] * self.n
        sequence = []

        while len(sequence) < self.n:
            found = False
            for i in range(self.n):
                if not finish[i] and all(
                    self.need[i][j] <= work[j] for j in range(self.m)
                ):
                    for j in range(self.m):
                        work[j] += self.allocation[i][j]
                    finish[i] = True
                    sequence.append(i)
                    found = True
                    break
            if not found:
                return False, None

        return True, sequence

    def request_resources(self, pid, request):
        """
        Resource-request algorithm.
        Returns (True, safe_sequence) if the request is granted,
        or (False, reason) if it is denied. A denied request leaves
        the state unchanged.
        """
        if not (0 <= pid < self.n):
            raise IndexError(f"Invalid process id {pid}")
        if len(request) != self.m:
            raise ValueError("Request vector length must equal number of resource types")

        # 1. Request must not exceed the declared need.
        for j in range(self.m):
            if request[j] > self.need[pid][j]:
                return False, f"Process {pid} exceeded its maximum claim"

        # 2. Request must be satisfiable with currently available resources.
        for j in range(self.m):
            if request[j] > self.available[j]:
                return False, f"Resources unavailable; process {pid} must wait"

        # 3. Tentatively allocate, then check safety.
        saved = deepcopy((self.available, self.allocation, self.need))
        for j in range(self.m):
            self.available[j] -= request[j]
            self.allocation[pid][j] += request[j]
            self.need[pid][j] -= request[j]

        safe, sequence = self.is_safe()
        if safe:
            return True, sequence

        # 4. Unsafe: roll back.
        self.available, self.allocation, self.need = saved
        return False, "Granting request would lead to an unsafe state"

    def release_resources(self, pid, release):
        """Return resources held by a process, e.g. after it finishes."""
        for j in range(self.m):
            if release[j] > self.allocation[pid][j]:
                raise ValueError(f"Process {pid} releasing more than it holds")
            self.allocation[pid][j] -= release[j]
            self.need[pid][j] += release[j]
            self.available[j] += release[j]

    def snapshot(self):
        """Return a copy of the current state, useful for logging or tests."""
        return {
            "available": list(self.available),
            "allocation": deepcopy(self.allocation),
            "need": deepcopy(self.need),
        }


def _is_valid_safe_sequence(banker, sequence):
    """Check that every process in the sequence can finish in turn."""
    if sorted(sequence) != list(range(banker.n)):
        return False
    work = list(banker.available)
    for i in sequence:
        if any(banker.need[i][j] > work[j] for j in range(banker.m)):
            return False
        for j in range(banker.m):
            work[j] += banker.allocation[i][j]
    return True


def _run_textbook_example():
    # Silberschatz textbook example: 5 processes, 3 resource types.
    available = [3, 3, 2]
    max_claim = [
        [7, 5, 3],
        [3, 2, 2],
        [9, 0, 2],
        [2, 2, 2],
        [4, 3, 3],
    ]
    allocation = [
        [0, 1, 0],
        [2, 0, 0],
        [3, 0, 2],
        [2, 1, 1],
        [0, 0, 2],
    ]

    banker = BankersAlgorithm(available, max_claim, allocation)

    safe, sequence = banker.is_safe()
    print("Initial state safe:", safe, "sequence:", sequence)
    assert safe and _is_valid_safe_sequence(banker, sequence)

    granted, info = banker.request_resources(1, [1, 0, 2])
    print("P1 requests [1,0,2]:", granted, info)
    assert granted and _is_valid_safe_sequence(banker, info)

    granted, info = banker.request_resources(4, [3, 3, 0])
    print("P4 requests [3,3,0]:", granted, info)
    assert not granted

    granted, info = banker.request_resources(0, [0, 2, 0])
    print("P0 requests [0,2,0]:", granted, info)
    assert not granted

    print("All textbook checks passed.")


if __name__ == "__main__":
    _run_textbook_example()