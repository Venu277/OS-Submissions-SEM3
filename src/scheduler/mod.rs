//! CA-6 Layers 4, 5, 6: Ready Queue, Round Robin Scheduler & Context Switching
//!
//! Specification: CA-6 Section 9 (Layers 4, 5, 6), Section 10 & Section 16 (TC1-TC6)
//!
//! Manages runnable threads in an ordered Ready Queue, executes Round Robin
//! time-slicing with assembly context switching, and supports cooperative/preemptive yielding.

pub mod context_switch;

use alloc::boxed::Box;
use alloc::collections::vec_deque::Iter;
use alloc::collections::VecDeque;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::Mutex;

use crate::kthread::{CpuContext, KThread, ThreadId, ThreadState};
use crate::println;
use crate::serial_println;

pub static SCHEDULER: Mutex<Option<RoundRobinScheduler>> = Mutex::new(None);

/// Global flag controlling timer-driven preemption (CA-6 Layer 6).
pub static PREEMPTION_ENABLED: AtomicBool = AtomicBool::new(false);

/// Enable or disable timer-driven preemption.
pub fn enable_preemption(enable: bool) {
    PREEMPTION_ENABLED.store(enable, Ordering::SeqCst);
}

/// Query whether timer-driven preemption is active.
pub fn is_preemption_enabled() -> bool {
    PREEMPTION_ENABLED.load(Ordering::SeqCst)
}

/// RAII Guard that temporarily disables preemption for a critical section.
pub struct PreemptionGuard {
    was_enabled: bool,
}

impl PreemptionGuard {
    pub fn new() -> Self {
        let was_enabled = PREEMPTION_ENABLED.swap(false, Ordering::SeqCst);
        PreemptionGuard { was_enabled }
    }
}

impl Drop for PreemptionGuard {
    fn drop(&mut self) {
        if self.was_enabled {
            PREEMPTION_ENABLED.store(true, Ordering::SeqCst);
        }
    }
}

/// Layer 4: FIFO Ready Queue of runnable threads.
pub struct ReadyQueue {
    queue: VecDeque<Box<KThread>>,
}

impl ReadyQueue {
    pub const fn new() -> Self {
        ReadyQueue {
            queue: VecDeque::new(),
        }
    }

    /// Insert a thread at the tail of the ready queue.
    pub fn enqueue(&mut self, thread: Box<KThread>) {
        self.queue.push_back(thread);
    }

    /// Dequeue the next runnable thread from the head of the ready queue.
    pub fn dequeue(&mut self) -> Option<Box<KThread>> {
        self.queue.pop_front()
    }

    /// Peek at the next thread without removing it from the queue.
    pub fn peek(&self) -> Option<&Box<KThread>> {
        self.queue.front()
    }

    /// Peek mutably at the next thread.
    pub fn peek_mut(&mut self) -> Option<&mut Box<KThread>> {
        self.queue.front_mut()
    }

    /// Remove a specific thread by its ID (e.g., when blocked or cancelled).
    pub fn remove(&mut self, id: ThreadId) -> Option<Box<KThread>> {
        if let Some(pos) = self.queue.iter().position(|t| t.id == id) {
            self.queue.remove(pos)
        } else {
            None
        }
    }

    /// Check if a thread is present in the ready queue.
    pub fn contains(&self, id: ThreadId) -> bool {
        self.queue.iter().any(|t| t.id == id)
    }

    /// Find a reference to a thread by ID.
    pub fn find(&self, id: ThreadId) -> Option<&Box<KThread>> {
        self.queue.iter().find(|t| t.id == id)
    }

    /// Iterate over threads in the queue.
    pub fn iter(&self) -> Iter<'_, Box<KThread>> {
        self.queue.iter()
    }

    /// Get current number of runnable threads in the queue.
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    /// Check if the ready queue is empty.
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
}

/// Layer 5 & 6: Preemptive & Cooperative Round Robin Scheduler.
pub struct RoundRobinScheduler {
    pub ready_queue: ReadyQueue,
    pub current_thread: Option<Box<KThread>>,
    pub terminated_queue: Vec<Box<KThread>>,
    pub default_quantum: u32,
    pub total_switches: u64,
}

