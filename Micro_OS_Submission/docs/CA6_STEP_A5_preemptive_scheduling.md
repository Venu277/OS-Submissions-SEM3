# CA-6 Step A5: Timer-Driven Preemptive Scheduling (Layer 6)

## 1. Goal
Connect the kernel scheduler to hardware timer interrupts (Intel 8253 Programmable Interval Timer / PIT IRQ 0) to provide involuntary, time-sliced preemptive multitasking. Guarantee that long-running or compute-bound tasks that never voluntarily yield CPU execution are automatically preempted upon quantum exhaustion, enabling subsequent ready threads to run fairly.

---

## 2. Assignment Section Addressed
- **Section 9, Layer 6 — Timer-Driven Scheduling:** "Connect the scheduler to a periodic timer mechanism where supported by the selected architecture. Demonstrate the difference between cooperative switching and timer-driven/preemptive-style scheduling."
- **Section 10 — Required Demonstrations:** "Demonstrate CPU-bound thread preemption without voluntary yield calls."
- **Section 16 & Section 38 — Test Cases & Evidence:** Automated test harness and structured event tracing confirming involuntary quantum expiration and fair round-robin CPU sharing.
- **Section 26 — Unsafe Rust Policy:** Documented encapsulation of PIT hardware I/O port writes and interrupt context switches with explicit invariant verification in `docs/UNSAFE_AUDIT.md` (U-08, U-09).

---

## 3. Files Created and Changed

| File Path | Status | One-Line Purpose |
|:---|:---|:---|
| `src/interrupts.rs` | Modified | Configured PIT Channel 0 to 100 Hz (`init_pit`) and wired `timer_tick()` to IRQ 0 handler. |
| `src/lib.rs` | Modified | Initialized 100 Hz PIT timer tick during kernel CPU subsystem setup. |
| `src/scheduler/mod.rs` | Modified | Implemented `PREEMPTION_ENABLED`, `PreemptionGuard`, `timer_tick()`, and `demo_preemptive_scheduling()`. |
| `src/shell.rs` | Modified | Registered `ca6 preempt` command for interactive preemptive scheduling demonstration. |
| `Cargo.toml` | Modified | Registered integration test target `preemptive_scheduling`. |
| `tests/preemptive_scheduling.rs` | Created | Automated QEMU test suite validating preemption flags, compute-bound preemption, and fair time-slicing. |
| `traces/step_a5_preemption.log` | Created | Captured raw serial execution trace from QEMU demonstrating involuntary preemption. |
| `results/step_a5_trace_events.csv` | Created | Parsed structured event dataset (52 events: `QUANTUM_EXPIRE`, `SWITCH`, `TASK_STATE`). |
| `docs/UNSAFE_AUDIT.md` | Modified | Registered unsafe blocks U-08 (PIT I/O ports) and U-09 (interrupt context switch). |
| `docs/CA6_STEP_A5_preemptive_scheduling.md` | Created | Technical architecture and viva preparation document for Step A5. |
| `PROGRESS.md` | Modified | Updated progress log and milestone status. |

---

## 4. How It Works

### Plain English Explanation (for Viva / Presentation)
In cooperative multitasking (Layer 5), a task continues running until it voluntarily chooses to yield the CPU by calling `yield_now()`. If a task contains a bug, an infinite loop, or intensive compute code, it starves all other tasks and freezes the operating system.

In preemptive multitasking (Layer 6), the hardware Programmable Interval Timer (PIT) fires an interrupt (IRQ 0) 100 times per second (every 10 milliseconds). Each timer tick temporarily pauses the running task and runs the kernel timer interrupt handler. The kernel decrements the running task's time slice (its "quantum"). When the time slice drops to zero, the kernel forcefully removes the task from the CPU, moves it to the back of the Ready Queue, restores the next task's saved CPU registers, and resumes execution. Even if a thread never yields, the kernel automatically regains control and ensures fair CPU allocation.

### Technical Architecture & Execution Flow
1. **Timer Programming (PIT 8253 Channel 0):**
   - The base oscillator runs at 1,193,182 Hz.
   - We program Channel 0 by writing command byte `0x34` (Mode 2, rate generator, lobyte/hibyte) to port `0x43`, and divisor `11931` (`1193182 / 100`) to data port `0x40`.
   - This delivers a timer interrupt to IDT vector 32 (IRQ 0) every 10 ms.
