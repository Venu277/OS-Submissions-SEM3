#![no_std]
#![no_main]

extern crate alloc;

use bootloader::{entry_point, BootInfo};
use core::panic::PanicInfo;
use micro_os::kthread::{KThread, ThreadId, ThreadState, KTHREAD_STACK_SIZE};
use micro_os::scheduler::ReadyQueue;
use micro_os::{allocator, exit_qemu, memory, serial_print, serial_println, QemuExitCode};
use x86_64::VirtAddr;

entry_point!(main);

extern "C" fn dummy_entry() {}

fn main(boot_info: &'static BootInfo) -> ! {
    serial_println!("==================================================");
    serial_println!("   CA-6 Layers 3 & 4: Task & Ready Queue Suite");
    serial_println!("==================================================");

    // 1. Initialize CPU, IDT, Paging & Heap Allocator
    micro_os::init();
    let phys_mem_offset = VirtAddr::new(boot_info.physical_memory_offset);
    let mut mapper = unsafe { memory::init(phys_mem_offset) };
    let mut frame_allocator =
        unsafe { memory::BootInfoFrameAllocator::init(&boot_info.memory_map) };
    allocator::init_heap(&mut mapper, &mut frame_allocator).expect("heap initialization failed");

    // TC1: Two Tasks (Section 16: Required Test Cases)
    serial_print!("tc1_two_tasks_representation_and_queueing...\t");
    test_tc1_two_tasks();
    serial_println!("[ok]");

    // TC2: 3 to 5 Tasks (Section 16: Required Test Cases)
    serial_print!("tc2_three_to_five_tasks_lifecycle...\t");
    test_tc2_five_tasks();
    serial_println!("[ok]");

    // TC3: Boundary Cases (Empty queue, stack boundary)
    serial_print!("tc3_boundary_cases_empty_queue_and_canary...\t");
    test_tc3_boundary_cases();
    serial_println!("[ok]");

    // TC4: Stress Case (High volume queueing & allocation)
    serial_print!("tc4_stress_case_50_tasks...\t");
    test_tc4_stress_tasks();
    serial_println!("[ok]");

    // TC5: Failure Cases (Illegal transitions & corrupted canary)
    serial_print!("tc5_failure_cases_rejected_operations...\t");
    test_tc5_failure_cases();
    serial_println!("[ok]");

    // TC6: Repeatability (10x Deterministic Cycle)
    serial_print!("tc6_repeatability_10_cycles...\t");
    test_tc6_repeatability();
    serial_println!("[ok]");

    serial_println!("==================================================");
    serial_println!("CA-6 Task & Ready Queue Suite: TC1-TC6 ALL PASSED");
    serial_println!("==================================================");

    exit_qemu(QemuExitCode::Success);
    micro_os::hlt_loop();
}

/// TC1: Two Tasks - Verified representation, isolated stacks, distinct IDs, and FIFO queueing
fn test_tc1_two_tasks() {
    let t1 = KThread::new("task_1", dummy_entry);
    let t2 = KThread::new("task_2", dummy_entry);

    assert_ne!(t1.id, t2.id);
    assert_eq!(t1.state, ThreadState::Ready);
    assert_eq!(t2.state, ThreadState::Ready);

    // Verify stack pointers are distinct and 16-byte aligned
    assert_ne!(t1.context.rsp, t2.context.rsp);
    assert_eq!(t1.context.rsp % 16, 0);
    assert_eq!(t2.context.rsp % 16, 0);

    let id1 = t1.id;
    let id2 = t2.id;

    let mut rq = ReadyQueue::new();
    rq.enqueue(alloc::boxed::Box::new(t1));
    rq.enqueue(alloc::boxed::Box::new(t2));
    assert_eq!(rq.len(), 2);

    let dequeued1 = rq.dequeue().expect("expected task_1");
    assert_eq!(dequeued1.id, id1);
    let dequeued2 = rq.dequeue().expect("expected task_2");
    assert_eq!(dequeued2.id, id2);
    assert!(rq.is_empty());
}

