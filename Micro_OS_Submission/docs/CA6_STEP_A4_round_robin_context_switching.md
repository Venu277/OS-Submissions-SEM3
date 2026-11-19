# CA-6 Step A4: Round Robin with Context Switching (Layer 5)

> **Specification Reference:** CA-6 Section 9 (Layer 5 - Round Robin Scheduler), Section 10 (Core Requirements), Section 16 (TC1–TC6), Section 26 (Unsafe Policy)

---

## 1. Goal

The objective of Step A4 is to implement low-level hardware CPU register context switching and cooperative Round Robin scheduling in MicroOS:
1. Implement the low-level AMD64 context switch routine (`switch_context`) in assembly (`core::arch::global_asm!`) using Intel syntax, preserving and restoring callee-saved registers (`r15`, `r14`, `r13`, `r12`, `rbx`, `rbp`), arithmetic flags (`rflags`), and the stack pointer (`rsp`) conforming to the System V AMD64 ABI.
2. Implement cooperative multitasking primitives:
   - `yield_now()`: allows the currently executing thread to voluntarily relinquish the CPU to the next ready task.
   - `exit_current_thread()`: safely marks a finished thread as `TERMINATED`, transitions execution to the next ready task, and parks the dead thread in `terminated_queue`.
   - `reap_terminated()`: cleans up and deallocates memory and stacks of finished threads.
   - Initial return trampoline: sets up each new thread's stack with its `entry_point` and `kthread_exit` return address so that normal function returns automatically trigger termination and reaping.
3. Emit real-time structured traces for every context switch (`[TRACE] tick=N event=SWITCH from=X to=Y reason=yield`).
4. Implement and verify a 3-task cooperative Round Robin demonstration with alternating execution patterns.
5. Create dedicated integration tests (`tests/round_robin_switch.rs`) verifying task ordering, repeated scheduling, completion, and reaping.

---

## 2. Assignment Sections Addressed

- **Section 9 (Layer 5 – Round Robin Scheduler):**
  - Assembly context switch saving and restoring callee-saved registers.
  - Ordered Round Robin scheduling queue using `VecDeque<Box<KThread>>`.
  - Cooperative yielding (`yield_now()`).
  - Thread lifecycle completion, exit, and memory reclamation (reaping).
- **Section 10 (Core Requirement 5 & 10):**
  - Working Round Robin scheduler with assembly context switch.
  - Repeatable test evidence showing alternating execution order.
- **Section 26 (Unsafe Policy):**
  - Encapsulated assembly routine behind safe Rust interfaces with documented `// SAFETY:` invariants recorded in `docs/UNSAFE_AUDIT.md`.

---

## 3. Files Created / Modified

| File | Type | Purpose |
|:---|:---|:---|
| `src/scheduler/context_switch.rs` | New | AMD64 assembly context switch routine (`switch_context`) saving/restoring callee-saved registers and `rsp` in Intel syntax. |
| `src/kthread/mod.rs` | Modified | Configured stack return trampoline (`kthread_exit` and `entry_point`) and added `new_main()` for the boot thread. |
| `src/scheduler/mod.rs` | Modified | Implemented `yield_now()`, `exit_current_thread()`, `reap_terminated()`, `wait_all_threads()`, and `demo_three_tasks_round_robin()`. |
| `tests/round_robin_switch.rs` | New | Automated QEMU integration test verifying interleaved execution pattern `[1, 2, 3, 1, 2, 3, 1, 2, 3]`, completion, and reaping. |
| `Cargo.toml` | Modified | Registered `[[test]] name = "round_robin_switch"`. |
| `docs/UNSAFE_AUDIT.md` | Modified | Documented safety invariants for `switch_context`, stack trampolines, and exit switches (U-05, U-06, U-07). |
| `traces/step_a4_round_robin.log` | New | Real QEMU execution trace capturing 40 machine trace events (16 context switches, 24 state transitions). |
| `results/step_a4_trace_events.csv` | New | Exported CSV dataset of round-robin context switch events. |
| `docs/CA6_STEP_A4_round_robin_context_switching.md` | New | Step documentation, technical explanation, and viva guide. |
| `PROGRESS.md` | Modified | Updated progress tracker with Step A4 status and test metrics. |

---

## 4. How It Works

