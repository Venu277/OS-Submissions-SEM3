#![no_std]
#![no_main]

extern crate alloc;

use bootloader::{entry_point, BootInfo};
use core::panic::PanicInfo;
use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use micro_os::scheduler::{init_scheduler, reap_terminated, spawn, wait_all_threads, yield_now};
use micro_os::{allocator, exit_qemu, memory, serial_print, serial_println, QemuExitCode};
use x86_64::VirtAddr;

entry_point!(main);

static LOG_IDX: AtomicUsize = AtomicUsize::new(0);
static EXEC_ORDER: [AtomicU32; 9] = [
    AtomicU32::new(0),
    AtomicU32::new(0),
    AtomicU32::new(0),
    AtomicU32::new(0),
    AtomicU32::new(0),
    AtomicU32::new(0),
    AtomicU32::new(0),
    AtomicU32::new(0),
    AtomicU32::new(0),
];

fn record_step(task_num: u32) {
    let idx = LOG_IDX.fetch_add(1, Ordering::SeqCst);
    if idx < EXEC_ORDER.len() {
        EXEC_ORDER[idx].store(task_num, Ordering::SeqCst);
    }
}

extern "C" fn worker_task_1() {
    for step in 1..=3 {
        record_step(1);
        serial_println!("  [Task 1 / Alpha] Iteration {}/3 running", step);
        yield_now();
    }
    serial_println!("  [Task 1 / Alpha] Returning from entry function.");
}

extern "C" fn worker_task_2() {
    for step in 1..=3 {
        record_step(2);
        serial_println!("  [Task 2 / Beta ] Iteration {}/3 running", step);
        yield_now();
    }
    serial_println!("  [Task 2 / Beta ] Returning from entry function.");
}

extern "C" fn worker_task_3() {
    for step in 1..=3 {
        record_step(3);
        serial_println!("  [Task 3 / Gamma] Iteration {}/3 running", step);
        yield_now();
    }
    serial_println!("  [Task 3 / Gamma] Returning from entry function.");
}

fn main(boot_info: &'static BootInfo) -> ! {
    serial_println!("==================================================");
    serial_println!("   CA-6 Layer 5: Round Robin & Context Switch Test");
    serial_println!("==================================================");

    // 1. Initialize CPU, IDT, Paging & Heap
    micro_os::init();
    let phys_mem_offset = VirtAddr::new(boot_info.physical_memory_offset);
    let mut mapper = unsafe { memory::init(phys_mem_offset) };
    let mut frame_allocator =
        unsafe { memory::BootInfoFrameAllocator::init(&boot_info.memory_map) };
    allocator::init_heap(&mut mapper, &mut frame_allocator).expect("heap initialization failed");

    // 2. Initialize Round Robin Scheduler
    serial_print!("rr::1_init_scheduler...\t");
    init_scheduler(10);
    serial_println!("[ok]");

    // 3. Spawn 3 Cooperative Kernel Threads
    serial_print!("rr::2_spawn_three_tasks...\t");
    let id1 = spawn("task_alpha", worker_task_1);
    let id2 = spawn("task_beta", worker_task_2);
    let id3 = spawn("task_gamma", worker_task_3);
    assert_ne!(id1, id2);
    assert_ne!(id2, id3);
    serial_println!("[ok]");

    // 4. Yield CPU to let the 3 tasks execute their round-robin iterations
    serial_println!("rr::3_executing_cooperative_round_robin_loop...");
    wait_all_threads();
    serial_println!("rr::3_round_robin_loop_completed!\t[ok]");

    // 5. Verify Interleaved Round Robin Execution Order [1, 2, 3, 1, 2, 3, 1, 2, 3]
    serial_print!("rr::4_verify_task_execution_order...\t");
    let expected = [1, 2, 3, 1, 2, 3, 1, 2, 3];
    for (i, &exp) in expected.iter().enumerate() {
        let actual = EXEC_ORDER[i].load(Ordering::SeqCst);
        assert_eq!(
            actual, exp,
            "Execution order mismatch at step {}: expected {}, got {}",
            i, exp, actual
        );
    }
    serial_println!("[ok] (Exact pattern [1, 2, 3, 1, 2, 3, 1, 2, 3] verified!)");

    // 6. Verify Task Completion and Reaping
    serial_print!("rr::5_task_completion_and_reaping...\t");
    let reaped_count = reap_terminated();
    assert_eq!(reaped_count, 3, "Expected 3 terminated threads reaped, got {}", reaped_count);
    serial_println!("[ok] (3/3 terminated threads reaped!)");

    serial_println!("==================================================");
    serial_println!("CA-6 Layer 5 Round Robin & Context Switch: ALL PASSED");
    serial_println!("==================================================");

    exit_qemu(QemuExitCode::Success);
    micro_os::hlt_loop();
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    micro_os::test_panic_handler(info)
}
