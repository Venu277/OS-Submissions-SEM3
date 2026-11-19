# MicroOS — CA-6 Step A7: Memory Allocator Evaluation & Benchmark

> **Specification Reference:** CA-6 Section 9 (Layer 8: Memory Management & Working Allocator) & Section 11 (Track B: Memory Management Specialization)  
> **Evaluation Mode:** Dual allocator comparison (Intrusive Linked List vs Bump Allocator)  
> **Trace Source:** `traces/step_a7_alloc.log` (20,467 bytes, 414 events)  
> **CSV Analysis:** `results/step_a7_trace_events.csv`

---

## 1. Goal

Fulfill Layer 8 requirements by providing a rigorous, empirical evaluation of dynamic memory management in MicroOS:
1. Implement a hand-written **Bump Allocator** (`BumpAllocator`) alongside the existing intrusive **Linked List Allocator** (`LinkedListAllocator`).
2. Add structured memory allocation and deallocation diagnostic tracing (`[TRACE] tick=N event=ALLOC size=S ok=1` and `event=DEALLOC size=S`) with atomic opt-in/opt-out control (`ALLOC_TRACING_ENABLED`).
3. Build a reproducible comparative benchmark suite executing 100 allocation requests across varied block sizes (32 B to 512 B), measuring TSC cycle latency, memory fragmentation, out-of-order deallocation reuse, and graceful OOM handling.
4. Integrate an interactive shell command (`ca6 alloc`) and automated kernel boot verification (`demo_memory_allocator()`).

---

## 2. Assignment Sections Addressed

- **Section 9 (Layer 8: Working Memory Allocator):** Kernel heap initialization, dynamic heap growth, and safe allocation/deallocation under Rust `GlobalAlloc`.
- **Section 11 (Track B: Memory Management Specialization):** Comparative allocator benchmarking, allocation latency distribution, internal/external fragmentation trade-off analysis, and graceful OOM handling without panics.
- **Section 26 (Unsafe Rust Policy):** Audited pointer arithmetic, heap initialization bounds, and alignment calculations under `UNSAFE_AUDIT.md` (entries `U-10` and `U-11`).

---

## 3. Files Created & Modified

| File | Purpose |
|:---|:---|
| `src/allocator/bump.rs` | Hand-written $O(1)$ bump pointer allocator with sequential allocation and full reset on zero active allocations. |
| `src/allocator.rs` | Added `pub mod bump;`, `ALLOC_TRACING_ENABLED`, structured allocation tracing helpers, `benchmark_allocator_comparison()`, and `demo_memory_allocator()`. |
| `src/shell.rs` | Added interactive CLI command `ca6 alloc` executing the dual-allocator benchmark and memory metrics. |
| `src/scheduler/mod.rs` | Integrated `demo_memory_allocator()` into the integrated `run_ca6_demo()` boot sequence. |
| `Cargo.toml` | Registered `allocator_benchmark` integration test target. |
| `tests/allocator_benchmark.rs` | Automated integration test suite validating basic alloc/dealloc, sequential bump, OOM rejection, and 100-block comparative benchmark. |
| `docs/UNSAFE_AUDIT.md` | Documented unsafe invariants for bump allocator memory bounds and benchmark TSC / heap pointer operations (`U-10`, `U-11`). |
| `traces/step_a7_alloc.log` | Raw serial trace capture from QEMU recording 414 events including 201 `ALLOC` and 101 `DEALLOC` events. |
| `results/step_a7_trace_events.csv` | Parsed CSV structured dataset extracted via `scripts/parse_trace.py`. |

---

## 4. How It Works (Viva Explanation & Architecture)

### Plain English Explanation
Operating systems need dynamic memory so threads can create data structures whose sizes aren't known ahead of time. However, how memory is allocated fundamentally dictates performance:
- A **Bump Allocator** is like an odometer: each allocation just moves a pointer forward by the requested size. It is blindingly fast ($O(1)$), but when an individual object is freed, it cannot reclaim that hole; memory is only reclaimed when *every* allocation is freed.
- An **Intrusive Linked List Allocator** keeps an in-memory list of free chunks threaded through the unallocated space itself. Finding a suitable chunk takes longer ($O(N)$ traversal), but when any chunk is freed, it is immediately returned to the free list and can be reused by future allocations.

### Architectural Comparison Diagram

