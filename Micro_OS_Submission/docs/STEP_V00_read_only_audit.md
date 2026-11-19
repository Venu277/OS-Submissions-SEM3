# Step V00: Read-Only System Audit & Gap Analysis

## Goal
Conduct a forensic, read-only audit of the entire existing codebase against the official university CA-6 specification (Sections 1–45) and real x86_64 hardware requirements. Verify build, tests, and QEMU execution; evaluate legacy assumptions versus bare-metal reality; discover physical laptop specs; and recommend a UEFI migration strategy.

## CA-6 Sections Addressed
- **Section 9:** Development Layers 1–10 baseline audit
- **Section 10:** Core Project Requirements compliance check
- **Section 11:** Specialization Track C (Concurrency Engineering) audit
- **Section 16:** Required Test Cases (TC1–TC6) runtime verification
- **Section 22:** Repository layout and version-control audit
- **Section 26:** `unsafe` Rust policy and audit verification
- **Section 42 & 43:** Final submission package and student checklist gap assessment

## Files Changed
- `docs/GAP_ANALYSIS.md`: Comprehensive Present/Partial/Missing compliance matrix across all CA-6 requirements, test suites, and hardware realities.
- `docs/STEP_V00_read_only_audit.md`: Detailed audit record, methodology, physical hardware findings, and UEFI evaluation.
- `PROGRESS.md`: Updated active project tracking for VenOS Step V0.

## How It Works

### Plain English (Student Viva Explanation)
Before modifying code to make an operating system boot on a real laptop, an OS engineer must inspect what is currently working, what is simulated, and what hardware is actually present. We audited the kernel repository without making breaking code changes. We compiled the kernel with the pinned `nightly-2025-06-01` toolchain, ran integration tests in QEMU, and inspected the source code to separate real hardware interactions from software simulations. We discovered that while the core OS algorithms (threads, Round Robin scheduler, spinlocks, and memory allocators) are functional in QEMU, the kernel currently boots using a 1980s-era PC BIOS mechanism and draws text to a legacy VGA buffer at `0xb8000`. On modern laptops (which have no BIOS and run pure UEFI firmware), this kernel cannot start. We also queried the real host laptop and confirmed its hardware: an Intel 13th Gen Core i5-1335U, an Intel Iris Xe GPU running at 1920x1200, an NVMe SSD, and MediaTek WiFi 6. Finally, we evaluated two modern UEFI bootloaders (`bootloader 0.11` vs `Limine`) and prepared the migration plan.

### Technical Architecture
```
+-----------------------------------------------------------------------------------+
| Existing State: Legacy PC BIOS (QEMU Only)                                        |
|                                                                                   |
|  [MBR / BIOS Bootloader 0.9]                                                      |
|           │                                                                       |
|           ├──> Legacy VGA Text Buffer (0xb8000)   ──> Fails on modern UEFI Laptop |
|           ├──> 8259 PIC + 8253 PIT Timer IRQ0     ──> Fails on modern UEFI Laptop |
|           └──> BIOS Memory Scanning for RSDP      ──> Fails on modern UEFI Laptop |
+-----------------------------------------------------------------------------------+
                                      │
                                      ▼
+-----------------------------------------------------------------------------------+
| Target State: Modern UEFI Bare Metal (VenOS Step V1+)                             |
|                                                                                   |
|  [UEFI Firmware / GPT FAT32 / BOOTX64.EFI (bootloader 0.11)]                      |
|           │                                                                       |
|           ├──> GOP Framebuffer Console (1920x1200, 32bpp) ──> Real Screen Display |
|           ├──> Local APIC Timer (HPET/ACPI Calibrated)    ──> Real Preemption     |
|           └──> UEFI System Table RSDP Pointer             ──> Real Hardware ACPI  |
+-----------------------------------------------------------------------------------+
```

## Build, Run, and Test Commands

1. **Verify Toolchain & Compile Kernel:**
   ```powershell
   cargo check
   ```
   *Result:* Exited code 0 in 1m 20s.

