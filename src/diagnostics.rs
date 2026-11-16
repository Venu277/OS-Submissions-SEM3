//! CA-6 Layer 2: Kernel Console Diagnostics & Structured Tracing
//!
//! Specification: CA-6 Section 9 (Layer 2 - Kernel Console and Diagnostics) & Section 23 (Trace Analysis)
//!
//! Provides:
//! - Uniform kernel log levels (`DEBUG`, `INFO`, `WARN`, `ERROR`) dispatched to both
//!   the VGA text console buffer and the UART 16550 serial port.
//! - Monotonically increasing PIT tick counter prefix on every log line (`[LEVEL tick=N]`).
//! - Machine-parseable structured tracing format over serial (`[TRACE] tick=N event=TYPE ...`)
//!   supporting automated trace capture and Python Pandas/CSV evaluation.

use core::fmt;
use spin::Mutex;

/// Classification of machine trace events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TraceKind {
    Custom = 0,
    Switch = 1,
    TaskState = 2,
    Alloc = 3,
    Dealloc = 4,
    LockAcq = 5,
    LockRel = 6,
    LockWait = 7,
    DeadlockPrevent = 8,
    QuantumExpire = 9,
}

impl TraceKind {
    pub const fn as_str(&self) -> &'static str {
        match self {
            TraceKind::Custom => "CUSTOM",
            TraceKind::Switch => "SWITCH",
            TraceKind::TaskState => "TASK_STATE",
            TraceKind::Alloc => "ALLOC",
            TraceKind::Dealloc => "DEALLOC",
            TraceKind::LockAcq => "LOCK_ACQ",
            TraceKind::LockRel => "LOCK_REL",
            TraceKind::LockWait => "LOCK_WAIT",
            TraceKind::DeadlockPrevent => "DEADLOCK_PREVENT",
            TraceKind::QuantumExpire => "QUANTUM_EXPIRE",
        }
    }
}

/// A compact, fixed-size structured trace record stored in the in-memory circular ring buffer.
#[derive(Debug, Clone, Copy)]
pub struct TraceRecord {
    pub tick: u64,
    pub tsc: u64,
    pub kind: TraceKind,
    pub task_id: u64,
    pub param1: u64,
    pub param2: u64,
}

impl Default for TraceRecord {
    fn default() -> Self {
        Self {
            tick: 0,
            tsc: 0,
            kind: TraceKind::Custom,
            task_id: 0,
            param1: 0,
            param2: 0,
        }
    }
}

/// Fixed capacity of in-kernel trace ring buffer (512 entries).
pub const TRACE_BUFFER_CAPACITY: usize = 512;

/// In-kernel circular ring buffer for historical trace inspection without dynamic heap allocation.
pub struct TraceRingBuffer {
    records: [TraceRecord; TRACE_BUFFER_CAPACITY],
    write_idx: usize,
    total_written: usize,
}

impl TraceRingBuffer {
    pub const fn new() -> Self {
        const INIT_RECORD: TraceRecord = TraceRecord {
            tick: 0,
            tsc: 0,
            kind: TraceKind::Custom,
            task_id: 0,
            param1: 0,
            param2: 0,
        };
        Self {
            records: [INIT_RECORD; TRACE_BUFFER_CAPACITY],
            write_idx: 0,
            total_written: 0,
        }
    }

    /// Push a new trace record into the circular ring buffer, overwriting oldest entry if full.
    pub fn push(&mut self, record: TraceRecord) {
        self.records[self.write_idx] = record;
        self.write_idx = (self.write_idx + 1) % TRACE_BUFFER_CAPACITY;
        self.total_written = self.total_written.saturating_add(1);
    }

    /// Reset and clear all trace records.
    pub fn clear(&mut self) {
        self.write_idx = 0;
        self.total_written = 0;
    }

    /// Number of active records currently residing in the buffer.
    pub fn count(&self) -> usize {
        self.total_written.min(TRACE_BUFFER_CAPACITY)
    }

    /// Cumulative number of trace events pushed since boot.
    pub fn total(&self) -> usize {
        self.total_written
    }

    /// Copy active records into a provided caller slice without heap allocation.
    /// Returns the number of records copied.
    pub fn copy_to_slice(&self, out: &mut [TraceRecord]) -> usize {
        let count = self.count().min(out.len());
        let start = if self.total_written > TRACE_BUFFER_CAPACITY {
            self.write_idx
        } else {
            0
        };
        for i in 0..count {
            let idx = (start + i) % TRACE_BUFFER_CAPACITY;
            out[i] = self.records[idx];
        }
        count
    }

    /// Retrieve a chronologically ordered snapshot of records.
    pub fn snapshot(&self) -> alloc::vec::Vec<TraceRecord> {
        let count = self.count();
        let mut out = alloc::vec::Vec::with_capacity(count);
        let start = if self.total_written > TRACE_BUFFER_CAPACITY {
            self.write_idx
        } else {
            0
        };
        for i in 0..count {
            let idx = (start + i) % TRACE_BUFFER_CAPACITY;
            out.push(self.records[idx]);
        }
        out
    }
}

