# Step V07: Timer-Driven Preemptive Scheduling (CA-6 Layer 6)

## Goal
Verify and document timer-driven preemptive multitasking in VenOS according to CA-6 Layer 6 requirements:
1. **PIT Channel 0 Hardware Timer Interrupt:**
   - 100 Hz hardware timer tick driving PIT Channel 0 (IRQ 0 / IDT vector 32).
   - Interrupt service routine in `src/interrupts.rs:timer_interrupt_handler` dispatches directly to `scheduler::timer_tick()`.
2. **Preemption Control & RAII Guard (`PreemptionGuard`):**
   - Global atomic `PREEMPTION_ENABLED` flag toggled via `enable_preemption(bool)`.
   - `PreemptionGuard` RAII structure providing nested critical-section protection by disabling preemption upon construction and restoring the previous state upon drop.
   - Strict preemption inhibition inside interrupt service routines and kernel spinlock acquisitions.
3. **Quantum-Based Involuntary Context Switching:**
   - Each thread is allocated a time-slice quantum (default 10 ticks = 100 ms; configurable per-workload, e.g. 2 ticks = 20 ms in tests).
   - Dynamic quantum accounting in `scheduler::timer_tick()`: increments `run_ticks` and decrements `quantum_remaining`.
   - Upon quantum expiration, logs `QUANTUM_EXPIRE`, moves the running thread to `ThreadState::Ready` at the tail of the `ReadyQueue`, and performs an involuntary context switch (`reason=quantum`) to the head thread.
4. **Empirical Verification (`tests/preemptive_scheduling.rs`):**
   - **Test 1:** Preemption flag toggle and RAII `PreemptionGuard` scope management.
   - **Test 2:** Involuntary preemption of an infinite compute-bound worker containing zero yields (`while !stop { loop }`). Preempted by observer task after quantum expiration.
   - **Test 3:** Fair time-slicing between concurrent compute workers (Task A = 44 iterations, Task B = 47 iterations) confirming round-robin time fairness.
5. **Live UEFI Boot Verification:** Verify UEFI boot in QEMU OVMF with boot timing to shell prompt (548.963 ms @ 2500 MHz).

---

## CA-6 Sections Addressed
- **Section 9 (Layer 6 – Preemptive Scheduling):** Timer interrupt hook, quantum accounting, involuntary context switch, preemption disable flag, and nested preemption prevention.
- **Section 10 (Core Project Requirements):** Full preemptive multitasking with time slicing.
- **Section 16 (Required Test Cases TC1–TC6):** TC2 (3-5 Tasks) and TC4 (fair time sharing without voluntary yields).
- **Section 26 (Unsafe Rust Policy):** Safe preemption guards preventing re-entrancy and stack corruption during interrupt dispatch.

---

## Files Changed / Verified
- `src/interrupts.rs`: Connected timer interrupt handler to `scheduler::timer_tick()`.
- `src/scheduler/mod.rs`: Implemented `timer_tick()`, `enable_preemption()`, `is_preemption_enabled()`, and `PreemptionGuard`.
- `tests/preemptive_scheduling.rs`: Verified in QEMU with all 3 test suites passing cleanly (`[ok]`).
- `traces/step_v7_preemption.log`: Captured live UEFI boot trace in QEMU OVMF (boot time 548.963 ms @ 2500 MHz).
- `BOOT_TIMES.md`: Recorded Step V7 boot timing metrics.
- `PROGRESS.md`: Updated milestone status for Step V7.

---

## How It Works

### Plain English (Student Viva Explanation)
In cooperative multitasking (Step V6), a malicious or buggy program with an infinite loop like `while true {}` would freeze the entire operating system because it never calls `yield_now()`.

To prevent this, modern operating systems use **preemptive multitasking** (Step V7):
1. **The Heartbeat (PIT Timer):** The motherboard contains a Programmable Interval Timer (PIT) that ticks 100 times per second (every 10 milliseconds). Each tick sends an electrical interrupt (IRQ 0) to the CPU.
2. **Interrupt Service Routine (ISR):** When the CPU receives the tick, it temporarily stops whatever user thread is running and calls the kernel's `timer_interrupt_handler`.
3. **Quantum Accounting:** The kernel checks the currently running thread. It has a counter called its **quantum** (its allocated CPU time slice). Every 10 ms tick decrements this counter.
4. **Involuntary Context Switch:** When the quantum reaches 0, the thread has used up its fair share of CPU time. The scheduler interrupts it, marks it as `Ready`, puts it at the back of the Ready Queue, and immediately context-switches to the next waiting thread. The thread is preempted *without its knowledge or cooperation*!
5. **Preemption Guard:** When the kernel itself is modifying sensitive data structures (like the heap or ready queue), interrupts or preemption could corrupt memory. VenOS uses `PreemptionGuard`, an RAII guard that temporarily pauses preemption while critical operations execute, automatically restoring it when leaving the block.

