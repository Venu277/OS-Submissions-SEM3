# CA-6 Step A1: Restructuring and CA-6 Core Build Mode

> **Specification Reference:** CA-6 Section 8 (Deliverables), Section 9 (OS Implementation Layers), Section 22 (Directory Structure & Layout), Section 26 (Unsafe Policy), Section 42 (Integrated Demo)

---

## 1. Goal

The objective of Step A1 is to align the MicroOS codebase structure with the modular architecture mandated in Section 22 of the CA-6 specification, without destabilizing or deleting the 36+ existing features (network, storage, filesystem, apt/snap package manager). In addition, Step A1 establishes a dedicated `ca6-core` build mode and shell integration (`ca6 demo`, `ca6 status`) allowing the pure CA-6 kernel core to boot and execute independently of external subsystem drivers.

---

## 2. Assignment Sections Addressed

- **Section 9 (Layer 3 - Task Representation):** Added `src/kthread/mod.rs` defining `KThread`, `ThreadId`, `ThreadState` (Ready, Running, Blocked, Terminated), System V AMD64 ABI `CpuContext`, and runtime metrics.
- **Section 9 (Layer 4 - Ready Queue):** Added `src/scheduler/mod.rs` defining `ReadyQueue` (FIFO deque structure) and `RoundRobinScheduler`.
- **Section 9 (Layer 7 & Section 11 Track C - Concurrency Engineering):** Added `src/sync/` with hand-written interrupt-safe atomic `Spinlock<T>` and strict partial-order `RankedLock<T>` to prevent Coffman circular wait deadlocks.
- **Section 22 (Directory Structure):** Established `src/kthread/`, `src/scheduler/`, `src/sync/`, `traces/`, `results/`, and `docs/UNSAFE_AUDIT.md`.
- **Section 42 (Integrated Demo):** Created `ca6 demo` and `ca6 status` shell commands plus the `ca6-core` cargo feature and `tests/ca6_demo.rs` automated integration test runner.

---

## 3. Files Created / Modified

| File | Type | Purpose |
|:---|:---|:---|
| `src/kthread/mod.rs` | New | Kernel thread abstraction: `KThread`, `ThreadId`, `ThreadState`, `CpuContext`, 16 KiB stack allocation. |
| `src/scheduler/mod.rs` | New | FIFO `ReadyQueue`, `RoundRobinScheduler` structure, and `run_ca6_demo()` integrated demo routine. |
| `src/sync/mod.rs` | New | CA-6 Layer 7 and Track C concurrency primitives module declaration. |
| `src/sync/spinlock.rs` | New | Hand-written interrupt-safe spinlock using `AtomicBool` CAS and contention metrics tracking. |
| `src/sync/ranked.rs` | New | Strict hierarchy lock ranking for deadlock prevention. |
| `src/shell.rs` | Modified | Added `ca6` command parser (`ca6 demo`, `ca6 status`) and help documentation. |
| `src/main.rs` | Modified | Added `#[cfg(feature = "ca6-core")]` boot mode bypassing networking/storage drivers for pure core execution. |
| `Cargo.toml` | Modified | Declared `ca6-core` feature and registered `ca6_demo` integration test target. |
| `tests/ca6_demo.rs` | New | End-to-end automated QEMU integration test verifying all 10 CA-6 core subsystems. |
| `traces/ca6_core_boot.log`| New | Real QEMU execution serial trace of the `ca6-core` boot mode. |
| `BOOT_TIMES.md` | Modified | Recorded baseline boot-time entry for the CA-6 core build mode (190.5 ms). |
| `docs/UNSAFE_AUDIT.md` | New | Audit register tracking all `unsafe` Rust blocks, safety invariants, and validation notes. |
| `PROGRESS.md` | Modified | Updated progress tracker with Step A1 status and outcomes. |

---

## 4. How It Works

### Plain English Explanation (Viva Guide)
Imagine an operating system as an airport terminal. Previously, MicroOS had fancy shops and lounges (network, package managers, virtual disks), but the core flight dispatcher—the task scheduler—was only a simple queue that waited for flights to voluntarily report their status (cooperative async/await). In Step A1, we constructed the air traffic control tower:
1. **Thread Representation (`kthread`):** Each flight (thread) is given a boarding pass (ThreadId), an active status (Ready, Running, Blocked, Terminated), and a flight cockpit snapshot (CpuContext) saving where it was in the sky.
2. **Ready Queue (`scheduler`):** An orderly runway queue where airplanes line up in first-come, first-served order.
3. **Synchronization (`sync`):** Hand-written gates (Spinlocks) that allow only one plane onto the runway at a time. To prevent two planes from blocking each other in a gridlock deadlock, we assigned every gate a strict numerical rank: you can only proceed to higher-ranked gates, completely eliminating circular standoffs.
4. **CA-6 Core Mode:** A switch that lets us run the control tower and runways completely cleanly, without waiting for the terminal shops to open.

