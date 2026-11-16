# Step V09: Track C Concurrency Specialization — Deadlock Prevention & Contention Telemetry (CA-6 Section 11)

## Goal
Verify and document the Track C Concurrency Engineering specialization in VenOS according to CA-6 Section 11 requirements:
1. **Mathematical Deadlock Prevention via Lock Ranking:**
   - Formal elimination of Coffman Condition 4 (Circular Wait) through strict partial order lock acquisition.
   - Each synchronization object is assigned a monotonic integer rank `LockRank = u32` (`src/sync/ranked.rs`).
   - A thread holding a lock of rank $R_{held}$ can strictly only acquire locks with rank $R_{req} > R_{held}$.
2. **Deterministic Rejection & Diagnostic Emission:**
   - Attempted out-of-order acquisitions ($R_{req} \le R_{held}$) are detected immediately in $O(1)$ time without blocking.
   - The acquisition is rejected with `Err(LockRankError::HierarchyViolation { requested_rank, currently_held_rank })`.
   - Emits a structured diagnostic trace event `DEADLOCK_PREVENT task_id=<id> req_rank=<R_req> held_rank=<R_held>` to UART serial and the in-kernel BSS circular ring buffer.
3. **LIFO Hierarchical Unwinding (`RankedLockGuard`):**
   - Implements RAII unwinding: each `RankedLockGuard` remembers `prev_rank`.
   - Upon `drop()`, the system-wide / thread-level `CURRENT_HELD_RANK` is atomically restored to the prior rank, supporting arbitrary nesting depths.
4. **Lock Contention Telemetry & Analysis:**
   - Real-time accounting of `acquisitions`, `contentions`, `spins_waited`, `max_spins`, and `hold_ticks`.
   - Live interactive in-kernel shell demonstration available via `ca6 sync` (`demo_synchronization()` in `src/sync/mod.rs`).
5. **Empirical Verification (`tests/concurrency_sync.rs`):**
   - **Test 3 (In-Order Acquisition):** Rank 10 -> Rank 20 acquired successfully in sequence; unwound cleanly back to 0.
   - **Test 4 (Deadlock Prevention):** Rank 20 held -> attempt to acquire Rank 10 immediately rejected with `HierarchyViolation` and `DEADLOCK_PREVENT` trace; Rank 20 retained and safely released.
   - **Test 5 (Three-Level Nesting):** Rank 100 -> Rank 200 -> Rank 300 acquired in strict order; reverse acquisitions at Rank 300 rejected; unwound cleanly $300 \to 200 \to 100 \to 0$.
6. **Live UEFI Boot Verification:** Verify UEFI boot in QEMU OVMF with boot timing to shell prompt (630.962 ms @ 2514 MHz).

---

## CA-6 Sections Addressed
- **Section 11 (Track C: Concurrency Specialization):** Strict lock ranking hierarchy, Coffman Condition 4 elimination, deadlock prevention rejection, contention telemetry, and hold-time metrics.
- **Section 9 (Layer 7 – Synchronization):** Full integration with hand-written spinlocks and RAII guards.
- **Section 16 (Required Test Cases TC1–TC6):** Stress testing and edge cases for concurrent lock acquisition and reverse-order deadlock prevention.
- **Section 26 (Unsafe Rust Policy):** Formal audit of lock ranking state swaps and memory ordering (`SeqCst` swap on rank updates).

---

## Files Changed / Verified
- `src/sync/ranked.rs`: Implemented `RankedLock<T>`, `RankedLockGuard<T>`, `LockRankError`, `CURRENT_HELD_RANK`, and `trace_deadlock_prevent`.
- `src/sync/mod.rs`: Verified `demo_synchronization()` implementing Parts 1 (Mutual Exclusion), 2 (Contention Metrics), and 3 (Lock Ranking Hierarchy).
- `src/shell.rs`: Verified `ca6 sync` CLI command.
- `tests/concurrency_sync.rs`: Verified in QEMU with Tests 3, 4, and 5 passing cleanly (`[ok]`).
- `traces/step_v9_track_c_concurrency.log`: Captured live UEFI boot trace in QEMU OVMF (boot time 630.962 ms @ 2514 MHz).
- `BOOT_TIMES.md`: Recorded Step V9 boot timing metrics.
- `PROGRESS.md`: Updated milestone status for Step V9.

---

## How It Works

### Plain English (Student Viva Explanation)
A **deadlock** occurs when two threads get permanently stuck waiting for each other. The classic example is the "deadly embrace":
- Thread 1 locks Lock A and waits for Lock B.
- Thread 2 locks Lock B and waits for Lock A.
Neither thread can proceed, and the system freezes completely.

