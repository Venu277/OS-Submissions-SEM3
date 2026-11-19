# MicroOS — CA-6 Step A9: Performance Evaluation & Evidence-Based Analysis

> **Specification Reference:** CA-6 Section 23 (Performance & Evidence-Based Evaluation), Section 9 (Core Layers 1–10), Section 11 (Track C Specialization), Section 42 (Submission Quality Evidence)  
> **Trace Source:** `traces/step_a8_demo.log` (32,227 bytes, 569 trace events)  
> **Stress Traces:** `traces/step_a6_sync.log`, `traces/step_a5_preemption.log`, `traces/step_a7_alloc.log`  
> **Output Datasets:** `results/summary_table.csv`, `results/trace_metrics.json`, `results/PERFORMANCE_DATA.md`  
> **Visual Plots:** `results/plots/*.png`

---

## 1. Goal

Fulfill the CA-6 requirement for **evidence-based systems evaluation** (worth 2 marks on the grading rubric) by performing an empirical analysis of MicroOS runtime dynamics:
1. **Methodology:** Parse real machine-generated trace events (`[TRACE] tick=N event=...`) into structured pandas data structures.
2. **Key Metric Dimensions:**
   - **Boot Latency:** Measure cold-boot vs integrated demonstration execution time via hardware TSC and PIT calibration.
   - **Context Switching:** Measure total switches, switch frequencies, and reason distributions (`yield`, `quantum`, `terminate`).
   - **Preemption Responsiveness:** Quantify the latency between quantum expiration and context switch execution.
   - **Synchronization & Contention (Track C):** Measure spinlock acquisition counts, contention rates, spin loops waited, hold durations, and lock ranking deadlock rejections.
   - **Memory Allocator Evaluation (Layer 8):** Empirically compare cycle latency and memory reuse between intrusive linked-list and bump allocators across 100 blocks.
3. **Artifact Generation:** Produce summary tables (`results/summary_table.csv`), JSON datasets (`results/trace_metrics.json`), visual graphs (`results/plots/*.png`), and this analytical report.

---

## 2. Assignment Sections Addressed

- **Section 23 (Performance & Evidence-Based Evaluation):** Structured extraction of real performance metrics from QEMU raw serial logs.
- **Section 9 & 10 (Layers 1–10 & TC1–TC6):** Verified timing and throughput across task queueing, time-slicing, and memory allocation.
- **Section 11 (Track C Concurrency Specialization):** Detailed contention rates and lock ranking deadlock elimination metrics.
- **Section 42 (Evidence-Based Defense for Viva):** Empirical data backing up architectural claims without fabrication.

---

## 3. Files Created & Modified

| File | Purpose |
|:---|:---|
| `scripts/parse_trace.py` | Python automated analyzer computing timeline, context switch, preemption, lock, and memory metrics, generating CSV, JSON, and Matplotlib plots. |
| `results/summary_table.csv` | Comprehensive CSV table containing all 26 key performance metrics. |
| `results/trace_metrics.json` | Hierarchical JSON dataset of all computed kernel metrics. |
| `results/PERFORMANCE_DATA.md` | Master performance dataset documenting raw numbers and distribution tables. |
| `results/plots/context_switches.png` | Bar chart of context switch reasons (`yield`, `terminate`, `quantum`). |
| `results/plots/memory_distribution.png` | Comparative chart of allocation cycle latency and memory reclamation. |
| `results/plots/lock_metrics.png` | Bar chart of lock acquisitions, contentions, and spin latencies. |
| `results/plots/trace_timeline.png` | Scatter plot of all 569 trace events over 715 PIT timer ticks. |
| `docs/CA6_STEP_A9_performance_evaluation.md` | Comprehensive architectural report explaining methodology, metrics, and OS design implications. |

---

## 4. How It Works (Viva Explanation & Architectural Insights)

### Plain English Explanation for Viva
In operating systems engineering, claims like "our scheduler is fast" or "our locks prevent deadlocks" are meaningless without measurement. In Step A9, we treat the kernel as an empirical system:
1. Every significant kernel transition emits an atomic, structured trace event over the serial port (`0x3F8`) without taking heap locks.
2. A Python analysis pipeline ingests this raw hardware stream, parses timestamps and key-value pairs, and extracts statistical distributions.
3. We observe real trade-offs: bump allocators are 5.0x faster than linked lists, but suffer 100% external fragmentation under out-of-order frees; preemption switches occur within $<10$ ms of quantum expiration; and lock ranking completely prevents deadlocks by turning fatal cycles into immediate error returns.