2. **Interrupt Entry:**
   - The CPU pushes the hardware interrupt frame (`SS`, `RSP`, `RFLAGS`, `CS`, `RIP`) onto the running task's private 16 KiB heap stack.
   - Hardware clears the Interrupt Flag (`IF = 0`) to prevent nested interrupts.
   - The handler sends End-of-Interrupt (EOI) to PIC 1 port `0x20` and invokes `crate::scheduler::timer_tick()`.
3. **Quantum Accounting & Involuntary Switch:**
   - `timer_tick()` attempts to acquire `SCHEDULER.try_lock()`. If locked (e.g. by another critical section), it safely skips to avoid deadlock in interrupt context.
   - It decrements `current.time_slice_remaining`.
   - When remaining ticks reach 0:
     - Emits `trace_quantum_expire(task_id, quantum)`.
     - Resets quantum to `default_quantum`.
     - Moves `current` to `ThreadState::Ready` and enqueues to tail of `ReadyQueue`.
     - Dequeues `next` from head of `ReadyQueue` and marks it `ThreadState::Running`.
     - Emits `trace_switch(from, to, "quantum")`.
     - Drops scheduler lock and calls `context_switch::switch_context(prev_ptr, next_ptr)`.
4. **Resumption:**
   - If `next` was previously preempted, `switch_context` returns to `timer_tick()`, returns to the interrupt handler, and executes `iretq`, cleanly restoring the interrupted computation state with interrupts re-enabled.

### Sequence Diagram

```mermaid
sequenceDiagram
    autonumber
    actor CPU as Hardware CPU
    participant TaskA as Worker Alpha (Compute Loop)
    participant PIT as 8253 PIT (IRQ 0)
    participant ISR as Timer Interrupt Handler
    participant Sched as RoundRobinScheduler
    participant TaskB as Worker Beta (Observer)

    TaskA->>TaskA: Compute loop (while !stop { count++ })
    PIT->>CPU: IRQ 0 (Vector 32)
    CPU->>ISR: Push [RIP, CS, RFLAGS, RSP, SS] & clear IF
    ISR->>ISR: Send PIC EOI (Vector 32 acknowledged)
    ISR->>Sched: timer_tick()
    Sched->>Sched: Decrement Alpha quantum (time_slice_remaining == 0)
    Sched->>Sched: trace_quantum_expire(Alpha, 3)
    Sched->>Sched: Alpha (Running -> Ready) enqueued
    Sched->>Sched: Beta (Ready -> Running) dequeued
    Sched->>Sched: trace_switch(Alpha, Beta, "quantum")
    Sched->>TaskB: switch_context(&Alpha.ctx, &Beta.ctx)
    TaskB->>TaskB: Execute preempt_worker_beta()
    TaskB->>TaskA: Set STOP_FLAG = true
    TaskB->>Sched: exit_current_thread()
    Sched->>TaskA: switch_context(&Beta.ctx, &Alpha.ctx)
    TaskA->>CPU: iretq restores pre-interrupt CPU state
    TaskA->>TaskA: Loop condition false -> clean exit
```

---

## 5. Verification Commands and Real Output

### A. Preemptive Scheduling Integration Test
```powershell
cargo test --test preemptive_scheduling
```

