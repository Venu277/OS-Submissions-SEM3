# Step V03: Input Subsystem, Structured Logging & Trace Ring Buffer

## Goal
Implement and verify the input, diagnostics, and structured tracing architecture required by CA-6 Layer 2 and Section 23:
1. **Input Pipeline Verification:** Verify PS/2 keyboard controller (i8042) scancode dispatch on IRQ 1 (vector 33), lock-free atomic SPSC queue (`ScancodeQueue`), and async executor task wakeup for HP laptop EC emulation (`ACPI\HPQ8001`) and virtual machines.
2. **In-Kernel Trace Ring Buffer:** Implement a static, zero-heap circular ring buffer (`TraceRingBuffer`) holding 512 compact `TraceRecord` events, supporting FIFO wrap-around overwrite and zero-allocation inspection via `copy_to_slice()`.
3. **Trace Format Specification:** Formalize the machine-parseable event schema in `docs/TRACE_FORMAT.md` covering `SWITCH`, `TASK_STATE`, `ALLOC`, `DEALLOC`, `LOCK_ACQ`, `LOCK_REL`, `LOCK_WAIT`, `DEADLOCK_PREVENT`, and `QUANTUM_EXPIRE`.
4. **Shell Diagnostic Commands:** Add `dmesg` / `trace` command with `-a` (dump all) and `-c` (clear) flags to allow inspection of stored trace events directly on screen and serial.
5. **Python Trace Analysis Suite:** Verify `scripts/parse_trace.py` with Python 3.11, generating CSV summaries (`results/summary_table.csv`), JSON metrics (`results/trace_metrics.json`), and high-resolution Matplotlib evaluation plots.
6. **Repeatable Integration Testing:** Expand `tests/diagnostics_trace.rs` to verify log level formatting, macro dispatch, structured serial emission, and in-kernel ring buffer wrap-around.

---

## CA-6 Sections Addressed
- **Section 9 (Layer 2 – Kernel Console and Diagnostics):** Dual-channel dispatch (VGA/GOP display + UART 16550 serial), log levels (`DEBUG`, `INFO`, `WARN`, `ERROR`), and in-memory trace ring buffer.
- **Section 9 (Layer 3 – Interrupts and Drivers):** Lock-free SPSC scancode queue and PS/2 keyboard IRQ 1 handling.
- **Section 23 (Trace Analysis):** Formalized trace grammar, timeline CSV export, and Python Pandas/Matplotlib parsing script.
- **Section 34 (Repeatable Tests):** Custom integration test `diagnostics_trace` passing 100% in QEMU.

---

## Files Changed
- `src/diagnostics.rs`: Added `TraceKind`, `TraceRecord`, `TraceRingBuffer` (512-record capacity), `TRACE_BUFFER` mutex singleton, `record_trace()`, `copy_to_slice()`, and updated all `trace_*` helpers to record into the ring buffer.
- `src/shell.rs`: Added `cmd_dmesg()` supporting `dmesg` and `trace` with `--clear` (`-c`) and `--all` (`-a`) flags; registered in `cmd_help()` and `execute_command()`.
- `tests/diagnostics_trace.rs`: Added `diag::4_trace_ring_buffer` unit test testing zero-allocation inspection via `copy_to_slice()` and FIFO wrap-around across $512 + 10$ records.
- `docs/TRACE_FORMAT.md`: Authored formal schema specification for machine-parseable trace lines and in-kernel ring buffer structures.
- `scripts/parse_trace.py`: Fixed fallback dictionary handling for benchmark metadata; verified with Python 3.11.
- `traces/step_v3_input_logging.log`: Captured live UEFI boot trace in QEMU OVMF (boot time 716.322 ms @ 2500 MHz).
- `BOOT_TIMES.md`: Recorded Step V3 boot time metrics.
- `PROGRESS.md`: Updated milestone status for Step V3.

---

## How It Works

### Plain English (Student Viva Explanation)
When debugging an operating system or evaluating thread scheduling and locks, we cannot rely on a debugger because stopping the CPU interferes with timer interrupts and real-time execution. Instead, operating systems use **tracing**: emitting lightweight, timestamped event records whenever significant actions occur (such as switching threads, allocating memory, or waiting for a spinlock).

In Step V3, we built a dual-channel logging system:
1. **The Serial Channel:** Whenever `kinfo!`, `kwarn!`, or `trace_switch()` is called, the kernel prints a formatted line over the serial port. Host test scripts capture these lines to plot Gantt charts and calculate thread scheduling statistics.
2. **The In-Kernel Trace Ring Buffer:** On physical bare-metal hardware without a connected serial cable, how can an engineer see what happened during boot? We reserved a fixed array of 512 trace records directly in kernel memory. Because it is pre-allocated with a static size, the kernel can record trace events inside interrupt handlers without allocating any heap memory. When 512 events are exceeded, the buffer seamlessly wraps around like a clock, overwriting the oldest event. Typing `dmesg` in the VenOS shell dumps these recorded events directly onto the graphical screen.

