# MicroOS — CA-6 Viva Voce Preparation Guide
## 20 Anticipated Examination Questions & Model Technical Answers

> **Course:** Operating System Concepts — Continuous Assessment 6 (CA-6)  
> **Target:** Viva Defense for 15 Marks  
> **Student:** Venugopal ([GitHub @Venu277](https://github.com/Venu277))  
> **Repository:** `https://github.com/Venu277/micro_os-continued.git`  
> **Master Trace:** `traces/step_a8_demo.log` (569 trace events, 715 timer ticks)

---

## Part 1: Core Architecture & Scheduling (Q1 – Q5)

### Q1: How does MicroOS represent a task, and why does an async Rust executor fail to satisfy CA-6 requirements?
**Model Answer:**
- In MicroOS, tasks are represented by the `KThread` struct ([src/kthread/mod.rs#L80-L88](file:///c:/Users/Venugopal/OneDrive/Desktop/micro_os-main/src/kthread/mod.rs#L80-L88)). A true kernel thread requires:
  1. A dedicated execution stack (16 KiB allocated from the heap in `stack: Vec<u8>`).
  2. A preserved hardware CPU register context (`CpuContext` containing callee-saved registers `rsp`, `rbp`, `rbx`, `r12`–`r15`, and `rflags`).
  3. An explicit 4-state lifecycle machine (`Ready`, `Running`, `Blocked`, `Terminated`).
- An **async Rust executor** (such as our earlier cooperative task runner in `src/task/executor.rs`) only multiplexes compiler-generated state machine `Future`s across a single shared stack. When a future awaits, it pauses, but if a task runs an infinite compute loop (`loop {}`), it monopolizes the CPU forever because there is no separate stack or timer-driven register swap. CA-6 explicitly requires true preemptive threads with saved register contexts.

### Q2: Walk through the exact mechanism of the assembly context switch.
**Model Answer:**
- The context switch is implemented in System V AMD64 assembly within [src/scheduler/context_switch.rs#L14-L44](file:///c:/Users/Venugopal/OneDrive/Desktop/micro_os-main/src/scheduler/context_switch.rs#L14-L44) via `switch_context(prev: *mut CpuContext, next: *const CpuContext)`.
- According to AMD64 calling conventions, the caller passes `prev` in `%rdi` and `next` in `%rsi`.
- `switch_context` performs:
  1. `pushfq ; pop rax` and stores `rax` into `prev->rflags` (saving hardware interrupt flags).
  2. Saves callee-saved registers into `prev`: `rbp`, `rbx`, `r12`, `r13`, `r14`, `r15`.
  3. Saves the current stack pointer: `mov [rdi + 48], rsp`.
  4. Swaps to the new thread's stack pointer: `mov rsp, [rsi + 48]`.
  5. Restores callee-saved registers from `next`: `r15`, `r14`, `r13`, `r12`, `rbx`, `rbp`.
  6. Restores interrupt flags: `push [rsi + 56] ; popfq`.
  7. Executes `ret`. The `ret` instruction pops the top address off the new stack (which was primed with the thread's entry point) into the instruction pointer `%rip`, resuming execution in the incoming thread.

### Q3: How does MicroOS prevent and detect kernel stack overflows?
**Model Answer:**
- In [src/kthread/mod.rs#L95-L98](file:///c:/Users/Venugopal/OneDrive/Desktop/micro_os-main/src/kthread/mod.rs#L95-L98), upon thread creation, MicroOS plants a 64-bit magic stack canary (`0xCA60_DEAD_BEEF_0001`) at the lowest address of the allocated 16 KiB stack buffer (`stack[0..8]`).
- Because x86_64 stacks grow downwards from the highest address (`stack_top`) toward `stack[0]`, any deep call chain or large stack buffer allocation will overwrite this lowest boundary.
- Before executing context switches or state transitions, the scheduler calls `t.check_stack_overflow()` ([src/kthread/mod.rs#L145-L151](file:///c:/Users/Venugopal/OneDrive/Desktop/micro_os-main/src/kthread/mod.rs#L145-L151)), which compares `stack[0..8]` against the magic bytes. If altered, execution is halted, preventing silent memory corruption. This is validated by TC3 and TC5 in [tests/kthread_ready_queue.rs](file:///c:/Users/Venugopal/OneDrive/Desktop/micro_os-main/tests/kthread_ready_queue.rs).

### Q4: Explain the Ready Queue invariants and time complexity.
**Model Answer:**
- The Ready Queue ([src/scheduler/mod.rs#L37-L122](file:///c:/Users/Venugopal/OneDrive/Desktop/micro_os-main/src/scheduler/mod.rs#L37-L122)) is implemented as an encapsulated FIFO deque (`VecDeque<Box<KThread>>`).
- Operations:
  - `enqueue(thread)`: Appends to the back in $O(1)$ amortized time.
  - `dequeue()`: Pops from the front in $O(1)$ time, maintaining strict Round Robin order.
  - `peek()` / `peek_mut()`: Inspects the head in $O(1)$ time without removal.
  - `remove(id)`: Removes a specific thread (e.g. for blocking or reaping) in $O(N)$ linear time.
- Invariants: Threads in the Ready Queue must have state `ThreadState::Ready` and unique `ThreadId`s. Enqueuing an already enqueued ID or an invalid state is prevented.

### Q5: What happens when a kernel thread finishes its entry function?
**Model Answer:**
- To prevent a thread from returning into arbitrary unmapped memory, [src/kthread/mod.rs#L107-L114](file:///c:/Users/Venugopal/OneDrive/Desktop/micro_os-main/src/kthread/mod.rs#L107-L114) initializes the new thread's stack with a return trampoline:
  ```text
  [stack_top - 8]  <- kthread_exit address
  [stack_top - 16] <- entry_point address
  ```
- When the thread returns from its entry function via `ret`, it pops `kthread_exit` ([src/scheduler/mod.rs#L273](file:///c:/Users/Venugopal/OneDrive/Desktop/micro_os-main/src/scheduler/mod.rs#L273)).
- `kthread_exit` immediately calls `exit_current_thread()` ([src/scheduler/mod.rs#L306-L344](file:///c:/Users/Venugopal/OneDrive/Desktop/micro_os-main/src/scheduler/mod.rs#L306-L344)), which transitions the thread to `Terminated`, pushes it to `terminated_queue`, selects the next ready thread, and context-switches away. Later, `reap_terminated()` frees the thread's memory.

---

## Part 2: Interrupts, Hardware Timing & Preemption (Q6 – Q10)

### Q6: How is the hardware timer configured, and what is the interrupt frequency?
**Model Answer:**
- MicroOS programs the Intel 8253/8254 Programmable Interval Timer (PIT) Channel 0 in Mode 3 (Square Wave Generator) in [src/interrupts.rs#L112-L124](file:///c:/Users/Venugopal/OneDrive/Desktop/micro_os-main/src/interrupts.rs#L112-L124).
- The PIT base oscillator runs at $1,193,182\text{ Hz}$. MicroOS calculates the 16-bit divisor:
  $$\text{Divisor} = \frac{1,193,182}{100} = 11,931 \ (\text{hex } \texttt{0x2E9B})$$
- This generates PIT IRQ0 interrupts at exactly **100 Hz** (1 interrupt every **10.0 ms**).

### Q7: How does MicroOS calibrate the Time Stamp Counter (TSC)?
**Model Answer:**
- In [src/interrupts.rs#L130-L165](file:///c:/Users/Venugopal/OneDrive/Desktop/micro_os-main/src/interrupts.rs#L130-L165) (`calibrate_tsc()`), the kernel uses PIT Channel 2 (gate-controlled speaker channel) to measure an exact 10 ms hardware delay window while sampling `_rdtsc()`.
- Measuring the TSC delta across the known hardware interval yields an accurate clock frequency:
  $$\text{TSC Frequency} = \Delta \text{TSC} \times 100\text{ Hz} = 2,502\text{ MHz}$$
- This calibrated frequency is used in `src/main.rs` to compute sub-millisecond boot times without drift.

### Q8: Describe the preemption sequence when a thread's time slice expires.
**Model Answer:**
- In [src/interrupts.rs#L77-L86](file:///c:/Users/Venugopal/OneDrive/Desktop/micro_os-main/src/interrupts.rs#L77-L86), when PIT IRQ0 fires:
  1. The CPU automatically pushes `ss`, `rsp`, `rflags`, `cs`, `rip` onto the TSS IST stack.
  2. The interrupt handler calls `crate::scheduler::timer_tick()`.
  3. In [src/scheduler/mod.rs#L225-L270](file:///c:/Users/Venugopal/OneDrive/Desktop/micro_os-main/src/scheduler/mod.rs#L225-L270), `timer_tick()` decrements `current.time_slice_remaining`.
  4. When the quantum reaches 0, it resets the quantum, transitions `current` to `Ready`, pushes it to the back of `ready_queue`, and dequeues the next `Ready` thread.
  5. It emits `[TRACE] tick=N event=QUANTUM_EXPIRE` and `event=SWITCH ... reason=quantum`.
  6. Finally, it executes `context_switch::switch_context(prev_ptr, next_ptr)` inside the interrupt handler before sending EOI to the PIC.

### Q9: What is the measured preemption responsiveness of MicroOS?
**Model Answer:**
- In `traces/step_a8_demo.log` (lines 245–248):
  ```text
  [TRACE] tick=366 event=QUANTUM_EXPIRE task=88 consumed=3
  [TRACE] tick=366 event=TASK_STATE task=88 state=READY
  [TRACE] tick=366 event=TASK_STATE task=89 state=RUNNING
  [TRACE] tick=366 event=SWITCH from=88 to=89 reason=quantum
  ```
- The time difference between `QUANTUM_EXPIRE` and `SWITCH` is **0 ticks** ($< 10\text{ ms}$). Preemption occurs synchronously within the exact same timer tick, achieving immediate preemption responsiveness.

### Q10: How does `PreemptionGuard` prevent reentrant preemption during critical sections?
**Model Answer:**
- In [src/scheduler/mod.rs#L30-L36](file:///c:/Users/Venugopal/OneDrive/Desktop/micro_os-main/src/scheduler/mod.rs#L30-L36), preemption is guarded by the atomic flag `PREEMPTION_ENABLED`.
- When the kernel enters a critical section or non-preemptive benchmark, it instantiates `PreemptionGuard::new()`, which sets `PREEMPTION_ENABLED` to `false`.
- When `timer_tick()` fires, it inspects `is_preemption_enabled()`. If false, it updates accounting ticks but bypasses quantum depletion and context switching.
- When `PreemptionGuard` drops, its RAII destructor automatically restores preemption.

---

## Part 3: Synchronization, Locks & Track C Concurrency (Q11 – Q15)

### Q11: How is the hand-written `Spinlock<T>` implemented without deadlocking itself?
**Model Answer:**
- In [src/sync/spinlock.rs#L20-L135](file:///c:/Users/Venugopal/OneDrive/Desktop/micro_os-main/src/sync/spinlock.rs#L20-L135), `Spinlock<T>` wraps an `UnsafeCell<T>` with an `AtomicBool` locked flag.
- Acquisition uses atomic compare-and-swap (`compare_exchange_weak`) with `Acquire` ordering on success and `Relaxed` on failure.
- If locked, it executes a tight spin loop with `core::hint::spin_loop()` (emitting the x86 `pause` instruction) while tracking spin count metrics.
- Upon acquiring the lock, it measures hold duration and returns an RAII `SpinlockGuard` that automatically resets `locked` with `Release` ordering on `Drop`.

### Q12: How does Track C strict partial order lock ranking eliminate deadlocks?
**Model Answer:**
- Deadlocks occur when there is a circular wait (Task A holds Lock 1 and waits for Lock 2; Task B holds Lock 2 and waits for Lock 1).
- In [src/sync/ranked.rs#L20-L95](file:///c:/Users/Venugopal/OneDrive/Desktop/micro_os-main/src/sync/ranked.rs#L20-L95), every `RankedLock<T>` is assigned an immutable integer `rank: u32`.
- When a thread attempts to acquire a lock via `try_lock_ranked()`, the method verifies:
  $$\text{Requested Rank } R_{\text{req}} > \text{Currently Held Rank } R_{\text{held}}$$
- If a thread holding Rank 20 attempts to acquire Rank 10, the operation is immediately rejected with `Err(LockRankError::HierarchyViolation)`.
- Because locks can only be acquired in strictly increasing numerical order, a cyclic dependency cannot form.

### Q13: Quote the exact trace log evidence of deadlock prevention in Step A8.
**Model Answer:**
- In `traces/step_a8_demo.log` (lines 328–330):
  ```text
  [TRACE] tick=476 event=DEADLOCK_PREVENT task=90 req_rank=10 held_rank=20
  [CA6-DEADLOCK] Out-of-order acquisition prevented: Lock ranking violation: requested rank 10 <= held rank 20 (deadlock prevented)
  ```
- Task 90 held `vfs_lock` (Rank 20) and attempted to acquire `driver_lock` (Rank 10). The acquisition was rejected cleanly, preventing a circular wait condition.

### Q14: What lock contention metrics does MicroOS measure?
**Model Answer:**
- MicroOS tracks four atomic metrics inside `Spinlock<T>` ([src/sync/spinlock.rs#L25-L35](file:///c:/Users/Venugopal/OneDrive/Desktop/micro_os-main/src/sync/spinlock.rs#L25-L35)):
  1. `acquisitions`: Total successful lock acquisitions.
  2. `contentions`: Times the lock was found already held on first attempt.
  3. `spins_waited`: Accumulated pause iterations spent waiting.
  4. `max_spins`: Maximum wait latency observed for a single acquisition.
- In `traces/step_a6_sync.log`, under two competing threads, the contention rate reached **50.0%** with a maximum wait latency of **31,520 spins**.

### Q15: How does the RAII `RankedLockGuard` manage nested lock releases?
**Model Answer:**
- When `RankedLockGuard` is instantiated ([src/sync/ranked.rs#L50-L60](file:///c:/Users/Venugopal/OneDrive/Desktop/micro_os-main/src/sync/ranked.rs#L50-L60)), it stores `previous_rank: u32` representing the rank held prior to acquiring this lock.
- When the guard drops, its `Drop` implementation restores `CURRENT_HELD_RANK` to `previous_rank`.
- This ensures that when nested locks unwind (e.g. Rank 10 -> Rank 20 -> Rank 30), the held rank decreases monotonically in LIFO order, allowing sibling acquisitions at lower ranks once outer locks are released.

---

## Part 4: Memory Management, Paging & Allocators (Q16 – Q18)

### Q16: How does MicroOS manage kernel memory and paging?
**Model Answer:**
- MicroOS runs in 64-bit Long Mode with 4-level paging ([src/memory.rs#L20-L55](file:///c:/Users/Venugopal/OneDrive/Desktop/micro_os-main/src/memory.rs#L20-L55)).
- The bootloader passes an identity-mapped physical memory offset. `memory::init()` reads CR3 to access the active Level 4 page table.
- A 256 KiB virtual heap `[0x4444_4444_0000 .. +256KiB]` is mapped using `BootInfoFrameAllocator` frames and registered under Rust's `#[global_allocator]`.

### Q17: Compare the algorithmic trade-offs between the Linked List Allocator and Bump Allocator.
**Model Answer:**
- **Bump Allocator (`src/allocator/bump.rs`):**
  - *Mechanism:* Advances a single `next` pointer linearly.
  - *Speed:* Ultra-fast $O(1)$ allocation (**47,040 cycles** vs 236,574 cycles; **5.03x speedup**).
  - *Limitation:* Cannot reclaim intermediate freed blocks; memory reuse is **0 Bytes** until all allocations are freed.
- **Intrusive Linked List Allocator (`src/allocator/linked_list.rs`):**
  - *Mechanism:* Maintains a free list threaded through unallocated memory nodes.
  - *Speed:* Slower $O(N)$ traversal (**236,574 cycles**).
  - *Advantage:* Reclaims arbitrary non-tail freed blocks immediately (**9,920 Bytes reused, 100% reclamation**).

### Q18: What is the significance of the structured allocation traces?
**Model Answer:**
- MicroOS emits `[TRACE] tick=N event=ALLOC size=S ok=1` and `event=DEALLOC size=S` over serial port `0x3F8`.
- In `traces/step_a8_demo.log`, the analyzer parsed **200 allocations** and **100 deallocations** totaling 39,680 Bytes across 5 size classes (32, 64, 128, 256, 512 Bytes).
- Because tracing writes directly to the UART hardware register without calling `alloc`, it provides zero-allocation observability into kernel heap activity without perturbing heap state.

---

## Part 5: The 5 Toughest External Examiner Questions (Q19 – Q20 + 3 Technical Probes)

### Q19 (Examiner Question): Why does your `switch_context` assembly assume a 16-byte stack alignment, and what catastrophic bug occurs if it is 8-byte aligned?
**Model Answer:**
- The System V AMD64 ABI specification requires that the stack pointer `%rsp` must be **16-byte aligned immediately before a `call` instruction is executed**. When `call` pushes the 8-byte return address, `%rsp` enters the callee aligned to $16k + 8$.
- Inside the callee, if SSE/AVX vector instructions (e.g. `movaps`, `movdqa`) or floating-point compiler-generated code run on a stack variable that is not 16-byte aligned, the CPU triggers a **General Protection Fault (`#GP`, Exception 13)** and crashes.
- In [src/kthread/mod.rs#L102-L106](file:///c:/Users/Venugopal/OneDrive/Desktop/micro_os-main/src/kthread/mod.rs#L102-L106), MicroOS explicitly aligns `stack_top` to 16 bytes:
  $$\text{aligned\_top} = \text{raw\_top} \ \& \ \sim 15$$
  Then, pushing `kthread_exit` (8 bytes) and `entry_point` (8 bytes) places `%rsp` exactly at a 16-byte boundary, guaranteeing `#GP` exceptions can never occur.

### Q20 (Examiner Question): Can Rust's compile-time type system guarantee deadlock freedom without runtime lock ranking?
**Model Answer:**
- Rust's type system (`Send` and `Sync`) guarantees **data-race freedom**, but it **does not guarantee deadlock freedom**.
- Compile-time deadlock prevention is possible in Rust only if lock acquisitions are encoded into affine types or session types (e.g. passing a token `Token<Rank10>` to acquire `Token<Rank20>`), but this restricts dynamic, data-dependent locking orders and recursive data structures.
- MicroOS combines Rust's compile-time RAII `Drop` safety (guaranteeing locks are never leaked) with a lightweight runtime ranking check (`try_lock_ranked()`). This rejects out-of-order acquisitions in $O(1)$ time while retaining full runtime flexibility.

### Q21 (Technical Probe): During Step A8, you encountered an allocation failure when allocating 32 KiB on a 256 KiB heap. What caused this, and why was it not a memory leak?
**Model Answer:**
- The failure was caused by **external memory fragmentation in the intrusive linked list allocator**, not a memory leak.
- After running TC1–TC6, round-robin, preemption, and synchronization demos, multiple 16 KiB thread stacks were allocated and freed. Because our linked list `add_free_region` ([src/allocator/linked_list.rs#L51-L56](file:///c:/Users/Venugopal/OneDrive/Desktop/micro_os-main/src/allocator/linked_list.rs#L51-L56)) prepended freed chunks to the head without contiguous block coalescing, the heap had over 200 KiB of total free memory, but the largest contiguous chunk was only 16 KiB.
- When the benchmark requested a single contiguous 32 KiB buffer, no single node could satisfy the request.
- **Engineering Fix:** We resolved this by allocating `TEST_HEAP_BUFFER` in the kernel's BSS segment ([src/allocator.rs#L135](file:///c:/Users/Venugopal/OneDrive/Desktop/micro_os-main/src/allocator.rs#L135)) with `#[repr(align(16))]`, decoupling benchmark test harnesses from dynamic kernel heap churn.

### Q22 (Technical Probe): Why is a spinlock unsafe to use in an interrupt handler if interrupts are not disabled?
**Model Answer:**
- If Thread A acquires Spinlock $L$ in thread context, and a hardware timer interrupt fires on the same CPU core before Thread A releases $L$:
- The interrupt service routine (ISR) begins executing on the same CPU and attempts to acquire Spinlock $L$.
- The ISR enters a spin loop waiting for `locked` to become false. However, Thread A cannot execute because the ISR is preempting it on the same core.
- The core enters an **infinite unrecoverable live-lock**.
- MicroOS prevents this in [src/scheduler/mod.rs#L310](file:///c:/Users/Venugopal/OneDrive/Desktop/micro_os-main/src/scheduler/mod.rs#L310) by wrapping scheduler critical sections in `x86_64::instructions::interrupts::without_interrupts(|| { ... })`, disabling the interrupt flag (`IF=0`) before acquiring spinlocks.

### Q23 (Technical Probe): If an external examiner asks you to demonstrate that timer preemption is truly involuntary and not cooperative, which trace and code do you show?
**Model Answer:**
1. **The Code:** Point to `preempt_worker_alpha` in [src/scheduler/mod.rs#L446-L459](file:///c:/Users/Venugopal/OneDrive/Desktop/micro_os-main/src/scheduler/mod.rs#L446-L459). Worker Alpha executes a CPU-bound while loop with `spin_loop()`, with **zero calls to `yield_now()` or any blocking function**.
2. **The Hardware Interrupt:** Show `timer_tick()` in [src/interrupts.rs#L83](file:///c:/Users/Venugopal/OneDrive/Desktop/micro_os-main/src/interrupts.rs#L83), called directly by PIT IRQ0.
3. **The Trace Evidence:** Show `traces/step_a8_demo.log` (lines 245–248):
   ```text
   [TRACE] tick=366 event=QUANTUM_EXPIRE task=88 consumed=3
   [TRACE] tick=366 event=SWITCH from=88 to=89 reason=quantum
   ```
   The switch reason is explicitly logged as `reason=quantum`, demonstrating that the CPU was involuntarily confiscated from Worker Alpha by hardware interrupt timer preemption.