/// Global thread-safe in-kernel trace buffer.
pub static TRACE_BUFFER: Mutex<TraceRingBuffer> = Mutex::new(TraceRingBuffer::new());

/// Record an event into the in-memory ring buffer with calibrated tick and TSC timestamps.
pub fn record_trace(kind: TraceKind, task_id: u64, param1: u64, param2: u64) {
    let tick = crate::interrupts::timer_ticks();
    let tsc = unsafe { core::arch::x86_64::_rdtsc() };
    let record = TraceRecord {
        tick,
        tsc,
        kind,
        task_id,
        param1,
        param2,
    };
    TRACE_BUFFER.lock().push(record);
}

/// Kernel diagnostic log levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Debug = 0,
    Info = 1,
    Warn = 2,
    Error = 3,
}

impl LogLevel {
    pub const fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Debug => "DEBUG",
            LogLevel::Info => "INFO ",
            LogLevel::Warn => "WARN ",
            LogLevel::Error => "ERROR",
        }
    }
}

/// Dispatch a diagnostic log message to both VGA display buffer and UART serial port.
/// Format: `[LEVEL tick=N] message`
pub fn log(level: LogLevel, args: fmt::Arguments) {
    let tick = crate::interrupts::timer_ticks();
    // Dispatch to VGA text display
    crate::println!("[{} tick={}] {}", level.as_str(), tick, args);
    // Dispatch to UART serial port
    crate::serial_println!("[{} tick={}] {}", level.as_str(), tick, args);
}

/// Emit a structured, machine-parseable event over the UART serial port.
/// Format: `[TRACE] tick=N event=EVENT_NAME key1=val1 key2=val2 ...`
pub fn trace(event: &str, fields: fmt::Arguments) {
    let tick = crate::interrupts::timer_ticks();
    crate::serial_println!("[TRACE] tick={} event={} {}", tick, event, fields);
}

/// Emit a task context switch trace event.
pub fn trace_switch(from_task: u64, to_task: u64, reason: &str) {
    record_trace(TraceKind::Switch, from_task, to_task, 0);
    let tick = crate::interrupts::timer_ticks();
    crate::serial_println!(
        "[TRACE] tick={} event=SWITCH from={} to={} reason={}",
        tick,
        from_task,
        to_task,
        reason
    );
}

/// Emit a task state change trace event.
pub fn trace_task_state(task_id: u64, state: &str) {
    record_trace(TraceKind::TaskState, task_id, 0, 0);
    let tick = crate::interrupts::timer_ticks();
    crate::serial_println!(
        "[TRACE] tick={} event=TASK_STATE task={} state={}",
        tick,
        task_id,
        state
    );
}

/// Emit a heap memory allocation trace event.
pub fn trace_alloc(size: usize, ok: bool) {
    record_trace(TraceKind::Alloc, 0, size as u64, if ok { 1 } else { 0 });
    let tick = crate::interrupts::timer_ticks();
    crate::serial_println!(
        "[TRACE] tick={} event=ALLOC size={} ok={}",
        tick,
        size,
        if ok { 1 } else { 0 }
    );
}

/// Emit a heap memory deallocation trace event.
pub fn trace_dealloc(size: usize) {
    record_trace(TraceKind::Dealloc, 0, size as u64, 0);
    let tick = crate::interrupts::timer_ticks();
    crate::serial_println!(
        "[TRACE] tick={} event=DEALLOC size={}",
        tick,
        size
    );
}

/// Emit a lock acquisition trace event.
pub fn trace_lock_acq(task_id: u64, lock_name: &str) {
    record_trace(TraceKind::LockAcq, task_id, 0, 0);
    let tick = crate::interrupts::timer_ticks();
    crate::serial_println!(
        "[TRACE] tick={} event=LOCK_ACQ task={} lock={}",
        tick,
        task_id,
        lock_name
    );
}

/// Emit a lock release trace event.
pub fn trace_lock_rel(task_id: u64, lock_name: &str) {
    record_trace(TraceKind::LockRel, task_id, 0, 0);
    let tick = crate::interrupts::timer_ticks();
    crate::serial_println!(
        "[TRACE] tick={} event=LOCK_REL task={} lock={}",
        tick,
        task_id,
        lock_name
    );
}

/// Emit a lock contention wait trace event.
pub fn trace_lock_wait(task_id: u64, lock_name: &str, waited_ticks: u64) {
    record_trace(TraceKind::LockWait, task_id, waited_ticks, 0);
    let tick = crate::interrupts::timer_ticks();
    crate::serial_println!(
        "[TRACE] tick={} event=LOCK_WAIT task={} lock={} waited={}",
        tick,
        task_id,
        lock_name,
        waited_ticks
    );
}

