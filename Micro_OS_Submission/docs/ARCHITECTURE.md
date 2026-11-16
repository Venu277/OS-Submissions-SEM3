# VenOS — System Architecture & Design Specification

## Overview
**VenOS** is a production-grade 64-bit operating system kernel developed in safe systems Rust for x86_64 processors. Designed for both real bare-metal hardware (verified on HP Pavilion 15 laptop with 13th Gen Intel Core i5-1335U) and virtualized environments (QEMU/OVMF), VenOS implements a clean, layered monolithic architecture with explicit separation of concerns across 10 functional layers.

---

## 1. High-Level Architectural Diagram

```mermaid
graph TD
    subgraph Layer9_10 ["Layers 9 & 10: User & Integrated Demonstration"]
        Shell["Interactive Graphical Shell (venos:/>)"]
        DemoSuite["CA-6 Integrated Demo Suite ('ca6 demo')"]
        DiagnosticsCLI["Kernel Diagnostics CLI ('dmesg', 'sysinfo', 'lspci')"]
    end

    subgraph Layer5_6 ["Layers 5 & 6: Scheduling & Preemption Engine"]
        ContextSwitch["Assembly switch_context (AMD64 System V ABI)"]
        RoundRobin["Round Robin Dispatcher (Cooperative yield_now)"]
        TimerPreempt["PIT 100 Hz Timer Tick Preemption (IRQ 0 / Vector 32)"]
        PreemptGuard["RAII PreemptionGuard (Nested Re-entrancy Guard)"]
    end

    subgraph Layer3_4 ["Layers 3 & 4: Task Management & Ready Queue"]
        KThread["Kernel Thread (KThread: ID, State, Metrics, Context)"]
        ReadyQueue["Thread-Safe FIFO ReadyQueue"]
        StackGuard["16 KiB Kernel Stack + 0xCA60_DEAD_BEEF_0001 Canary"]
    end

    subgraph Layer7 ["Layer 7: Synchronization & Concurrency (Track C)"]
        Spinlock["Hand-Written Spinlock<T> (AtomicBool CAS + PAUSE)"]
        LockRanking["RankedLock<T> (Deadlock Prevention via Strict Hierarchy)"]
        ContentionStats["Telemetry Counters (Acquisitions, Contentions, Spins, Hold)"]
    end

    subgraph Layer8 ["Layer 8: Dynamic Memory Management (Track B)"]
        FrameAlloc["BootInfoFrameAllocator (UEFI MemoryRegions & BIOS Map)"]
        AdaptiveHeap["Adaptive Kernel Heap Sizer (RAM / 64, 256 KiB .. 4 MiB)"]
        LinkedListAlloc["Self-Written Intrusive LinkedListAllocator"]
        BumpAlloc["Self-Written Sequential BumpAllocator"]
    end

    subgraph Layer1_2 ["Layers 1 & 2: Hardware Abstraction & Discovery"]
        UEFI_Boot["UEFI 64-bit Bootloader (bootloader_api 0.11)"]
        GOP_Console["GOP Framebuffer Text Console (Anti-aliased Glyph Engine)"]
        ACPI_MADT["ACPI Table Parser (RSDP physical address handover)"]
        PCI_Scan["256-Bus PCI Bus Enumeration & Friendly Device Names"]
        CPU_Profile["CPUID Discovery & PIT Dynamic Frequency Calibration"]
        TraceRingBuf["512-Record In-Kernel Static Circular Trace Buffer (BSS)"]
    end

    Shell --> RoundRobin
    DemoSuite --> ContextSwitch
    TimerPreempt --> RoundRobin
    RoundRobin --> ReadyQueue
    ReadyQueue --> KThread
    RoundRobin --> ContextSwitch
    Spinlock --> PreemptGuard
    LockRanking --> Spinlock
    KThread --> StackGuard
    StackGuard --> AdaptiveHeap
    AdaptiveHeap --> FrameAlloc
    GOP_Console --> UEFI_Boot
```

---

## 2. Kernel Memory Layout

```
+------------------------------------+ 0xFFFF_FFFF_FFFF_FFFF  (Top of Virtual Address Space)
|          Kernel Space              |
|                                    |
|  Recursive Page Tables / Paging    |
|                                    |
|  Kernel Heap (Dynamically Sized)   |
|  [0x4444_4444_0000 .. +1720 KiB]   | <-- Adaptive Heap (1/64th RAM clamped 256K..4M)
|                                    |
|  Physical Memory Direct Mapping    |
|  [0x0000_8000_0000 .. Framebuffer] | <-- UEFI GOP Linear Framebuffer (1280x800 / 1920x1200)
|                                    |
+------------------------------------+ 0x0000_0000_0020_0000
|  Kernel Code, Data & BSS           |
|  - .text: Executable instructions  |
|  - .rodata: Constants, font glyphs |
|  - .bss: 512-record TraceRingBuffer| <-- Statically allocated, zero heap dependency
+------------------------------------+ 0x0000_0000_0000_0000  (Bottom of Address Space)
```

---

## 3. Task Lifecycle State Machine

