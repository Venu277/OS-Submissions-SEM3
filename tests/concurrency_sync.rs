#![no_std]
#![no_main]

extern crate alloc;

use bootloader::{entry_point, BootInfo};
use core::panic::PanicInfo;
use core::sync::atomic::{AtomicBool, Ordering};
use micro_os::scheduler::{
    enable_preemption, init_scheduler, reap_terminated, spawn, wait_all_threads, yield_now,
};
use micro_os::sync::ranked::{LockRankError, RankedLock, CURRENT_HELD_RANK};
use micro_os::sync::spinlock::Spinlock;
use micro_os::{allocator, exit_qemu, memory, serial_print, serial_println, QemuExitCode};
use x86_64::VirtAddr;

entry_point!(main);

// Shared synchronization objects for Test 2: Preemptive Contention
static SPIN_CONTENTION_LOCK: Spinlock<u64> = Spinlock::with_name("preempt_contention_lock", 0);
static HOLDER_FINISHED: AtomicBool = AtomicBool::new(false);
static CONTENDER_ACQUIRED: AtomicBool = AtomicBool::new(false);

extern "C" fn lock_holder_worker() {
    serial_println!("  [Holder Task] Acquiring spinlock...");
    let mut guard = SPIN_CONTENTION_LOCK.lock();
    *guard = 777;
    serial_println!("  [Holder Task] Lock acquired; yielding CPU while holding lock to induce contention...");
    yield_now();
    serial_println!("  [Holder Task] Resumed via timer preemption; releasing lock now.");
    drop(guard);
    HOLDER_FINISHED.store(true, Ordering::SeqCst);
}

extern "C" fn lock_contender_worker() {
    serial_println!("  [Contender Task] Attempting to acquire lock held by Holder Task...");
    let mut guard = SPIN_CONTENTION_LOCK.lock();
    serial_println!("  [Contender Task] Lock acquired successfully after spinning!");
    assert_eq!(*guard, 777);
    *guard = 888;
    CONTENDER_ACQUIRED.store(true, Ordering::SeqCst);
    drop(guard);
}

