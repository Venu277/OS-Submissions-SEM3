# MicroOS — Machine-Parseable Trace Specification

> **Specification Reference:** CA-6 Section 9 (Layer 2 - Kernel Console and Diagnostics) & Section 23 (Trace Analysis)

All trace messages emitted over the UART 16550 serial port follow a strict, line-delimited key-value grammar suitable for automated streaming ingestion by Python / Pandas, awk, and test evaluation harnesses.

---

## 1. Line Grammar

```text
[TRACE] tick=<INT> event=<EVENT_TYPE> <KEY_1>=<VAL_1> <KEY_2>=<VAL_2> ...
```

### Grammar Rules
1. **Header Prefix:** Every machine-readable trace line begins with the literal identifier `[TRACE]`.
2. **Monotonic Tick:** `tick=<u64>` represents the 8253 PIT timer tick count at the exact moment the event occurred.
3. **Event Identifier:** `event=<UPPERCASE_STRING>` specifies the event category.
4. **Key-Value Pairs:** Additional attributes are serialized as space-separated `<key>=<value>` tokens without whitespace surrounding the `=` sign.
5. **No Embedded Newlines:** Each trace event occupies exactly one physical line.

---

## 2. Event Taxonomy & Field Definitions

### 1. `SWITCH` — Task Context Switch
Emitted whenever the scheduler switches CPU execution between threads.
```text
[TRACE] tick=120 event=SWITCH from=1 to=2 reason=quantum
```
| Key | Type | Description |
|:---|:---|:---|
| `tick` | `u64` | Current PIT interrupt tick count |
| `from` | `u64` | Outgoing `ThreadId` |
| `to` | `u64` | Incoming `ThreadId` |
| `reason` | `str` | Reason for context switch (`quantum`, `yield`, `block`, `terminate`) |

---

### 2. `TASK_STATE` — Thread Lifecycle State Transition
Emitted on any transition in the 4-state lifecycle model.
```text
[TRACE] tick=121 event=TASK_STATE task=2 state=RUNNING
```
| Key | Type | Description |
|:---|:---|:---|
| `task` | `u64` | Target `ThreadId` |
| `state` | `str` | Destination state (`READY`, `RUNNING`, `BLOCKED`, `TERMINATED`) |

---

### 3. `ALLOC` & `DEALLOC` — Dynamic Memory Allocation
Emitted on kernel heap allocation and deallocation requests.
```text
[TRACE] tick=125 event=ALLOC size=64 ok=1
[TRACE] tick=140 event=DEALLOC size=64
```
| Key | Type | Description |
|:---|:---|:---|
| `size` | `usize` | Allocation request size in bytes |
| `ok` | `u8` | `1` if allocation succeeded; `0` if heap memory was exhausted |

---

### 4. `LOCK_ACQ`, `LOCK_REL` & `LOCK_WAIT` — Concurrency Synchronization
Emitted by spinlocks and ranked locks (Track C Concurrency Specialization).
```text
[TRACE] tick=130 event=LOCK_WAIT task=2 lock=VGA waited=4
[TRACE] tick=134 event=LOCK_ACQ task=2 lock=VGA
[TRACE] tick=138 event=LOCK_REL task=2 lock=VGA
```
| Key | Type | Description |
|:---|:---|:---|
| `task` | `u64` | Thread acquiring or releasing the lock |
| `lock` | `str` | Identifier of the lock (e.g. `VGA`, `READY_QUEUE`, `ALLOCATOR`) |
| `waited` | `u64` | Spin-wait duration in ticks (for contention analysis) |

---

### 5. `DEADLOCK_PREVENT` — Lock Ranking Violation Rejection
Emitted when strict partial order lock acquisition detects and prevents an out-of-order circular wait (Track C).
```text
[TRACE] tick=150 event=DEADLOCK_PREVENT task=2 req_rank=10 held_rank=20
```
| Key | Type | Description |
|:---|:---|:---|
| `task` | `u64` | Thread attempting the out-of-order lock acquisition |
| `req_rank` | `u32` | Requested lock rank |
| `held_rank` | `u32` | Currently held highest lock rank by the calling thread |

---

### 6. `QUANTUM_EXPIRE` — Preemptive Time-Slice Expiration
Emitted by the timer interrupt handler when a running thread consumes its assigned quantum.
```text
[TRACE] tick=160 event=QUANTUM_EXPIRE task=1 consumed=10
```
| Key | Type | Description |
|:---|:---|:---|
| `task` | `u64` | Preempted `ThreadId` |
| `consumed` | `u32` | Number of ticks elapsed in current time slice |

---

## 3. Parsing Specification

The trace format is designed for parsing with standard regular expressions:
```python
import re

TRACE_REGEX = re.compile(r"^\[TRACE\]\s+tick=(?P<tick>\d+)\s+event=(?P<event>\w+)\s+(?P<fields>.*)$")
KV_REGEX = re.compile(r"(?P<key>\w+)=(?P<val>[^\s]+)")
```
See `scripts/parse_trace.py` for reference implementation.