impl RoundRobinScheduler {
    pub fn new(default_quantum: u32) -> Self {
        RoundRobinScheduler {
            ready_queue: ReadyQueue::new(),
            current_thread: Some(Box::new(KThread::new_main("kmain"))),
            terminated_queue: Vec::new(),
            default_quantum,
            total_switches: 0,
        }
    }

    /// Create and enqueue a new kernel thread directly into the ready queue.
    pub fn spawn_thread(&mut self, name: &str, entry_point: extern "C" fn()) -> ThreadId {
        let thread = Box::new(KThread::new(name, entry_point));
        let id = thread.id;
        self.ready_queue.enqueue(thread);
        id
    }

    /// Select the next runnable thread according to Round Robin policy.
    pub fn select_next(&mut self) -> Option<Box<KThread>> {
        self.ready_queue.dequeue()
    }

    /// Reap all terminated threads, freeing their stacks and resources.
    pub fn reap_terminated(&mut self) -> usize {
        let count = self.terminated_queue.len();
        self.terminated_queue.clear();
        count
    }
}

/// Initialize the global kernel scheduler with the main thread.
pub fn init_scheduler(quantum: u32) {
    let mut sched = SCHEDULER.lock();
    *sched = Some(RoundRobinScheduler::new(quantum));
}

/// Spawn a new thread in the global scheduler.
pub fn spawn(name: &str, entry: extern "C" fn()) -> ThreadId {
    let mut sched_lock = SCHEDULER.lock();
    if sched_lock.is_none() {
        *sched_lock = Some(RoundRobinScheduler::new(10));
    }
    sched_lock.as_mut().unwrap().spawn_thread(name, entry)
}

/// Voluntarily yield the CPU to the next ready thread in the Round Robin queue.
pub fn yield_now() {
    let mut prev_ptr: *mut CpuContext = core::ptr::null_mut();
    let mut next_ptr: *const CpuContext = core::ptr::null();

    x86_64::instructions::interrupts::without_interrupts(|| {
        let mut sched_lock = SCHEDULER.lock();
        if let Some(ref mut sched) = *sched_lock {
            if sched.ready_queue.is_empty() {
                // No other runnable thread, continue executing current thread
                return;
            }

            let mut current = sched.current_thread.take().expect("no current thread running");
            let mut next = sched.ready_queue.dequeue().expect("ready queue was checked non-empty");

            let _ = current.transition_to(ThreadState::Ready);
            let _ = next.transition_to(ThreadState::Running);

            current.record_switch();
            next.record_switch();
            current.time_slice_remaining = sched.default_quantum;
            sched.total_switches += 1;

            let prev_id = current.id.0;
            let next_id = next.id.0;
            crate::diagnostics::trace_switch(prev_id, next_id, "yield");

            prev_ptr = &mut current.context as *mut CpuContext;
            next_ptr = &next.context as *const CpuContext;

            sched.ready_queue.enqueue(current);
            sched.current_thread = Some(next);
        }
    });

    if !prev_ptr.is_null() && !next_ptr.is_null() {
        // SAFETY: `prev_ptr` and `next_ptr` point to valid, heap-pinned `CpuContext` instances
        // in Box<KThread> allocations. Interrupts are disabled across the switch.
        // Callee-saved registers and stack pointers are preserved according to System V ABI.
        unsafe {
            context_switch::switch_context(prev_ptr, next_ptr);
        }
    }
}

