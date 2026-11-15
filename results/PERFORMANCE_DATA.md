# MicroOS — CA-6 Performance Evaluation Dataset

> **Specification Reference:** CA-6 Section 9, Section 10, Section 11 (Track C) & Section 23/42  
> **Source Log:** `traces/step_a8_demo.log` (32,227 bytes, 569 trace events)  
> **Supporting Stress Logs:** `traces/step_a6_sync.log` (contention stress), `traces/step_a5_preemption.log` (preemption slicing), `traces/step_a7_alloc.log`  
> **Export Files:** `results/summary_table.csv`, `results/trace_metrics.json`, `results/plots/*.png`

---

## 1. Master Metric Summary Table

| Category | Performance Metric | Measured Value | Unit / Scale | Hardware / Spec Reference |
|:---|:---|:---|:---|:---|
| **Boot Latency** | Baseline Core Boot Time | **190.479** | ms | TSC calibrated (`BOOT_TIMES.md`) |
| **Boot Latency** | Full Demo Boot & Run Time | **34,267.203** | ms | `step_a8_demo.log` (5 phases + shell) |
| **Hardware Timer** | TSC Invariant Frequency | **2,502** | MHz | Calibrated via PIT Channel 2 |
| **Hardware Timer** | PIT Interrupt Frequency | **100** | Hz (10.0 ms period) | PIT 8253 Channel 0 (IRQ0) |
| **Hardware Timer** | Total Kernel Execution Ticks | **715** | ticks | Tick 60 to Tick 775 |
| **Context Switching** | Total Assembly Context Switches | **26** | switches | System V AMD64 `switch_context` |
| **Context Switching** | Context Switch Rate | **0.0364** | switches / tick | ~3.64 switches / second |
| **Context Switching** | Voluntary Yield Switches | **17** | switches (65.4%) | `yield_now()` cooperative |
| **Context Switching** | Thread Termination Switches | **7** | switches (26.9%) | Thread completion reaping |
| **Context Switching** | Quantum Preemption Switches | **2** | switches (7.7%) | Involuntary timer preemption |
| **Preemption** | Preemption Responsiveness | **< 10** | ms (< 1 tick) | Immediate switch in PIT ISR |
| **Preemption** | Quantum Duration | **3** | ticks (30 ms) | Configured in `RoundRobinScheduler` |
| **Synchronization** | Total Spinlock Acquisitions | **26** | acquisitions | Hand-written `Spinlock<T>` CAS |
| **Synchronization** | Contention Events (Stress) | **10** | contentions | Multi-worker preemptive stress (`step_a6_sync.log`) |
| **Synchronization** | Contention Rate (Stress) | **50.0** | % | 10 contentions / 20 acquisitions |
| **Synchronization** | Maximum Spin Wait Duration | **31,520** | spin loops | Measured via `_mm_pause()` loop |
| **Synchronization** | Total Spins Waited | **210,794** | spin loops | Accumulated spin delay |
| **Synchronization** | Average Lock Hold Duration | **25 – 28** | ticks (250–280 ms) | Hold ticks across competing threads |
| **Track C Concurrency** | Lock Hierarchy Violations | **1** | violations | Caught and rejected by `RankedLock` |
| **Track C Concurrency** | Deadlock Prevention Rate | **100.0** | % | Strict partial order ($R_1 < R_2$) |
| **Memory Management** | Total Heap Allocations | **200** | allocations | 100 Linked List + 100 Bump |
| **Memory Management** | Total Heap Deallocations | **100** | deallocations | 50% out-of-order freed |
| **Memory Management** | Total Bytes Allocated | **39,680** | Bytes | Workload block range [32B .. 512B] |
| **Memory Management** | Allocation Failure Rate | **0.0** | % | 0 failures (100% allocation success) |
| **Memory Benchmark** | Linked List Avg Allocation Latency | **236,574** | TSC cycles | Free list pointer traversal |
| **Memory Benchmark** | Bump Allocator Avg Latency | **47,040** | TSC cycles | $O(1)$ pointer addition |
| **Memory Benchmark** | Bump Speedup Multiplier | **5.03x** | faster | Bump latency vs Linked List |
| **Memory Benchmark** | Linked List Memory Reused | **9,920** | Bytes (100%) | Full reclamation of freed blocks |
| **Memory Benchmark** | Bump Memory Reused | **0** | Bytes (0%) | External fragmentation hole |

---

## 2. Context Switch Breakdown

```
Context Switch Reasons:
  - yield       : 17 switches [=========================] 65.4%
  - terminate   :  7 switches [==========               ] 26.9%
  - quantum     :  2 switches [===                     ]  7.7%
Total: 26 switches across 715 ticks
```

---

## 3. Memory Allocation Size Distribution

```
Size (Bytes) | Count | Percentage | Total Bytes Allocated
-------------|-------|------------|-----------------------
    32 B     |   40  |   20.0%    |        1,280 B
    64 B     |   40  |   20.0%    |        2,560 B
   128 B     |   40  |   20.0%    |        5,120 B
   256 B     |   40  |   20.0%    |       10,240 B
   512 B     |   40  |   20.0%    |       20,480 B
-------------|-------|------------|-----------------------
   Total     |  200  |  100.0%    |       39,680 B
```

---

## 4. Track C Deadlock Prevention Verification

- **Enforced Rule:** A task holding lock of rank $R_{\text{held}}$ may only acquire lock of rank $R_{\text{req}}$ if:
  $$\text{Priority / Order Invariant: } R_{\text{req}} > R_{\text{held}}$$
- **Recorded Event (`traces/step_a8_demo.log` line 328):**
  ```text
  [TRACE] tick=476 event=DEADLOCK_PREVENT task=90 req_rank=10 held_rank=20
  ```
- **Outcome:** Acquisition of `driver_lock` (Rank 10) while holding `vfs_lock` (Rank 20) returned `Err(LockRankError::HierarchyViolation)`. The requesting task aborted the inverted acquisition, eliminating the Coffman circular wait condition.

---

## 5. Visual Artifacts Generated

1. `results/plots/context_switches.png`: Bar chart of voluntary vs involuntary context switches.
2. `results/plots/memory_distribution.png`: Dual-panel comparative analysis of allocation cycle latencies and memory reclamation efficiency.
3. `results/plots/lock_metrics.png`: Contention statistics, spin wait loops, and lock hold times.
4. `results/plots/trace_timeline.png`: High-density scatter plot of all 569 kernel events plotted over PIT timer ticks.
