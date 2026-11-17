# Deadlock Management: Banker's Avoidance vs Detection with Recovery

Simulation-based comparison of two deadlock strategies. Standard library only, no installs needed.

## Layout

    deadlock-sim/
    ├── README.md
    ├── src/
    │   ├── banker.py       avoidance: safety check and request admission
    │   ├── detector.py     detection: multi-instance detection algorithm
    │   ├── recovery.py     victim selection for recovery
    │   ├── workload.py     seeded workload generator
    │   └── simulator.py    event loop for one strategy on one workload
    ├── tests/
    │   ├── test_banker.py
    │   ├── test_detector.py
    │   └── test_simulator.py
    ├── experiments/
    │   └── run_all.py      runs all configs and seeds, writes results/metrics.csv
    └── results/            created by run_all.py

## Setup (VS Code terminal)

1. Open the `deadlock-sim` folder in VS Code (File > Open Folder).
2. Open a terminal with Ctrl+` (Cmd+` on macOS).
3. Check your Python version (3.8 or newer is needed):

       python --version

   If that fails on macOS or Linux, try `python3 --version` and use `python3` in the commands below.

## Run

Run from the `deadlock-sim` folder.

1. Run the tests:

       python -m unittest discover -s tests -v

2. Run one demo simulation:

       python src/simulator.py

3. Run the full experiment (writes `results/metrics.csv` and prints a summary):

       python experiments/run_all.py

## Model assumptions (keep fixed for both strategies)

- Each process has a maximum claim per resource type, which Banker's needs in advance. Detection ignores it.
- A process requests one unit at a time, choosing among types it still needs.
- Once a process holds its full claim, it keeps the resources for 1 to 5 ticks, then releases them.
- Detection runs every 5 ticks. Recovery aborts the deadlocked process holding the fewest units, which releases its resources and restarts it from scratch.
- Both strategies receive the same workload (maximums, hold times) for each seed. Each process's request order comes from its own seeded RNG, so timing differences can change the order after the first divergence.