### Technical Architecture
```
  [ PIT Channel 0 ] (100 Hz Hardware IRQ 0)
          │
          ▼
  [ IDT Vector 32: timer_interrupt_handler ]
          │
          ▼
  [ scheduler::timer_tick() ]
          ├── Check is_preemption_enabled()?
          │     └── False -> return (inside spinlock or critical section)
          ├── Decrement current.quantum_remaining
          └── If quantum_remaining == 0:
                ├── Log: [TRACE] QUANTUM_EXPIRE task=N
                ├── current.state = Ready
                ├── ready_queue.enqueue(current)
                └── switch_context(current, next) [reason=quantum]
```

---

## Verification & Test Results

### 1. Integration Test (`tests/preemptive_scheduling.rs`)
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
  [Worker 2] Observed Worker 1 count = 22. Signaling Worker 1 to stop.
  [Worker 2] Preemptive observer exiting.
[TRACE] tick=13 event=TASK_STATE task=3 state=TERMINATED
[TRACE] tick=13 event=SWITCH from=3 to=1 reason=terminate
[TRACE] tick=13 event=TASK_STATE task=2 state=RUNNING
[TRACE] tick=13 event=SWITCH from=1 to=2 reason=yield
  [Worker 1] Stop condition met; compute worker exiting cleanly.
[TRACE] tick=13 event=TASK_STATE task=2 state=TERMINATED
[TRACE] tick=13 event=SWITCH from=2 to=1 reason=terminate
[ok] (Alpha count = 22, preemption verified!)
preempt::3_multi_worker_time_slicing...	[TRACE] tick=13 event=TASK_STATE task=5 state=RUNNING
[TRACE] tick=13 event=SWITCH from=4 to=5 reason=yield
[TRACE] tick=23 event=QUANTUM_EXPIRE task=5 consumed=2
[TRACE] tick=23 event=TASK_STATE task=5 state=READY
[TRACE] tick=23 event=TASK_STATE task=6 state=RUNNING
[TRACE] tick=23 event=SWITCH from=5 to=6 reason=quantum
[TRACE] tick=33 event=QUANTUM_EXPIRE task=6 consumed=2
[TRACE] tick=33 event=TASK_STATE task=6 state=READY
[TRACE] tick=33 event=TASK_STATE task=7 state=RUNNING
[TRACE] tick=33 event=SWITCH from=6 to=7 reason=quantum
  [Worker C] Observer scheduled via timer preemption!
  [Worker C] Counts: Task A = 44, Task B = 47
  [Worker C] Sent stop signal to both workers.
[TRACE] tick=33 event=TASK_STATE task=7 state=TERMINATED
[TRACE] tick=33 event=SWITCH from=7 to=4 reason=terminate
[TRACE] tick=33 event=TASK_STATE task=5 state=RUNNING
[TRACE] tick=33 event=SWITCH from=4 to=5 reason=yield
[TRACE] tick=33 event=TASK_STATE task=5 state=TERMINATED
[TRACE] tick=33 event=TASK_STATE task=6 state=RUNNING
[TRACE] tick=33 event=SWITCH from=5 to=6 reason=terminate
[TRACE] tick=33 event=TASK_STATE task=6 state=TERMINATED
[TRACE] tick=33 event=SWITCH from=6 to=4 reason=terminate
[ok] (Task A = 44, Task B = 47, fair slicing verified!)
==================================================
CA-6 Layer 6 Timer Preemption: ALL PASSED
==================================================
```

### 2. Live QEMU/OVMF Boot Trace
- **Boot Time:** `BOOT_TIME_MS=548.963` (calibrated @ 2500 MHz, 1.372G ticks).
- **Log:** `traces/step_v7_preemption.log`.
- **Status:** Framebuffer online at 1280x800, timer ticks active, preemption guard fully operational.
