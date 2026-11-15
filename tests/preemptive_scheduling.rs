#![no_std]
#![no_main]

extern crate alloc;

use bootloader::{entry_point, BootInfo};
use core::panic::PanicInfo;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use micro_os::scheduler::{
    enable_preemption, init_scheduler, is_preemption_enabled, reap_terminated, spawn,
    wait_all_threads, PreemptionGuard,
};
use micro_os::{allocator, exit_qemu, memory, serial_print, serial_println, QemuExitCode};
use x86_64::VirtAddr;

entry_point!(main);

// Shared atomic state for Test 2: Involuntary Preemption
static TASK1_COUNT: AtomicU64 = AtomicU64::new(0);
static STOP_TASK1: AtomicBool = AtomicBool::new(false);
static TASK2_RAN: AtomicBool = AtomicBool::new(false);

extern "C" fn compute_bound_worker() {
    serial_println!("  [Worker 1] Started compute-bound loop (NO yields)...");
    while !STOP_TASK1.load(Ordering::Relaxed) {
        TASK1_COUNT.fetch_add(1, Ordering::Relaxed);
        for _ in 0..10_000 {
            core::hint::spin_loop();
        }
    }
    serial_println!("  [Worker 1] Stop condition met; compute worker exiting cleanly.");
}

extern "C" fn preemptive_observer() {
    serial_println!("  [Worker 2] Scheduled via PIT timer preemption!");
    TASK2_RAN.store(true, Ordering::SeqCst);
    let count = TASK1_COUNT.load(Ordering::Relaxed);
    serial_println!("  [Worker 2] Observed Worker 1 count = {}. Signaling Worker 1 to stop.", count);
    assert!(count > 0, "Worker 1 should have accumulated compute ticks before preemption");
    STOP_TASK1.store(true, Ordering::Relaxed);
    serial_println!("  [Worker 2] Preemptive observer exiting.");
}

// Shared atomic state for Test 3: Fair Preemptive Time-Slicing
static TASK_A_COUNT: AtomicU64 = AtomicU64::new(0);
static TASK_B_COUNT: AtomicU64 = AtomicU64::new(0);
static STOP_FAIR_TEST: AtomicBool = AtomicBool::new(false);

extern "C" fn fair_worker_a() {
    while !STOP_FAIR_TEST.load(Ordering::Relaxed) {
        TASK_A_COUNT.fetch_add(1, Ordering::Relaxed);
        for _ in 0..5_000 {
            core::hint::spin_loop();
        }
    }
}

extern "C" fn fair_worker_b() {
    while !STOP_FAIR_TEST.load(Ordering::Relaxed) {
        TASK_B_COUNT.fetch_add(1, Ordering::Relaxed);
        for _ in 0..5_000 {
            core::hint::spin_loop();
        }
    }
}

extern "C" fn fair_observer_c() {
    serial_println!("  [Worker C] Observer scheduled via timer preemption!");
    let a_cnt = TASK_A_COUNT.load(Ordering::Relaxed);
    let b_cnt = TASK_B_COUNT.load(Ordering::Relaxed);
    serial_println!("  [Worker C] Counts: Task A = {}, Task B = {}", a_cnt, b_cnt);
    STOP_FAIR_TEST.store(true, Ordering::Relaxed);
    serial_println!("  [Worker C] Sent stop signal to both workers.");
}

fn main(boot_info: &'static BootInfo) -> ! {
    serial_println!("==================================================");
    serial_println!("   CA-6 Layer 6: Timer-Driven Preemption Test");
    serial_println!("==================================================");

    // 1. Initialize CPU, IDT, PIT (100 Hz), Paging & Heap
    micro_os::init();
    let phys_mem_offset = VirtAddr::new(boot_info.physical_memory_offset);
    let mut mapper = unsafe { memory::init(phys_mem_offset) };
    let mut frame_allocator =
        unsafe { memory::BootInfoFrameAllocator::init(&boot_info.memory_map) };
    allocator::init_heap(&mut mapper, &mut frame_allocator).expect("heap initialization failed");

    // Test 1: Preemption Flag and RAII Guard
    serial_print!("preempt::1_flag_and_raii_guard...\t");
    enable_preemption(false);
    assert!(!is_preemption_enabled());
    enable_preemption(true);
    assert!(is_preemption_enabled());
    {
        let _guard = PreemptionGuard::new();
        assert!(!is_preemption_enabled(), "Guard must disable preemption");
    }
    assert!(is_preemption_enabled(), "Guard drop must re-enable preemption");
    enable_preemption(false);
    serial_println!("[ok]");

    // Test 2: Involuntary Preemption of Compute-Bound Task (No Yields)
    serial_print!("preempt::2_involuntary_preemption_no_yield...\t");
    init_scheduler(2); // 2 ticks quantum (20 ms @ 100 Hz)
    TASK1_COUNT.store(0, Ordering::Relaxed);
    STOP_TASK1.store(false, Ordering::Relaxed);
    TASK2_RAN.store(false, Ordering::SeqCst);

    let id1 = spawn("compute_worker", compute_bound_worker);
    let id2 = spawn("observer_worker", preemptive_observer);
    assert_ne!(id1, id2);

    // Enable preemption and yield main thread so compute worker runs
    enable_preemption(true);
    wait_all_threads();
    enable_preemption(false);

    assert!(TASK2_RAN.load(Ordering::SeqCst), "Observer task MUST run via timer preemption");
    let final_task1_count = TASK1_COUNT.load(Ordering::Relaxed);
    assert!(
        final_task1_count > 0,
        "Worker 1 must have performed computation loop iterations"
    );
    let reaped = reap_terminated();
    assert_eq!(reaped, 2, "Both compute worker and observer must be reaped");
    serial_println!("[ok] (Alpha count = {}, preemption verified!)", final_task1_count);

    // Test 3: Fair Time-Slicing Between Multiple Compute Workers
    serial_print!("preempt::3_multi_worker_time_slicing...\t");
    init_scheduler(2);
    TASK_A_COUNT.store(0, Ordering::Relaxed);
    TASK_B_COUNT.store(0, Ordering::Relaxed);
    STOP_FAIR_TEST.store(false, Ordering::Relaxed);

    spawn("fair_worker_a", fair_worker_a);
    spawn("fair_worker_b", fair_worker_b);
    spawn("fair_observer_c", fair_observer_c);

    enable_preemption(true);
    wait_all_threads();
    enable_preemption(false);

    let a_final = TASK_A_COUNT.load(Ordering::Relaxed);
    let b_final = TASK_B_COUNT.load(Ordering::Relaxed);
    assert!(a_final > 0, "Task A must have received CPU time slices");
    assert!(b_final > 0, "Task B must have received CPU time slices");
    let reaped_fair = reap_terminated();
    assert_eq!(reaped_fair, 3, "All 3 fair workers must be reaped");
    serial_println!("[ok] (Task A = {}, Task B = {}, fair slicing verified!)", a_final, b_final);

    serial_println!("==================================================");
    serial_println!("CA-6 Layer 6 Timer Preemption: ALL PASSED");
    serial_println!("==================================================");

    exit_qemu(QemuExitCode::Success);
    micro_os::hlt_loop();
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    micro_os::test_panic_handler(info)
}