### Technical Architecture
```
  [Kernel Subsystems]
    - Scheduler: context switch, preemption
    - Allocator: alloc, dealloc
    - Concurrency: lock acq, wait, deadlock prevent
            │
            ├──> calls `diagnostics::trace_*(...)`
            │
            ├───[Channel 1: Serial Output]───> UART 16550 Port 0x3F8 ──> Host Log File
            │                                                               │
            │                                                               ▼
            │                                                    `scripts/parse_trace.py`
            │                                                               │
            │                                                               ├──> summary_table.csv
            │                                                               ├──> trace_metrics.json
            │                                                               └──> plots/*.png
            │
            └───[Channel 2: In-Memory BSS]───> `TraceRingBuffer` (512 records, FIFO wrap)
                                                    │
                                                    ▼
                                            Shell `dmesg` Command
                                            (Displays on GOP Screen)
```

---

## Verification & Trace Evidence

### 1. Repeatable Integration Test: `tests/diagnostics_trace.rs`
```text
==================================================
   CA-6 Layer 2 Diagnostics & Trace Test Suite
==================================================
diag::1_log_levels...	[ok]
diag::2_log_dispatch_macros...	[ok]
diag::3_structured_machine_traces...	[ok]
diag::4_trace_ring_buffer...	[ok]
==================================================
CA-6 Diagnostics & Tracing Test: ALL PASSED
==================================================
```
- **Log Levels:** Validated string formatting and ordering (`DEBUG < INFO < WARN < ERROR`).
- **Log Macros:** Dispatched `kinfo!`, `kwarn!`, `kerror!`, `kdebug!` to display and serial without faults.
- **Structured Traces:** Emitted `SWITCH`, `TASK_STATE`, `ALLOC`, `LOCK_WAIT`, `LOCK_ACQ`, `LOCK_REL`, `DEADLOCK_PREVENT`, and `DEALLOC`.
- **Ring Buffer Wrap-around:** Verified zero-allocation slice copy (`copy_to_slice`), capacity clamping at 512, cumulative counting ($512 + 10 = 522$), and oldest-record FIFO replacement.

### 2. Python Trace Parser Verification (`scripts/parse_trace.py`)
Executed with Python 3.11.9:
```text
========================================================================
          MicroOS CA-6 Flagship Performance Evaluation Report
========================================================================
[EXPORT] Performance summary table saved to: results/summary_table.csv
[EXPORT] Trace metrics JSON saved to: results/trace_metrics.json
[PLOTS] Generated 4 high-resolution evaluation figures in results/plots/
```

### 3. Live UEFI Boot in QEMU OVMF (`traces/step_v3_input_logging.log`)
- Kernel ELF size: 10,130,360 bytes (9.66 MB).
- UEFI GPT FAT32 Image: 11,599,872 bytes (11.06 MB).
- Calibrated TSC frequency: **2500 MHz**.
- Elapsed TSC ticks: 1,791,075,595.
- Boot time to shell prompt: **716.322 ms**.

---

## Gap Analysis Matrix Status After Step V3

| Gap ID | Description | Status | Resolution |
|--------|-------------|--------|------------|
| **GAP-08** | In-kernel circular trace ring buffer | **RESOLVED** | Implemented static 512-entry `TraceRingBuffer` with zero-allocation `copy_to_slice()` in `src/diagnostics.rs`. |
| **GAP-09** | Formalized trace format specification | **RESOLVED** | Created `docs/TRACE_FORMAT.md` standardizing line grammar, event taxonomy, and CSV mapping. |
| **GAP-10** | In-shell trace inspection command (`dmesg`) | **RESOLVED** | Implemented `dmesg` / `trace` command in `src/shell.rs` with `-a` and `-c` flags. |
| **GAP-11** | Python trace evaluation tool compatibility | **RESOLVED** | Verified `scripts/parse_trace.py` with Python 3.11, generating CSV, JSON, and evaluation plots. |

---

## Next Steps
Proceed to **Step V4: Memory Allocator Verification & CA-6 Layer 8 Hardening**:
- Verify hand-written intrusive linked list allocator (`src/allocator/linked_list.rs`) and sequential bump allocator (`src/allocator/bump.rs`).
- Implement allocation failure / out-of-memory handling test (TC5: 100-block workload + OOM stress).
- Validate structured `ALLOC` and `DEALLOC` trace emission during heap operations.
- Author `docs/STEP_V04_memory_allocator.md`.