/// Emit a deadlock prevention reject event (Track C).
pub fn trace_deadlock_prevent(task_id: u64, requested_rank: u32, held_rank: u32) {
    record_trace(TraceKind::DeadlockPrevent, task_id, requested_rank as u64, held_rank as u64);
    let tick = crate::interrupts::timer_ticks();
    crate::serial_println!(
        "[TRACE] tick={} event=DEADLOCK_PREVENT task={} req_rank={} held_rank={}",
        tick,
        task_id,
        requested_rank,
        held_rank
    );
}

/// Emit a preemptive quantum expiration trace event (Layer 6).
pub fn trace_quantum_expire(task_id: u64, consumed_ticks: u32) {
    record_trace(TraceKind::QuantumExpire, task_id, consumed_ticks as u64, 0);
    let tick = crate::interrupts::timer_ticks();
    crate::serial_println!(
        "[TRACE] tick={} event=QUANTUM_EXPIRE task={} consumed={}",
        tick,
        task_id,
        consumed_ticks
    );
}

/// Diagnostic Log: INFO level
#[macro_export]
macro_rules! kinfo {
    ($($arg:tt)*) => {
        $crate::diagnostics::log($crate::diagnostics::LogLevel::Info, format_args!($($arg)*))
    };
}

/// Diagnostic Log: WARN level
#[macro_export]
macro_rules! kwarn {
    ($($arg:tt)*) => {
        $crate::diagnostics::log($crate::diagnostics::LogLevel::Warn, format_args!($($arg)*))
    };
}

/// Diagnostic Log: ERROR level
#[macro_export]
macro_rules! kerror {
    ($($arg:tt)*) => {
        $crate::diagnostics::log($crate::diagnostics::LogLevel::Error, format_args!($($arg)*))
    };
}

/// Diagnostic Log: DEBUG level
#[macro_export]
macro_rules! kdebug {
    ($($arg:tt)*) => {
        $crate::diagnostics::log($crate::diagnostics::LogLevel::Debug, format_args!($($arg)*))
    };
}

/// Structured Machine Trace Event
#[macro_export]
macro_rules! ktrace {
    ($event:expr, $($arg:tt)*) => {
        $crate::diagnostics::trace($event, format_args!($($arg)*))
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case]
    fn test_log_level_formatting() {
        assert_eq!(LogLevel::Debug.as_str(), "DEBUG");
        assert_eq!(LogLevel::Info.as_str(), "INFO ");
        assert_eq!(LogLevel::Warn.as_str(), "WARN ");
        assert_eq!(LogLevel::Error.as_str(), "ERROR");
    }

    #[test_case]
    fn test_log_level_ordering() {
        assert!(LogLevel::Debug < LogLevel::Info);
        assert!(LogLevel::Info < LogLevel::Warn);
        assert!(LogLevel::Warn < LogLevel::Error);
    }

    #[test_case]
    fn test_trace_helpers_execution() {
        // Test that trace helpers format cleanly without crashing
        trace_switch(1, 2, "quantum");
        trace_task_state(1, "RUNNING");
        trace_alloc(64, true);
        trace_dealloc(64);
        trace_lock_acq(1, "VGA");
        trace_lock_wait(2, "VGA", 4);
        trace_lock_rel(1, "VGA");
        trace_deadlock_prevent(2, 10, 20);

        let snap = TRACE_BUFFER.lock().snapshot();
        assert!(snap.len() >= 8, "Snapshot should contain recorded trace events");
    }

    #[test_case]
    fn test_trace_ring_buffer_wrap() {
        let mut rb = TraceRingBuffer::new();
        assert_eq!(rb.count(), 0);
        assert_eq!(rb.total(), 0);

        // Push 10 events
        for i in 0..10 {
            rb.push(TraceRecord {
                tick: i as u64,
                tsc: 1000 + i as u64,
                kind: TraceKind::Switch,
                task_id: i as u64,
                param1: 0,
                param2: 0,
            });
        }
        assert_eq!(rb.count(), 10);
        assert_eq!(rb.total(), 10);
        let s = rb.snapshot();
        assert_eq!(s.len(), 10);
        assert_eq!(s[0].task_id, 0);
        assert_eq!(s[9].task_id, 9);

        // Push until wrap-around past capacity
        for i in 10..(TRACE_BUFFER_CAPACITY + 5) {
            rb.push(TraceRecord {
                tick: i as u64,
                tsc: 1000 + i as u64,
                kind: TraceKind::Alloc,
                task_id: i as u64,
                param1: i as u64,
                param2: 1,
            });
        }
        assert_eq!(rb.count(), TRACE_BUFFER_CAPACITY);
        assert_eq!(rb.total(), TRACE_BUFFER_CAPACITY + 5);
        let s_wrap = rb.snapshot();
        assert_eq!(s_wrap.len(), TRACE_BUFFER_CAPACITY);
        // Oldest 5 entries (0..5) should have been overwritten; first element is 5
        assert_eq!(s_wrap[0].task_id, 5);
        assert_eq!(s_wrap[TRACE_BUFFER_CAPACITY - 1].task_id, (TRACE_BUFFER_CAPACITY + 4) as u64);
    }
}
