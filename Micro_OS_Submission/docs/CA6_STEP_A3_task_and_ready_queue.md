# CA-6 Step A3: Task Representation & Ready Queue (Layers 3 & 4)

> **Specification Reference:** CA-6 Section 9 (Layer 3 - Task Representation, Layer 4 - Ready Queue), Section 10 (Core Requirements), Section 15 & 16 (Testing Strategy & TC1–TC6)

---

## 1. Goal

The objective of Step A3 is to establish a rigorous, production-grade task representation and ready queue subsystem in MicroOS satisfying CA-6 Layers 3 & 4:
1. Implement a complete kernel thread structure (`KThread`) featuring:
   - Unique thread identifier (`ThreadId`)
   - Explicit 4-state lifecycle model (`READY`, `RUNNING`, `BLOCKED`, `TERMINATED`)
   - Validated state transition machine with rejection of illegal transitions (TC5 failure testing)
   - Callee-saved AMD64 System V ABI register context (`CpuContext`) including instruction pointer `rip` and interrupt flags `rflags`
   - Dedicated 16 KiB heap-allocated kernel stack
   - Stack canary boundary guard (`0xCA60_DEAD_BEEF_0001`) with automatic overflow detection (TC3 boundary testing)
   - Runtime accounting metrics (run ticks, wait ticks, context switch count)
2. Implement an ordered FIFO `ReadyQueue` supporting insertion (`enqueue`), selection (`dequeue`), inspection (`peek`, `peek_mut`), ID-based removal (`remove`), and linear search (`find`, `contains`).
3. Fully implement and verify all required assignment test cases: **TC1** (Two Tasks), **TC2** (3–5 Tasks), **TC3** (Boundary Cases), **TC4** (Stress Queueing), **TC5** (Failure Cases), and **TC6** (Repeatability).
4. Integrate shell diagnostic commands (`ca6 tasks`, `ca6 tc`).

---

## 2. Assignment Sections Addressed

- **Section 9 (Layer 3 – Task Representation):**
  - Unique identification: `ThreadId` via atomic counter.
  - Lifecycle state machine: `READY`, `RUNNING`, `BLOCKED`, `TERMINATED`.
  - CPU Context: `CpuContext` preserving `r15`, `r14`, `r13`, `r12`, `rbx`, `rbp`, `rsp`, `rflags`, `rip`.
  - Kernel stack: Dedicated 16 KiB `Vec<u8>` heap allocation per thread with 16-byte System V ABI alignment.
- **Section 9 (Layer 4 – Ready Queue):**
  - FIFO deque ordering ensuring fairness.
  - Non-destructive `peek()` and `peek_mut()` for scheduler decisions.
  - ID-targeted `remove()` for blocked threads or cancellation.
- **Section 16 (Required Test Cases TC1–TC6):**
  - **TC1:** Two tasks representation, distinct stacks, and FIFO ordering.
  - **TC2:** 3–5 tasks lifecycle rotation and execution accounting.
  - **TC3:** Boundary case (empty queue handling, stack boundary limit via canary).
  - **TC4:** Stress test (50 tasks lifecycle across 5 stress batches).
  - **TC5:** Controlled failure case (illegal state transition rejection & corrupted stack canary detection).
  - **TC6:** Deterministic repeatability across 10 consecutive queueing cycles.

---

## 3. Files Created / Modified

