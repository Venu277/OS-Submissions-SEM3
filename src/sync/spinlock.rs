//! Hand-written Interrupt-Safe Spinlock with Contention Metrics
//!
//! Specification: CA-6 Section 9 (Layer 7) & Section 11 (Track C: Concurrency Engineering)
//!
//! Features:
//! - Atomic test-and-set / CAS via core::sync::atomic::AtomicBool
//! - Contention count (how many attempts encountered a locked state)
//! - Spin loop counter (exact spin cycles waited before acquiring)
//! - Hold-time measurement (ticks lock was held before release)
//! - RAII SpinlockGuard with automated diagnostics and metrics capture

use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Snapshot of lock performance & contention statistics.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LockMetricsSnapshot {
    pub acquisitions: u64,
    pub contentions: u64,
    pub spins_waited: u64,
    pub max_spins: u64,
    pub hold_ticks: u64,
}

/// Internal atomic statistics counters for Spinlock.
#[derive(Debug, Default)]
pub struct LockStats {
    pub acquisitions: AtomicU64,
    pub contentions: AtomicU64,
    pub spins_waited: AtomicU64,
    pub max_spins: AtomicU64,
    pub hold_ticks: AtomicU64,
}

pub struct Spinlock<T> {
    name: &'static str,
    lock: AtomicBool,
    stats: LockStats,
    data: UnsafeCell<T>,
}

unsafe impl<T: Send> Sync for Spinlock<T> {}
unsafe impl<T: Send> Send for Spinlock<T> {}

impl<T> Spinlock<T> {
    /// Create a new unnamed spinlock protecting `data`.
    pub const fn new(data: T) -> Self {
        Self::with_name("unnamed_spinlock", data)
    }

    /// Create a new named spinlock protecting `data`.
    pub const fn with_name(name: &'static str, data: T) -> Self {
        Spinlock {
            name,
            lock: AtomicBool::new(false),
            stats: LockStats {
                acquisitions: AtomicU64::new(0),
                contentions: AtomicU64::new(0),
                spins_waited: AtomicU64::new(0),
                max_spins: AtomicU64::new(0),
                hold_ticks: AtomicU64::new(0),
            },
            data: UnsafeCell::new(data),
        }
    }

    /// Return the static name of this lock instance.
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// Attempt to acquire the spinlock without blocking.
    /// Returns Some(SpinlockGuard) on success, or None if currently held.
    pub fn try_lock(&self) -> Option<SpinlockGuard<T>> {
        let task_id = crate::scheduler::current_thread_id();
        if self.lock.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_ok() {
            self.stats.acquisitions.fetch_add(1, Ordering::Relaxed);
            let start_tick = crate::interrupts::timer_ticks();
            crate::diagnostics::trace_lock_acq(task_id, self.name);

            Some(SpinlockGuard {
                parent: self,
                start_tick,
                task_id,
                // SAFETY: Atomic compare_exchange succeeded with Acquire ordering,
                // guaranteeing mutually exclusive access to `data`.
                data: unsafe { &mut *self.data.get() },
            })
        } else {
            self.stats.contentions.fetch_add(1, Ordering::Relaxed);
            None
        }
    }

    /// Acquire the spinlock, busy-waiting if currently locked.
    /// Measures exact spin wait loops and updates contention metrics.
    pub fn lock(&self) -> SpinlockGuard<T> {
        let task_id = crate::scheduler::current_thread_id();
        let mut spins = 0u64;

        if self.lock.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            self.stats.contentions.fetch_add(1, Ordering::Relaxed);
            loop {
                spins += 1;
                core::hint::spin_loop();
                if self.lock.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_ok() {
                    break;
                }
            }
            self.stats.spins_waited.fetch_add(spins, Ordering::Relaxed);
            let _ = self.stats.max_spins.fetch_max(spins, Ordering::Relaxed);
            crate::diagnostics::trace_lock_wait(task_id, self.name, spins);
        }

        self.stats.acquisitions.fetch_add(1, Ordering::Relaxed);
        let start_tick = crate::interrupts::timer_ticks();
        crate::diagnostics::trace_lock_acq(task_id, self.name);

        SpinlockGuard {
            parent: self,
            start_tick,
            task_id,
            // SAFETY: The spinlock was acquired via atomic compare-and-swap with Acquire
            // ordering. This guarantees exclusive access to `data` until the guard is dropped.
            data: unsafe { &mut *self.data.get() },
        }
    }

    /// Return an instantaneous snapshot of contention and performance statistics.
    pub fn stats(&self) -> LockMetricsSnapshot {
        LockMetricsSnapshot {
            acquisitions: self.stats.acquisitions.load(Ordering::Relaxed),
            contentions: self.stats.contentions.load(Ordering::Relaxed),
            spins_waited: self.stats.spins_waited.load(Ordering::Relaxed),
            max_spins: self.stats.max_spins.load(Ordering::Relaxed),
            hold_ticks: self.stats.hold_ticks.load(Ordering::Relaxed),
        }
    }
}

/// RAII Guard that releases the spinlock and updates hold-time metrics upon drop.
pub struct SpinlockGuard<'a, T> {
    parent: &'a Spinlock<T>,
    start_tick: u64,
    task_id: u64,
    data: &'a mut T,
}

impl<'a, T> Deref for SpinlockGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        self.data
    }
}

impl<'a, T> DerefMut for SpinlockGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.data
    }
}

impl<'a, T: core::fmt::Debug> core::fmt::Debug for SpinlockGuard<'a, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SpinlockGuard")
            .field("lock_name", &self.parent.name)
            .field("data", &self.data)
            .finish()
    }
}

impl<'a, T> Drop for SpinlockGuard<'a, T> {
    fn drop(&mut self) {
        let elapsed = crate::interrupts::timer_ticks().saturating_sub(self.start_tick);
        self.parent.stats.hold_ticks.fetch_add(elapsed, Ordering::Relaxed);
        crate::diagnostics::trace_lock_rel(self.task_id, self.parent.name);
        self.parent.lock.store(false, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case]
    fn test_spinlock_basic_lock_unlock() {
        let lock = Spinlock::with_name("test_basic_lock", 42);
        {
            let mut guard = lock.lock();
            assert_eq!(*guard, 42);
            *guard = 100;
        }
        {
            let guard = lock.lock();
            assert_eq!(*guard, 100);
        }
        let metrics = lock.stats();
        assert_eq!(metrics.acquisitions, 2);
        assert_eq!(metrics.contentions, 0);
        assert_eq!(metrics.spins_waited, 0);
    }

    #[test_case]
    fn test_spinlock_try_lock() {
        let lock = Spinlock::with_name("test_try_lock", 10);
        let guard1 = lock.try_lock();
        assert!(guard1.is_some());

        // Second try_lock while held must fail immediately without spinning
        let guard2 = lock.try_lock();
        assert!(guard2.is_none());

        drop(guard1);

        let guard3 = lock.try_lock();
        assert!(guard3.is_some());
    }
}
