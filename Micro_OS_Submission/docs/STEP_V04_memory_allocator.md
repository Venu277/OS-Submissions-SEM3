# Step V04: Self-Written Allocator Verification & Layer 8 Hardening

## Goal
Harden and verify the dynamic memory allocation subsystem according to CA-6 Layer 8 and Track B requirements:
1. **Self-Written Allocator Implementation:** Verify that the kernel's dynamic memory allocator is 100% self-written in Rust without external heap crate dependencies:
   - Primary Allocator: `src/allocator/linked_list.rs` (intrusive singly linked list with immediate memory reuse on deallocation).
   - Baseline Comparison Allocator: `src/allocator/bump.rs` (fast $O(1)$ pointer-advance sequential allocator).
2. **Dual-Bootloader Physical Frame Allocation:** Ensure `BootInfoFrameAllocator` in `src/memory.rs` transparently accepts both modern UEFI `&'static bootloader_api::info::MemoryRegions` and legacy BIOS `&'static bootloader::bootinfo::MemoryMap` via the generic trait `IntoFrameSource`.
3. **Graceful Out-of-Memory (OOM) Handling:** Verify that both allocators handle oversized allocation requests gracefully by returning a null pointer (`ptr::null_mut()`) rather than panicking or faulting.
4. **Empirical Workload Benchmark:** Execute the 100-block allocation comparison benchmark measuring CPU cycle latencies, fragmentation resistance, and memory reclamation:
   - Linked List Average Latency: **12,286 CPU cycles**.
   - Bump Allocator Average Latency: **2,424 CPU cycles** ($5.07\times$ faster).
   - Memory Reclaimed after freeing 50% of blocks: Linked List = **9,920 Bytes** reclaimed; Bump = **0 Bytes** (cannot reclaim non-tail allocations).
5. **Repeatable Integration Testing:** Pass `tests/heap_allocation.rs` (simple box, large vector, many boxes, long-lived box, memory reuse) and `tests/allocator_benchmark.rs` in QEMU.
6. **Live UEFI Verification:** Verify kernel boot in QEMU OVMF with adaptive heap sizing (1720 KiB mapped), reaching shell readiness in **559.001 ms**.

---

## CA-6 Sections Addressed
- **Section 9 (Layer 8 – Dynamic Memory Management):** Self-written memory allocator implementing `core::alloc::GlobalAlloc`, heap initialization, free list traversal, and memory reuse.
- **Section 10 (Core Project Requirements):** No external allocator crates (`linked_list_allocator` or `buddy_alloc` are strictly excluded). Hand-written data structures.
- **Section 11 (Track B – Memory Subsystem Option):** Empirical comparison between intrusive free-list and sequential bump allocator, including latency measurements, fragmentation behavior, and reuse metrics.
- **Section 34 (Repeatable Tests):** Test Case TC5 (100 allocations, 50% freed, verification of reuse, oversized request OOM test).

---

## Files Changed
- `src/memory.rs`: Implemented `FrameSource` enum and `IntoFrameSource` trait, enabling `BootInfoFrameAllocator::init()` to consume both modern UEFI `MemoryRegions` and legacy BIOS `MemoryMap` across integration tests.
- `src/allocator/linked_list.rs`: Verified intrusive singly-linked list allocator; verified pointer alignment, node splitting, and `free_memory()` tracking.
- `src/allocator/bump.rs`: Verified sequential bump allocator; verified $O(1)$ pointer advancement, graceful OOM null-return, and full reset on zero active allocations.
- `src/allocator.rs`: Verified `benchmark_allocator_comparison()`, adaptive heap calculation, and atomic heap size tracking.
- `tests/heap_allocation.rs`: Verified passing in QEMU under unified `BootInfoFrameAllocator`.
- `tests/allocator_benchmark.rs`: Verified passing in QEMU under unified `BootInfoFrameAllocator`.
- `traces/step_v4_memory_allocator.log`: Captured live UEFI boot trace in QEMU OVMF (boot time 559.001 ms @ 2498 MHz).
- `BOOT_TIMES.md`: Recorded Step V4 boot time metrics.
- `PROGRESS.md`: Updated milestone status for Step V4.

---

## How It Works

### Plain English (Student Viva Explanation)
When an operating system runs user or kernel programs, it frequently needs dynamic data structures whose sizes cannot be known at compile time (such as dynamic vectors `Vec`, strings `String`, and heap boxes `Box`). The standard library relies on an underlying allocator to manage a large region of RAM called the **heap**.