/// CA-6 Layer 6: Timer tick hook called from the PIT IRQ0 interrupt handler.
///
/// Increments execution metrics, decrements remaining time slice,
/// and performs an involuntary context switch when the quantum is exhausted.
pub fn timer_tick() {
    if !is_preemption_enabled() {
        return;
    }

    let mut prev_ptr: *mut CpuContext = core::ptr::null_mut();
    let mut next_ptr: *const CpuContext = core::ptr::null();

    {
        // Use try_lock to avoid deadlock if interrupt occurred during a scheduler operation
        let mut sched_lock = match SCHEDULER.try_lock() {
            Some(lock) => lock,
            None => return,
        };

        if let Some(ref mut sched) = *sched_lock {
            if sched.ready_queue.is_empty() {
                if let Some(ref mut current) = sched.current_thread {
                    current.metrics.run_ticks = current.metrics.run_ticks.saturating_add(1);
                    current.time_slice_remaining = sched.default_quantum;
                }
                return;
            }

            if let Some(ref mut current) = sched.current_thread {
                current.record_run_ticks(1);

                if current.time_slice_remaining > 0 {
                    // Quantum has not expired yet
                    return;
                }
            } else {
                return;
            }

            // Quantum has expired: involuntary preemptive context switch
            let mut current = sched.current_thread.take().expect("no running thread");
            let mut next = sched.ready_queue.dequeue().expect("ready queue checked non-empty");

            let current_id = current.id.0;
            let next_id = next.id.0;
            let quantum = sched.default_quantum;

            // Emit structured trace event
            crate::diagnostics::trace_quantum_expire(current_id, quantum);

            // Reset quantum for outgoing thread
            current.time_slice_remaining = quantum;

            let _ = current.transition_to(ThreadState::Ready);
            let _ = next.transition_to(ThreadState::Running);

            current.record_switch();
            next.record_switch();
            sched.total_switches += 1;

            crate::diagnostics::trace_switch(current_id, next_id, "quantum");

            prev_ptr = &mut current.context as *mut CpuContext;
            next_ptr = &next.context as *const CpuContext;

            sched.ready_queue.enqueue(current);
            sched.current_thread = Some(next);
        }
    }

    if !prev_ptr.is_null() && !next_ptr.is_null() {
        // SAFETY: `prev_ptr` and `next_ptr` point to valid heap-allocated CpuContext structs
        // within Box<KThread> instances. Preemption occurs in timer interrupt context
        // where hardware interrupts are disabled. Context switch preserves all callee-saved registers.
        unsafe {
            context_switch::switch_context(prev_ptr, next_ptr);
        }
    }
}

/// Exit the currently running thread and switch away to the next ready thread.
pub fn exit_current_thread() -> ! {
    let mut prev_ptr: *mut CpuContext = core::ptr::null_mut();
    let mut next_ptr: *const CpuContext = core::ptr::null();

    x86_64::instructions::interrupts::without_interrupts(|| {
        let mut sched_lock = SCHEDULER.lock();
        if let Some(ref mut sched) = *sched_lock {
            let mut current = sched.current_thread.take().expect("no current thread to exit");
            let _ = current.transition_to(ThreadState::Terminated);

            let mut next = sched.ready_queue.dequeue().expect("no runnable thread left to schedule on exit");
            let _ = next.transition_to(ThreadState::Running);

            next.record_switch();
            sched.total_switches += 1;

            let prev_id = current.id.0;
            let next_id = next.id.0;
            crate::diagnostics::trace_switch(prev_id, next_id, "terminate");

            prev_ptr = &mut current.context as *mut CpuContext;
            next_ptr = &next.context as *const CpuContext;

            sched.terminated_queue.push(current);
            sched.current_thread = Some(next);
        }
    });

    if !prev_ptr.is_null() && !next_ptr.is_null() {
        // SAFETY: `prev_ptr` and `next_ptr` point to valid `CpuContext` structs.
        unsafe {
            context_switch::switch_context(prev_ptr, next_ptr);
        }
    }

    loop {
        x86_64::instructions::hlt();
    }
}

/// Reap all terminated threads, freeing their resources.
pub fn reap_terminated() -> usize {
    let mut sched_lock = SCHEDULER.lock();
    if let Some(ref mut sched) = *sched_lock {
        sched.reap_terminated()
    } else {
        0
    }
}