```mermaid
stateDiagram-v2
    [*] --> Ready: KThread::new() / spawn()
    Ready --> Running: Scheduler Dispatch (dequeue)
    Running --> Ready: Voluntary yield_now() OR Timer Preemption (Quantum Expire)
    Running --> Blocked: Lock Acquisition Wait (Spinlock / Contention)
    Blocked --> Ready: Lock Released / Event Triggered
    Running --> Terminated: Thread Returns (thread_exit_trampoline)
    Terminated --> [*]: Memory Reaped by reap_terminated()
    
    note right of Running
        Canary Checked:
        0xCA60_DEAD_BEEF_0001
        Verified before context switch
    end note
```

---

## 4. AMD64 Assembly Context Switch Protocol

When transferring execution from `prev` thread to `next` thread via `switch_context(prev, next)`:

```mermaid
sequenceDiagram
    participant Sched as Scheduler Core
    participant Regs as CPU Hardware Registers
    participant PrevCtx as Prev Thread CpuContext
    participant PrevStk as Prev Thread Stack
    participant NextCtx as Next Thread CpuContext
    participant NextStk as Next Thread Stack

    Sched->>Regs: Call switch_context(rdi=prev, rsi=next)
    Note over Regs,PrevStk: Hardware pushes return address (RIP) onto PrevStk
    Regs->>PrevCtx: Save r15, r14, r13, r12, rbx, rbp
    Regs->>PrevCtx: Pushfq / Pop RAX -> Save rflags
    Regs->>PrevCtx: Save RSP (points to PrevStk)
    
    NextCtx->>Regs: Load RSP from next.context.rsp (Switch Stack!)
    NextCtx->>Regs: Load rflags (Push RAX / Popfq)
    NextCtx->>Regs: Restore rbp, rbx, r12, r13, r14, r15
    Regs->>NextStk: Execute `ret` instruction
    Note over Regs,NextStk: Hardware pops RIP from NextStk into CPU
    Note over Regs: Next thread resumes execution transparently!
```

---

## 5. Track C Concurrency & Deadlock Prevention

### Mathematical Lock Ranking Model
Let $\mathcal{L} = \{L_1, L_2, \dots, L_k\}$ be the set of synchronization objects in the operating system. Each lock is assigned a unique, immutable monotonic integer rank:
$$\text{Rank}: \mathcal{L} \to \mathbb{N}^+$$

A thread $T$ holding a set of active locks with maximum currently held rank:
$$R_{\text{held}} = \max(\{0\} \cup \{\text{Rank}(L) \mid L \in \text{HeldBy}(T)\})$$
is legally permitted to request lock $L_{\text{req}}$ if and only if:
$$\text{Rank}(L_{\text{req}}) > R_{\text{held}}$$

```
                       [ Rank 30: User Data Lock ]
                                    ▲
                                    │  Legal: Ascending Acquisition (Strict Order)
                       [ Rank 20: VFS Inode Lock ]
                                    ▲
                                    │
                       [ Rank 10: Device Driver Lock ]
                                    ▲
                                    │
                       [ Rank 0:  Unheld Baseline ]
```

### Coffman Condition Elimination
- **Condition 1 (Mutual Exclusion):** Preserved for data integrity via `Spinlock<T>`.
- **Condition 2 (Hold and Wait):** Allowed.
- **Condition 3 (No Preemption):** Preserved for lock safety.
- **Condition 4 (Circular Wait):** **MATHEMATICALLY ELIMINATED.**
  - *Proof:* Suppose a circular wait chain exists: $T_1$ holds $L_1$ and waits for $L_2$; $T_2$ holds $L_2$ and waits for $L_3$; $\dots$; $T_n$ holds $L_n$ and waits for $L_1$. Under our acquisition protocol, this requires:
    $$\text{Rank}(L_1) < \text{Rank}(L_2) < \dots < \text{Rank}(L_n) < \text{Rank}(L_1)$$
    $$\implies \text{Rank}(L_1) < \text{Rank}(L_1)$$
    which is an immediate contradiction ($x \not< x$). Therefore, circular wait cannot exist, and deadlocks are impossible.

---

## 6. Device Abstraction & Hardware Driver Matrix

| Subsystem | Hardware Interface | Bare-Metal Target (HP Pavilion) | QEMU / Virtual Machine | Source Implementation |
|---|---|---|---|---|
| **Display** | UEFI GOP Framebuffer | 1920x1200 16:10 Intel Iris Xe | 1280x800 16:10 QEMU Cirrus/StdVGA | `src/framebuffer.rs` |
| **Timer** | Intel 8253/8254 PIT Ch 0 & 2 | 100 Hz IRQ 0 + Dynamic TSC Calibrator | 100 Hz IRQ 0 + Dynamic TSC Calibrator | `src/interrupts.rs` |
| **Keyboard** | Intel 8042 PS/2 Controller | EC PS/2 Emulation (`ACPI\HPQ8001`) | Standard i8042 PS/2 Controller | `src/interrupts.rs` |
| **Storage** | NVMe PCIe / ATA PIO | WD SN5000S NVMe SSD (PCIe Gen4) | ATA IDE Block Drive + RAM Disk | `src/storage/` |
| **ACPI** | UEFI RSDP Pointer | Physical RSDP Table Handover | OVMF / Bochs RSDP Handover | `src/acpi.rs` |
| **Ethernet** | Intel 82540EM (e1000) | Not Present on Motherboard | Emulated Gigabit NIC + smoltcp | `src/net/e1000.rs` |
| **Wireless** | MediaTek MT7921 Wi-Fi 6 | Discovered on PCIe, Software Emulation | Bridged Software Emulation | `src/net/wifi.rs` |