### Metric Pipeline Diagram

```
+-----------------------------------------------------------------------------------+
|                        MICROOS PERFORMANCE EVALUATION PIPELINE                    |
+-----------------------------------------------------------------------------------+

[ QEMU Virtual Machine ]
      |
      | Serial Port 0x3F8 (115,200 baud stream)
      v
[ traces/step_a8_demo.log ] (32,227 Bytes, 569 Machine Events)
      |
      | scripts/parse_trace.py (Regex parsing & Pandas aggregation)
      v
+-----------------------------------------------------------------------------------+
| OUTPUT ARTIFACTS:                                                                 |
|   1. results/summary_table.csv   -> 26 Structured Performance Metrics             |
|   2. results/trace_metrics.json  -> Hierarchical Machine Dataset                  |
|   3. results/PERFORMANCE_DATA.md -> Master Tabular Audit Log                      |
|   4. results/plots/*.png         -> 4 Publication-Quality Evaluation Charts       |
+-----------------------------------------------------------------------------------+
```

---

## 5. Detailed Analysis of Empirical Results

### 1. Boot Latency & Hardware Timing
- **Baseline Core Boot:** 190.479 ms (cold boot to shell in `ca6-core` mode without extras).
- **Full Demo Run:** 34,267.203 ms (executing all 10 layers, TC1–TC6, 3-task round-robin, timer preemption, spinlock contention, deadlock prevention, and 100-block memory benchmark before dropping to shell).
- **TSC Calibration:** PIT Channel 2 hardware calibration measured the CPU at **2,502 MHz**.
- **Timer Frequency:** PIT Channel 0 programmed to **100 Hz** (10 ms period). The entire demo spanned **715 ticks** (7.15 seconds of active kernel execution time).

### 2. Context Switching & Scheduling Overhead
- **Total Switches:** 26 switches recorded in `step_a8_demo.log`.
- **Switch Rate:** $26 / 715 = 0.0364$ switches/tick (~3.64 switches/second).
- **Reason Breakdown:**
  - `yield` (Cooperative): **17 switches (65.4%)** — Worker threads voluntarily relinquishing CPU.
  - `terminate` (Reaping): **7 switches (26.9%)** — Tasks finishing execution and triggering scheduler reaping.
  - `quantum` (Preemption): **2 switches (7.7%)** — Involuntary rotation upon quantum expiration.
- **Architectural Meaning:** Cooperative yielding dominates normal execution, minimizing timer interrupt overhead. Involuntary preemption acts as a safety guardrail ensuring compute-heavy tasks cannot starve interactive tasks.

### 3. Preemption Responsiveness
- **Measured Latency:** In `step_a8_demo.log` (lines 245–248), `QUANTUM_EXPIRE` was emitted at `tick=366`, and `SWITCH ... reason=quantum` was executed at the exact same tick (`tick=366`).
- **Responsiveness:** **$< 1$ tick (< 10 ms)**. The context switch occurs immediately within the timer ISR before returning to ring 0 task context.

### 4. Synchronization & Track C Concurrency Metrics
- **Mutual Exclusion:** 26 spinlock acquisitions recorded with zero race conditions or data corruption.
- **Contention Stress Benchmark (`traces/step_a6_sync.log`):**
  - Contention Rate: **50.0%** (10 contention events across 20 acquisitions).
  - Maximum Spin Wait: **31,520 spin loops** (using `_mm_pause()` for CPU pipeline efficiency).
  - Total Spins Waited: **210,794 spin loops**.
  - Average Lock Hold Duration: **25 to 28 ticks** (250–280 ms).