/// Query the ThreadId of the currently executing thread (0 if unassigned or scheduler locked).
pub fn current_thread_id() -> u64 {
    if let Some(guard) = SCHEDULER.try_lock() {
        if let Some(ref sched) = *guard {
            if let Some(ref current) = sched.current_thread {
                return current.id.0;
            }
        }
    }
    0
}

/// Wait until all spawned threads in the ready queue complete their execution.
pub fn wait_all_threads() {
    loop {
        let has_more = {
            let sched_lock = SCHEDULER.lock();
            if let Some(ref sched) = *sched_lock {
                !sched.ready_queue.is_empty()
            } else {
                false
            }
        };
        if has_more {
            yield_now();
        } else {
            break;
        }
    }
}

// ---------- 3-Task Cooperative Round Robin Demonstration ----------

extern "C" fn demo_task_alpha() {
    for step in 1..=3 {
        println!("  [Task 1 / Worker Alpha] Executing step {}/3", step);
        serial_println!("  [Task 1 / Worker Alpha] Executing step {}/3", step);
        yield_now();
    }
    println!("  [Task 1 / Worker Alpha] Finished all steps. Exiting.");
    serial_println!("  [Task 1 / Worker Alpha] Finished all steps. Exiting.");
}

extern "C" fn demo_task_beta() {
    for step in 1..=3 {
        println!("  [Task 2 / Worker Beta ] Executing step {}/3", step);
        serial_println!("  [Task 2 / Worker Beta ] Executing step {}/3", step);
        yield_now();
    }
    println!("  [Task 2 / Worker Beta ] Finished all steps. Exiting.");
    serial_println!("  [Task 2 / Worker Beta ] Finished all steps. Exiting.");
}

extern "C" fn demo_task_gamma() {
    for step in 1..=3 {
        println!("  [Task 3 / Worker Gamma] Executing step {}/3", step);
        serial_println!("  [Task 3 / Worker Gamma] Executing step {}/3", step);
        yield_now();
    }
    println!("  [Task 3 / Worker Gamma] Finished all steps. Exiting.");
    serial_println!("  [Task 3 / Worker Gamma] Finished all steps. Exiting.");
}

/// Run the 3-Task Cooperative Round Robin Demonstration.
pub fn demo_three_tasks_round_robin() {
    println!("==================================================");
    println!("   CA-6 3-Task Round Robin Scheduling Demo");
    println!("==================================================");
    println!("Spawning 3 cooperative threads on dedicated 16 KiB stacks...");

    init_scheduler(10);

    spawn("worker_alpha", demo_task_alpha);
    spawn("worker_beta", demo_task_beta);
    spawn("worker_gamma", demo_task_gamma);

    println!("Starting Round Robin scheduling loop (yielding CPU from main):");
    wait_all_threads();

    let reaped = reap_terminated();
    println!("All 3 tasks finished cleanly! Reaped {} terminated threads.", reaped);
    println!("==================================================");
    serial_println!("[CA6-DEMO] 3-task round-robin completed successfully; reaped={}", reaped);
}

// ---------- Layer 6: Preemptive Timer-Driven Scheduling Demonstration ----------

static ALPHA_SPIN_COUNT: AtomicU64 = AtomicU64::new(0);
static STOP_PREEMPT_DEMO: AtomicBool = AtomicBool::new(false);

extern "C" fn preempt_worker_alpha() {
    println!("  [Worker Alpha] Starting CPU-bound computation loop (NO voluntary yields)...");
    serial_println!("  [Worker Alpha] Starting CPU-bound computation loop (NO voluntary yields)...");

    while !STOP_PREEMPT_DEMO.load(Ordering::Relaxed) {
        ALPHA_SPIN_COUNT.fetch_add(1, Ordering::Relaxed);
        for _ in 0..10_000 {
            core::hint::spin_loop();
        }
    }

    println!("  [Worker Alpha] Preempted successfully and received stop signal! Exiting.");
    serial_println!("  [Worker Alpha] Preempted successfully and received stop signal! Exiting.");
}

