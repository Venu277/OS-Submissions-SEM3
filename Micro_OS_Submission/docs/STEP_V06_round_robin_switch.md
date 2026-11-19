# Step V06: Round Robin Scheduler & Assembly Context Switch (CA-6 Layer 5)

## Goal
Verify and document the Round Robin scheduler and the low-level AMD64 assembly context switch routine in VenOS:
1. **Low-Level Assembly Context Switch (`switch_context`):**
   - Implemented via `core::arch::global_asm!` in `src/scheduler/context_switch.rs`.
   - Conforms strictly to the System V AMD64 ABI calling convention: saves and restores callee-saved registers (`r15`, `r14`, `r13`, `r12`, `rbx`, `rbp`), arithmetic flags (`rflags`), and the stack pointer (`rsp`).
   - Instruction pointer (`rip`) transfer is achieved gracefully via the hardware `call` and `ret` instructions.
2. **Cooperative Multitasking & Round Robin Dispatch (`schedule` / `yield_now`):**
   - Cooperative scheduling via `yield_now()`, voluntarily surrendering the CPU to the next ready task.
   - Dispatch loop in `src/scheduler/mod.rs` taking the next task from the FIFO `ReadyQueue`, performing stack canary safety checks, transitioning states (`Running -> Ready`, `Ready -> Running`), and invoking `switch_context()`.
   - Thread exit handler (`thread_exit_trampoline`) transitioning finished threads to `ThreadState::Terminated` and invoking `schedule()` to hand over control without returning into unmapped stack frames.
   - Resource reaping (`reap_terminated()`) reclaiming kernel stacks and thread control blocks.
3. **Execution Order Verification (`tests/round_robin_switch.rs`):**
   - Verified 3 concurrent worker threads interleaving in exact deterministic round-robin order: `[1, 2, 3, 1, 2, 3, 1, 2, 3]`.
   - Verified thread termination and 3/3 task stack reclamation.
4. **Live UEFI Boot Verification:** Verify UEFI boot in QEMU OVMF with boot timing to shell prompt (1248.373 ms @ 2495 MHz).

---

## CA-6 Sections Addressed
- **Section 9 (Layer 5 – Round Robin Scheduler):** Assembly context switch routine, cooperative yield, quantum scheduling, state updates, idle thread handling.
- **Section 10 (Core Project Requirements):** Complete preemptible/cooperative kernel thread execution model with context save/restore.
- **Section 16 (Required Test Cases TC1–TC6):** TC1 & TC2 execution order interleaving, voluntary yield validation, and post-termination thread reaping.
- **Section 26 (Unsafe Rust Policy):** Formal audit of assembly context switch invariants (`switch_context` pointer safety, alignment, and stack integrity).

---

## Files Changed
- `src/scheduler/context_switch.rs`: Verified AMD64 `switch_context` assembly routine saving/restoring callee-saved registers and `rflags`.
- `src/scheduler/mod.rs`: Verified Round Robin dispatch loop, thread trampoline, `yield_now()`, and `reap_terminated()`.
- `tests/round_robin_switch.rs`: Verified in QEMU with exact execution order `[1, 2, 3, 1, 2, 3, 1, 2, 3]` and 100% test pass (`[ok]`).
- `traces/step_v6_round_robin_switch.log`: Captured live UEFI boot trace in QEMU OVMF (boot time 1248.373 ms @ 2495 MHz).
- `BOOT_TIMES.md`: Recorded Step V6 boot timing metrics.
- `PROGRESS.md`: Updated milestone status for Step V6.

---

## How It Works

### Plain English (Student Viva Explanation)
When you run multiple programs or tasks on a single CPU core, the computer gives the illusion of simultaneous execution by rapidly switching between them. This process is called **multitasking**, and the mechanism that swaps one running task for another is the **context switch**.

In Step V6, VenOS implements:
1. **The Assembly Context Switch:** When switching from Task A to Task B, the CPU cannot just jump to Task B's code immediately—Task A has data stored inside the CPU's physical registers. If Task B overwrites those registers, Task A will crash when it resumes. To prevent this, `switch_context`:
   - Saves Task A's registers (`r15`, `r14`, `r13`, `r12`, `rbx`, `rbp`, `rflags`, and `rsp`) into Task A's `CpuContext` struct.
   - Loads Task B's stack pointer (`rsp`) from Task B's `CpuContext`. The CPU is now using Task B's private stack!
   - Restores Task B's registers and CPU flags from Task B's `CpuContext`.
   - Executes `ret`, which pops Task B's saved instruction pointer (`rip`) off its stack into the CPU, instantly resuming Task B exactly where it was paused.