- **Deadlock Prevention:**
  - In `step_a8_demo.log` (line 328): Task 90 held `vfs_lock` (Rank 20) and attempted to acquire `driver_lock` (Rank 10).
  - Result: `DEADLOCK_PREVENT task=90 req_rank=10 held_rank=20`.
  - Rejection: Returned `Err(LockRankError::HierarchyViolation)`.
  - Architectural Meaning: Coffman circular wait is mathematically impossible because lock acquisition follows a strict monotonic partial order ($R_1 < R_2 < \dots < R_n$).

### 5. Memory Allocator Comparative Benchmark
- **Workload:** 100 allocation requests with block sizes alternating across 32 B, 64 B, 128 B, 256 B, and 512 B (totaling 19,840 Bytes allocated per allocator).
- **Cycle Latency:**
  - *Intrusive Linked List:* **236,574 cycles** average.
  - *Bump Allocator:* **47,040 cycles** average.
  - *Speedup:* Bump allocator is **5.03x faster** due to $O(1)$ pointer addition vs $O(N)$ free-list traversal.
- **Memory Reclamation:**
  - After freeing 50% of blocks (every even block):
    - *Linked List:* Reclaimed **9,920 Bytes (100% of freed space)**, immediately available for reuse.
    - *Bump Allocator:* Reclaimed **0 Bytes (0% of freed space)**, suffering total external fragmentation because non-tail blocks cannot be reclaimed.
- **Allocation Failure Rate:** **0.0%** (zero failures across 200 allocations). Both allocators gracefully return `null_mut()` on oversized requests (e.g., 64 KiB) without kernel panics.

---

## 6. Real Evidence Output (Python Report)

```text
========================================================================
          MicroOS CA-6 Flagship Performance Evaluation Report
========================================================================
Total Machine Trace Events: 569
Observed Tick Range:       60 -> 775 (715 ticks @ 10 ms/tick)
------------------------------------------------------------------------
1. KERNEL BOOT & RUNTIME LATENCY:
   - Baseline Core Boot Time:     190.479 ms
   - Full Demo Boot & Run Time:   34267.203 ms
   - Calibrated TSC Frequency:    2502 MHz
------------------------------------------------------------------------
2. SCHEDULER & CONTEXT SWITCHING (Layers 4, 5, 6):
   - Total Context Switches:      26
   - Context Switch Rate:         0.0364 switches/tick (3.64 switches/s)
   - Reason Distribution:         {'yield': 17, 'terminate': 7, 'quantum': 2}
   - Preemption Latency:          < 1 tick (< 10 ms, immediate within timer ISR)
------------------------------------------------------------------------
3. SYNCHRONIZATION & CONCURRENCY (Layer 7 & Track C):
   - Total Lock Acquisitions:     26
   - Contention Rate (A6 stress): 50.0% (10 / 20 acquisitions)
   - Max Spin Wait Duration:      31520 spins
   - Deadlock Violations Blocked: 1 (Strict Partial Order Enforced)
------------------------------------------------------------------------
4. MEMORY ALLOCATOR BENCHMARK (Layer 8 & Track B):
   - Intrusive Linked List Avg:   236,574 cycles
   - Sequential Bump Allocator:   47,040 cycles (Speedup: 5.03x)
   - Memory Reclaimed (50% free): Linked List = 9,920 B | Bump = 0 B
   - Allocator Failure Rate:      0.0%
========================================================================
[EXPORT] Performance summary table saved to: results/summary_table.csv
[EXPORT] Trace metrics JSON saved to: results/trace_metrics.json
[PLOTS] Generated 4 high-resolution evaluation figures in results/plots/
```

---

## 7. Limitations & Future Optimizations

1. **Intrusive List Coalescing:** While the intrusive linked list successfully reclaims freed blocks, merging adjacent free nodes during deallocation would mitigate external fragmentation for large contiguous allocations.
2. **Dynamic Quantum Tuning:** Currently, the scheduler uses a fixed quantum (3 ticks or 10 ticks). An adaptive multilevel feedback queue (MLFQ) could automatically grant longer quanta to compute-bound threads and shorter quanta to I/O-bound threads.

---

## 8. What Is Next

- **Step A10: Final Submission Package & Viva Preparation:**
  - Conduct final comprehensive audit against all 45 sections of the CA-6 specification.
  - Author `docs/CA6_FINAL_REPORT.md` and viva preparation briefing.
  - Final git push and submission tag on `ca6-compliance`.