### Plain English Explanation (Viva Guide)
Imagine an actor reading a script on stage. If another actor needs to take the stage:
1. **Saving Context:** The first actor bookmarks their script (their current line of code, `rip`), puts down their props (their callee-saved registers: `rbx`, `rbp`, `r12`–`r15`), and notes where they were standing on stage (the stack pointer, `rsp`).
2. **Switching Context (`switch_context`):** The stage manager swaps the bookmark and props for the second actor.
3. **Resuming:** The second actor picks up their own props, looks at their private bookmark on their own stack, and resumes reading their script right where they left off.
4. **Natural Exit Trampoline:** When a thread reaches the end of its function and calls `return`, the CPU pops the next address from the stack—which we pre-loaded with `kthread_exit`. The thread gracefully bows, reports to the director that it is `TERMINATED`, and the stage manager removes it from the stage so its memory can be recycled.

### Technical Explanation & Assembly Walkthrough

```
                      switch_context(prev: *mut CpuContext, next: *const CpuContext)
                                (rdi: prev_ctx, rsi: next_ctx)

    Thread A Stack (Outgoing)                             Thread B Stack (Incoming)
+--------------------------------+                  +--------------------------------+
|          ... data ...          |                  |          ... data ...          |
+--------------------------------+                  +--------------------------------+
| Caller Return Address (%rip)   | <--- %rsp (old)  | Caller Return Address (%rip)   | <--- %rsp (new)
+--------------------------------+                  +--------------------------------+

1. mov [rdi + 0x00..0x28], r15..rbp  (Save A's callee-saved registers into A.context)
2. pushfq; pop rax; mov [rdi + 0x38], rax  (Save A's arithmetic flags)
3. mov [rdi + 0x30], rsp             (Save A's current stack pointer into A.context.rsp)

4. mov rsp, [rsi + 0x30]             (SWITCH CPU STACK: Load B's stack pointer into %rsp!)
5. mov rax, [rsi + 0x38]; push rax; popfq  (Restore B's flags)
6. mov rbp..r15, [rsi + 0x28..0x00]  (Restore B's callee-saved registers)

7. ret                               (Pops return address from B's stack into %rip -> Resumes B!)
```

When a thread is newly created, its stack is pre-initialized with:
- `stack_top - 8`: address of `kthread_exit`
- `stack_top - 16`: address of `entry_point`
When `switch_context` executes `ret`, it pops `entry_point` into `rip`, and begins executing the thread function with `rsp % 16 == 8` (conforming to the System V ABI). When `entry_point` executes `ret`, it pops `kthread_exit`, cleanly terminating the thread.

---

## 5. Verification Commands & Real Output