**Real Serial Output from QEMU:**
```
==================================================
   CA-6 Layer 6: Timer-Driven Preemption Test
==================================================
preempt::1_flag_and_raii_guard...	[ok]
preempt::2_involuntary_preemption_no_yield...	[TRACE] tick=3 event=TASK_STATE task=2 state=RUNNING
[TRACE] tick=3 event=SWITCH from=1 to=2 reason=yield
  [Worker 1] Started compute-bound loop (NO yields)...
[TRACE] tick=13 event=QUANTUM_EXPIRE task=2 consumed=2
[TRACE] tick=13 event=TASK_STATE task=2 state=READY
[TRACE] tick=13 event=TASK_STATE task=3 state=RUNNING
[TRACE] tick=13 event=SWITCH from=2 to=3 reason=quantum
  [Worker 2] Scheduled via PIT timer preemption!
  [Worker 2] Observed Worker 1 count = 23. Signaling Worker 1 to stop.
  [Worker 2] Preemptive observer exiting.
[TRACE] tick=13 event=TASK_STATE task=3 state=TERMINATED
[TRACE] tick=13 event=SWITCH from=3 to=1 reason=terminate
[TRACE] tick=13 event=TASK_STATE task=2 state=RUNNING
[TRACE] tick=13 event=SWITCH from=1 to=2 reason=yield
  [Worker 1] Stop condition met; compute worker exiting cleanly.
[TRACE] tick=13 event=TASK_STATE task=2 state=TERMINATED
[TRACE] tick=13 event=SWITCH from=2 to=1 reason=terminate
[ok] (Alpha count = 23, preemption verified!)
preempt::3_multi_worker_time_slicing...	[TRACE] tick=14 event=TASK_STATE task=5 state=RUNNING
[TRACE] tick=14 event=SWITCH from=4 to=5 reason=yield
[TRACE] tick=24 event=QUANTUM_EXPIRE task=5 consumed=2
[TRACE] tick=24 event=TASK_STATE task=5 state=READY
[TRACE] tick=24 event=TASK_STATE task=6 state=RUNNING
[TRACE] tick=24 event=SWITCH from=5 to=6 reason=quantum
[TRACE] tick=34 event=QUANTUM_EXPIRE task=6 consumed=2
[TRACE] tick=34 event=TASK_STATE task=6 state=READY
[TRACE] tick=34 event=TASK_STATE task=7 state=RUNNING
[TRACE] tick=34 event=SWITCH from=6 to=7 reason=quantum
  [Worker C] Observer scheduled via timer preemption!
  [Worker C] Counts: Task A = 46, Task B = 46
  [Worker C] Sent stop signal to both workers.
[TRACE] tick=34 event=TASK_STATE task=7 state=TERMINATED
[TRACE] tick=34 event=SWITCH from=7 to=4 reason=terminate
[TRACE] tick=34 event=TASK_STATE task=5 state=RUNNING
[TRACE] tick=34 event=SWITCH from=4 to=5 reason=yield
[TRACE] tick=34 event=TASK_STATE task=5 state=TERMINATED
[TRACE] tick=34 event=TASK_STATE task=6 state=RUNNING
[TRACE] tick=34 event=SWITCH from=5 to=6 reason=terminate
[TRACE] tick=34 event=TASK_STATE task=6 state=TERMINATED
[TRACE] tick=34 event=SWITCH from=6 to=4 reason=terminate
[ok] (Task A = 46, Task B = 46, fair slicing verified!)
==================================================
CA-6 Layer 6 Timer Preemption: ALL PASSED
==================================================
```

### B. Full Test Suite Regression Check
```powershell
cargo test
```
**Result:** **72 passing tests across 15 test suites** with zero failures and zero compiler warnings.

### C. Live Trace Capture & Automated Parsing
```powershell
powershell -ExecutionPolicy Bypass -File scripts\run_trace.ps1 -TraceName step_a5_preemption -CoreMode -DurationSeconds 5
py -3 scripts\parse_trace.py --trace traces\step_a5_preemption.log --csv results\step_a5_trace_events.csv
```

**Parsed Distribution from `results/step_a5_trace_events.csv`:**
- Total Trace Events: **52**
- `TASK_STATE`: 30
- `SWITCH`: 21
- `QUANTUM_EXPIRE`: 1
- Tick Range: min=13, max=37 (24 ticks = 240 ms)

---

## 6. Limitations and Assumptions
1. **Single-Core IRQ 0:** Preemption is currently driven by PIC IRQ 0 on bootstrap processor Core 0. SMP / multicore preemption using Local APIC timer interrupts is supported by our MADT table parsing (Step 8) but kept deferred to preserve simplicity in CA-6 single-CPU demonstration mode.
2. **Preemption Depth:** Preemption is controlled globally via `PREEMPTION_ENABLED` and locally via `PreemptionGuard`. Disabling interrupts (`cli`) also automatically prevents timer preemption during low-level scheduler and memory operations.

---

## 7. What Is Next
- **CA-6 Step A6: Synchronization & Track C Concurrency Specialization:**
  - Enhance `Spinlock<T>` with fine-grained contention metrics (`spins_waited`, `hold_ticks`, `acquire_count`).
  - Demonstrate shared-resource race condition with preemption enabled, followed by corrected execution using `Spinlock<T>`.
  - Validate Track C strict partial order lock ranking (`RankedLock<T>`) preventing deadlock conditions.