fn main(boot_info: &'static BootInfo) -> ! {
    serial_println!("==================================================");
    serial_println!("   CA-6 Layer 7 & Track C: Concurrency Sync Test");
    serial_println!("==================================================");

    // 1. Initialize CPU, IDT, PIT (100 Hz), Paging & Heap
    micro_os::init();
    let phys_mem_offset = VirtAddr::new(boot_info.physical_memory_offset);
    let mut mapper = unsafe { memory::init(phys_mem_offset) };
    let mut frame_allocator =
        unsafe { memory::BootInfoFrameAllocator::init(&boot_info.memory_map) };
    allocator::init_heap(&mut mapper, &mut frame_allocator).expect("heap initialization failed");

    // Test 1: Spinlock Mutual Exclusion & Try-Lock
    serial_print!("sync::1_spinlock_mutual_exclusion...\t");
    let lock = Spinlock::with_name("unit_lock", 42);
    {
        let mut guard = lock.lock();
        assert_eq!(*guard, 42);
        *guard = 99;
        assert!(lock.try_lock().is_none(), "Lock must be non-reentrant / exclusive");
    }
    {
        let guard = lock.try_lock();
        assert!(guard.is_some(), "Lock should be available after drop");
        assert_eq!(*guard.unwrap(), 99);
    }
    let stats1 = lock.stats();
    assert_eq!(stats1.acquisitions, 2);
    assert_eq!(stats1.contentions, 1, "Failed try_lock should record 1 contention event");
    serial_println!("[ok]");

    // Test 2: Spinlock Contention & Metrics under Multi-Task Competition
    serial_print!("sync::2_spinlock_contention_metrics...\t");
    init_scheduler(2); // 2 ticks quantum (20 ms @ 100 Hz)
    HOLDER_FINISHED.store(false, Ordering::SeqCst);
    CONTENDER_ACQUIRED.store(false, Ordering::SeqCst);

    spawn("lock_holder", lock_holder_worker);
    spawn("lock_contender", lock_contender_worker);

    enable_preemption(true);
    wait_all_threads();
    enable_preemption(false);

    let reaped = reap_terminated();
    assert_eq!(reaped, 2, "Expected 2 worker threads reaped");
    assert!(HOLDER_FINISHED.load(Ordering::SeqCst));
    assert!(CONTENDER_ACQUIRED.load(Ordering::SeqCst));

    let stats2 = SPIN_CONTENTION_LOCK.stats();
    assert!(stats2.acquisitions >= 2, "Expected at least 2 acquisitions");
    assert!(stats2.contentions >= 1, "Expected at least 1 contention event during concurrent lock attempt");
    assert!(stats2.spins_waited > 0, "Contender should have waited for spins before acquiring");
    assert!(stats2.max_spins > 0, "Max spins should be > 0");

    serial_println!(
        "[ok] (Acquisitions={}, Contentions={}, SpinsWaited={}, MaxSpins={})",
        stats2.acquisitions,
        stats2.contentions,
        stats2.spins_waited,
        stats2.max_spins
    );

    // Test 3: Lock Ranking In-Order Acquisition Success (Strict Partial Order)
    serial_print!("sync::3_lock_ranking_in_order_success...\t");
    CURRENT_HELD_RANK.store(0, Ordering::SeqCst);
    let lock_a = RankedLock::with_name("lock_device", 10, 100);
    let lock_b = RankedLock::with_name("lock_filesystem", 20, 200);

    {
        let mut guard_a = lock_a.try_lock_ranked().expect("Acquiring rank 10 must succeed");
        assert_eq!(*guard_a, 100);
        *guard_a = 101;
        assert_eq!(CURRENT_HELD_RANK.load(Ordering::Relaxed), 10);

        let mut guard_b = lock_b.try_lock_ranked().expect("Acquiring rank 20 while holding rank 10 must succeed");
        assert_eq!(*guard_b, 200);
        *guard_b = 201;
        assert_eq!(CURRENT_HELD_RANK.load(Ordering::Relaxed), 20);

        drop(guard_b);
        assert_eq!(CURRENT_HELD_RANK.load(Ordering::Relaxed), 10, "Dropping guard_b must restore held rank to 10");
    }
    assert_eq!(CURRENT_HELD_RANK.load(Ordering::Relaxed), 0, "Dropping guard_a must restore held rank to 0");
    serial_println!("[ok]");

    // Test 4: Lock Ranking Deadlock Prevention (Out-of-Order Rejection)
    serial_print!("sync::4_lock_ranking_deadlock_prevention_reject...\t");
    CURRENT_HELD_RANK.store(0, Ordering::SeqCst);
    {
        let _guard_b = lock_b.try_lock_ranked().expect("Acquiring rank 20 succeeds");
        assert_eq!(CURRENT_HELD_RANK.load(Ordering::Relaxed), 20);

        // Attempting to acquire Rank 10 while holding Rank 20 must be REJECTED!
        let reject_result = lock_a.try_lock_ranked();
        assert!(reject_result.is_err(), "Out-of-order acquisition MUST be rejected");
        assert_eq!(
            reject_result.unwrap_err(),
            LockRankError::HierarchyViolation {
                requested_rank: 10,
                currently_held_rank: 20
            }
        );
        assert_eq!(CURRENT_HELD_RANK.load(Ordering::Relaxed), 20, "Held rank must remain 20 after rejection");
    }
    assert_eq!(CURRENT_HELD_RANK.load(Ordering::Relaxed), 0, "Held rank must be 0 after guard drop");
    serial_println!("[ok] (Hierarchy inversion detected and prevented!)");

    // Test 5: Three-Level Hierarchical Lock Nesting & LIFO Unwind
    serial_print!("sync::5_three_level_hierarchy_nesting...\t");
    let l1 = RankedLock::with_name("level_1", 100, 1);
    let l2 = RankedLock::with_name("level_2", 200, 2);
    let l3 = RankedLock::with_name("level_3", 300, 3);

    let g1 = l1.try_lock_ranked().expect("L1 ok");
    assert_eq!(CURRENT_HELD_RANK.load(Ordering::Relaxed), 100);

    let g2 = l2.try_lock_ranked().expect("L2 ok");
    assert_eq!(CURRENT_HELD_RANK.load(Ordering::Relaxed), 200);

    let g3 = l3.try_lock_ranked().expect("L3 ok");
    assert_eq!(CURRENT_HELD_RANK.load(Ordering::Relaxed), 300);

    // Any reverse acquisition while at 300 must fail
    assert!(l1.try_lock_ranked().is_err());
    assert!(l2.try_lock_ranked().is_err());

    drop(g3);
    assert_eq!(CURRENT_HELD_RANK.load(Ordering::Relaxed), 200);
    drop(g2);
    assert_eq!(CURRENT_HELD_RANK.load(Ordering::Relaxed), 100);
    drop(g1);
    assert_eq!(CURRENT_HELD_RANK.load(Ordering::Relaxed), 0);
    serial_println!("[ok]");

    serial_println!("==================================================");
    serial_println!("CA-6 Layer 7 & Track C Concurrency Sync: ALL PASSED");
    serial_println!("==================================================");

    exit_qemu(QemuExitCode::Success);
    micro_os::hlt_loop();
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    micro_os::test_panic_handler(info)
}
