//! Lock Ranking Hierarchy for Deadlock Prevention
//!
//! Specification: CA-6 Section 11 (Track C: Concurrency Engineering)
//!
//! Implements strict partial order lock acquisition to eliminate
//! Coffman Condition 4 (Circular Wait).

use core::sync::atomic::{AtomicU32, Ordering};
use crate::sync::spinlock::{Spinlock, SpinlockGuard};

pub type LockRank = u32;

/// Thread-local highest held lock rank.
pub static CURRENT_HELD_RANK: AtomicU32 = AtomicU32::new(0);

pub struct RankedLock<T> {
    pub rank: LockRank,
    inner: Spinlock<T>,
}

impl<T> RankedLock<T> {
    pub const fn new(rank: LockRank, data: T) -> Self {
        RankedLock {
            rank,
            inner: Spinlock::new(data),
        }
    }

    /// Acquire the lock if and only if its rank is greater than any currently held lock rank.
    pub fn try_lock_ranked(&self) -> Result<RankedLockGuard<T>, &'static str> {
        let current = CURRENT_HELD_RANK.load(Ordering::Relaxed);
        if self.rank <= current {
            return Err("Lock ranking violation: out-of-order acquisition attempted!");
        }

        let guard = self.inner.lock();
        let prev = CURRENT_HELD_RANK.swap(self.rank, Ordering::SeqCst);

        Ok(RankedLockGuard {
            _guard: guard,
            prev_rank: prev,
        })
    }
}

pub struct RankedLockGuard<'a, T> {
    _guard: SpinlockGuard<'a, T>,
    prev_rank: LockRank,
}

impl<'a, T> Drop for RankedLockGuard<'a, T> {
    fn drop(&mut self) {
        CURRENT_HELD_RANK.store(self.prev_rank, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case]
    fn test_lock_ranking_order_success() {
        let lock_a = RankedLock::new(10, 1);
        let lock_b = RankedLock::new(20, 2);

        // In-order acquisition (10 then 20) succeeds
        let guard_a = lock_a.try_lock_ranked().expect("should acquire rank 10");
        let guard_b = lock_b.try_lock_ranked().expect("should acquire rank 20");
        assert_eq!(CURRENT_HELD_RANK.load(Ordering::Relaxed), 20);
        drop(guard_b);
        assert_eq!(CURRENT_HELD_RANK.load(Ordering::Relaxed), 10);
        drop(guard_a);
        assert_eq!(CURRENT_HELD_RANK.load(Ordering::Relaxed), 0);
    }

    #[test_case]
    fn test_lock_ranking_violation_rejected() {
        let lock_a = RankedLock::new(10, 1);
        let lock_b = RankedLock::new(20, 2);

        let _guard_b = lock_b.try_lock_ranked().expect("should acquire rank 20");
        // Out-of-order acquisition (trying to acquire 10 while holding 20) must fail
        let err = lock_a.try_lock_ranked();
        assert!(err.is_err());
    }
}