2. **Cooperative Scheduling (`yield_now`):** Each task periodically yields the CPU voluntarily (`yield_now()`). The scheduler puts the yielding task at the tail of the Ready Queue and picks the task at the head of the queue, creating a fair, rotating cycle: Task 1 -> Task 2 -> Task 3 -> Task 1 ...
3. **Clean Termination & Reaping:** When a task finishes its function, it jumps into `thread_exit_trampoline()`. It marks its state as `Terminated` and yields the CPU forever. Later, the scheduler's garbage collector (`reap_terminated()`) frees the finished task's 16 KiB stack memory.

### Technical Architecture
```
                         CPU Registers
      (r15, r14, r13, r12, rbx, rbp, rflags, rsp, rip)
                           │         ▲
                 mov [rdi] │         │ mov [rsi]
              (save prev)  ▼         │ (restore next)
                ┌───────────────────────────────┐
                │   CpuContext (Task A / Prev)  │
                └───────────────────────────────┘
                ┌───────────────────────────────┐
                │   CpuContext (Task B / Next)  │
                └───────────────────────────────┘

                 [ Ready Queue (FIFO) ]
      Head -> [ Task 1 ] -> [ Task 2 ] -> [ Task 3 ] -> Tail
                 │                          ▲
                 └────── Enqueued to Tail ──┘ (after yield)
```

### Low-Level AMD64 Assembly Routine (`switch_context`)
The assembly routine in `src/scheduler/context_switch.rs` implements the standard System V AMD64 ABI callee-saved preservation protocol:
```asm
.global switch_context
.type switch_context, @function
switch_context:
    // Step 1: Save outgoing thread execution context
    // Argument 1 (rdi): *mut CpuContext (prev thread)
    mov [rdi + 0x00], r15
    mov [rdi + 0x08], r14
    mov [rdi + 0x10], r13
    mov [rdi + 0x18], r12
    mov [rdi + 0x20], rbx
    mov [rdi + 0x28], rbp

    // Save CPU rflags (arithmetic status, interrupt flag, etc.)
    pushfq
    pop rax
    mov [rdi + 0x38], rax

    // Save current stack pointer (rsp)
    mov [rdi + 0x30], rsp

    // Step 2: Restore incoming thread execution context
    // Argument 2 (rsi): *const CpuContext (next thread)
    mov rsp, [rsi + 0x30]        // Switch to target thread's stack

    // Restore rflags
    mov rax, [rsi + 0x38]
    push rax
    popfq

    // Restore callee-saved registers
    mov rbp, [rsi + 0x28]
    mov rbx, [rsi + 0x20]
    mov r12, [rsi + 0x18]
    mov r13, [rsi + 0x10]
    mov r14, [rsi + 0x08]
    mov r15, [rsi + 0x00]

    // Step 3: Resume execution
    ret                         // Pops return address from target stack into rip
```

---

## Verification & Test Results

### 1. Integration Test (`tests/round_robin_switch.rs`)
The test spawns 3 cooperative kernel threads (`task_alpha`, `task_beta`, `task_gamma`) each performing 3 iterations with voluntary `yield_now()`:
```
==================================================
   CA-6 Layer 5: Round Robin & Context Switch Test
==================================================
rr::1_init_scheduler... [ok]
rr::2_spawn_three_tasks...      [ok]
rr::3_executing_cooperative_round_robin_loop...
  [Task 1 / Alpha] Iteration 1/3 running
  [Task 2 / Beta ] Iteration 1/3 running
  [Task 3 / Gamma] Iteration 1/3 running
  [Task 1 / Alpha] Iteration 2/3 running
  [Task 2 / Beta ] Iteration 2/3 running
  [Task 3 / Gamma] Iteration 2/3 running
  [Task 1 / Alpha] Iteration 3/3 running
  [Task 1 / Alpha] Returning from entry function.
  [Task 2 / Beta ] Iteration 3/3 running
  [Task 2 / Beta ] Returning from entry function.
  [Task 3 / Gamma] Iteration 3/3 running
  [Task 3 / Gamma] Returning from entry function.
rr::3_round_robin_loop_completed!       [ok]
rr::4_verify_task_execution_order...    [ok]
rr::5_verify_thread_termination_and_reap...     [ok]
```
- **Execution Order Verification:** `EXEC_ORDER` array matches `[1, 2, 3, 1, 2, 3, 1, 2, 3]` with 100% precision.
- **Reaping Verification:** `reap_terminated()` confirmed all 3 terminated threads reaped and deallocated.

### 2. Live QEMU/OVMF Boot Trace
- **Boot Time:** `BOOT_TIME_MS=1248.373` (calibrated @ 2495 MHz).
- **Log:** `traces/step_v6_round_robin_switch.log`.
- **Framebuffer:** 1280x800 GOP console, 142x50 text grid, interactive shell online.
