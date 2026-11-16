# MicroOS — Boot Time Log

## Measurement Method

| Field | Description |
|-------|-------------|
| **Kernel time (ms)** | From `_start` to prompt / readiness. Measured via `_rdtsc()`, calibrated dynamically against PIT Channel 2 (10 ms one-shot). Printed over serial as `BOOT_TIME_MS=<ms>.<us>`. |
| **QEMU total (s)** | VM power-on to serial "kernel ready" / prompt message. Measured with stopwatch, average of 3 runs. |
| **VMware total (s)** | Same, measured in VMware Workstation (available from Step 5 onward). |
| **Method** | Hardware timer calibration & measurement methodology. |

---

## Boot Time Table

| Step | Component Added | Kernel Init (ms) | QEMU Total (s) | VMware Total (s) | Method | Date |
|------|----------------|-----------------|----------------|-----------------|--------|------|
| Step 0 | Baseline (IDT only) | 43.972 ms | ~1.5 s | — | TSC ticks ÷ 2GHz est.; QEMU serial capture | 2026-11-14 |
| Step 1 | GDT/TSS, PIC remap, Timer & Keyboard IRQs | 41.896 ms | ~1.4 s | — | Dynamic PIT Ch2 calibrated TSC (@ 2505 MHz) | 2026-11-14 |
| Step 2 | Paging, Frame Allocator, Kernel Heap, `alloc` | 50.801 ms | ~1.4 s | — | Dynamic PIT Ch2 calibrated TSC (@ 2484 MHz) | 2026-11-14 |
| Step 3 | Async Executor, Shell Prompt, CMOS RTC, Clock | 54.341 ms | ~1.4 s | — | Dynamic PIT Ch2 calibrated TSC (@ 2491 MHz) | 2026-11-14 |
| Step 4 | Storage, ATA PIO Driver, MicroFS on-disk filesystem, VFS | 90.428 ms | ~1.5 s | — | Dynamic PIT Ch2 calibrated TSC (@ 2501 MHz) | 2026-11-14 |
| Step 5 | VMDK Packaging & VMware Config (Booted from VMDK) | 76.471 ms | ~1.5 s | Ready | Dynamic PIT Ch2 calibrated TSC (@ 2510 MHz) | 2026-11-14 |
| Step 6 | LAN Networking (Intel e1000 DMA driver, smoltcp stack) | 147.455 ms | ~1.5 s | Ready | Dynamic PIT Ch2 calibrated TSC (@ 2494 MHz) | 2026-11-15 |
| Step 7 | WiFi Layer (802.11 Driver Trait, Simulation, WPA2) | 108.725 ms | ~1.5 s | Ready | Dynamic PIT Ch2 calibrated TSC (@ 2499 MHz) | 2026-11-15 |
| Step 8 | Laptop Hardware (UEFI, NVMe, ACPI, i5-1335U) | 154.183 ms | ~1.5 s | Ready | Dynamic PIT Ch2 calibrated TSC (@ 2498 MHz) | 2026-11-15 |
| Step 9 | Package Manager (`apt` / `dpkg` & Debian format) | 152.735 ms | ~1.5 s | Ready | Dynamic PIT Ch2 calibrated TSC (@ 2495 MHz) | 2026-11-15 |
| Step 10 | Snap Package Store (Containers, Mounts, `snap.yaml`) | 227.163 ms | ~1.5 s | Ready | Dynamic PIT Ch2 calibrated TSC (@ 2493 MHz) | 2026-11-15 |
| **Step 11** | **Final Release Integration & System Smoke Verification** | **216.105 ms** | **~1.5 s** | Ready | **Dynamic PIT Ch2 calibrated TSC (@ 2495 MHz)** | 2026-11-15 |
| **Step A1** | **CA-6 Core Build Mode (`ca6-core` / clean scheduler baseline)** | **190.479 ms** | **~1.3 s** | Ready | **Dynamic PIT Ch2 calibrated TSC (@ 2495 MHz)** | 2026-11-15 |
| **Step V0** | **Read-Only System Audit & CA-6 Gap Analysis** | **190.479 ms** | **~1.3 s** | Ready | **Baseline QEMU/SeaBIOS run** | 2026-11-16 |
| **Step V1** | **UEFI Boot (`bootloader 0.11`) & GOP Framebuffer Console** | **577.672 ms** | **~2.1 s** | — | **QEMU/OVMF (EDK2 x86_64, TSC @ 2502 MHz, 1.445G ticks)** | 2026-11-16 |
| **Step V2** | **Native Hardware Discovery & Adaptation (GOP ratio, RAM, adaptive heap, UEFI ACPI, PCI names, sysinfo)** | **827.375 ms** | **~2.2 s** | — | **QEMU/OVMF (EDK2 x86_64, TSC @ 2490 MHz, 2.060G ticks)** | 2026-11-16 |
| **Step V3** | **Input Subsystem, Diagnostics & In-Kernel Trace Ring Buffer (512-record BSS buffer, dmesg, parse_trace.py)** | **716.322 ms** | **~2.1 s** | — | **QEMU/OVMF (EDK2 x86_64, TSC @ 2500 MHz, 1.791G ticks)** | 2026-11-16 |
| **Step V4** | **Self-Written Allocator Verification & Layer 8 Hardening (Intrusive LL vs Bump, Dual Bootloader Map)** | **559.001 ms** | **~2.0 s** | — | **QEMU/OVMF (EDK2 x86_64, TSC @ 2498 MHz, 1.396G ticks)** | 2026-11-16 |
| **Step V5** | **Task Representation & Ready Queue (KThread, CpuContext, 16K Stack + Canary, TC1-TC6)** | **560.011 ms** | **~2.0 s** | — | **QEMU/OVMF (EDK2 x86_64, TSC @ 2498 MHz, 1.399G ticks)** | 2026-11-16 |
| **Step V6** | **Round Robin Scheduler & Assembly Context Switch (AMD64 switch_context, ABI callee-saved, yield_now)** | **1248.373 ms** | **~2.2 s** | — | **QEMU/OVMF (EDK2 x86_64, TSC @ 2495 MHz, 3.115G ticks)** | 2026-11-16 |
| **Step V7** | **Timer-Driven Preemptive Scheduling (PIT 100 Hz IRQ 0, quantum slicing, involuntary context switch)** | **548.963 ms** | **~2.0 s** | — | **QEMU/OVMF (EDK2 x86_64, TSC @ 2500 MHz, 1.372G ticks)** | 2026-11-16 |
| **Step V8** | **Synchronization Subsystem (Hand-written Spinlock, CAS Acquire/Release, contention telemetry)** | **1624.377 ms** | **~2.2 s** | — | **QEMU/OVMF (EDK2 x86_64, TSC @ 2496 MHz, 4.054G ticks)** | 2026-11-16 |
| **Step V9** | **Track C Concurrency Specialization (Lock Ranking hierarchy, Coffman Condition 4 elimination, DEADLOCK_PREVENT)** | **630.962 ms** | **~2.0 s** | — | **QEMU/OVMF (EDK2 x86_64, TSC @ 2514 MHz, 1.586G ticks)** | 2026-11-16 |
| **Step V10** | **Integrated Demo & Test Cases TC1–TC6 (Layers 1-10 verified, parse_trace.py, 4 evaluation plots)** | **1268.942 ms** | **~2.2 s** | — | **QEMU/OVMF (EDK2 x86_64, TSC @ 2499 MHz, 3.171G ticks)** | 2026-11-16 |
| **Step V11** | **Networking Status & MediaTek MT7921 Disclosure (Hardware audit, NETWORKING_STATUS.md)** | **1477.311 ms** | **~2.2 s** | — | **QEMU/OVMF (EDK2 x86_64, TSC @ 2500 MHz, 3.693G ticks)** | 2026-11-16 |