/// TC2: 3 to 5 Tasks - Lifecycle transitions and queue rotations
fn test_tc2_five_tasks() {
    let mut rq = ReadyQueue::new();
    let mut ids = [ThreadId(0); 5];

    for i in 0..5 {
        let t = KThread::new("worker", dummy_entry);
        ids[i] = t.id;
        rq.enqueue(alloc::boxed::Box::new(t));
    }
    assert_eq!(rq.len(), 5);

    // Rotate through queue: dequeue, transition to Running, record tick, transition back to Ready, re-enqueue
    for i in 0..5 {
        let mut t = rq.dequeue().expect("task in queue");
        assert_eq!(t.id, ids[i]);
        assert_eq!(t.state, ThreadState::Ready);

        assert!(t.transition_to(ThreadState::Running).is_ok());
        t.record_run_ticks(2);
        assert!(t.transition_to(ThreadState::Ready).is_ok());

        rq.enqueue(t);
    }
    assert_eq!(rq.len(), 5);

    // Verify metrics persisted across rotation
    let t = rq.peek().unwrap();
    assert_eq!(t.id, ids[0]);
    assert_eq!(t.metrics.run_ticks, 2);
}

/// TC3: Boundary Cases - Empty queue, removal of non-existent ID, single-thread edge
fn test_tc3_boundary_cases() {
    let mut rq = ReadyQueue::new();
    // Boundary 1: Dequeue on empty queue returns None
    assert!(rq.dequeue().is_none());
    assert!(rq.peek().is_none());
    assert_eq!(rq.len(), 0);

    // Boundary 2: Removal of non-existent ID returns None
    assert!(rq.remove(ThreadId(99999)).is_none());

    // Boundary 3: Stack canary at boundary
    let t = KThread::new("boundary_thread", dummy_entry);
    assert!(!t.check_stack_overflow());
    assert_eq!(t.stack.len(), KTHREAD_STACK_SIZE);
}

/// TC4: Stress Case - High volume creation, queueing, and recycling of 50 tasks across 5 stress batches
fn test_tc4_stress_tasks() {
    let mut total_processed = 0;
    const BATCH_SIZE: usize = 10;
    const NUM_BATCHES: usize = 5;

    for _batch in 0..NUM_BATCHES {
        let mut rq = ReadyQueue::new();
        for _i in 0..BATCH_SIZE {
            let t = KThread::new("stress_worker", dummy_entry);
            rq.enqueue(alloc::boxed::Box::new(t));
        }
        assert_eq!(rq.len(), BATCH_SIZE);

        while let Some(mut t) = rq.dequeue() {
            assert!(!t.check_stack_overflow());
            assert!(t.transition_to(ThreadState::Running).is_ok());
            assert!(t.transition_to(ThreadState::Terminated).is_ok());
            total_processed += 1;
        }
        assert!(rq.is_empty());
    }
    assert_eq!(total_processed, BATCH_SIZE * NUM_BATCHES);
}

/// TC5: Failure Cases - Controlled rejected invalid operations
fn test_tc5_failure_cases() {
    let mut t = KThread::new("fail_worker", dummy_entry);

    // Failure 1: Direct transition from Ready to Blocked is rejected
    let err1 = t.transition_to(ThreadState::Blocked);
    assert!(err1.is_err());
    assert_eq!(t.state, ThreadState::Ready);

    // Failure 2: Direct transition from Ready to Terminated is rejected
    let err2 = t.transition_to(ThreadState::Terminated);
    assert!(err2.is_err());
    assert_eq!(t.state, ThreadState::Ready);

    // Failure 3: Terminated thread attempting to revive to Running is rejected
    assert!(t.transition_to(ThreadState::Running).is_ok());
    assert!(t.transition_to(ThreadState::Terminated).is_ok());
    let err3 = t.transition_to(ThreadState::Running);
    assert!(err3.is_err());

    // Failure 4: Corrupted stack canary rejects state transition
    let mut t_corrupt = KThread::new("corrupt_worker", dummy_entry);
    t_corrupt.stack[0] = 0xDE;
    t_corrupt.stack[1] = 0xAD;
    assert!(t_corrupt.check_stack_overflow());
    let err4 = t_corrupt.transition_to(ThreadState::Running);
    assert!(err4.is_err());
}

/// TC6: Repeatability - Deterministic 10-cycle execution producing identical results
fn test_tc6_repeatability() {
    for _cycle in 0..10 {
        let mut rq = ReadyQueue::new();
        let t1 = KThread::new("rep_a", dummy_entry);
        let id1 = t1.id;
        let t2 = KThread::new("rep_b", dummy_entry);
        let id2 = t2.id;

        rq.enqueue(alloc::boxed::Box::new(t1));
        rq.enqueue(alloc::boxed::Box::new(t2));

        let popped1 = rq.dequeue().expect("expected rep_a");
        let popped2 = rq.dequeue().expect("expected rep_b");

        assert_eq!(popped1.id, id1);
        assert_eq!(popped2.id, id2);
        assert!(rq.is_empty());
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    micro_os::test_panic_handler(info)
}