### A. Dedicated Test Suite Execution (`cargo test --test round_robin_switch`)
```powershell
cargo test --test round_robin_switch
```
**Real QEMU Serial Output:**
```
==================================================
   CA-6 Layer 5: Round Robin & Context Switch Test
==================================================
rr::1_init_scheduler...	[ok]
rr::2_spawn_three_tasks...	[ok]
rr::3_executing_cooperative_round_robin_loop...
[TRACE] tick=4 event=TASK_STATE task=2 state=RUNNING
[TRACE] tick=4 event=SWITCH from=1 to=2 reason=yield
  [Task 1 / Alpha] Iteration 1/3 running
[TRACE] tick=4 event=TASK_STATE task=2 state=READY
[TRACE] tick=4 event=TASK_STATE task=3 state=RUNNING
[TRACE] tick=4 event=SWITCH from=2 to=3 reason=yield
  [Task 2 / Beta ] Iteration 1/3 running
[TRACE] tick=5 event=TASK_STATE task=3 state=READY
[TRACE] tick=5 event=TASK_STATE task=4 state=RUNNING
[TRACE] tick=5 event=SWITCH from=3 to=4 reason=yield
  [Task 3 / Gamma] Iteration 1/3 running
[TRACE] tick=5 event=TASK_STATE task=4 state=READY
[TRACE] tick=5 event=SWITCH from=4 to=1 reason=yield
[TRACE] tick=5 event=TASK_STATE task=2 state=RUNNING
[TRACE] tick=5 event=SWITCH from=1 to=2 reason=yield
  [Task 1 / Alpha] Iteration 2/3 running
[TRACE] tick=5 event=TASK_STATE task=2 state=READY
[TRACE] tick=5 event=TASK_STATE task=3 state=RUNNING
[TRACE] tick=5 event=SWITCH from=2 to=3 reason=yield
  [Task 2 / Beta ] Iteration 2/3 running
[TRACE] tick=5 event=TASK_STATE task=3 state=READY
[TRACE] tick=5 event=TASK_STATE task=4 state=RUNNING
[TRACE] tick=5 event=SWITCH from=3 to=4 reason=yield
  [Task 3 / Gamma] Iteration 2/3 running
[TRACE] tick=5 event=TASK_STATE task=4 state=READY
[TRACE] tick=5 event=SWITCH from=4 to=1 reason=yield
[TRACE] tick=5 event=TASK_STATE task=2 state=RUNNING
[TRACE] tick=5 event=SWITCH from=1 to=2 reason=yield
  [Task 1 / Alpha] Iteration 3/3 running
[TRACE] tick=5 event=TASK_STATE task=2 state=READY
[TRACE] tick=5 event=TASK_STATE task=3 state=RUNNING
[TRACE] tick=5 event=SWITCH from=2 to=3 reason=yield
  [Task 2 / Beta ] Iteration 3/3 running
[TRACE] tick=5 event=TASK_STATE task=3 state=READY
[TRACE] tick=5 event=TASK_STATE task=4 state=RUNNING
[TRACE] tick=5 event=SWITCH from=3 to=4 reason=yield
  [Task 3 / Gamma] Iteration 3/3 running
[TRACE] tick=5 event=TASK_STATE task=4 state=READY
[TRACE] tick=5 event=SWITCH from=4 to=1 reason=yield
[TRACE] tick=5 event=TASK_STATE task=2 state=RUNNING
[TRACE] tick=5 event=SWITCH from=1 to=2 reason=yield
  [Task 1 / Alpha] Returning from entry function.
[TRACE] tick=5 event=TASK_STATE task=2 state=TERMINATED
[TRACE] tick=5 event=TASK_STATE task=3 state=RUNNING
[TRACE] tick=5 event=SWITCH from=2 to=3 reason=terminate
  [Task 2 / Beta ] Returning from entry function.
[TRACE] tick=5 event=TASK_STATE task=3 state=TERMINATED
[TRACE] tick=5 event=TASK_STATE task=4 state=RUNNING
[TRACE] tick=5 event=SWITCH from=3 to=4 reason=terminate
  [Task 3 / Gamma] Returning from entry function.
[TRACE] tick=5 event=TASK_STATE task=4 state=TERMINATED
[TRACE] tick=5 event=SWITCH from=4 to=1 reason=terminate
rr::3_round_robin_loop_completed!	[ok]
rr::4_verify_task_execution_order...	[ok] (Exact pattern [1, 2, 3, 1, 2, 3, 1, 2, 3] verified!)
rr::5_task_completion_and_reaping...	[ok] (3/3 terminated threads reaped!)
==================================================
CA-6 Layer 5 Round Robin & Context Switch: ALL PASSED
==================================================
```

### B. Python Machine-Trace Log Parser (`scripts/parse_trace.py`)
```powershell
py -3 scripts\parse_trace.py --trace traces\step_a4_round_robin.log --csv results\step_a4_trace_events.csv
```
**Parser Report:**
```text
============================================================
      MicroOS Trace & Diagnostic Analysis Report
============================================================
Source Trace File:   traces\step_a4_round_robin.log
Total Trace Events:  40
Total Console Logs:  0
------------------------------------------------------------
Trace Event Distribution:
  - TASK_STATE          :    24
  - SWITCH              :    16

Tick Range: min=2, max=3, delta=1
============================================================
[EXPORT] Trace CSV saved to: results\step_a4_trace_events.csv
```

### C. Full Regression Suite (All 67 Tests Passed)
```powershell
cargo test
```
- **27 / 27 unit tests** passed in `src/lib.rs`.
- **40 / 40 integration tests** passed across all 13 test targets (`round_robin_switch`, `kthread_ready_queue`, `ca6_demo`, `diagnostics_trace`, `filesystem`, `hardware`, `heap_allocation`, `networking`, `pkg_apt`, `smoke_integration`, `snap_store`, `stack_overflow`, `wifi`).

---

## 6. Limitations

- Context switching in Step A4 is currently cooperative (threads call `yield_now()`). Preemptive time-slicing triggered by the hardware 8253 PIT timer interrupt will be wired in Step A5.

---

## 7. What is Next (Step A5)

In **Step A5 (Timer-Driven Preemptive Scheduling - Layer 6)**:
- Connect the 8253 PIT timer IRQ0 interrupt handler to decrement the running thread's `time_slice_remaining`.
- When quantum expires (`time_slice_remaining == 0`), force an involuntary context switch from the interrupt handler.
- Verify preemption on a CPU-bound thread that does not call `yield_now()`.
- Trace quantum expirations (`[TRACE] tick=N event=QUANTUM_EXPIRE task=X consumed=10`).