2. **Execute Layers 3 & 4 Task & Ready Queue Integration Test:**
   ```powershell
   cargo test --test kthread_ready_queue
   ```
   *Result:* Exited code 0. TC1 through TC6 passed with structured `[TRACE]` logs.

3. **Execute CA-6 Integrated 10-Layer Demo Test:**
   ```powershell
   cargo test --test ca6_demo
   ```
   *Result:* Exited code 0. Verified all 10 layers, task switching, preemption, lock ranking, and allocator benchmarks.

## Real Evidence

1. **Stray `micro_os` File Audit:**
   Found in git history: originally added as a git submodule (`mode 160000 micro_os` in commit `47f2cceb`) and cleanly removed in commit `bc779451`. No stray binary file remains in the root.

2. **Hand-Written Allocators:**
   `src/allocator/linked_list.rs` contains a hand-written intrusive singly linked-list allocator with memory reclamation; `src/allocator/bump.rs` contains a hand-written sequential bump allocator.

3. **Hand-Written Synchronization:**
   `src/sync/spinlock.rs` contains a hand-written `Spinlock<T>` with atomic CAS and contention tracking. Kernel singletons outside `src/sync` currently use `spin::Mutex`. An interrupt-safe spinlock is not yet implemented.

4. **Hardware Simulation vs Reality in Step 8:**
   - CPUID (`src/cpu.rs`): Real hardware assembly instructions (`__cpuid`).
   - ACPI (`src/acpi.rs`): Real table parser, but relies on BIOS EBDA/ROM scanning for RSDP.
   - NVMe (`src/storage/nvme.rs`): Hardcoded software simulation (`NvmeDevice::new_laptop_profile()`).
   - Package manager (`apt` / `snap`): No user-mode ELF execution; `snap run` prints simulated output strings from a hardcoded `match` statement.

5. **Discovered Laptop Hardware (Physical Host):**
   - **CPU:** Intel Core i5-1335U (10 cores: 2P + 8E, 12 threads)
   - **GPU:** Intel Iris Xe Graphics (`PCI\VEN_8086&DEV_A7A1`, 1920x1200 @ 60Hz, 32bpp)
   - **Storage:** Western Digital PC SN5000S NVMe SSD (512 GB)
   - **Keyboard:** Standard PS/2 Keyboard via EC emulation (`ACPI\HPQ8001`)
   - **Touchpad:** ELAN Input Device (`HID\ELAN07CF`)
   - **USB Controllers:** Intel USB 3.10 xHCI (`VEN_8086&DEV_51ED`), Intel USB 3.20 xHCI (`VEN_8086&DEV_A71E`), USB4 Host Router (`VEN_8086&DEV_A73E`)
   - **WiFi:** MediaTek MT7921 Wi-Fi 6 (`PCI\VEN_14C3&DEV_7961`, Subsystem `1A3B:4682`)
   - **Ethernet:** None
   - **Firmware:** UEFI Class 3 (No CSM / No legacy BIOS)
   - **Secure Boot:** **Active / Enabled** (`UEFISecureBootEnabled = 1`)

6. **UEFI Migration Recommendation:**
   Recommend `bootloader 0.11` (`bootloader_api 0.11.17` + host image builder crate). Fully compatible with Windows host and `nightly-2025-06-01`, exposes UEFI GOP framebuffer, memory regions, and direct RSDP pointer.

## Boot Time
- **Environment:** QEMU/SeaBIOS (Legacy)
- **Baseline Kernel Entry to Shell Ready:** ~154 ms (historical baseline).
- *Note:* Real hardware boot timing will be recorded starting in Step V1 once UEFI boot is operational.

## Limitations
1. Kernel currently cannot boot on the target laptop because it lacks UEFI support.
2. Text output relies on VGA mode `0xb8000`, which is unmapped and non-functional under UEFI.
3. Preemption timer relies on legacy 8253 PIT, which is not routed on modern mobile platforms.
4. NVMe driver is simulated in memory.

## Next Step
**Step V1:** Migrate to UEFI boot (`bootloader_api 0.11`) and implement a graphical framebuffer console with an embedded bitmap font.
