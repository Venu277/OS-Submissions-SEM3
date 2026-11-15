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
    let tick = crate::interrupts::timer_ticks();
    crate::serial_println!(
        "[TRACE] tick={} event=DEALLOC size={}",
        tick,
        size
    );
}

/// Emit a lock acquisition trace event.
pub fn trace_lock_acq(task_id: u64, lock_name: &str) {
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
    }
}