Edward Coffman Jr. proved in 1971 that a deadlock requires four simultaneous conditions:
1. Mutual Exclusion (threads have exclusive access).
2. Hold and Wait (threads hold resources while waiting for more).
3. No Preemption (resources cannot be forcibly taken).
4. **Circular Wait (a closed loop of threads waiting for each other).**

In Step V9, VenOS implements **Track C: Concurrency Engineering** by mathematically destroying Condition 4 (Circular Wait):
1. **Lock Ranking:** Every lock is assigned a numerical rank (e.g. Device Driver Lock = 10, VFS Lock = 20, User Data Lock = 30).
2. **Strict Partial Ordering:** A thread is only allowed to acquire locks in strictly ascending order ($10 \to 20 \to 30$).
3. **Deadlock Prevention (Not Just Detection!):** If a thread holding Lock 20 attempts to acquire Lock 10, VenOS does not wait or freeze. It immediately intercepts the call, rejects the acquisition with an error, logs a `DEADLOCK_PREVENT` trace event, and lets the thread handle the situation safely. Because circular wait is mathematically impossible under strict ascending order, **deadlocks can never occur in VenOS!**
4. **Automatic Unwinding:** When a thread releases Lock 30, the system automatically drops its held rank back to 20, then back to 10, and finally to 0 when all locks are released.

### Technical Architecture
```
                  Lock Hierarchy Hierarchy (Ranks)
                  
                   [ Rank 30: User Lock ]
                             ▲
                             │  Must acquire in strictly
                             │  ascending order (R_req > R_held)
                   [ Rank 20: VFS Lock  ]
                             ▲
                             │
                   [ Rank 10: Driver Lock ]
                             ▲
                             │
                      [ Rank 0: Unlocked ]

  -------------------------------------------------------------
  Scenario A (Legal In-Order):
    Hold Rank 10 -> Acquire Rank 20 (20 > 10)  ===> SUCCESS!

  Scenario B (Illegal Reverse-Order Deadlock Hazard):
    Hold Rank 20 -> Acquire Rank 10 (10 <= 20) ===> REJECTED!
    ===> Emits: [TRACE] DEADLOCK_PREVENT req_rank=10 held_rank=20
```

---

## Verification & Test Results

### 1. Integration Test (`tests/concurrency_sync.rs`)
```
sync::3_lock_ranking_in_order_success...	[TRACE] tick=18 event=LOCK_ACQ task=1 lock=lock_device
[TRACE] tick=18 event=LOCK_ACQ task=1 lock=lock_filesystem
[TRACE] tick=18 event=LOCK_REL task=1 lock=lock_filesystem
[TRACE] tick=18 event=LOCK_REL task=1 lock=lock_device
[ok]
sync::4_lock_ranking_deadlock_prevention_reject...	[TRACE] tick=19 event=LOCK_ACQ task=1 lock=lock_filesystem
[TRACE] tick=19 event=DEADLOCK_PREVENT task=1 req_rank=10 held_rank=20
[TRACE] tick=19 event=LOCK_REL task=1 lock=lock_filesystem
[ok] (Hierarchy inversion detected and prevented!)
sync::5_three_level_hierarchy_nesting...	[TRACE] tick=19 event=LOCK_ACQ task=1 lock=level_1
[TRACE] tick=19 event=LOCK_ACQ task=1 lock=level_2
[TRACE] tick=19 event=LOCK_ACQ task=1 lock=level_3
[TRACE] tick=19 event=DEADLOCK_PREVENT task=1 req_rank=100 held_rank=300
[TRACE] tick=19 event=DEADLOCK_PREVENT task=1 req_rank=200 held_rank=300
[TRACE] tick=19 event=LOCK_REL task=1 lock=level_3
[TRACE] tick=19 event=LOCK_REL task=1 lock=level_2
[TRACE] tick=19 event=LOCK_REL task=1 lock=level_1
[ok]
==================================================
CA-6 Layer 7 & Track C Concurrency Sync: ALL PASSED
==================================================
```

### 2. Live QEMU/OVMF Boot Trace
- **Boot Time:** `BOOT_TIME_MS=630.962` (calibrated @ 2514 MHz, 1.586G ticks).
- **Log:** `traces/step_v9_track_c_concurrency.log`.
- **Status:** Track C Concurrency Specialization fully verified, `ca6 sync` CLI command ready.