In university operating systems courses, students often make the mistake of pulling in an external open-source crate (such as `linked_list_allocator`). In VenOS, to strictly comply with CA-6 Layer 8:
1. **Intrusive Singly-Linked List Allocator:** We store metadata directly inside the freed memory blocks themselves. Each unused block of memory contains a small `ListNode` header specifying its byte size and a pointer to the next free block. When `alloc(size, align)` is called, the allocator walks the list looking for a block large enough. If the block is larger than requested, it splits the block, returns the allocated portion, and keeps the remainder in the free list. When `dealloc()` is called, it prepends the freed block back into the free list.
2. **Sequential Bump Allocator (Comparison Baseline):** For comparison, we also wrote a bump allocator. It maintains a single integer pointer (`next`). Every allocation simply increments the pointer forward: $O(1)$ constant time! It is over $5\times$ faster than the linked list. However, because it only advances in one direction, freeing an individual object in the middle cannot reclaim memory. Only when *every* allocated block is freed can the pointer reset to the beginning of the heap.
3. **Out-of-Memory Safety:** If a program asks for 1 GB on a 2 MB heap, both allocators check their bounds and return a null pointer (`0x0`) instead of crashing the kernel.

### Technical Comparison
| Metric | Intrusive Linked List (`LinkedListAllocator`) | Sequential Bump (`BumpAllocator`) |
|--------|-----------------------------------------------|-----------------------------------|
| **Allocation Latency** | 12,286 CPU cycles ($O(N)$ free list search) | 2,424 CPU cycles ($O(1)$ pointer addition) |
| **Speed Multiplier** | Baseline | **5.07x faster** |
| **Memory Reclamation** | **100% of freed blocks** (9,920 Bytes reused) | **0% of freed blocks** (cannot reclaim non-tail) |
| **Fragmentation** | Resists external fragmentation via reuse | Severe external fragmentation under mixed lifetimes |
| **OOM Handling** | Returns `null_mut()` cleanly | Returns `null_mut()` cleanly |
| **Primary Use in VenOS** | Kernel `GlobalAlloc` general-purpose heap | Scratch buffers, short-lived tasks, temporary metrics |

---

## Verification & Trace Evidence

### 1. Repeatable Integration Test: `tests/allocator_benchmark.rs`
Executed via `cargo test --test allocator_benchmark`:
```text
==================================================
   CA-6 Layer 8 & Track B: Allocator Benchmark
==================================================
alloc::1_linked_list_allocation_and_reuse...	[ok]
alloc::2_bump_allocator_sequential_and_reset...	[ok]
alloc::3_graceful_oom_handling...	[ok]
alloc::4_benchmark_100_blocks_comparison...	[ok] (LL_avg=12286 cyc, Bump_avg=2424 cyc, LL_reused=9920 B, Bump_reused=0 B)
==================================================
CA-6 Layer 8 Allocator Benchmark: ALL PASSED
==================================================
```

### 2. Repeatable Integration Test: `tests/heap_allocation.rs`
Executed via `cargo test --test heap_allocation`:
```text
heap_allocation::simple_allocation...	[ok]
heap_allocation::large_vec...	[ok]
heap_allocation::many_boxes...	[ok]
heap_allocation::many_boxes_long_lived...	[ok]
heap_allocation::memory_reuse...	[ok]
```

### 3. Live UEFI Boot in QEMU OVMF (`traces/step_v4_memory_allocator.log`)
- Kernel ELF size: 10,376,464 bytes (9.9 MB).
- UEFI GPT FAT32 Image: 12,648,448 bytes (12.06 MB).
- Memory detected: 12,415 MiB total, 107 MiB usable.
- Adaptive Kernel Heap allocated: **1,720 KiB** ($1/64$th of usable RAM).
- Calibrated TSC frequency: **2498 MHz**.
- Boot time to shell prompt: **559.001 ms**.

---

## Gap Analysis Matrix Status After Step V4

| Gap ID | Description | Status | Resolution |
|--------|-------------|--------|------------|
| **GAP-12** | Self-written allocator verification | **RESOLVED** | Verified `src/allocator/linked_list.rs` and `src/allocator/bump.rs` are 100% hand-written without external crate dependencies. |
| **GAP-13** | Dual-bootloader frame allocator support | **RESOLVED** | Implemented `IntoFrameSource` trait supporting both UEFI `MemoryRegions` and BIOS `MemoryMap` seamlessly. |
| **GAP-14** | TC5 Allocator Test Case Verification | **RESOLVED** | Verified 100-block workload, 50% deallocation memory reuse (9,920 B), and graceful OOM handling. |

---

## Next Steps
Proceed to **Step V5: Task Representation and Ready Queue (CA-6 Layers 4 & 5)**:
- Verify `Task` / `KThread` representation: Task ID, Task State (`Ready`, `Running`, `Blocked`, `Terminated`), saved CPU registers/context, stack pointer, and kernel stack allocation with stack guard.
- Verify thread-safe `ReadyQueue` priority / round-robin scheduling queues.
- Author `docs/STEP_V05_task_ready_queue.md`.
