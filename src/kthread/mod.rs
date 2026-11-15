//! CA-6 Layer 3: Kernel Thread Abstraction & Execution Context
//!
//! Specification: CA-6 Section 9 (Layer 3) & Section 10 (Task Management)
//!
//! Represents a preemptible kernel thread of execution with:
//! - Unique thread identifier (`ThreadId`)
//! - Explicit lifecycle states (`Ready`, `Running`, `Blocked`, `Terminated`)
//! - Saved CPU callee-saved registers & instruction pointer (`CpuContext`)
//! - Dedicated heap-allocated kernel stack
//! - Scheduling accounting (runtime ticks, wait ticks, context switch count)

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};

/// Default kernel stack size per thread: 16 KiB.
pub const KTHREAD_STACK_SIZE: usize = 16 * 1024;

/// Unique thread identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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
    pub fn as_str(&self) -> &'static str {
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
    /// Create a new kernel thread with a dedicated stack.
    pub fn new(name: &str, _entry_point: extern "C" fn()) -> Self {
        let stack = vec![0u8; KTHREAD_STACK_SIZE];
        let stack_top = stack.as_ptr() as u64 + KTHREAD_STACK_SIZE as u64;

        let mut context = CpuContext::default();
        // Align stack pointer to 16 bytes for System V ABI
        context.rsp = stack_top & !0xF;

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
    }

    #[test_case]
    fn test_thread_state_transitions() {
        let mut t = KThread::new("worker_2", dummy_thread_entry);
        assert_eq!(t.state.as_str(), "READY");
        t.state = ThreadState::Running;
        assert_eq!(t.state.as_str(), "RUNNING");
        t.state = ThreadState::Blocked;
        assert_eq!(t.state.as_str(), "BLOCKED");
        t.state = ThreadState::Terminated;
        assert_eq!(t.state.as_str(), "TERMINATED");
    }
}