extern "C" fn preempt_worker_beta() {
    println!("  [Worker Beta ] Scheduled via PIT timer preemption!");
    serial_println!("  [Worker Beta ] Scheduled via PIT timer preemption!");

    let count = ALPHA_SPIN_COUNT.load(Ordering::Relaxed);
    println!("  [Worker Beta ] Validated Alpha computed {} iterations while holding CPU.", count);
    serial_println!("  [Worker Beta ] Validated Alpha computed {} iterations while holding CPU.", count);
    println!("  [Worker Beta ] Signaling Alpha to terminate...");
    serial_println!("  [Worker Beta ] Signaling Alpha to terminate...");

    STOP_PREEMPT_DEMO.store(true, Ordering::Relaxed);
    println!("  [Worker Beta ] Worker Beta exiting.");
    serial_println!("  [Worker Beta ] Worker Beta exiting.");
}

/// Run the CA-6 Timer-Driven Preemptive Scheduling Demonstration (Layer 6).
pub fn demo_preemptive_scheduling() {
    println!("==================================================");
    println!("   CA-6 Layer 6: Timer-Driven Preemptive Scheduling Demo");
    println!("==================================================");
    println!("Specification: CA-6 Section 9 (Layer 6) & Section 10");
    println!("Scenario: Worker Alpha executes an infinite CPU-bound loop with NO yields.");
    println!("          Timer interrupt (PIT IRQ0) preempts Alpha when quantum expires.");
    println!("          Worker Beta runs, observes Alpha progress, and signals completion.");
    println!("--------------------------------------------------");

    ALPHA_SPIN_COUNT.store(0, Ordering::Relaxed);
    STOP_PREEMPT_DEMO.store(false, Ordering::Relaxed);

    // Initialize scheduler with a small quantum of 3 ticks (30 ms @ 100 Hz)
    init_scheduler(3);

    let id_a = spawn("worker_alpha", preempt_worker_alpha);
    let id_b = spawn("worker_beta", preempt_worker_beta);

    println!("Spawned Worker Alpha (task={}) and Worker Beta (task={})", id_a.0, id_b.0);
    println!("Enabling timer-driven preemption (quantum = 3 ticks)...");
    serial_println!("[CA6-PREEMPT] Enabling timer preemption; quantum=3 ticks");

    enable_preemption(true);

    // Wait for all spawned threads to finish
    wait_all_threads();

    enable_preemption(false);

    let reaped = reap_terminated();
    println!("--------------------------------------------------");
    println!("Preemptive demo finished! Reaped {} terminated threads.", reaped);
    println!("Worker Alpha completed {} compute iterations.", ALPHA_SPIN_COUNT.load(Ordering::Relaxed));
    println!("==================================================");
    serial_println!(
        "[CA6-DEMO] Preemptive scheduling completed successfully; alpha_count={}, reaped={}",
        ALPHA_SPIN_COUNT.load(Ordering::Relaxed),
        reaped
    );
}

/// Execute the CA-6 Integrated Core Demonstration (Section 9 Layer 10 & Section 42).
pub fn run_ca6_demo() {
    println!("==================================================");
    println!("   CA-6 Rust Micro-OS Core Architecture Demo");
    println!("==================================================");
    println!("Specification: CA-6 Flagship Engineering Project (15 Marks)");
    println!("Selected Specialization: Track C - Concurrency Engineering");
    println!();
    println!("Core OS Subsystem Status:");
    println!("  [Layer 1] Kernel Boot:          OPERATIONAL (x86_64, 4-level paging)");
    println!("  [Layer 2] Diagnostics:          ONLINE (VGA text + 115200 serial)");
    println!("  [Layer 3] Task Representation:  INITIALIZED (KThread, CpuContext, 16KiB Stack)");
    println!("  [Layer 4] Ready Queue:          READY (FIFO queue, peek/remove/find)");
    println!("  [Layer 5] Round Robin Sched:    ONLINE (Assembly switch_context + yield_now)");
    println!("  [Layer 6] Preemption Trigger:   CONNECTED (PIT IRQ0)");
    println!("  [Layer 7] Synchronization:      INITIALIZED (Spinlock & Lock Ranking)");
    println!("  [Layer 8] Memory Allocator:     ACTIVE (256 KiB intrusive linked list)");
    println!("==================================================");

    demo_three_tasks_round_robin();
    demo_preemptive_scheduling();
    crate::sync::demo_synchronization();
}

