//! CA-6 Layer 3: Kernel Thread Abstraction & Execution Context
//!
//! Specification: CA-6 Section 9 (Layer 3) & Section 10 (Task Management)
//!
//! Represents a preemptible kernel thread of execution with:
//! - Unique thread identifier (`ThreadId`)
//! - Explicit lifecycle states (`Ready`, `Running`, `Blocked`, `Terminated`)
//! - Strict state machine transition validation (TC5 failure testing)
//! - Saved CPU callee-saved registers & instruction pointer (`CpuContext`)
//! - Dedicated heap-allocated kernel stack with stack canary overflow detection (TC3)
//! - Scheduling accounting (runtime ticks, wait ticks, context switch count)

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};

/// Default kernel stack size per thread: 16 KiB.
pub const KTHREAD_STACK_SIZE: usize = 16 * 1024;

/// Stack canary magic value placed at the base of the thread stack (TC3 stack guard).
pub const STACK_CANARY_MAGIC: u64 = 0xCA60_DEAD_BEEF_0001;

/// Unique thread identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ThreadId(pub u64);

impl ThreadId {
    pub fn new() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        ThreadId(NEXT_ID.fetch_add(1, Ordering::Relaxed))
    }
}

/// CA-6 Four-State Thread Lifecycle Model (Section 9, Layer 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadState {
    Ready,
    Running,
    Blocked,
    Terminated,
}

impl ThreadState {
    pub const fn as_str(&self) -> &'static str {
        match self {
            ThreadState::Ready => "READY",
            ThreadState::Running => "RUNNING",
            ThreadState::Blocked => "BLOCKED",
            ThreadState::Terminated => "TERMINATED",
        }
    }
}

/// Hardware CPU execution context preserved across context switches.
/// Follows System V AMD64 ABI callee-saved register conventions.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct CpuContext {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub rbx: u64,
    pub rbp: u64,
    pub rsp: u64,
    pub rflags: u64,
    pub rip: u64,
}

/// Scheduling & runtime performance metrics for CA-6 evaluation.
#[derive(Debug, Clone, Copy, Default)]
pub struct ThreadMetrics {
    pub run_ticks: u64,
    pub wait_ticks: u64,
    pub switch_count: u64,
}

/// Runnable kernel thread abstraction.
pub struct KThread {
    pub id: ThreadId,
    pub name: String,
    pub state: ThreadState,
    pub context: CpuContext,
    pub stack: Vec<u8>,
    pub time_slice_remaining: u32,
    pub metrics: ThreadMetrics,
}

impl KThread {
    /// Create a new kernel thread with a dedicated stack and initialized context.
    pub fn new(name: &str, entry_point: extern "C" fn()) -> Self {
        let mut stack = vec![0u8; KTHREAD_STACK_SIZE];

        // Plant the 64-bit stack canary at the base (lowest address) of the stack
        let canary_bytes = STACK_CANARY_MAGIC.to_le_bytes();
        stack[0..8].copy_from_slice(&canary_bytes);

        // Stack grows downward from highest address towards base.
        // Align the top of the stack to 16 bytes conforming to System V AMD64 ABI.
        let stack_top = ((stack.as_ptr() as usize + KTHREAD_STACK_SIZE) & !0xF) as *mut u64;

        // Prepare return trampoline on stack:
        // stack_top[-1] = kthread_exit (popped if entry_point returns)
        // stack_top[-2] = entry_point  (popped by switch_context ret)
        // SAFETY: `stack_top` points to the top of our newly allocated 16 KiB buffer.
        // Offsets -1 (top - 8 bytes) and -2 (top - 16 bytes) are strictly within
        // the allocated memory bounds of `stack`.
        unsafe {
            stack_top.sub(1).write(kthread_exit as usize as u64);
            stack_top.sub(2).write(entry_point as usize as u64);
        }

        let mut context = CpuContext::default();
        // Set rsp to the entry_point slot (16-byte aligned before switch_context ret)
        context.rsp = unsafe { stack_top.sub(2) as u64 };
        context.rip = entry_point as usize as u64;
        context.rflags = 0x202; // IF (Interrupt Flag) enabled

        KThread {
            id: ThreadId::new(),
            name: String::from(name),
            state: ThreadState::Ready,
            context,
            stack,
            time_slice_remaining: 10, // Default 10 ticks
            metrics: ThreadMetrics::default(),
        }
    }

    /// Create a descriptor for the main boot thread (already running on CPU).
    pub fn new_main(name: &str) -> Self {
        KThread {
            id: ThreadId::new(),
            name: String::from(name),
            state: ThreadState::Running,
            context: CpuContext::default(),
            stack: Vec::new(),
            time_slice_remaining: 10,
            metrics: ThreadMetrics::default(),
        }
    }

    /// Explicitly transition the thread to a new lifecycle state.
    /// Rejects invalid state transitions with a descriptive error (TC5 failure testing).
    pub fn transition_to(&mut self, next: ThreadState) -> Result<(), &'static str> {
        // First check stack integrity
        if self.check_stack_overflow() {
            return Err("Stack overflow detected via corrupted stack canary!");
        }

        let valid = match (self.state, next) {
            // Ready transitions
            (ThreadState::Ready, ThreadState::Running) => true,
            // Running transitions
            (ThreadState::Running, ThreadState::Ready) => true,
            (ThreadState::Running, ThreadState::Blocked) => true,
            (ThreadState::Running, ThreadState::Terminated) => true,
            // Blocked transitions
            (ThreadState::Blocked, ThreadState::Ready) => true,
            // Re-entrant same-state (no-op)
            (s, n) if s == n => true,
            // Any other transition is illegal
            _ => false,
        };

