# CA-6 Step A2: Kernel Diagnostics & Structured Tracing (Layer 2)

> **Specification Reference:** CA-6 Section 9 (Layer 2 - Kernel Console and Diagnostics) & Section 23 (Trace Analysis)

---

## 1. Goal

The objective of Step A2 is to implement a dual-channel kernel diagnostics and machine-parseable structured tracing engine that makes the internal mechanisms of the operating system completely transparent and auditable. Step A2 delivers:
1. Log severity levels (`DEBUG`, `INFO`, `WARN`, `ERROR`) output simultaneously to the VGA text display and the 115200 baud UART serial port, with each line prefixed by the hardware timer tick count (`[LEVEL tick=N]`).
2. A formal, machine-parseable trace format (`[TRACE] tick=N event=TYPE ...`) emitted over serial for automated offline analysis.
3. An automated QEMU serial trace capture script (`scripts/run_trace.ps1`).
4. An automated trace log parsing and CSV extraction tool (`scripts/parse_trace.py`).
5. A comprehensive trace format specification ([docs/TRACE_FORMAT.md](file:///c:/Users/Venugopal/OneDrive/Desktop/micro_os-main/docs/TRACE_FORMAT.md)).

---

## 2. Assignment Sections Addressed

- **Section 9 (Layer 2 – Kernel Console and Diagnostics):**
  - Informational logging (`kinfo!`).
  - Warning and error diagnostics (`kwarn!`, `kerror!`).
  - Structured task and debugging output (`kdebug!`, `ktrace!`).
  - Monotonic hardware tick synchronization via 8253 PIT timer interrupt counter.
- **Section 23 (Trace Analysis & Evidence-Based Evaluation):**
  - Line-delimited key-value event taxonomy (`SWITCH`, `TASK_STATE`, `ALLOC`, `DEALLOC`, `LOCK_ACQ`, `LOCK_REL`, `LOCK_WAIT`, `DEADLOCK_PREVENT`, `QUANTUM_EXPIRE`).
  - Python trace parser converting raw serial `.log` files to Pandas/CSV datasets with event distribution tables and tick ranges.
- **Section 15 (Testing Strategy):**
  - Integration test `tests/diagnostics_trace.rs` automatically verifying log macros, trace helpers, and serial outputs.

---

## 3. Files Created / Modified

| File | Type | Purpose |
|:---|:---|:---|
| `src/diagnostics.rs` | New | Layer 2 diagnostic logging (`LogLevel`, `log()`, `trace()`, `kinfo!`, `kwarn!`, `kerror!`, `kdebug!`, `ktrace!`) and unit tests. |
| `src/lib.rs` | Modified | Registered `pub mod diagnostics;` in the kernel crate root. |
| `src/shell.rs` | Modified | Added `ca6 trace` subcommand to exercise the diagnostics and tracing engine live from the shell prompt. |
| `docs/TRACE_FORMAT.md` | New | Formal trace format specification with event schemas, grammar rules, and field dictionaries. |
| `scripts/run_trace.ps1` | New | Automated PowerShell harness to execute QEMU with `-serial file:traces/<name>.log` and auto-terminate after capture. |
| `scripts/parse_trace.py` | New | Python trace parser transforming serial `.log` files into CSV datasets (`results/step_a2_trace_events.csv`) with summary reports. |
| `tests/diagnostics_trace.rs` | New | Automated QEMU integration test verifying all log levels and structured machine trace events. |
| `Cargo.toml` | Modified | Registered `[[test]] name = "diagnostics_trace"`. |
| `traces/step_a2_diagnostics.log` | New | Real QEMU execution trace captured during automated test and script execution. |
| `results/step_a2_trace_events.csv` | New | Exported CSV table of parsed trace events for tabular data analysis. |
| `PROGRESS.md` | Modified | Updated progress tracker with Step A2 status, test counts, and decisions. |

---

## 4. How It Works

### Plain English Explanation (Viva Guide)
Imagine operating a train network in the dark. If a train stops, you do not know whether it ran out of fuel, had an engine fault, or was waiting at a red signal. Layer 2 acts as the dispatch radio and black-box telemetry recorder:
1. **Human Diagnostics (Console Logs):** Whenever the kernel boots, notices low memory, or detects an error, it broadcasts a human-readable message to both the station screen (VGA display) and the telegraph line (UART serial port), e.g., `[INFO tick=120] System boot complete`. The `tick=120` timestamp proves exactly when it happened.
2. **Machine Tracing (Black-Box Telemetry):** Whenever a task is switched, memory is allocated, or a lock is contested, the kernel emits a single-line telegraph message starting with `[TRACE]`, followed by structured key-value pairs (e.g., `[TRACE] tick=120 event=SWITCH from=1 to=2 reason=quantum`).
3. **Automated Analysis:** Our Python tool (`scripts/parse_trace.py`) reads this black-box telemetry, counts every switch, allocation, and lock wait, and exports a CSV spreadsheet ready for graphing and statistical analysis.

### Technical Architecture Diagram

```
+-------------------------------------------------------------------------+
|                              Kernel Code                                |
|   kinfo!, kwarn!, kerror!, kdebug!             ktrace!, trace_switch()  |
+--------------------+------------------------------------+---------------+
                     |                                    |
                     v                                    v
+------------------------------------+   +--------------------------------+
|       crate::diagnostics::log      |   |    crate::diagnostics::trace   |
|   - Fetch PIT timer_ticks()        |   |   - Fetch PIT timer_ticks()    |
|   - Format "[LEVEL tick=N] msg"    |   |   - Format "[TRACE] tick=N .." |
+---------+--------------------+-----+   +----------------+---------------+
          |                    |                          |
          v                    v                          v
+-------------------+   +-------------------------------------------------+
| VGA Text Buffer   |   |           UART 16550 Serial Port                |
| (Screen Display)  |   |           (COM1 @ I/O port 0x3F8)               |
+-------------------+   +-------------------------+-----------------------+
                                                  |
                                                  v
                               +------------------------------------------+
                               |     QEMU: -serial file:traces/<name>.log |
                               +------------------+-----------------------+
                                                  |
                                                  v
                               +------------------------------------------+
                               |         scripts/parse_trace.py           |
                               |  - Summary report (event distribution)   |
                               |  - CSV export (results/<name>.csv)       |
                               +------------------------------------------+
```

Every diagnostic and trace function reads `crate::interrupts::timer_ticks()`, which loads an `AtomicU64` incremented on every 8253 PIT IRQ0 interrupt. All string formatting uses standard Rust `format_args!` and delegates to the thread-safe, interrupt-safe `_print` routines in `vga_buffer` and `serial`.

---

## 5. Verification Commands & Real Output

### A. Dedicated Layer 2 Test Run (`cargo test --test diagnostics_trace`)
```powershell
cargo test --test diagnostics_trace
```
**Output:**
```
==================================================
   CA-6 Layer 2 Diagnostics & Trace Test Suite
==================================================
diag::1_log_levels...	[ok]
diag::2_log_dispatch_macros...	[INFO  tick=0] System boot diagnostics test message: info
[WARN  tick=0] System resource warning threshold test message: warn
[ERROR tick=0] Controlled error diagnostic test message: error
[DEBUG tick=0] Low-level register trace test message: debug
[ok]
diag::3_structured_machine_traces...	[TRACE] tick=0 event=SWITCH from=1 to=2 reason=quantum
[TRACE] tick=0 event=TASK_STATE task=2 state=RUNNING
[TRACE] tick=1 event=ALLOC size=128 ok=1
[TRACE] tick=1 event=LOCK_WAIT task=2 lock=SPINLOCK_0 waited=3
[TRACE] tick=1 event=LOCK_ACQ task=2 lock=SPINLOCK_0
[TRACE] tick=1 event=LOCK_REL task=2 lock=SPINLOCK_0
[TRACE] tick=1 event=DEADLOCK_PREVENT task=2 req_rank=10 held_rank=20
[TRACE] tick=1 event=DEALLOC size=128
[ok]
==================================================
CA-6 Diagnostics & Tracing Test: ALL PASSED
==================================================
```

### B. Python Trace Log Parser Execution (`scripts/parse_trace.py`)
```powershell
py -3 scripts\parse_trace.py --trace traces\step_a2_diagnostics.log --csv results\step_a2_trace_events.csv
```
**Output:**
```
============================================================
      MicroOS Trace & Diagnostic Analysis Report
============================================================
Source Trace File:   traces\step_a2_diagnostics.log
Total Trace Events:  8
Total Console Logs:  4
------------------------------------------------------------
Trace Event Distribution:
  - ALLOC               :     1
  - DEADLOCK_PREVENT    :     1
  - DEALLOC             :     1
  - LOCK_ACQ            :     1
  - LOCK_REL            :     1
  - LOCK_WAIT           :     1
  - SWITCH              :     1
  - TASK_STATE          :     1

Tick Range: min=0, max=1, delta=1

Diagnostic Logs by Level:
  - [DEBUG]:     1
  - [ERROR]:     1
  - [INFO]:     1
  - [WARN]:     1
============================================================
[EXPORT] Trace CSV saved to: results\step_a2_trace_events.csv
```

### C. Exported Tabular Dataset (`results/step_a2_trace_events.csv`)
```csv
line,tick,event,from,held_rank,lock,ok,reason,req_rank,size,state,task,to,waited
10,0,SWITCH,1,,,,quantum,,,,,2,
11,0,TASK_STATE,,,,,,,,RUNNING,2,,
12,1,ALLOC,,,,1,,,128,,,,
13,1,LOCK_WAIT,,,SPINLOCK_0,,,,,,2,,3
14,1,LOCK_ACQ,,,SPINLOCK_0,,,,,,2,,
15,1,LOCK_REL,,,SPINLOCK_0,,,,,,2,,
16,1,DEADLOCK_PREVENT,,20,,,,10,,,2,,
17,1,DEALLOC,,,,,,,128,,,,
```

### D. Full Test Suite Regression (All 61 Tests Passed)
```powershell
cargo test
```
- 23 / 23 unit tests passed in `src/lib.rs` (including log formatting, ordering, and trace generation).
- 38 / 38 integration tests passed across all 11 integration test targets (`ca6_demo`, `diagnostics_trace`, `filesystem`, `hardware`, `heap_allocation`, `networking`, `pkg_apt`, `smoke_integration`, `snap_store`, `stack_overflow`, `wifi`).

---

## 6. Limitations

- Trace events in Step A2 are emitted synchronously over the UART serial port at 115200 baud. Serial I/O adds a few microseconds of latency per event; in high-frequency profiling loops, an in-memory ring buffer trace sink can be introduced if serial throughput becomes a bottleneck.

---

## 7. What is Next (Step A3)

In **Step A3 (Task Representation and Ready Queue - Layers 3 and 4)**:
- Fully implement `KThread` task management with isolated heap-allocated stacks and stack overflow protection.
- Implement explicit lifecycle state machine transitions with boundary validation.
- Expand `ReadyQueue` with priority ordering or multi-queue structures.
- Implement unit and integration tests for queue ordering, state validation, and invalid transition rejection (TC3 & TC5).
