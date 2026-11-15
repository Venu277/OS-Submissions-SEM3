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
| **U-01** | `src/sync/spinlock.rs` | 49 | Dereference raw pointer from `UnsafeCell::get()` to return mutable reference `&mut T` in `SpinlockGuard`. | Atomic CAS (`compare_exchange`) succeeded with `Ordering::Acquire`, guaranteeing exclusive mutual exclusion. | `test_spinlock_basic_lock_unlock` unit test & `ca6_demo` integration test. |
| **U-02** | `src/memory.rs` | 35 | Access active 4-level page table root via physical memory offset mapping. | Bootloader provided valid identity-mapped physical memory offset. | `test_box_allocation`, `test_vec_allocation`. |
| **U-03** | `src/allocator.rs` | 40 | Initialize heap range with page table mapping and frame allocation. | Heap virtual range `[0x_4444_4444_0000 .. +256KiB]` is unoccupied. | QEMU boot test, dynamic allocator stress testing. |
| **U-04** | `src/interrupts.rs` | 31 | Load Interrupt Descriptor Table via `lidt` instruction and initialize 8259 PIC. | IDT entries contain valid segment selectors and interrupt handler addresses. | `test_breakpoint_exception`, PIT IRQ0 preemption triggers. |

---

## Safety Invariant Verification Summary

1. **Zero Unsoundness:** All `unsafe` blocks are encapsulated strictly behind safe abstractions (`Spinlock`, `RankedLock`, `KThread`).
2. **Comment Enforcement:** Every `unsafe` block in new modules contains an explicit `// SAFETY:` rationale comment.
3. **Automated Validation:** All unsafe-backed primitives are tested under QEMU test harnesses.
