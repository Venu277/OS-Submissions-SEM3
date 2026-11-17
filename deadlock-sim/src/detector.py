"""Deadlock detection for multiple-instance resources.

Follows the detection algorithm in Silberschatz, Operating System Concepts
(Section 8.6.2). Processes that hold nothing cannot be part of a resource
deadlock, so they start as finished.
"""


def find_deadlocked(available, allocation, requests):
    """Return the indices of deadlocked processes (empty list if none).

    requests[i] is the pending request vector of process i. It must be all
    zeros for processes that are not waiting.
    """
    n = len(allocation)
    m = len(available)
    work = list(available)
    finish = [all(v == 0 for v in allocation[i]) for i in range(n)]

    progressed = True
    while progressed:
        progressed = False
        for i in range(n):
            if not finish[i] and all(requests[i][j] <= work[j] for j in range(m)):
                for j in range(m):
                    work[j] += allocation[i][j]
                finish[i] = True
                progressed = True

    return [i for i in range(n) if not finish[i]]