---

## Notes

- In Step V11, networking and hardware status disclosures were documented in `docs/NETWORKING_STATUS.md`. The target HP Pavilion laptop contains a physical MediaTek MT7921 Wi-Fi 6 PCIe NIC (`0x14C3:0x7961`) detected on the PCI bus with simulated software fallback, but no physical RJ-45 Ethernet. In virtualized QEMU environments, the emulated Intel e1000 Gigabit NIC (`0x8086:0x100E`) and smoltcp network stack provide network connectivity. Boot time to shell prompt: **1477.311 ms** (calibrated @ 2500 MHz, raw trace: `traces/step_v11_networking_status.log`).

- In Step V10, the end-to-end integrated demonstration (`tests/ca6_demo.rs` and `ca6 demo`) and required test suite (TC1–TC6) were verified in QEMU. All 10 architectural layers were validated in a single unified test. The full smoke integration test (`tests/smoke_integration.rs`) verified CPU discovery, MicroFS filesystem hierarchy, WPA2 4-Way handshake WiFi simulation, Debian `apt`/`dpkg`, and Snap container management. Trace analysis (`scripts/parse_trace.py`) evaluated 569 machine events, generating updated CSV summaries, JSON metrics, and 4 high-resolution plots. Boot time to shell prompt: **1268.942 ms** (calibrated @ 2499 MHz, raw trace: `traces/step_v10_integrated_demo.log`).