| File | Type | Purpose |
|:---|:---|:---|
| `src/kthread/mod.rs` | Modified | Added `rip` to `CpuContext`, stack canary guard (`STACK_CANARY_MAGIC`), `transition_to` validation with trace emissions, `check_stack_overflow()`, and metrics accounting. |
| `src/scheduler/mod.rs` | Modified | Expanded `ReadyQueue` with `peek`, `peek_mut`, `remove`, `contains`, `find`, `iter`, and `spawn_thread`. |
| `src/shell.rs` | Modified | Added `ca6 tasks` (thread roster display) and `ca6 tc` (TC1-TC6 automated test execution) CLI commands. |
| `tests/kthread_ready_queue.rs` | New | Comprehensive automated integration test harness executing TC1 through TC6 inside QEMU. |
| `Cargo.toml` | Modified | Registered `[[test]] name = "kthread_ready_queue"`. |
| `traces/step_a3_task_queue.log` | New | Real QEMU execution trace capturing 112 machine task state transitions. |
| `results/step_a3_trace_events.csv` | New | Parsed CSV dataset of task state transitions across TC1–TC6. |
| `docs/CA6_STEP_A3_task_and_ready_queue.md` | New | Step documentation, technical explanation, and viva guide. |
| `PROGRESS.md` | Modified | Updated progress tracker with Step A3 completion, test counts, and decisions. |

---

## 4. How It Works

### Plain English Explanation (Viva Guide)
In an operating system, CPU hardware does not understand "threads" or "processes"—the CPU only executes instructions pointed to by its program counter (`rip`) using registers and a stack pointer (`rsp`).
To run multiple programs concurrently:
1. **Task Snapshot (`KThread` & `CpuContext`):** When a thread pauses, we save its entire brain state—its registers, its next instruction, and its stack location—into a `KThread` memory card.
2. **Dedicated Stack & Guard Canary:** Every thread gets its own private 16 KiB sandbox stack. Because stacks grow downward in x86_64, we stamp a secret digital watermark (`0xCA60_DEAD_BEEF_0001`) at the very bottom. If a runaway function overflows its memory sandbox, it overwrites the watermark, and the kernel immediately halts the thread before memory corruption spreads.
3. **Strict Lifecycle Rules:** A thread cannot magically jump from `Terminated` back into `Running`, nor from `Ready` directly into `Blocked`. Every transition is checked against an explicit rulebook; any illegal action is rejected with an error.
4. **Ready Queue:** Like passengers boarding a flight, runnable threads wait in a FIFO queue. The scheduler takes the person at the front, lets them run for their quantum, and puts them at the back if they still have work to do.

### Technical Architecture Diagram

```
+-----------------------------------------------------------------------------------+
|                                  ReadyQueue                                       |
|  [Head] -> KThread (ID: 1) -> KThread (ID: 2) -> KThread (ID: 3) -> [Tail]        |
|  Operations: enqueue(), dequeue(), peek(), remove(id), contains(id), find(id)     |
+----------------------------------------+------------------------------------------+
                                         |
                                         v
+-----------------------------------------------------------------------------------+
|                                   KThread                                         |
|  +-----------------------------------------------------------------------------+  |
|  | id: ThreadId (atomic increment)   | name: String                            |  |
|  | state: ThreadState (READY -> RUNNING -> BLOCKED/READY -> TERMINATED)        |  |
|  | time_slice_remaining: u32         | metrics: ThreadMetrics (run/wait/switch)|  |
|  +-----------------------------------------------------------------------------+  |
|  | CpuContext (System V AMD64 ABI):                                            |  |
|  |   r15, r14, r13, r12, rbx, rbp, rsp (16-byte aligned), rflags (0x202), rip   |  |
|  +-----------------------------------------------------------------------------+  |
|  | Dedicated Stack: Vec<u8> (16 KiB heap allocation)                            |  |
|  |   High Address (stack.as_ptr() + 16KiB) <--- initial rsp (aligned)            |  |
|  |   ... stack frames grow downward ...                                        |  |
|  |   Low Address (stack[0..8]): [0xCA60_DEAD_BEEF_0001] (Stack Canary Guard)   |  |
|  +-----------------------------------------------------------------------------+  |
+-----------------------------------------------------------------------------------+
```

---

## 5. Verification Commands & Real Output

