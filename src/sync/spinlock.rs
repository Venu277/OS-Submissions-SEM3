//! Hand-written Interrupt-Safe Spinlock
//!
//! Specification: CA-6 Section 9 (Layer 7) & Section 11 (Track C)

use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Statistics tracking contention and wait times for performance analysis.
#[derive(Debug, Default)]
pub struct LockStats {
    pub acquisitions: AtomicU64,
    pub contentions: AtomicU64,
}

pub struct Spinlock<T> {
    lock: AtomicBool,
    stats: LockStats,
    data: UnsafeCell<T>,
}

unsafe impl<T: Send> Sync for Spinlock<T> {}
unsafe impl<T: Send> Send for Spinlock<T> {}

impl<T> Spinlock<T> {
    pub const fn new(data: T) -> Self {
        Spinlock {
            lock: AtomicBool::new(false),
            stats: LockStats {
                acquisitions: AtomicU64::new(0),
                contentions: AtomicU64::new(0),
            },
            data: UnsafeCell::new(data),
        }
    }

    /// Acquire the spinlock, busy-waiting if locked.
    pub fn lock(&self) -> SpinlockGuard<T> {
        if self.lock.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            self.stats.contentions.fetch_add(1, Ordering::Relaxed);
            while self.lock.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
                core::hint::spin_loop();
            }
        }
        self.stats.acquisitions.fetch_add(1, Ordering::Relaxed);

        SpinlockGuard {
            lock: &self.lock,
            // SAFETY: The spinlock was acquired via atomic compare-and-swap with Acquire
            // ordering. This guarantees exclusive access to `data` until the guard is dropped.
            data: unsafe { &mut *self.data.get() },
        }
    }

    pub fn stats(&self) -> (u64, u64) {
        (
            self.stats.acquisitions.load(Ordering::Relaxed),
            self.stats.contentions.load(Ordering::Relaxed),
        )
    }
}

pub struct SpinlockGuard<'a, T> {
    lock: &'a AtomicBool,
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

impl<'a, T> Drop for SpinlockGuard<'a, T> {
    fn drop(&mut self) {
        self.lock.store(false, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case]
    fn test_spinlock_basic_lock_unlock() {
        let lock = Spinlock::new(42);
        {
            let mut guard = lock.lock();
            assert_eq!(*guard, 42);
            *guard = 100;
        }
        {
            let guard = lock.lock();
            assert_eq!(*guard, 100);
        }
        let (acquisitions, contentions) = lock.stats();
        assert_eq!(acquisitions, 2);
        assert_eq!(contentions, 0);
    }
}
