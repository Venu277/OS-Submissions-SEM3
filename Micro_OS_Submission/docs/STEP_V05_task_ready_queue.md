# Step V05: Task Representation & Ready Queue (CA-6 Layers 4 & 5)

## Goal
Verify and document the core task abstraction and thread-safe ready queue management according to CA-6 Layer 3 and Layer 4 requirements:
1. **Thread Representation (`KThread`):**
   - Unique monotonic identifier (`ThreadId`).
   - Four-state lifecycle model: `Ready`, `Running`, `Blocked`, `Terminated`.
   - Saved execution context (`CpuContext`: `r15`, `r14`, `r13`, `r12`, `rbx`, `rbp`, `rsp`, `rflags`, `rip`) conforming to System V AMD64 ABI callee-saved conventions.
   - Dedicated 16 KiB kernel stack with stack canary guard (`0xCA60_DEAD_BEEF_0001`) at the stack base to detect and reject stack overflow.
   - Runtime performance accounting: `run_ticks`, `wait_ticks`, `switch_count`.
2. **Ready Queue Management (`ReadyQueue`):**
   - FIFO queueing of runnable threads (`enqueue`, `dequeue`, `peek`, `remove`, `contains`).
   - Strict state machine transition validation (`transition_to()`), rejecting illegal transitions (e.g. `Terminated -> Running`).
3. **TC1–TC6 Test Suite Verification:**
   - Execute `tests/kthread_ready_queue.rs` in QEMU:
     - **TC1:** Two Tasks (distinct IDs, separate stacks, 16-byte alignment, FIFO ordering).
     - **TC2:** 3 to 5 Tasks (lifecycle transitions `Ready -> Running -> Blocked -> Terminated`).
     - **TC3:** Boundary Cases (empty queue dequeue, stack canary validation).
     - **TC4:** Stress Case (50 tasks spawned, queued, and completed).
     - **TC5:** Failure Cases (rejected illegal transitions, corrupted canary detection).
     - **TC6:** Repeatability (10x deterministic cycles).
4. **Live UEFI Boot Verification:** Verify UEFI boot in QEMU OVMF with boot timing to shell prompt (560.011 ms @ 2498 MHz).

---

## CA-6 Sections Addressed
- **Section 9 (Layer 3 – Task Representation):** Task ID, state enum, context struct, stack allocation, stack guard canary.
- **Section 9 (Layer 4 – Ready Queue):** Enqueue, dequeue, priority/FIFO ordering, thread lookup, and state tracking.
- **Section 10 (Core Project Requirements):** Complete preemptible kernel thread data structures.
- **Section 16 (Required Test Cases TC1–TC6):** Full coverage across TC1 (Two Tasks), TC2 (3-5 Tasks), TC3 (Boundary), TC4 (Stress), TC5 (Failure), and TC6 (Repeatability).

---

## Files Changed
- `src/kthread/mod.rs`: Verified `KThread`, `ThreadId`, `ThreadState`, `CpuContext`, `ThreadMetrics`, and stack canary verification.
- `src/scheduler/mod.rs`: Verified `ReadyQueue`, `enqueue`, `dequeue`, `remove`, and preemption guards.
- `tests/kthread_ready_queue.rs`: Verified in QEMU across all 6 test cases TC1 through TC6 (all `[ok]`).
- `traces/step_v5_task_ready_queue.log`: Captured live UEFI boot trace in QEMU OVMF (boot time 560.011 ms @ 2498 MHz).
- `BOOT_TIMES.md`: Recorded Step V5 boot timing metrics.
- `PROGRESS.md`: Updated milestone status for Step V5.

---

## How It Works

### Plain English (Student Viva Explanation)
A multitasking operating system does not run programs as one continuous block of code. Instead, it breaks work into distinct **threads of execution**. Each thread must have its own isolated memory space for local variables and function calls (its **stack**) and a structure that remembers what the CPU was doing when the thread was paused (its **CPU context**).

In Step V5, we verified VenOS's task architecture:
1. **Thread Representation (`KThread`):** When a thread is created, VenOS allocates a 16 KiB stack on the kernel heap. At the very bottom of this stack, we write a special 64-bit number called a **stack canary** (`0xCA60_DEAD_BEEF_0001`). If a recursive function or deep call stack runs out of room and overflows the stack, it overwrites this canary value. Before switching back to a thread, the kernel checks this canary; if corrupted, it immediately terminates the thread, protecting adjacent kernel memory.
2. **State Machine (`ThreadState`):** Every thread is in one of four states:
   - `Ready`: Waiting in the Ready Queue for CPU time.
   - `Running`: Currently executing on the CPU core.
   - `Blocked`: Waiting for a lock or I/O event.
   - `Terminated`: Finished execution and awaiting reclamation.
   Illegal transitions (such as waking a dead thread from `Terminated` back to `Running`) are detected and rejected.
