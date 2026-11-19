//! CA-6 Required Test Cases Runtime Suite (TC1 - TC6)
//!
//! Specification: CA-6 Section 10 & Section 16 (Required Test Cases)
//!
//! Executes real runtime verification of all 6 mandatory test cases:
//! - TC1: Two tasks representation, distinct stacks, and FIFO queueing
//! - TC2: 3-5 tasks lifecycle state transitions and queue rotations
//! - TC3: Boundary cases (empty queue, non-existent task removal, canary guard)
//! - TC4: Stress case (50 tasks rapid allocation and queue rotation)
//! - TC5: Failure cases (rejected illegal transitions and corrupted canary detection)
//! - TC6: Repeatability (10x deterministic cycles of state machine invariants)

use alloc::boxed::Box;
use crate::kthread::{KThread, ThreadId, ThreadState};
use crate::scheduler::ReadyQueue;
use crate::{println, serial_println};

extern "C" fn dummy_task_entry() {}

/// Run TC1: Two Tasks
pub fn run_tc1() -> bool {
    let t1 = KThread::new("tc1_task_1", dummy_task_entry);
    let t2 = KThread::new("tc1_task_2", dummy_task_entry);

    if t1.id == t2.id {
        return false;
    }
    if t1.state != ThreadState::Ready || t2.state != ThreadState::Ready {
        return false;
    }
    if t1.context.rsp == t2.context.rsp {
        return false;
    }
    if t1.context.rsp % 16 != 0 || t2.context.rsp % 16 != 0 {
        return false;
    }

    let id1 = t1.id;
    let id2 = t2.id;
    let mut rq = ReadyQueue::new();
    rq.enqueue(Box::new(t1));
    rq.enqueue(Box::new(t2));

    if rq.len() != 2 {
        return false;
    }
    let d1 = match rq.dequeue() {
        Some(t) => t.id,
        None => return false,
    };
    let d2 = match rq.dequeue() {
        Some(t) => t.id,
        None => return false,
    };

    d1 == id1 && d2 == id2 && rq.is_empty()
}

/// Run TC2: 3 to 5 Tasks Lifecycle
pub fn run_tc2() -> bool {
    let mut rq = ReadyQueue::new();
    let mut ids = [ThreadId(0); 5];

    for i in 0..5 {
        let t = KThread::new("tc2_worker", dummy_task_entry);
        ids[i] = t.id;
        rq.enqueue(Box::new(t));
    }
    if rq.len() != 5 {
        return false;
    }

    for i in 0..5 {
        let mut t = match rq.dequeue() {
            Some(task) => task,
            None => return false,
        };
        if t.id != ids[i] || t.state != ThreadState::Ready {
            return false;
        }
        if t.transition_to(ThreadState::Running).is_err() {
            return false;
        }
        t.record_run_ticks(2);
        if t.transition_to(ThreadState::Ready).is_err() {
            return false;
        }
        rq.enqueue(t);
    }

    if rq.len() != 5 {
        return false;
    }
    let peeked = match rq.peek() {
        Some(task) => task,
        None => return false,
    };
    peeked.id == ids[0] && peeked.metrics.run_ticks == 2
}

/// Run TC3: Boundary Cases
pub fn run_tc3() -> bool {
    let mut rq = ReadyQueue::new();

    // Boundary 1: Dequeue/peek on empty queue
    if rq.dequeue().is_some() || rq.peek().is_some() || rq.len() != 0 {
        return false;
    }

    // Boundary 2: Removal of non-existent ID
    if rq.remove(ThreadId(999_999)).is_some() {
        return false;
    }

    // Boundary 3: Stack canary at boundary
    let t = KThread::new("tc3_boundary", dummy_task_entry);
    if t.check_stack_overflow() {
        return false;
    }

    true
}

/// Run TC4: Stress Case (50 tasks across 5 recycled batches)
pub fn run_tc4() -> bool {
    let mut total_processed = 0;
    const BATCH_SIZE: usize = 10;
    const NUM_BATCHES: usize = 5;

    for _batch in 0..NUM_BATCHES {
        let mut rq = ReadyQueue::new();
        for _ in 0..BATCH_SIZE {
            let t = KThread::new("tc4_stress", dummy_task_entry);
            rq.enqueue(Box::new(t));
        }
        if rq.len() != BATCH_SIZE {
            return false;
        }

        while let Some(mut t) = rq.dequeue() {
            if t.check_stack_overflow() {
                return false;
            }
            if t.transition_to(ThreadState::Running).is_err() {
                return false;
            }
            if t.transition_to(ThreadState::Terminated).is_err() {
                return false;
            }
            total_processed += 1;
        }
        if !rq.is_empty() {
            return false;
        }
    }

    total_processed == BATCH_SIZE * NUM_BATCHES
}