- In Step V9, Track C Concurrency Specialization was verified in QEMU. VenOS mathematically prevents deadlocks by enforcing strict partial order lock acquisition (`RankedLock<T>`), eliminating Coffman Condition 4 (Circular Wait). Attempted out-of-order acquisitions are deterministically detected and rejected in $O(1)$ time with structured `DEADLOCK_PREVENT` trace logging. Nested hierarchical unwinding was verified up to three levels ($300 \to 200 \to 100 \to 0$). Boot time to shell prompt: **630.962 ms** (calibrated @ 2514 MHz, raw trace: `traces/step_v9_track_c_concurrency.log`).

- In Step V8, hand-written synchronization primitives (`Spinlock<T>` and RAII `SpinlockGuard`) were verified in QEMU. The implementation employs `AtomicBool` CAS with Acquire/Release semantics, x86 `PAUSE` spin loops, non-blocking `try_lock()`, and automatic contention telemetry (`acquisitions`, `contentions`, `spins_waited`, `max_spins`, `hold_ticks`). Under preemptive competition between holder and contender worker tasks, mutual exclusion was guaranteed, logging 184,814 spin cycles and 1 contention event. Boot time to shell prompt: **1624.377 ms** (calibrated @ 2496 MHz, raw trace: `traces/step_v8_sync.log`).

- In Step V7, timer-driven preemptive multitasking was verified in QEMU. Programmable Interval Timer (PIT) Channel 0 generates 100 Hz interrupts dispatching to `scheduler::timer_tick()`. A compute-bound worker with zero voluntary yields was successfully preempted upon quantum expiration, allowing an observer thread to schedule cleanly. Fair time-slicing was confirmed across multiple concurrent tasks (44 vs 47 compute iterations). Boot time to shell prompt: **548.963 ms** (calibrated @ 2500 MHz, raw trace: `traces/step_v7_preemption.log`).

- In Step V6, the cooperative Round Robin scheduler and AMD64 assembly context switch routine (`switch_context`) were verified in QEMU. The low-level assembly switch cleanly preserves System V ABI callee-saved registers (`r15`, `r14`, `r13`, `r12`, `rbx`, `rbp`), `rflags`, and `rsp`, resuming via `ret`. The test verified 3 concurrent worker threads executing in exact interleaved round-robin order `[1, 2, 3, 1, 2, 3, 1, 2, 3]` and 3/3 terminated threads successfully reaped. Boot time to shell prompt: **1248.373 ms** (calibrated @ 2495 MHz, raw trace: `traces/step_v6_round_robin_switch.log`).

- In Step V5, kernel thread representations (`KThread`, `ThreadId`, `ThreadState`, `CpuContext`, `ReadyQueue`) and stack canary overflow guards were verified across TC1 through TC6. Boot time to shell prompt: **560.011 ms** (calibrated @ 2498 MHz, raw trace: `traces/step_v5_task_ready_queue.log`).

