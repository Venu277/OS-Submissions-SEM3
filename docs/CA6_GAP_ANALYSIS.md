# CA-6 Micro-OS Engineering Project: Comprehensive Gap Analysis & Baseline Audit

**Assessment:** CA-6 of 7 | 15 Marks  
**Course:** Operating Systems  
**Specification:** [CA6_Rust_Micro_OS_Engineering_Project.md](https://github.com/sujeetjagtap/Operating-System-Concepts/blob/main/Activities-and-Assignments/CA6_Rust_Micro_OS_Engineering_Project.md)  
**Date:** 2026-11-15  
**Current Branch:** `ca6-compliance`  
**Workspace:** `C:\Users\Venugopal\OneDrive\Desktop\micro_os-main`  

---

## 1. Executive Summary

This document establishes the formal baseline audit for transitioning the **MicroOS** repository into full compliance with the academic specification **"CA-6 Rust Micro-OS Engineering Project"** (15 marks).

The educational philosophy of CA-6 (Section 3) dictates:
> *"Do not build a large operating system. Build a small operating system that makes OS mechanisms visible."*  
> *"It runs is not sufficient evidence. Students must show what the OS mechanism is doing."* (Section 17)

While the existing repository contains extensive extra engineering (Ext2-style on-disk filesystem, Intel e1000 DMA networking, 802.11 WiFi simulation, ACPI discovery, Debian `apt`/`dpkg`, and Canonical `snap` store abstractions), **it currently lacks the central academic requirements of CA-6: kernel thread representation with saved CPU execution context, a preemptive Round Robin scheduler with low-level register switching, and hand-written synchronization primitives.**

The existing task subsystem relies on a cooperative Rust `Future` / async executor, which does not save CPU hardware registers, does not switch execution stacks, and cannot preempt running tasks on a timer tick. This gap analysis details the exact delta between the repository's current state and the 15-mark grading rubric, providing file-level evidence and a non-destructive remediation plan.

---

## 2. Specific Audit Findings

### 2.1 Stray `micro_os` Entry in Repo Root
- **Finding:** A directory entry named `micro_os` exists in the repository root.
- **Git Inode Type:** Git submodule link (`160000 commit 3903e352246396731ca6dc143833e147e0a3b81f`).
- **Contents:** Contains an outdated Nov 2, 2026 attempt with a duplicate `Cargo.toml`, `.cargo/config.toml`, and a 467-byte `src/main.rs`.
- **Verdict:** Stray legacy artifact that is completely disconnected from the active kernel build. It should be removed from git tracking and deleted to prevent examiner confusion during grading.

### 2.2 Memory Allocator: Hand-Written vs External Crate
- **Finding:** `src/allocator/linked_list.rs` is **100% hand-written in pure Rust**.
- **Evidence:** Lines 1–140 of `src/allocator/linked_list.rs` define an intrusive linked list (`struct ListNode { size: usize, next: Option<&'static mut ListNode> }`), implementing `add_free_region()`, `find_region()`, `alloc_from_region()`, `size_align()`, and raw pointer writes (`node_ptr.write(node)`). It does not wrap `linked_list_allocator` or any third-party crate.
- **Compliance:** Satisfies Core Requirement 8 and Layer 8 (Memory Management) with excellent Rust implementation quality.

### 2.3 Synchronization Primitives: Hand-Written vs `spin` Crate
- **Finding:** **There are zero hand-written synchronization primitives in the repository.**
- **Evidence:** `git grep "use spin::" src/` reveals 14 files relying exclusively on `spin::Mutex` from the external `spin` crate (v0.9.8).
- **Verdict:** Fails Core Requirement 7 (basic synchronization) and Section 9 (Layer 7). To earn marks, a native, hand-written interrupt-safe spinlock (`src/sync/spinlock.rs`) and ranked locks (`src/sync/ranked.rs`) must be implemented from atomic primitives (`core::sync::atomic::AtomicBool`).

### 2.4 Hardware Detection vs Hardcoded / Simulated Profiles (Step 8 Audit)
- **CPUID (`src/cpu.rs`):** **REAL.** Executes hardware `__cpuid` and `__cpuid_count` instructions (Leaf 0 vendor, Leaf 1 family/model/stepping/features, Leaf 7 extended features, Leaves 0x80000002..4 brand string).
- **ACPI (`src/acpi.rs`):** **REAL.** Physically scans EBDA (`0x80000`) and BIOS ROM (`0xE0000..0x100000`) for the RSDP `"RSD PTR "` signature, verifies checksums, traverses the XSDT/RSDT table pointer array, and extracts real FADT reset ports and MADT Local/IO APIC addresses.
- **NVMe (`src/storage/nvme.rs`):** **HYBRID/SIMULATED.** `find_nvme()` performs real PCI bus scanning for Class `0x010802`. However, because hypervisors (QEMU/VMware) do not attach a physical PCIe NVMe card by default, line 190 falls back to `NvmeDevice::new_laptop_profile()`, which is an in-memory software RAM cache simulating 1,000,000,000 sectors (512 GB).
- **UEFI:** **NOT PRESENT.** MicroOS boots exclusively via the BIOS/MBR boot sector handoff of `bootloader` v0.9.35. No UEFI GOP, PE/COFF loader, or UEFI Boot Services exist.
- **Documentation Discrepancy:** Earlier documentation referred to "Intel i5-1335U Profile" as an intrinsic target; in reality, QEMU boots its default virtual CPU unless `-cpu host` is explicitly passed.

### 2.5 Debian `apt` and Snap Store Execution Reality
- **Finding:** MicroOS has **NO Ring 3 user mode, NO privilege separation, NO syscall vector (`syscall`/`sysret`), and NO ELF binary loader.**
- **Debian `apt` (`src/pkg/`):** Real RFC 822 key-value parsing, real `ar` container extraction, real USTAR `tar` unpacking, and real Kahn's topological dependency resolver. However, unpacked files in `/bin/` are inert byte buffers stored in MicroFS; they cannot be executed as separate processes.
- **Snap Store (`src/snap/`):** Real `SNAP01` binary container parser/builder, real YAML manifest parsing (`meta/snap.yaml`), and real loopback mount directory hierarchy (`/snap/<name>/<rev>`). However, `snap run <app>` does not execute a binary via `execve()`; instead, `SnapManager::run()` matches hardcoded application names (`micro-calc`, `system-monitor`, `nano-editor`) and executes in-kernel simulated routines.
- **Verdict:** Impressive format parsers, but fundamentally simulated execution.

### 2.6 Test Suite Baseline Verification
- **Command Executed:** `cargo test` on nightly-2025-06-01.
- **Result:** **10 test suites, 49 tests passed, 0 failures, exit code 0.**
  - `unittests (src/lib.rs)`: 11 passed
  - `filesystem (tests/filesystem.rs)`: 5 passed
  - `hardware (tests/hardware.rs)`: 5 passed
  - `heap_allocation (tests/heap_allocation.rs)`: 5 passed
  - `networking (tests/networking.rs)`: 4 passed
  - `pkg_apt (tests/pkg_apt.rs)`: 4 passed
  - `smoke_integration (tests/smoke_integration.rs)`: 5 passed
  - `snap_store (tests/snap_store.rs)`: 4 passed
  - `stack_overflow (tests/stack_overflow.rs)`: 1 passed
  - `wifi (tests/wifi.rs)`: 5 passed

### 2.7 Privacy & Personal Data Audit (`laptop_hardware.txt`)
- **Finding:** `laptop_hardware.txt` contains generic hardware specifications: Intel Core i5-1335U, 16 GB LPDDR5, 512 GB SSD, MediaTek MT7921, Intel Iris Xe.
- **Verdict:** Contains no serial numbers, MAC addresses, or PII. However, because it documents the private machine of the author rather than the emulator target, it should be sanitized or replaced with the standard CA-6 QEMU target specification.

---

## 3. Analysis: Async Executor vs Preemptive Kernel Scheduler

A critical question raised in the assignment specification is:
> **Does the existing async executor satisfy the task and scheduler requirements of CA-6?**

**Answer: NO.** The table below contrasts the existing async executor with CA-6 academic requirements:

| Dimension | Existing `src/task/executor.rs` | CA-6 Requirement (Layers 3–6) | Compliant? |
| :--- | :--- | :--- | :---: |
| **Execution Abstraction** | Rust cooperative `Future<Output = ()>` | Kernel Thread (`KThread`) with dedicated stack | ❌ **No** |
| **Context Representation** | `core::task::Context` (Waker reference) | CPU register context (`rsp`, `rip`, `rbx`, `rbp`, `r12`–`r15`, flags) | ❌ **No** |
| **Stack Allocation** | Shared kernel stack during polling | Dedicated per-task kernel stack allocated from heap | ❌ **No** |
| **Switching Mechanism** | Cooperative function call (`poll()`) | Low-level CPU context switch (`switch_context` assembly) | ❌ **No** |
| **Preemption** | Impossible; compute-bound loop hangs OS | Timer interrupt decrements quantum and forces switch | ❌ **No** |
| **State Machine** | Binary: `Poll::Ready` vs `Poll::Pending` | Standard OS states: `READY`, `RUNNING`, `BLOCKED`, `TERMINATED` | ❌ **No** |
| **Scheduling Algorithm** | FIFO wake-queue polling | Round Robin with configurable quantum & metrics | ❌ **No** |

**Conclusion:** The async executor must be retained for existing async keyboard processing, while a **real kernel thread abstraction (`src/kthread/`) and preemptive Round Robin scheduler (`src/scheduler/`) must be introduced alongside it.**

---

## 4. CA-6 Core Requirements Compliance Matrix (Section 10)

| # | Core Requirement | Status | Current Codebase Location | Gap / Required Remediation |
| :-: | :--- | :---: | :--- | :--- |
| **1** | **Rust Kernel Boot** | ✅ **Present** | `src/main.rs:16`, `src/lib.rs:34` | Bootloader handoff, VGA banner, IDT/GDT initialization, and serial online are fully functional. |
| **2** | **Kernel Console & Diagnostics** | ⚠️ **Partial** | `src/serial.rs`, `src/vga_buffer.rs` | Has `println!` and `serial_println!`. **Missing:** log levels (`INFO`, `WARN`, `ERROR`, `DEBUG`) with tick timestamps and machine-parseable structured traces (`[TRACE] tick=... event=...`). |
| **3** | **Task Representation** | ❌ **Missing** | `src/task/mod.rs:24` | Only cooperative `Future` wrapper exists. **Missing:** Kernel thread struct with thread ID, state (`READY`, `RUNNING`, `BLOCKED`, `TERMINATED`), saved CPU register context, and dedicated stack. |
| **4** | **Ready Queue** | ❌ **Missing** | `src/task/executor.rs:37` | Only `VecDeque<TaskId>` waker queue exists. **Missing:** Standalone scheduler ready queue with explicit `enqueue()`, `dequeue()`, `peek()`, and task ordering. |
| **5** | **Round Robin Scheduler** | ❌ **Missing** | None | **Missing:** Round Robin scheduling logic, time slice tracking, and assembly-level CPU context switching (`switch_context`). |
| **6** | **Timer-Driven Preemption** | ⚠️ **Partial** | `src/interrupts.rs:136` | PIT timer tick interrupts exist and increment ticks, but do NOT preempt or switch tasks. **Missing:** Preemptive timer tick hook that decrements remaining quantum and triggers reschedule. |
| **7** | **Basic Synchronization** | ⚠️ **Partial** | External `spin` crate | Only external `spin::Mutex` used. **Missing:** Hand-written interrupt-safe spinlock (`Spinlock<T>`) implemented from `core::sync::atomic::AtomicBool`. |
| **8** | **Memory Allocation** | ✅ **Present** | `src/allocator/linked_list.rs` | Hand-written 256 KiB intrusive linked-list allocator is fully functional. (Will add bump allocator for comparison in Step A7). |
| **9** | **Integrated Demo** | ⚠️ **Partial** | `src/shell.rs` | Shell exists with many extraneous commands. **Missing:** Dedicated `ca6 demo` command demonstrating boot -> tasks -> preemption -> synchronization -> termination. |
| **10** | **Repeatable Test Evidence** | ⚠️ **Partial** | `tests/` | 10 integration test suites exist. **Missing:** Dedicated tests for required cases TC1–TC6 and Python trace visualization. |

---

## 5. Required Test Cases Audit (Section 16)

| Test Case | Description | Current Status | Remediation Required |
| :--- | :--- | :---: | :--- |
| **TC1 – Minimum Configuration** | Two tasks alternating under Round Robin | ❌ **Missing** | Create test with 2 kernel threads demonstrating cooperative and preemptive alternation. |
| **TC2 – Multiple Tasks** | 3–5 controlled tasks scheduled concurrently | ❌ **Missing** | Create test with 4 tasks running arithmetic workloads, recording execution order. |
| **TC3 – Boundary Case** | Smallest meaningful parameter (quantum = 1 tick, 1 task) | ❌ **Missing** | Verify quantum = 1 tick and single-task queue behavior without deadlocking or CPU starvation. |
| **TC4 – Stress Case** | High task count / heavy allocation and lock contention | ❌ **Missing** | Create stress test with 10+ tasks, frequent allocations, and intensive spinlock contention. |
| **TC5 – Failure Case** | Controlled invalid operation / rejected request | ❌ **Missing** | Test invalid state transitions (e.g. `TERMINATED` -> `RUNNING`), lock ranking violations, or out-of-memory rejection. |
| **TC6 – Repeatability** | Identical experiment repeated 5+ times with trace comparison | ❌ **Missing** | Automated script running 5 identical runs, proving deterministic scheduling orders and timing bounds. |

---

## 6. Specialization Track Selection: Track C (Concurrency Engineering)

The CA-6 specification (Section 11) requires selecting one advanced specialization track. As planned, we select **Track C – Concurrency Engineering**:

- **Core Requirement:** Basic lock/synchronization (hand-written spinlock).
- **Advanced Deliverables:**
  1. **Lock Ranking Architecture (`src/sync/ranked.rs`):** Strict partial ordering of lock acquisitions; prevents circular wait (Coffman Condition 4). Tracks per-task held lock ranks; dynamic detection and rejection of out-of-order acquisitions (TC5).
  2. **Deadlock Demonstration:** Reproduce classic 2-task opposite-order deadlock without ranking, then demonstrate deadlock prevention with ranking enforced.
  3. **Lock Contention Instrumentation:** Track lock acquisition counts, total wait ticks, and contention rates per lock.
  4. **Contention Scaling Experiment:** Benchmark contention metrics under 2, 3, and 5 competing tasks.

---

## 7. Submission Package & Documentation Checklist (Sections 22, 23, 42)

| Required Deliverable | Assignment Section | Current Status | Target Location |
| :--- | :---: | :---: | :--- |
| **Architecture Document** | Sec 22, 24 | ⚠️ Needs Update | `docs/architecture.md` (Data/control flow diagram: boot -> task -> scheduler -> timer -> sync -> memory -> hardware) |
| **Design Document** | Sec 22 | ⚠️ Needs Update | `docs/design.md` (Trade-offs, Round Robin rationale, nightly justification) |
| **Linux vs MicroOS Comparison** | Sec 20 | ❌ Missing | `docs/LINUX_COMPARISON.md` (Tasks, scheduling, memory, sync, security comparison) |
| **`unsafe` Rust Audit** | Sec 26 | ❌ Missing | `docs/UNSAFE_AUDIT.md` (Catalog of every unsafe block, reason, invariant, validation) |
| **Testing Document & Debug Log** | Sec 15, 21 | ❌ Missing | `docs/testing.md` and `docs/debug_log.md` (Systematic test tables, failure logs) |
| **Python Analysis Pipeline** | Sec 19 | ❌ Missing | `scripts/parse_trace.py`, `scripts/analyze.py` (Trace parsing, fairness, timeline plots) |
| **Technical Report (20 Sections)** | Sec 23 | ❌ Missing | `docs/REPORT.md` (Comprehensive academic submission paper) |
| **Viva Preparation Guide** | Sec 32 | ❌ Missing | `docs/VIVA_PREP.md` (Answers to 20 viva questions + 10 likely extra questions) |
| **Timed 5-Minute Demo Script** | Sec 42 | ❌ Missing | `docs/DEMO_SCRIPT.md` (Step-by-step commands, expected outputs, fallback log) |
| **AI Usage Disclosure & Contribution** | Sec 25, 31 | ❌ Missing | `docs/AI_USAGE_DISCLOSURE.md`, `docs/CONTRIBUTION_STATEMENT.md` |

---

## 8. Out-of-Scope Features Inventory

The repository contains substantial earlier work that exceeds the CA-6 specification. Per user instruction, **these modules will NOT be deleted or moved**; they will remain intact while the CA-6 core runs independently:
- **ATA PIO Driver & MicroFS Filesystem** (`src/storage/ata.rs`, `src/fs/`)
- **Intel e1000 Gigabit LAN Driver & smoltcp Stack** (`src/net/e1000.rs`, `src/net/mod.rs`)
- **802.11 WiFi Simulation & WPA2 Handshake** (`src/net/wifi.rs`)
- **NVMe 1.4+ Controller Simulation** (`src/storage/nvme.rs`)
- **Debian Package Manager (`apt` / `dpkg`)** (`src/pkg/`)
- **Snap Container Store** (`src/snap/`)

---

## 9. Next Steps (Step A1 Onward)

- **Step A1:** Establish CA-6 module architecture (`src/kthread/`, `src/scheduler/`, `src/sync/`, `scripts/`, `traces/`, `results/`) and add `ca6 demo` command to shell.
- **Step A2:** Implement kernel diagnostics, 4 log levels (`INFO`/`WARN`/`ERROR`/`DEBUG`), tick timestamps, and machine-parseable structured tracing.
- **Step A3:** Implement kernel thread representation (`KThread`), state machine, and ready queue.
- **Step A4:** Implement Round Robin scheduler with low-level assembly CPU context switching.
- **Step A5:** Connect PIT timer interrupt to preemptive quantum countdown.
- **Step A6:** Implement hand-written interrupt-safe spinlock and race-condition demonstration.
- **Step A7:** Add memory allocation statistics, second allocator, and fragmentation tests.
- **Step A8:** Implement Track C Concurrency Specialization (Lock ranking, deadlock demo, contention metrics).
- **Step A9:** Implement TC1–TC6 and Python trace analysis pipeline.
- **Step A10:** Author all technical documentation, viva preparation, and report.
- **Step A11:** Final clean-build verification and release tagging.
