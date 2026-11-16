# VenOS Machine Trace Format Specification (CA-6 Layer 2 & Section 23)

## 1. Overview & Principles

VenOS implements a dual-channel diagnostics architecture designed for real-time observability, automated post-mortem grading, and performance evaluation:
1. **Serial Port Channel (UART 16550):** Formatted lines emitted with prefix `[TRACE]` over 115200-8N1 serial port for automated host capture into `.log` / `.csv` files.
2. **In-Kernel Circular Ring Buffer:** A zero-heap-allocation, static memory ring buffer (`TraceRingBuffer`) of 512 compact records (`TraceRecord`) residing in kernel BSS, readable interactively via the shell `dmesg` / `trace` command.

### Design Principles
- **Zero Heap Overhead in ISR:** Trace event generation never allocates dynamic heap memory (`Box`, `Vec`, `String`). Scancode queuing and timer ticks operate with fixed structures.
- **Deterministic Serialization:** Space-separated key-value pairs allow fast, regex-free or regular expression parsing across Python, Bash, and Rust test runners.
- **Monotonic Dual Timestamps:** Every event is stamped with:
  - `tick`: PIT timer ticks (10 ms resolution @ 100 Hz).
  - `tsc`: Raw 64-bit Time Stamp Counter ticks calibrated against PIT Channel 2.

---

## 2. Serial Emission Format

Every structured trace line adheres to the single-line grammar:
```text
[TRACE] tick=<TICK> event=<EVENT_TYPE> <KEY_1>=<VAL_1> <KEY_2>=<VAL_2> ...
```

### Regular Expression Grammar
```regex
^\[TRACE\] tick=(?P<tick>\d+) event=(?P<event>[A-Z_]+)( (?P<fields>.*))?$
```

---

## 3. Catalog of Trace Events

### 3.1 Task Scheduling & Context Switching
| Event Name | Key-Value Fields | Description | Example |
|------------|------------------|-------------|---------|
| `SWITCH` | `from=<u64> to=<u64> reason=<str>` | Emitted by scheduler assembly context switch (`switch_context`). `reason` can be `quantum`, `yield`, `term`, `block`. | `[TRACE] tick=42 event=SWITCH from=1 to=2 reason=quantum` |
| `TASK_STATE` | `task=<u64> state=<str>` | Emitted on task lifecycle transition (`READY`, `RUNNING`, `BLOCKED`, `TERMINATED`). | `[TRACE] tick=42 event=TASK_STATE task=2 state=RUNNING` |
| `QUANTUM_EXPIRE` | `task=<u64> consumed=<u32>` | Emitted by PIT timer ISR when active thread exhausts its allotted round-robin quantum. | `[TRACE] tick=50 event=QUANTUM_EXPIRE task=2 consumed=5` |

### 3.2 Dynamic Memory Allocation (Layer 8)
| Event Name | Key-Value Fields | Description | Example |
|------------|------------------|-------------|---------|
| `ALLOC` | `size=<usize> ok=<0\|1>` | Emitted on heap allocation requests. `ok=1` indicates success; `ok=0` indicates out-of-memory. | `[TRACE] tick=12 event=ALLOC size=64 ok=1` |
| `DEALLOC` | `size=<usize>` | Emitted on memory deallocation and return to the allocator's free list. | `[TRACE] tick=14 event=DEALLOC size=64` |

### 3.3 Synchronization & Concurrency (Track C)
| Event Name | Key-Value Fields | Description | Example |
|------------|------------------|-------------|---------|
| `LOCK_ACQ` | `task=<u64> lock=<str>` | Emitted immediately after successful mutual exclusion lock acquisition. | `[TRACE] tick=18 event=LOCK_ACQ task=1 lock=VFS` |
| `LOCK_REL` | `task=<u64> lock=<str>` | Emitted immediately after releasing the lock. | `[TRACE] tick=22 event=LOCK_REL task=1 lock=VFS` |
| `LOCK_WAIT` | `task=<u64> lock=<str> waited=<u64>` | Emitted when a thread experiences contention and waits for `waited` ticks before acquiring the lock. | `[TRACE] tick=20 event=LOCK_WAIT task=2 lock=VFS waited=4` |
| `DEADLOCK_PREVENT` | `task=<u64> req_rank=<u32> held_rank=<u32>` | Emitted when strict lock ranking hierarchy rejects an out-of-order acquisition, preventing potential deadlock. | `[TRACE] tick=25 event=DEADLOCK_PREVENT task=3 req_rank=10 held_rank=20` |

---

## 4. In-Memory Ring Buffer (`TraceRingBuffer`)

Located in `src/diagnostics.rs`:
```rust
#[derive(Debug, Clone, Copy)]
pub struct TraceRecord {
    pub tick: u64,
    pub tsc: u64,
    pub kind: TraceKind,
    pub task_id: u64,
    pub param1: u64,
    pub param2: u64,
}
```

- **Capacity:** 512 entries statically allocated in kernel BSS (zero heap usage).
- **Wrap Policy:** FIFO overwrite (oldest entries are overwritten when capacity is exceeded).
- **Access API:**
  - `record_trace(kind, task_id, param1, param2)`: Push record with live tick and TSC.
  - `snapshot()`: Extract chronological vector of records.
  - `clear()`: Reset pointers.
- **Shell Commands:**
  - `dmesg`: Displays the most recent 30 events.
  - `dmesg -a` / `dmesg --all`: Dumps the entire circular buffer.
  - `dmesg -c` / `dmesg --clear`: Clears the buffer.

---

## 5. Python Parser Integration (`scripts/parse_trace.py`)

A host Python tool parses raw serial logs into structured Pandas/CSV summaries:
```bash
# Human-readable summary
python scripts/parse_trace.py traces/step_v2_hw_discovery.log --summary

# Export structured JSON
python scripts/parse_trace.py traces/step_v2_hw_discovery.log --json out.json

# Export CSV timeline
python scripts/parse_trace.py traces/step_v2_hw_discovery.log --csv timeline.csv
```

### CSV Schema
```csv
timestamp_tick,event,task_id,param1,param2,raw_line
```