3. **The Ready Queue (`ReadyQueue`):** The Ready Queue is a FIFO (First-In, First-Out) data structure that stores pointers to all threads that are ready to run. When the timer ticks or a thread yields, the scheduler pops the thread at the front of the queue, runs it, and puts the previous thread back at the end of the queue.

### Technical Architecture
```
+---------------------------------------------------------------------------------------+
| `KThread` Structure (16 KiB Stack Layout)                                             |
|                                                                                       |
|   High Address (0x4444...3FFF) ───► [ Top of Stack (16-byte aligned for AMD64 ABI) ]  |
|                                     │                                                 |
|                                     │   Local variables, return addresses,            |
|                                     │   callee-saved register push space              |
|                                     ▼   (Grows downward)                              |
|                                     ...                                               |
|   Low Address  (0x4444...0000) ───► [ Stack Canary: 0xCA60_DEAD_BEEF_0001 (8 bytes) ] |
+---------------------------------------------------------------------------------------+

+---------------------------------------------------------------------------------------+
| Four-State Lifecycle Transitions (`transition_to()`)                                 |
|                                                                                       |
|                 ┌─────────────► [ RUNNING ] ─────────────┐                            |
|                 │                  │   ▲                 │                            |
|                 │                  │   │                 ▼                            |
|           (dequeue/switch) (yield/irq0)│ (unblock)  (exit/reap)                       |
|                 │                  │   │                 │                            |
|                 │                  ▼   │                 ▼                            |
|   [ NEW ] ──► [ READY ]           [ BLOCKED ]      [ TERMINATED ]                     |
|                 ▲                                        │                            |
|                 └────────────────────────────────────────┘ (ILLEGAL - TC5 Rejected)   |
+---------------------------------------------------------------------------------------+
```

---

## Verification & Trace Evidence

### 1. Repeatable Integration Test: `tests/kthread_ready_queue.rs`
Executed via `cargo test --test kthread_ready_queue`:
```text
==================================================
   CA-6 Layers 3 & 4: Task & Ready Queue Suite
==================================================
tc1_two_tasks_representation_and_queueing...	[ok]
tc2_three_to_five_tasks_lifecycle...	[ok]
tc3_boundary_cases_empty_queue_and_canary...	[ok]
tc4_stress_case_50_tasks...	[ok]
tc5_failure_cases_rejected_operations...	[ok]
tc6_repeatability_10_cycles...	[ok]
==================================================
CA-6 Task & Ready Queue Suite: TC1-TC6 ALL PASSED
==================================================
```
- **TC1 (Two Tasks):** Distinct IDs (task 1 vs task 2), isolated stack pointers, 16-byte alignment, FIFO dequeue ordering.
- **TC2 (3-5 Tasks):** Rotated 5 tasks through `Ready -> Running -> Ready` and verified queue preservation.
- **TC3 (Boundary Cases):** Empty queue dequeue returns `None`; stack canary verification passes on untouched stack.
- **TC4 (Stress 50 Tasks):** Enqueued 50 tasks, transitioned each through `Running -> Terminated`, verified clean queue emptying.
- **TC5 (Failure Cases):** Corrupted stack canary detected by `verify_stack_integrity() == false`; illegal transition `Terminated -> Running` rejected with error.
- **TC6 (Repeatability):** Ran 10 consecutive cycles of creation, queueing, and termination with identical deterministic outcomes.

### 2. Live UEFI Boot in QEMU OVMF (`traces/step_v5_task_ready_queue.log`)
- Kernel ELF size: 10,376,464 bytes (9.9 MB).
- UEFI GPT FAT32 Image: 12,648,448 bytes (12.06 MB).
- Calibrated TSC frequency: **2498 MHz**.
- Elapsed TSC ticks: 1,399,314,388.
- Boot time to shell prompt: **560.011 ms**.

---

## Gap Analysis Matrix Status After Step V5

| Gap ID | Description | Status | Resolution |
|--------|-------------|--------|------------|
| **GAP-15** | Task representation & isolated stack guards | **RESOLVED** | Verified `KThread` with `CpuContext`, 16 KiB stack, and `0xCA60_DEAD_BEEF_0001` canary. |
| **GAP-16** | Ready queue FIFO ordering and lifecycle | **RESOLVED** | Verified `ReadyQueue` enqueue, dequeue, and state validation in `tests/kthread_ready_queue.rs`. |
| **GAP-17** | TC1–TC6 Task & Ready Queue compliance | **RESOLVED** | All 6 required CA-6 test cases passed 100% in QEMU. |

---

## Next Steps
Proceed to **Step V6: Round Robin Scheduler & Assembly Context Switch (CA-6 Layer 5)**:
- Verify assembly `switch_context(&mut old_ctx, &new_ctx)` in `src/scheduler/context_switch.rs`.
- Verify register preservation (System V AMD64 callee-saved: `rbx`, `rsp`, `rbp`, `r12`-`r15`).
- Verify cooperative yielding (`yield_now()`), voluntary completion, and dead task reaping.
- Run `tests/round_robin_switch.rs`.
- Author `docs/STEP_V06_round_robin_switch.md`.