### A. Dedicated Test Suite Execution (`cargo test --test kthread_ready_queue`)
```powershell
cargo test --test kthread_ready_queue
```
**Real QEMU Serial Output:**
```
==================================================
   CA-6 Layers 3 & 4: Task & Ready Queue Suite
==================================================
tc1_two_tasks_representation_and_queueing...	[ok]
tc2_three_to_five_tasks_lifecycle...	[TRACE] tick=1 event=TASK_STATE task=3 state=RUNNING
[TRACE] tick=1 event=TASK_STATE task=3 state=READY
[TRACE] tick=1 event=TASK_STATE task=4 state=RUNNING
[TRACE] tick=1 event=TASK_STATE task=4 state=READY
[TRACE] tick=1 event=TASK_STATE task=5 state=RUNNING
[TRACE] tick=1 event=TASK_STATE task=5 state=READY
[TRACE] tick=1 event=TASK_STATE task=6 state=RUNNING
[TRACE] tick=1 event=TASK_STATE task=6 state=READY
[TRACE] tick=1 event=TASK_STATE task=7 state=RUNNING
[TRACE] tick=1 event=TASK_STATE task=7 state=READY
[ok]
tc3_boundary_cases_empty_queue_and_canary...	[ok]
tc4_stress_case_50_tasks...	[TRACE] tick=1 event=TASK_STATE task=9 state=RUNNING
[TRACE] tick=1 event=TASK_STATE task=9 state=TERMINATED
... (50 tasks across 5 stress batches) ...
[TRACE] tick=2 event=TASK_STATE task=58 state=TERMINATED
[ok]
tc5_failure_cases_rejected_operations...	[TRACE] tick=2 event=TASK_STATE task=59 state=RUNNING
[TRACE] tick=2 event=TASK_STATE task=59 state=TERMINATED
[ok]
tc6_repeatability_10_cycles...	[ok]
==================================================
CA-6 Task & Ready Queue Suite: TC1-TC6 ALL PASSED
==================================================
```

### B. Python Trace Parser Report (`scripts/parse_trace.py`)
```powershell
py -3 scripts\parse_trace.py --trace traces\step_a3_task_queue.log --csv results\step_a3_trace_events.csv
```
**Output:**
```
============================================================
      MicroOS Trace & Diagnostic Analysis Report
============================================================
Source Trace File:   traces\step_a3_task_queue.log
Total Trace Events:  112
Total Console Logs:  0
------------------------------------------------------------
Trace Event Distribution:
  - TASK_STATE          :   112

Tick Range: min=1, max=2, delta=1
============================================================
[EXPORT] Trace CSV saved to: results\step_a3_trace_events.csv
```

### C. Full Regression Suite (All 66 Tests Passed)
```powershell
cargo test
```
- **27 / 27 unit tests** passed in `src/lib.rs` (including state transitions, invalid transitions, canary detection, and queue operations).
- **39 / 39 integration tests** passed across all 12 test binaries (`kthread_ready_queue`, `ca6_demo`, `diagnostics_trace`, `filesystem`, `hardware`, `heap_allocation`, `networking`, `pkg_apt`, `smoke_integration`, `snap_store`, `stack_overflow`, `wifi`).

---

## 6. Limitations

- Context switching assembly (`switch_context`) has not yet been executed live on the CPU hardware; threads currently transition between states, queues, and preserve registers in data structures. Low-level context switching with inline assembly will be implemented in Step A4.

---

## 7. What is Next (Step A4)

In **Step A4 (Round Robin with Context Switching - Layer 5)**:
- Implement the low-level AMD64 context switch function (`switch_context`) in assembly (`core::arch::global_asm!`) preserving callee-saved registers (`r15`, `r14`, `r13`, `r12`, `rbx`, `rbp`, `rsp`).
- Add cooperative `yield_now()` allowing threads to voluntarily yield the CPU to the next ready task.
- Demonstrate 3 cooperative tasks printing in an alternating round-robin pattern.
- Trace every context switch (`[TRACE] tick=N event=SWITCH from=X to=Y reason=yield`).
- Update `docs/UNSAFE_AUDIT.md` with the context switch assembly invariants.