#[cfg(test)]
mod tests {
    use super::*;

    extern "C" fn test_fn() {}

    #[test_case]
    fn test_ready_queue_fifo() {
        let mut rq = ReadyQueue::new();
        assert_eq!(rq.len(), 0);
        assert!(rq.is_empty());

        let t1 = Box::new(KThread::new("t1", test_fn));
        let id1 = t1.id;
        let t2 = Box::new(KThread::new("t2", test_fn));
        let id2 = t2.id;

        rq.enqueue(t1);
        rq.enqueue(t2);

        assert_eq!(rq.len(), 2);
        assert!(!rq.is_empty());

        let popped1 = rq.dequeue().expect("expected t1");
        assert_eq!(popped1.id, id1);
        assert_eq!(rq.len(), 1);

        let popped2 = rq.dequeue().expect("expected t2");
        assert_eq!(popped2.id, id2);
        assert_eq!(rq.len(), 0);
        assert!(rq.is_empty());
    }

    #[test_case]
    fn test_ready_queue_peek_and_remove() {
        let mut rq = ReadyQueue::new();
        let t1 = Box::new(KThread::new("task_a", test_fn));
        let id1 = t1.id;
        let t2 = Box::new(KThread::new("task_b", test_fn));
        let id2 = t2.id;
        let t3 = Box::new(KThread::new("task_c", test_fn));
        let id3 = t3.id;

        rq.enqueue(t1);
        rq.enqueue(t2);
        rq.enqueue(t3);

        assert_eq!(rq.peek().unwrap().id, id1);
        assert_eq!(rq.len(), 3);

        let removed = rq.remove(id2).expect("should remove task_b");
        assert_eq!(removed.id, id2);
        assert_eq!(rq.len(), 2);
        assert!(!rq.contains(id2));

        assert_eq!(rq.dequeue().unwrap().id, id1);
        assert_eq!(rq.dequeue().unwrap().id, id3);
        assert!(rq.is_empty());
    }

    #[test_case]
    fn test_round_robin_spawn_and_reap() {
        let mut sched = RoundRobinScheduler::new(10);
        assert_eq!(sched.default_quantum, 10);
        assert_eq!(sched.total_switches, 0);

        let id1 = sched.spawn_thread("alpha", test_fn);
        let _id2 = sched.spawn_thread("beta", test_fn);
        assert_eq!(sched.ready_queue.len(), 2);

        let t_next = sched.select_next().expect("should select alpha");
        assert_eq!(t_next.id, id1);
        assert_eq!(t_next.name, "alpha");

        sched.terminated_queue.push(t_next);
        assert_eq!(sched.reap_terminated(), 1);
        assert_eq!(sched.terminated_queue.len(), 0);
    }

    #[test_case]
    fn test_preemption_flag_and_guard() {
        enable_preemption(false);
        assert!(!is_preemption_enabled());

        enable_preemption(true);
        assert!(is_preemption_enabled());

        {
            let _guard = PreemptionGuard::new();
            assert!(!is_preemption_enabled());
        }
        // Guard dropped, preemption restored
        assert!(is_preemption_enabled());

        enable_preemption(false);
        assert!(!is_preemption_enabled());
    }

    #[test_case]
    fn test_scheduler_quantum_accounting() {
        let sched = RoundRobinScheduler::new(5);
        assert_eq!(sched.default_quantum, 5);

        let mut t1 = Box::new(KThread::new("t1", test_fn));
        assert_eq!(t1.time_slice_remaining, 10);
        t1.time_slice_remaining = 5;

        // Decrement quantum
        t1.time_slice_remaining -= 1;
        assert_eq!(t1.time_slice_remaining, 4);

        // Reset quantum
        t1.reset_quantum(sched.default_quantum);
        assert_eq!(t1.time_slice_remaining, 5);
    }
}
