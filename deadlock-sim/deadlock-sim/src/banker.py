"""Banker's algorithm: deadlock avoidance via safety checking.

State layout (lists of ints):
    available[j]        free instances of resource type j
    maximum[i][j]       maximum claim of process i for type j
    allocation[i][j]    instances of type j currently held by process i
"""


class BankerState:
    def __init__(self, available, maximum, allocation):
        self.n = len(allocation)
        self.m = len(available)
        self.available = list(available)
        self.maximum = [list(row) for row in maximum]
        self.allocation = [list(row) for row in allocation]
        self.safety_checks = 0
        for i in range(self.n):
            for j in range(self.m):
                if self.allocation[i][j] > self.maximum[i][j]:
                    raise ValueError(
                        f"process {i}: allocation exceeds maximum for type {j}"
                    )

    def need(self, i):
        return [self.maximum[i][j] - self.allocation[i][j] for j in range(self.m)]

    def safe_sequence(self):
        """Return a safe ordering of process indices, or None if the state is unsafe."""
        self.safety_checks += 1
        needs = [self.need(i) for i in range(self.n)]
        work = list(self.available)
        finish = [False] * self.n
        sequence = []
        progressed = True
        while progressed:
            progressed = False
            for i in range(self.n):
                if not finish[i] and all(needs[i][j] <= work[j] for j in range(self.m)):
                    for j in range(self.m):
                        work[j] += self.allocation[i][j]
                    finish[i] = True
                    sequence.append(i)
                    progressed = True
        return sequence if all(finish) else None

    def is_safe(self):
        return self.safe_sequence() is not None

    def _shift(self, i, vec, sign):
        for j in range(self.m):
            self.available[j] -= sign * vec[j]
            self.allocation[i][j] += sign * vec[j]

    def request(self, i, req):
        """Grant req to process i only if the resulting state is safe.

        Returns True if granted. Returns False and leaves the state unchanged otherwise.
        """
        need = self.need(i)
        if any(req[j] > need[j] for j in range(self.m)):
            raise ValueError(f"process {i} requested more than its declared maximum")
        if any(req[j] > self.available[j] for j in range(self.m)):
            return False
        self._shift(i, req, +1)
        if self.is_safe():
            return True
        self._shift(i, req, -1)
        return False

    def grant_unchecked(self, i, req):
        """Grant without a safety check (used by the detection strategy)."""
        self._shift(i, req, +1)

    def release(self, i):
        """Return all of process i's allocated resources to the pool."""
        for j in range(self.m):
            self.available[j] += self.allocation[i][j]
            self.allocation[i][j] = 0