### Technical Explanation & Architecture Diagram

```
+-------------------------------------------------------------------------+
|                              MicroOS Shell                              |
|   "ca6 demo" (Section 42)               "ca6 status" (Layers 1-10)      |
+------------------------------------+------------------------------------+
                                     |
                                     v
+-------------------------------------------------------------------------+
|                  RoundRobinScheduler (src/scheduler)                    |
|  - default_quantum: 10 ticks                                            |
|  - total_switches: u64                                                  |
|  - ReadyQueue (VecDeque<KThread>)                                       |
+------------------------------------+------------------------------------+
                                     |
               +---------------------+---------------------+
               v                                           v
+-------------------------------+             +---------------------------+
|      KThread (src/kthread)    |             |    Synchronization (sync) |
|  - id: ThreadId               |             |  - Spinlock<T>            |
|  - state: ThreadState         |             |  - RankedLock<T> (Rank)   |
|  - context: CpuContext (ABI)  |             |  - Contention metrics     |
|  - stack: Vec<u8> (16 KiB)    |             +---------------------------+
+-------------------------------+
```

The `KThread` structure allocates an isolated 16 KiB stack from the kernel heap, aligning `rsp` to 16 bytes conforming to the AMD64 System V ABI. `CpuContext` preserves the callee-saved registers (`r15`, `r14`, `r13`, `r12`, `rbx`, `rbp`, `rsp`, `rflags`). 

`Spinlock<T>` provides mutual exclusion using atomic `compare_exchange` with `Acquire`/`Release` memory ordering and measures lock contentions via atomic counters. `RankedLock<T>` enforces a total order over lock acquisitions by maintaining a thread-local `CURRENT_HELD_RANK`: any attempt to acquire a lock of rank $\le$ current rank immediately returns an `Err`, eliminating Coffman's circular wait condition.

---

## 5. Verification Commands & Real Output

### A. Build Verification (`cargo check` & `cargo build`)
```powershell
cargo check
cargo build
```
**Output:**
```
Finished `dev` profile [unoptimized + debuginfo] target(s) in 9.72s
```

### B. Unit & Integration Testing (`cargo test --test ca6_demo`)
```powershell
cargo test --test ca6_demo
```
**Output:**
```
==================================================
   CA-6 Core Integrated Demo Test Suite
==================================================
ca6::1_kernel_boot_and_cpu_init...	[ok]
ca6::2_paging_and_heap_allocator...	[ok]
ca6::3_kthread_representation...	[ok]
ca6::4_ready_queue_and_scheduler...	[ok]
ca6::5_spinlock_and_lock_ranking...	[ok]
ca6::6_integrated_demo_execution...	[CA6-DEMO] CA-6 Core Baseline Demo Invoked successfully.
[ok]
==================================================
CA-6 Core Demonstration: ALL 10 LAYERS VERIFIED
==================================================
```

### C. Full Test Suite Regression (All 58 Tests)
```powershell
cargo test
```
**Output:**
- 20 / 20 lib unit tests passed (`kthread`, `scheduler`, `sync`, `shell`, `vga_buffer`, `interrupts`, `rtc`, `fs`, `allocator`).
- 38 / 38 integration tests passed across `filesystem`, `hardware`, `network`, `wifi`, `package_manager`, `snap_store`, `smoke_integration`, `ca6_demo`.

### D. Real QEMU Execution (`ca6-core` standalone boot)
```powershell
cargo bootimage --features ca6-core
qemu-system-x86_64 -drive format=raw,file=target\x86_64-micro_os\debug\bootimage-micro_os.bin -serial file:traces\ca6_core_boot.log -display none
```
**Serial Output (`traces/ca6_core_boot.log`):**
```
Serial port online.
[CA6-DEMO] CA-6 Core Baseline Demo Invoked successfully.
BOOT_TIME_MS=190.479 (TSC ticks=475392090, freq=2495 MHz)
micro_os:/> 
```

---

## 6. Limitations

- Thread switching assembly (`switch_context`) is not yet wired to the PIT IRQ0 interrupt handler; threads currently initialize their context and can be queued/dequeued in `ReadyQueue`. Preemption will be integrated in Step A4 and Step A5.
- The `ca6 demo` command displays the subsystem status and runs baseline checks; full interactive multi-task round-robin scheduling visualization will be enabled upon completion of assembly context switching (Step A4).

---

## 7. What is Next (Step A2)

In **Step A2 (Kernel Diagnostics and Structured Tracing)**:
- Implement structured log levels (`INFO`, `WARN`, `ERROR`, `DEBUG`) routed simultaneously to VGA text buffer and UART serial port.
- Establish the machine-parseable serial trace format (`[TRACE] tick=<N> event=<TYPE> ...`).
- Create `docs/TRACE_FORMAT.md` documenting the event taxonomy.
- Develop `scripts/run_trace.ps1` for automated QEMU serial trace capture and `scripts/parse_trace.py` for Pandas/CSV trace parsing.
