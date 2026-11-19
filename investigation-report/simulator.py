"""
simulator.py - Discrete-time simulator comparing Banker's avoidance with
detection-and-recovery on identical request traces.

Model (all processes arrive at time 0):
  - Each process has a max claim per resource type and a fixed order of
    single-unit requests that fills that claim. This order is the trace.
  - It acquires its units one at a time, works for `duration` steps while
    holding everything, then releases all units and finishes.
  - A request that cannot be granted blocks the process. It retries each step.
  - Avoidance: every request goes through BankersAlgorithm.request_resources.
  - Detection: a request is granted whenever a unit is free. The detector runs
    every `detect_every` steps. Deadlocks are broken with abort_one recovery,
    and each victim restarts from scratch.
"""

import random
import time

from bankers import BankersAlgorithm
from detection import DetectionSystem


class Process:
    def __init__(self, max_claim, order, duration):
        self.max_claim = max_claim  # total units of each type the process will hold
        self.order = order          # resource type for each unit requested, in order
        self.duration = duration    # steps of work once all units are held


def generate_trace(seed, n_processes, total, max_demand=3, max_duration=5):
    """Build the process set. Same seed gives the same set for every strategy."""
    rng = random.Random(seed)
    m = len(total)
    processes = []
    for _ in range(n_processes):
        claim = [rng.randint(0, min(max_demand, total[j])) for j in range(m)]
        if sum(claim) == 0:
            claim[rng.randrange(m)] = 1
        order = [j for j in range(m) for _ in range(claim[j])]
        rng.shuffle(order)
        processes.append(Process(claim, order, rng.randint(1, max_duration)))
    return processes


def simulate(strategy, processes, total, detect_every=1, max_steps=5000):
    if strategy not in ("avoidance", "detection"):
        raise ValueError("strategy must be 'avoidance' or 'detection'")

    n, m = len(processes), len(total)
    max_claim = [list(p.max_claim) for p in processes]
    alloc = [[0] * m for _ in range(n)]
    req = [[0] * m for _ in range(n)]
    avail = list(total)

    # Avoidance keeps its own state inside BankersAlgorithm, which is the source of truth.
    bk = BankersAlgorithm(avail, max_claim, alloc) if strategy == "avoidance" else None

    ptr = [0] * n
    phase = ["acquire"] * n          # acquire -> work -> done
    blocked = [False] * n
    work_left = [0] * n
    elapsed = [0] * n                # steps since last (re)start, used as work_done
    finish_time = [None] * n
    restarts = [0] * n

    stats = dict(
        delayed_attempts=0,          # blocked process-steps (both strategies)
        unsafe_denials=0,            # avoidance only: denied because unsafe
        deadlock_events=0,           # detection only: recovery episodes
        victims=0,
        units_lost=0,
        steps_lost=0,
        detector_calls=0,
        detector_time=0.0,
        decision_time=0.0,           # avoidance only: time in request_resources
        unsafe_steps=0,              # steps ending in an unsafe state
    )
    completed = 0
    t = 0

    while completed < n and t < max_steps:
        # 1. Each acquiring process attempts its next unit request.
        for i in range(n):
            if phase[i] != "acquire":
                continue
            j = processes[i].order[ptr[i]]
            unit = [1 if k == j else 0 for k in range(m)]

            if strategy == "avoidance":
                start = time.perf_counter()
                granted, info = bk.request_resources(i, unit)
                stats["decision_time"] += time.perf_counter() - start
                if not granted and "unsafe" in info:
                    stats["unsafe_denials"] += 1
            else:
                granted = avail[j] >= 1
                if granted:
                    avail[j] -= 1
                    alloc[i][j] += 1

            if granted:
                blocked[i] = False
                req[i] = [0] * m
                ptr[i] += 1
                if ptr[i] == len(processes[i].order):
                    phase[i] = "work"
                    work_left[i] = processes[i].duration
            else:
                blocked[i] = True
                req[i] = unit
                stats["delayed_attempts"] += 1

        # 2. Working processes progress one step. Finished ones release everything.
        for i in range(n):
            if phase[i] != "work":
                continue
            work_left[i] -= 1
            if work_left[i] == 0:
                phase[i] = "done"
                finish_time[i] = t + 1
                completed += 1
                if strategy == "avoidance":
                    bk.release_resources(i, list(bk.allocation[i]))
                    bk.max_claim[i] = [0] * m   # finished: no further need
                    bk.need[i] = [0] * m
                else:
                    for j in range(m):
                        avail[j] += alloc[i][j]
                    alloc[i] = [0] * m
                    max_claim[i] = [0] * m

        # 3. Detection: run the detector periodically while anything is blocked.
        if strategy == "detection" and any(blocked) and (t + 1) % detect_every == 0:
            system = DetectionSystem(avail, alloc, req, work_done=elapsed)
            start = time.perf_counter()
            res = system.recover_abort_one()
            stats["detector_time"] += time.perf_counter() - start
            stats["detector_calls"] += 1

            if res.victims:
                stats["deadlock_events"] += 1
                stats["victims"] += len(res.victims)
                stats["units_lost"] += res.units_lost
                stats["steps_lost"] += res.steps_lost
                avail, alloc, req = system.available, system.allocation, system.request
                for v in res.victims:
                    ptr[v] = 0
                    phase[v] = "acquire"
                    blocked[v] = False
                    work_left[v] = 0
                    elapsed[v] = 0
                    restarts[v] += 1

        # 4. End-of-step bookkeeping and safety check.
        for i in range(n):
            if phase[i] != "done":
                elapsed[i] += 1

        if strategy == "avoidance":
            state = (bk.available, bk.max_claim, bk.allocation)
        else:
            state = (avail, max_claim, alloc)
        if not BankersAlgorithm(*state).is_safe()[0]:
            stats["unsafe_steps"] += 1

        t += 1

    finished = [ft for ft in finish_time if ft is not None]
    stats.update(
        strategy=strategy,
        processes=n,
        completed=completed,
        timed_out=completed < n,
        makespan=t,
        throughput=completed / t if t else 0.0,
        avg_turnaround=sum(finished) / len(finished) if finished else None,
        restarts=sum(restarts),
    )
    return stats


def _run_checks():
    total = [4, 3, 3]
    for seed in range(10):
        for n in (4, 8):
            procs = generate_trace(seed, n, total)
            a = simulate("avoidance", procs, total)
            d = simulate("detection", procs, total)

            assert not a["timed_out"], (seed, n, "avoidance timed out")
            assert a["completed"] == n
            assert a["unsafe_steps"] == 0, (seed, n, "avoidance entered an unsafe state")
            assert a["deadlock_events"] == 0

            assert d["victims"] >= d["deadlock_events"]
            assert d["completed"] <= n
            if d["deadlock_events"] == 0:
                assert d["units_lost"] == 0 and d["steps_lost"] == 0

    # Same seed must give the same trace and the same avoidance result.
    p1 = generate_trace(3, 8, total)
    p2 = generate_trace(3, 8, total)
    r1 = simulate("avoidance", p1, total)
    r2 = simulate("avoidance", p2, total)
    assert r1["makespan"] == r2["makespan"]
    assert r1["delayed_attempts"] == r2["delayed_attempts"]

    print("All simulator checks passed.")


if __name__ == "__main__":
    total = [4, 3, 3]
    procs = generate_trace(seed=1, n_processes=8, total=total)
    for strategy in ("avoidance", "detection"):
        print(simulate(strategy, procs, total))
    _run_checks()