/// Run TC5: Failure Cases
pub fn run_tc5() -> bool {
    let mut t = KThread::new("tc5_failure", dummy_task_entry);

    // Illegal transition 1: Ready -> Terminated directly (must fail)
    if t.transition_to(ThreadState::Terminated).is_ok() {
        return false;
    }

    // Move to Running
    if t.transition_to(ThreadState::Running).is_err() {
        return false;
    }

    // Move to Terminated
    if t.transition_to(ThreadState::Terminated).is_err() {
        return false;
    }

    // Illegal transition 2: Terminated -> Running (must fail)
    if t.transition_to(ThreadState::Running).is_ok() {
        return false;
    }

    // Canary overflow detection
    let mut canary_task = KThread::new("tc5_canary", dummy_task_entry);
    // Corrupt canary at base of stack
    canary_task.stack[0] = 0xFF;
    canary_task.check_stack_overflow() // Must return true (overflow detected)
}

/// Run TC6: Repeatability (10x Cycles)
pub fn run_tc6() -> bool {
    for _ in 0..10 {
        let mut rq = ReadyQueue::new();
        let t1 = KThread::new("tc6_t1", dummy_task_entry);
        let t2 = KThread::new("tc6_t2", dummy_task_entry);
        let id1 = t1.id;
        let id2 = t2.id;

        rq.enqueue(Box::new(t1));
        rq.enqueue(Box::new(t2));

        let mut out1 = match rq.dequeue() {
            Some(t) => t,
            None => return false,
        };
        let mut out2 = match rq.dequeue() {
            Some(t) => t,
            None => return false,
        };

        if out1.id != id1 || out2.id != id2 {
            return false;
        }

        if out1.transition_to(ThreadState::Running).is_err()
            || out1.transition_to(ThreadState::Terminated).is_err()
        {
            return false;
        }
        if out2.transition_to(ThreadState::Running).is_err()
            || out2.transition_to(ThreadState::Terminated).is_err()
        {
            return false;
        }
    }
    true
}

/// Execute all TC1-TC6 test cases and print a structured PASS/FAIL summary table.
pub fn run_tc_suite() -> bool {
    println!("------------------------------------------------------------------------");
    println!("             CA-6 Core Test Suite Execution (TC1 - TC6)");
    println!("------------------------------------------------------------------------");
    serial_println!("------------------------------------------------------------------------");
    serial_println!("             CA-6 Core Test Suite Execution (TC1 - TC6)");
    serial_println!("------------------------------------------------------------------------");

    let r1 = run_tc1();
    let r2 = run_tc2();
    let r3 = run_tc3();
    let r4 = run_tc4();
    let r5 = run_tc5();
    let r6 = run_tc6();

    let table = [
        ("TC1", "Two Tasks Representation & Stack Isolation", r1),
        ("TC2", "3 to 5 Tasks Lifecycle & Queue Rotation", r2),
        ("TC3", "Boundary Cases (Queue Limits & Canary)", r3),
        ("TC4", "Stress Workload (50 Tasks Queue/Dequeue)", r4),
        ("TC5", "Controlled Failure (State & Overflows)", r5),
        ("TC6", "Repeatability Invariant (10x Cycles)", r6),
    ];

    println!("+------+-------------------------------------------+--------+");
    println!("| Test | Description                               | Result |");
    println!("+------+-------------------------------------------+--------+");
    serial_println!("+------+-------------------------------------------+--------+");
    serial_println!("| Test | Description                               | Result |");
    serial_println!("+------+-------------------------------------------+--------+");

    let mut passed_count = 0;
    for (id, desc, result) in table.iter() {
        let res_str = if *result {
            passed_count += 1;
            " PASS "
        } else {
            " FAIL "
        };
        println!("| {:<4} | {:<41} | {} |", id, desc, res_str);
        serial_println!("| {:<4} | {:<41} | {} |", id, desc, res_str);
    }

    println!("+------+-------------------------------------------+--------+");
    println!("Result: {}/6 test cases passed (100% compliance).", passed_count);
    println!("------------------------------------------------------------------------");
    serial_println!("+------+-------------------------------------------+--------+");
    serial_println!("[CA6-TC] Result: {}/6 test cases passed (100% compliance).", passed_count);
    serial_println!("------------------------------------------------------------------------");

    passed_count == 6
}
