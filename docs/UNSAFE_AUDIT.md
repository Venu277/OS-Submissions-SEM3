# MicroOS — Unsafe Rust Audit Log

> **Specification Reference:** CA-6 Section 26 (Unsafe Rust Policy & Encapsulation)
>
> **Policy:** Every `unsafe` block must state:
> 1. Exact file and line location.
> 2. Architectural reason why `unsafe` is mandatory.
> 3. Documented safety invariants and preconditions.
> 4. Verification & testing methodology.

---

## Audit Table

| ID | Module / File | Line(s) | Architectural Reason | Invariant / Precondition | Validation Method |
|:---|:---|:---|:---|:---|:---|
| **U-01** | `src/sync/spinlock.rs` | 88, 126 | Dereference raw pointer from `UnsafeCell::get()` to return mutable reference `&mut T` in `SpinlockGuard`. | Atomic CAS (`compare_exchange`) succeeded with `Ordering::Acquire`, guaranteeing exclusive mutual exclusion. | `test_spinlock_basic_lock_unlock`, `tests/concurrency_sync.rs`. |
| **U-02** | `src/memory.rs` | 35 | Access active 4-level page table root via physical memory offset mapping. | Bootloader provided valid identity-mapped physical memory offset. | `test_box_allocation`, `test_vec_allocation`. |
| **U-03** | `src/allocator.rs` | 40 | Initialize heap range with page table mapping and frame allocation. | Heap virtual range `[0x_4444_4444_0000 .. +256KiB]` is unoccupied. | QEMU boot test, dynamic allocator stress testing. |
| **U-04** | `src/interrupts.rs` | 31 | Load Interrupt Descriptor Table via `lidt` instruction and initialize 8259 PIC. | IDT entries contain valid segment selectors and interrupt handler addresses. | `test_breakpoint_exception`, PIT IRQ0 preemption triggers. |
| **U-05** | `src/kthread/mod.rs` | 108 | Write initial return addresses (`kthread_exit` and `entry_point`) to top of thread stack buffer. | Pointer arithmetic remains strictly within the allocated bounds of the 16 KiB stack buffer `[stack_top - 16 .. stack_top]`. | `test_kthread_initialization`, `round_robin_switch` test. |
| **U-06** | `src/scheduler/mod.rs` | 187 | Invoke assembly `switch_context` during cooperative `yield_now()`. | `prev_ptr` and `next_ptr` point to valid, aligned, heap-pinned `CpuContext` structs inside `Box<KThread>`. Interrupts disabled. | `round_robin_switch` integration test, exact sequence verified. |
| **U-07** | `src/scheduler/mod.rs` | 231 | Invoke assembly `switch_context` during thread exit `exit_current_thread()`. | Outgoing thread marked `Terminated`, incoming thread marked `Running`, valid stack pointers restored. | `round_robin_switch` task completion and reaping test. |
| **U-08** | `src/interrupts.rs` | 118 | Write command byte and 16-bit divisor to PIT 8253 I/O ports 0x43 and 0x40. | Writing valid mode 0x34 and divisor clamped to `[1, 65535]`; does not touch unmapped memory. | 100 Hz timer tick frequency verified in QEMU PIT IRQ0 handler. |
| **U-09** | `src/scheduler/mod.rs` | 300 | Invoke assembly `switch_context` during involuntary timer preemption `timer_tick()`. | `prev_ptr` and `next_ptr` point to valid heap-pinned `CpuContext` structs. Executed in interrupt context (IF=0). | `tests/preemptive_scheduling.rs` compute-bound preemption test. |

---

## Safety Invariant Verification Summary

1. **Zero Unsoundness:** All `unsafe` blocks are encapsulated strictly behind safe abstractions (`Spinlock`, `RankedLock`, `KThread`).
2. **Comment Enforcement:** Every `unsafe` block in new modules contains an explicit `// SAFETY:` rationale comment.
3. **Automated Validation:** All unsafe-backed primitives are tested under QEMU test harnesses.
