"""Discrete-time simulator that runs one strategy on one workload.

Each process repeatedly requests one unit of a random type it still needs.
Once it holds its full claim it keeps the resources for `hold` ticks and then
releases everything. A process whose request cannot be granted waits and retries
on the next tick.

Strategies:
    "avoidance"  Banker's algorithm: grant only if the state stays safe.
    "detection"  Grant if units are free. Every `detect_interval` ticks, look for
                 deadlock and recover by aborting one victim per deadlock.
"""

import os
import random
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from banker import BankerState          # noqa: E402
from detector import find_deadlocked    # noqa: E402
from recovery import choose_victim      # noqa: E402
from workload import generate_workload  # noqa: E402


def _detect(state, pending, done, m):
    requests = []
    for i, p in enumerate(pending):
        waiting = p is not None and not done[i]
        requests.append([1 if (waiting and j == p) else 0 for j in range(m)])
    return find_deadlocked(state.available, state.allocation, requests)


def simulate(workload, strategy, detect_interval=5, max_ticks=5000):
    if strategy not in ("avoidance", "detection"):
        raise ValueError("strategy must be 'avoidance' or 'detection'")

    total = workload["total"]
    maximum = workload["maximum"]
    holds = workload["hold"]
    n, m = len(maximum), len(total)

    state = BankerState(total, maximum, [[0] * m for _ in range(n)])
    rngs = [random.Random(s) for s in workload["seeds"]]

    pending = [None] * n     # resource type being requested (one unit at a time)
    hold_left = [None] * n   # ticks remaining while holding the full claim
    done = [False] * n
    wait_ticks = [0] * n
    deadlocks = aborts = detection_passes = 0
    ticks = 0

    def release_all(i):
        state.release(i)
        pending[i] = None
        hold_left[i] = None

    while ticks < max_ticks and not all(done):
        ticks += 1

        for i in range(n):
            if done[i]:
                continue

            if hold_left[i] is not None:
                hold_left[i] -= 1
                if hold_left[i] <= 0:
                    release_all(i)
                    done[i] = True
                continue

            if pending[i] is None:
                options = [j for j in range(m) if state.need(i)[j] > 0]
                if not options:
                    hold_left[i] = holds[i]
                    continue
                pending[i] = rngs[i].choice(options)

            vec = [1 if j == pending[i] else 0 for j in range(m)]
            if strategy == "avoidance":
                granted = state.request(i, vec)
            else:
                granted = all(state.available[j] >= vec[j] for j in range(m))
                if granted:
                    state.grant_unchecked(i, vec)

            if granted:
                pending[i] = None
            else:
                wait_ticks[i] += 1

        if strategy == "detection" and ticks % detect_interval == 0:
            detection_passes += 1
            dead = _detect(state, pending, done, m)
            while dead:
                deadlocks += 1
                victim = choose_victim(dead, state.allocation)
                release_all(victim)
                aborts += 1
                dead = _detect(state, pending, done, m)

    completed = sum(done)
    return {
        "strategy": strategy,
        "processes": n,
        "completed": completed,
        "unfinished": n - completed,
        "ticks": ticks,
        "throughput": round(completed / ticks, 4) if ticks else 0.0,
        "deadlocks": deadlocks,
        "aborts": aborts,
        "mean_wait": round(sum(wait_ticks) / n, 4),
        "safety_checks": state.safety_checks,
        "detection_passes": detection_passes,
    }


if __name__ == "__main__":
    wl = generate_workload(num_processes=6, total_resources=[8, 8, 8], max_claim=6, seed=1)
    for strategy in ("avoidance", "detection"):
        print(simulate(wl, strategy))
