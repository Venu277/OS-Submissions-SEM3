//! CA-6 Layer 7 & Track C: Kernel Synchronization & Lock Ranking
//!
//! Specification: CA-6 Section 9 (Layer 7), Section 10 & Section 11 (Track C)
//!
//! Provides:
//! - Hand-written atomic spinlocks with contention metrics and hold time
//! - Strict lock hierarchy / ranked locks preventing Coffman circular wait deadlocks
//! - Interactive demonstrations of shared resource race correction, contention metrics, and deadlock prevention

pub mod spinlock;
pub mod ranked;

use core::sync::atomic::{AtomicU64, Ordering};
use crate::println;
use crate::serial_println;
use crate::sync::ranked::{RankedLock, CURRENT_HELD_RANK};
use crate::sync::spinlock::Spinlock;

// Shared state for synchronization demos
static SHARED_SPINLOCK_COUNTER: Spinlock<u64> = Spinlock::with_name("shared_sync_counter", 0);
static CONTENDED_LOCK: Spinlock<u64> = Spinlock::with_name("contended_mutex", 0);
static WORKER_ITERATIONS: AtomicU64 = AtomicU64::new(0);

extern "C" fn worker_contention_a() {
    for _ in 0..10 {
        let mut guard = CONTENDED_LOCK.lock();
        *guard += 1;
        // Hold lock briefly to generate realistic contention
        for _ in 0..1_000 {
            core::hint::spin_loop();
        }
        WORKER_ITERATIONS.fetch_add(1, Ordering::Relaxed);
    }
}

extern "C" fn worker_contention_b() {
    for _ in 0..10 {
        let mut guard = CONTENDED_LOCK.lock();
        *guard += 1;
        for _ in 0..1_000 {
            core::hint::spin_loop();
        }
        WORKER_ITERATIONS.fetch_add(1, Ordering::Relaxed);
    }
}

/// Run the full CA-6 Synchronization and Track C Concurrency Engineering demonstration.
pub fn demo_synchronization() {
    println!("==================================================");
    println!("   CA-6 Layer 7 & Track C: Concurrency Engineering");
    println!("==================================================");
    println!("Specification: CA-6 Section 9 (Layer 7) & Section 11 (Track C)");
    println!("Selected Specialization: Track C - Concurrency Engineering");
    println!("--------------------------------------------------");

    // Part 1: Shared Resource Protection (Layer 7 requirement)
    println!("Part 1: Shared-Resource Synchronization with Spinlock<T>");
    serial_println!("[CA6-SYNC] Part 1: Testing Spinlock mutual exclusion...");
    {
        let mut g1 = SHARED_SPINLOCK_COUNTER.lock();
        *g1 = 100;
        println!("  Initial protected counter value = {}", *g1);
    }
    {
        let mut g2 = SHARED_SPINLOCK_COUNTER.lock();
        *g2 += 250;
        println!("  Updated protected counter value = {}", *g2);
    }
    let stats1 = SHARED_SPINLOCK_COUNTER.stats();
    println!("  Spinlock acquisitions={}, contentions={}", stats1.acquisitions, stats1.contentions);
    serial_println!(
        "[CA6-SYNC] Spinlock verified: acquisitions={}, contentions={}",
        stats1.acquisitions,
        stats1.contentions
    );

    // Part 2: Lock Contention Metrics (Track C requirement)
    println!("--------------------------------------------------");
    println!("Part 2: Lock Contention Metrics under Competing Threads");
    serial_println!("[CA6-SYNC] Part 2: Evaluating lock contention metrics...");

    WORKER_ITERATIONS.store(0, Ordering::Relaxed);
    crate::scheduler::init_scheduler(5);
    crate::scheduler::spawn("worker_contend_a", worker_contention_a);
    crate::scheduler::spawn("worker_contend_b", worker_contention_b);

    crate::scheduler::wait_all_threads();
    let reaped = crate::scheduler::reap_terminated();

    let metrics = CONTENDED_LOCK.stats();
    let protected_val = { *CONTENDED_LOCK.lock() };
    println!("  Competing workers finished ({} reaped, {} iterations total).", reaped, WORKER_ITERATIONS.load(Ordering::Relaxed));
    println!("  Protected counter final value: {}", protected_val);
    println!("  Contention Statistics for '{}':", CONTENDED_LOCK.name());
    println!("    - Total Acquisitions: {}", metrics.acquisitions);
    println!("    - Contention Events:  {}", metrics.contentions);
    println!("    - Total Spins Waited: {}", metrics.spins_waited);
    println!("    - Max Spins Waited:   {}", metrics.max_spins);
    println!("    - Lock Hold Ticks:    {}", metrics.hold_ticks);

    serial_println!(
        "[CA6-SYNC] Contention metrics: acq={}, cont={}, spins={}, max_spins={}, hold_ticks={}",
        metrics.acquisitions,
        metrics.contentions,
        metrics.spins_waited,
        metrics.max_spins,
        metrics.hold_ticks
    );

    // Part 3: Deadlock Prevention via Lock Ranking (Track C requirement)
    println!("--------------------------------------------------");
    println!("Part 3: Deadlock Prevention via Lock Ranking Hierarchy");
    serial_println!("[CA6-SYNC] Part 3: Demonstrating lock ranking hierarchy...");

    CURRENT_HELD_RANK.store(0, Ordering::SeqCst);
    let lock_level_1 = RankedLock::with_name("driver_lock", 10, 100);
    let lock_level_2 = RankedLock::with_name("vfs_lock", 20, 200);

    println!("  Scenario A: In-Order Acquisition (Rank 10 -> Rank 20)");
    {
        let g_a = lock_level_1.try_lock_ranked();
        assert!(g_a.is_ok(), "Rank 10 acquisition must succeed");
        println!("    [Acquired] Lock '{}' (Rank 10)", lock_level_1.name);

        let g_b = lock_level_2.try_lock_ranked();
        assert!(g_b.is_ok(), "Rank 20 acquisition while holding Rank 10 must succeed");
        println!("    [Acquired] Lock '{}' (Rank 20)", lock_level_2.name);
        println!("    [SUCCESS] In-order acquisition respected strict partial order.");
    }
    assert_eq!(CURRENT_HELD_RANK.load(Ordering::Relaxed), 0);

    println!("  Scenario B: Out-of-Order Acquisition Attempt (Rank 20 -> Rank 10)");
    {
        let _g_b = lock_level_2.try_lock_ranked().expect("Rank 20 acquisition succeeds");
        println!("    [Acquired] Lock '{}' (Rank 20)", lock_level_2.name);

        println!("    [Attempt] Attempting to acquire Lock '{}' (Rank 10) while holding Rank 20...", lock_level_1.name);
        let err_result = lock_level_1.try_lock_ranked();
        match err_result {
            Ok(_) => panic!("Fatal: Lock ranking failed to prevent out-of-order acquisition!"),
            Err(e) => {
                println!("    [PREVENTED] Deadlock prevented! Error: {}", e);
                serial_println!("[CA6-DEADLOCK] Out-of-order acquisition prevented: {}", e);
            }
        }
    }
    assert_eq!(CURRENT_HELD_RANK.load(Ordering::Relaxed), 0);

    println!("--------------------------------------------------");
    println!("CA-6 Synchronization & Track C Demo Completed Successfully!");
    println!("==================================================");
    serial_println!("[CA6-DEMO] Synchronization & Track C validation completed successfully.");
}