```
+-----------------------------------------------------------------------------------+
|                            KERNEL MEMORY MANAGEMENT                              |
+-----------------------------------------------------------------------------------+

1. INTRUSIVE LINKED LIST ALLOCATOR (Reusable, Free List Traversal):
   Heap Base
   [0x0000] ---> [ Block 1: 64B (USED) ]
   [0x0040] ---> [ FreeNode: size=128B, next=0x0180 ] (Available for reuse)
   [0x00C0] ---> [ Block 2: 192B (USED) ]
   [0x0180] ---> [ FreeNode: size=256B, next=NULL ]   (Available for reuse)
   -> Out-of-order free reclaims holes immediately!

2. BUMP ALLOCATOR (Linear, O(1) Speed, Monotonic):
   Heap Base                                                    Heap Top
   [ Block 1 ][ Block 2 ][ Block 3 ][ Free Hole? (Cannot Reuse) ][ Next ] -------->
                                                                 ^
                                                            bump pointer
   -> Fast pointer advance, but cannot reclaim intermediate holes until ALL freed.
```

### Empirical Trade-off Matrix

| Metric | Linked List Allocator | Bump Allocator | Winner / Analysis |
|:---|:---|:---|:---|
| **Average Latency** | ~7,400 – 10,770 TSC cycles | ~1,120 – 4,310 TSC cycles | **Bump (2.5x – 6.6x faster)** |
| **Complexity** | $O(N)$ free-list search | $O(1)$ pointer addition | **Bump** |
| **Fragmentation Handling** | Reclaims arbitrary freed blocks | High external fragmentation | **Linked List** |
| **Memory Reused (50% freed)** | **9,920 Bytes reclaimed** | **0 Bytes reclaimed** | **Linked List** |
| **OOM Behavior** | Graceful `null_mut()` | Graceful `null_mut()` | **Tie (both handle OOM safely)** |

---

## 5. Build, Test, and Verification Commands

```powershell
# 1. Compile kernel
cargo build

# 2. Run dedicated allocator benchmark test suite
cargo test --test allocator_benchmark

# 3. Run full test suite across kernel
cargo test

# 4. Capture automated QEMU trace
powershell -ExecutionPolicy Bypass -File scripts\run_trace.ps1 -TraceName step_a7_alloc -CoreMode -DurationSeconds 6

# 5. Parse trace to CSV
py -3 scripts\parse_trace.py --trace traces\step_a7_alloc.log --csv results\step_a7_trace_events.csv
```

---

## 6. Real Evidence Output

### A. Dedicated Test Output (`cargo test --test allocator_benchmark`)
```text
==================================================
   CA-6 Layer 8 & Track B: Allocator Benchmark
==================================================
alloc::1_linked_list_allocation_and_reuse...	[ok]
alloc::2_bump_allocator_sequential_and_reset...	[ok]
alloc::3_graceful_oom_handling...	[ok]
alloc::4_benchmark_100_blocks_comparison...	[ok] (LL_avg=7442 cyc, Bump_avg=1124 cyc, LL_reused=9920 B, Bump_reused=0 B)
==================================================
CA-6 Layer 8 Allocator Benchmark: ALL PASSED
==================================================
```

### B. QEMU Kernel Execution Output (`traces/step_a7_alloc.log`)
```text
[CA6-ALLOC] Benchmark: LL_avg=10770 cyc, Bump_avg=4313 cyc, LL_reused=9920 B, Bump_reused=0 B
BOOT_TIME_MS=748.144 (TSC ticks=1866640177, freq=2495 MHz)
micro_os:/> 
```

### C. Trace Parser Distribution (`scripts/parse_trace.py`)
```text
============================================================
      MicroOS Trace & Diagnostic Analysis Report
============================================================
Source Trace File:   traces\step_a7_alloc.log
Total Trace Events:  414
Total Console Logs:  0
------------------------------------------------------------
Trace Event Distribution:
  - ALLOC               :   201
  - DEALLOC             :   101
  - TASK_STATE          :    34
  - LOCK_ACQ            :    26
  - LOCK_REL            :    26
  - SWITCH              :    24
  - QUANTUM_EXPIRE      :     1
  - DEADLOCK_PREVENT    :     1
============================================================
[EXPORT] Trace CSV saved to: results\step_a7_trace_events.csv
```

---

## 7. Limitations & Future Work

1. **Intrusive Node Coalescing:** The current linked list merges contiguous free blocks sequentially, but could benefit from a buddy-allocation or slab-cache layer for fixed-size kernel objects (`KThread`, `TaskControlBlock`).
2. **Dynamic Heap Resizing:** The heap virtual boundary is currently fixed at 256 KiB; paging could dynamically allocate page frames on demand when approaching heap exhaustion.

---

## 8. What Is Next

- **Step A8: Integrated Demo & Verification (`ca6 demo` Full Suite):**
  - Verify and consolidate the end-to-end `ca6 demo` exercising all 10 core OS layers sequentially (Boot, Console/Tracing, KThread, Ready Queue, Round Robin Switch, Timer Preemption, Spinlock, Ranked Lock Deadlock Prevention, Memory Allocator Benchmark, Shell Mode).
  - Validate TC1 through TC6 automated repeatability.
