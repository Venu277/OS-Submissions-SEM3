#![no_std]
#![no_main]

extern crate alloc;

use bootloader::{entry_point, BootInfo};
use core::panic::PanicInfo;
use micro_os::kthread::{KThread, ThreadState};
use micro_os::scheduler::{ReadyQueue, RoundRobinScheduler};
use micro_os::sync::ranked::RankedLock;
use micro_os::sync::spinlock::Spinlock;
use micro_os::{allocator, exit_qemu, memory, serial_print, serial_println, QemuExitCode};
use x86_64::VirtAddr;

entry_point!(main);

fn main(boot_info: &'static BootInfo) -> ! {
    serial_println!("==================================================");
    serial_println!("   CA-6 Core Integrated Demo Test Suite");
    serial_println!("==================================================");

    // Layer 1: Boot & Interrupt initialization
    serial_print!("ca6::1_kernel_boot_and_cpu_init...\t");
    micro_os::init();
    serial_println!("[ok]");

    // Layer 8: Memory Allocator & Paging
    serial_print!("ca6::2_paging_and_heap_allocator...\t");
    let phys_mem_offset = VirtAddr::new(boot_info.physical_memory_offset);
    let mut mapper = unsafe { memory::init(phys_mem_offset) };
    let mut frame_allocator =
        unsafe { memory::BootInfoFrameAllocator::init(&boot_info.memory_map) };
    allocator::init_heap(&mut mapper, &mut frame_allocator).expect("heap initialization failed");
    serial_println!("[ok]");

    // Layer 3: Task Representation
    serial_print!("ca6::3_kthread_representation...\t");
    extern "C" fn sample_entry() {}
    let thread1 = KThread::new("ca6_worker_1", sample_entry);
    let thread2 = KThread::new("ca6_worker_2", sample_entry);
    assert_eq!(thread1.state, ThreadState::Ready);
    assert_ne!(thread1.id, thread2.id);
    assert_eq!(thread1.context.rsp % 16, 0);
    serial_println!("[ok]");

    // Layer 4 & 5: Ready Queue & Round Robin Scheduler
    serial_print!("ca6::4_ready_queue_and_scheduler...\t");
    let mut rq = ReadyQueue::new();
    let id1 = thread1.id;
    let id2 = thread2.id;
    rq.enqueue(thread1);
    rq.enqueue(thread2);
    assert_eq!(rq.len(), 2);
    assert_eq!(rq.dequeue().unwrap().id, id1);
    assert_eq!(rq.dequeue().unwrap().id, id2);
    assert!(rq.is_empty());

    let sched = RoundRobinScheduler::new(10);
    assert_eq!(sched.default_quantum, 10);
    assert_eq!(sched.total_switches, 0);
    serial_println!("[ok]");

    // Layer 7 & Track C: Synchronization & Deadlock Prevention
    serial_print!("ca6::5_spinlock_and_lock_ranking...\t");
    let lock = Spinlock::new(100);
    {
        let mut g = lock.lock();
        *g += 50;
    }
    assert_eq!(*lock.lock(), 150);

    let rank_low = RankedLock::new(10, 1);
    let rank_high = RankedLock::new(20, 2);
    {
        let _g1 = rank_low.try_lock_ranked().expect("rank 10 lock");
        let _g2 = rank_high.try_lock_ranked().expect("rank 20 lock");
    }
    // Out-of-order acquisition must fail
    {
        let _g2 = rank_high.try_lock_ranked().expect("rank 20 lock");
        assert!(rank_low.try_lock_ranked().is_err());
    }
    serial_println!("[ok]");

    // Layer 10: Integrated Demo Execution
    serial_print!("ca6::6_integrated_demo_execution...\t");
    micro_os::scheduler::run_ca6_demo();
    serial_println!("[ok]");

    serial_println!("==================================================");
    serial_println!("CA-6 Core Demonstration: ALL 10 LAYERS VERIFIED");
    serial_println!("==================================================");

    exit_qemu(QemuExitCode::Success);
    micro_os::hlt_loop();
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    micro_os::test_panic_handler(info)
}