- In Step V4, the memory allocation subsystem was verified across both self-written allocators (intrusive linked list vs sequential bump). Unified `BootInfoFrameAllocator` with `IntoFrameSource` accepts both UEFI memory regions and legacy BIOS maps across all tests. Benchmark recorded linked list average latency at 12,286 cycles and 9,920 B reclaimed after 50% deallocation. Boot time to shell prompt: **559.001 ms** (calibrated @ 2498 MHz, raw trace: `traces/step_v4_memory_allocator.log`).

- In Step V3, dual-channel diagnostic logging dispatches structured events to both UART serial port 0x3F8 and an in-memory 512-entry static circular ring buffer (`TraceRingBuffer`) in kernel BSS with zero heap allocation. The `dmesg` / `trace` shell command allows live trace inspection. Python trace analysis suite (`scripts/parse_trace.py`) parses logs and produces CSV summaries, JSON metrics, and Matplotlib evaluation plots. Boot time to shell prompt: **716.322 ms** (calibrated @ 2500 MHz, raw trace: `traces/step_v3_input_logging.log`).

- In Step V2, hardware discovery dynamically classifies the display aspect ratio (e.g. 16:10 / 16:9), computes usable system RAM across all bootloader memory regions, sizes the kernel heap adaptively (clamped 256 KiB to 4 MiB, 1724 KiB allocated in test run), consumes the UEFI-provided ACPI RSDP physical address (`boot_info.rsdp_addr`), enriches PCI enumeration with vendor and friendly device names across all 256 buses, and replaces hardcoded laptop profiles with truthful hardware queries. Boot time to shell prompt: **827.375 ms** (calibrated @ 2490 MHz, raw trace: `traces/step_v2_hw_discovery.log`).

- In Step 3, the boot time was measured from raw CPU power-on (`_start`) through CPU initialization, GDT/TSS, IDT, PIC, paging, 100 KiB heap setup, and task executor instantiation directly to the appearance of the interactive `micro_os> ` prompt.
- Time from CPU reset to an active interactive shell prompt is just **54.3 ms**!
- In Step 6, the boot time includes full PCI bus scanning, Intel e1000 controller hardware reset, MMIO page table mapping, DMA ring buffer descriptor initialization (16 Tx + 32 Rx), and `smoltcp` routing/interface initialization, reaching prompt readiness in **147.5 ms**.
- In Step 7, the 802.11 wireless subsystem is initialized during network startup with zero-cost overhead (<0.1 ms), reaching prompt readiness in **108.7 ms** (measured @ 2499 MHz).
- In Step 8, physical memory scanning for ACPI tables (RSDP, XSDT/RSDT, FADT, MADT) and NVMe storage controller registration brings prompt readiness to **154.2 ms** (measured @ 2498 MHz).
- In Step 9, package manager initialization (RFC 822 repository index registration and `/var/lib/dpkg/status` validation) adds negligible overhead (<1 ms), reaching interactive shell prompt readiness in **152.7 ms** (measured @ 2495 MHz).
- In Step 10, preloading the Snap Store catalog (5 snaps with embedded manifests and container TOCs) and initializing the `/snap` mount registry brings prompt readiness to **227.2 ms** (measured @ 2493 MHz).
- In Step 11, the unified release image initializes all hardware drivers, ACPI tables, NVMe profile, e1000 DMA, 802.11 wireless simulation, MicroFS, Debian apt repository, and Snap Store, presenting the interactive shell prompt in **216.1 ms** (calibrated @ 2495 MHz).
- In Step A1, the `ca6-core` standalone boot mode initializes the pure CA-6 core kernel (GDT, IDT, paging, heap allocator, `kthread`, `scheduler`, and `sync`) without the networking/apt/snap/filesystem drivers, reaching shell readiness and running the CA-6 core demonstration banner in **190.5 ms** (calibrated @ 2495 MHz, raw log: `traces/ca6_core_boot.log`).