        if valid {
            crate::diagnostics::trace_task_state(self.id.0, next.as_str());
            self.state = next;
            Ok(())
        } else {
            Err("Invalid thread lifecycle state transition attempted!")
        }
    }

    /// Check whether the stack canary at the base of the thread stack is intact.
    /// Returns `true` if an overflow has corrupted the canary.
    pub fn check_stack_overflow(&self) -> bool {
        if self.stack.len() < 8 {
            return true;
        }
        let mut canary_bytes = [0u8; 8];
        canary_bytes.copy_from_slice(&self.stack[0..8]);
        let val = u64::from_le_bytes(canary_bytes);
        val != STACK_CANARY_MAGIC
    }

    /// Record runtime ticks consumed by the thread.
    pub fn record_run_ticks(&mut self, ticks: u64) {
        self.metrics.run_ticks = self.metrics.run_ticks.saturating_add(ticks);
        self.time_slice_remaining = self.time_slice_remaining.saturating_sub(ticks as u32);
    }

    /// Record wait ticks elapsed while waiting in the ready queue.
    pub fn record_wait_ticks(&mut self, ticks: u64) {
        self.metrics.wait_ticks = self.metrics.wait_ticks.saturating_add(ticks);
    }

    /// Increment the number of times this thread was context switched.
    pub fn record_switch(&mut self) {
        self.metrics.switch_count = self.metrics.switch_count.saturating_add(1);
    }

    /// Reset time slice to default or scheduler quantum.
    pub fn reset_quantum(&mut self, quantum: u32) {
        self.time_slice_remaining = quantum;
    }
}

/// Trampoline exit function executed when a kernel thread function returns.
pub extern "C" fn kthread_exit() -> ! {
    crate::scheduler::exit_current_thread();
}

#[cfg(test)]
mod tests {
    use super::*;

    extern "C" fn dummy_thread_entry() {}

    #[test_case]
    fn test_thread_id_generation() {
        let id1 = ThreadId::new();
        let id2 = ThreadId::new();
        assert!(id2.0 > id1.0);
    }

    #[test_case]
    fn test_kthread_initialization() {
        let t = KThread::new("worker_1", dummy_thread_entry);
        assert_eq!(t.name, "worker_1");
        assert_eq!(t.state, ThreadState::Ready);
        assert_eq!(t.time_slice_remaining, 10);
        assert_eq!(t.stack.len(), KTHREAD_STACK_SIZE);
        assert_eq!(t.context.rsp % 16, 0); // System V stack alignment check
        assert_eq!(t.context.rip, dummy_thread_entry as usize as u64);
        assert!(!t.check_stack_overflow());
    }

    #[test_case]
    fn test_thread_valid_state_transitions() {
        let mut t = KThread::new("worker_2", dummy_thread_entry);
        assert_eq!(t.state, ThreadState::Ready);

        // Ready -> Running
        assert!(t.transition_to(ThreadState::Running).is_ok());
        assert_eq!(t.state, ThreadState::Running);

        // Running -> Blocked
        assert!(t.transition_to(ThreadState::Blocked).is_ok());
        assert_eq!(t.state, ThreadState::Blocked);

        // Blocked -> Ready
        assert!(t.transition_to(ThreadState::Ready).is_ok());
        assert_eq!(t.state, ThreadState::Ready);

        // Ready -> Running
        assert!(t.transition_to(ThreadState::Running).is_ok());

        // Running -> Terminated
        assert!(t.transition_to(ThreadState::Terminated).is_ok());
        assert_eq!(t.state, ThreadState::Terminated);
    }

    #[test_case]
    fn test_thread_invalid_state_transitions_rejected() {
        let mut t = KThread::new("worker_fail", dummy_thread_entry);

        // Ready -> Blocked directly (illegal)
        assert!(t.transition_to(ThreadState::Blocked).is_err());

        // Ready -> Terminated directly (illegal)
        assert!(t.transition_to(ThreadState::Terminated).is_err());

        // Move to Terminated legally
        assert!(t.transition_to(ThreadState::Running).is_ok());
        assert!(t.transition_to(ThreadState::Terminated).is_ok());

        // Terminated -> Running (illegal)
        assert!(t.transition_to(ThreadState::Running).is_err());
        // Terminated -> Ready (illegal)
        assert!(t.transition_to(ThreadState::Ready).is_err());
    }

    #[test_case]
    fn test_stack_canary_overflow_detection() {
        let mut t = KThread::new("stack_guard_test", dummy_thread_entry);
        assert!(!t.check_stack_overflow());

        // Simulate stack corruption overwriting canary
        t.stack[0] = 0xAA;
        t.stack[1] = 0xBB;
        assert!(t.check_stack_overflow());

        // State transition should be rejected due to corrupted canary
        assert!(t.transition_to(ThreadState::Running).is_err());
    }

    #[test_case]
    fn test_thread_metrics_accounting() {
        let mut t = KThread::new("metrics_test", dummy_thread_entry);
        t.record_run_ticks(3);
        assert_eq!(t.metrics.run_ticks, 3);
        assert_eq!(t.time_slice_remaining, 7);

        t.record_wait_ticks(15);
        assert_eq!(t.metrics.wait_ticks, 15);

        t.record_switch();
        t.record_switch();
        assert_eq!(t.metrics.switch_count, 2);

        t.reset_quantum(20);
        assert_eq!(t.time_slice_remaining, 20);
    }
}
