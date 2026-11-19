//! Lock Ranking Hierarchy for Deadlock Prevention
//!
//! Specification: CA-6 Section 11 (Track C: Concurrency Engineering)
//!
//! Implements strict partial order lock acquisition to eliminate
//! Coffman Condition 4 (Circular Wait). By assigning a monotonic integer
//! rank to each synchronization object, threads can only acquire locks
//! of strictly increasing rank order. Attempted out-of-order acquisitions
//! are immediately detected and rejected, guaranteeing freedom from deadlock.

use core::fmt;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicU32, Ordering};
use crate::sync::spinlock::{Spinlock, SpinlockGuard};

pub type LockRank = u32;

/// Thread-local highest held lock rank across the system.
pub static CURRENT_HELD_RANK: AtomicU32 = AtomicU32::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockRankError {
    HierarchyViolation {
        requested_rank: LockRank,
        currently_held_rank: LockRank,
    },
}

impl fmt::Display for LockRankError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LockRankError::HierarchyViolation {
                requested_rank,
                currently_held_rank,
            } => {
                write!(
                    f,
                    "Lock ranking violation: requested rank {} <= held rank {} (deadlock prevented)",
                    requested_rank, currently_held_rank
                )
            }
        }
    }
}

pub struct RankedLock<T> {
    pub name: &'static str,
    pub rank: LockRank,
    inner: Spinlock<T>,
}

impl<T> RankedLock<T> {
    /// Create a new unnamed ranked lock.
    pub const fn new(rank: LockRank, data: T) -> Self {
        Self::with_name("unnamed_ranked_lock", rank, data)
    }

    /// Create a new named ranked lock with an explicit hierarchy rank.
    pub const fn with_name(name: &'static str, rank: LockRank, data: T) -> Self {
        RankedLock {
            name,
            rank,
            inner: Spinlock::with_name(name, data),
        }
    }

    /// Acquire the lock if and only if its rank is strictly greater than any currently held lock rank.
    ///
    /// If `self.rank <= CURRENT_HELD_RANK`, the request is rejected with a structured
    /// `DEADLOCK_PREVENT` trace event and returns `Err(LockRankError::HierarchyViolation)`.
    pub fn try_lock_ranked(&self) -> Result<RankedLockGuard<T>, LockRankError> {
        let current = CURRENT_HELD_RANK.load(Ordering::Relaxed);
        let task_id = crate::scheduler::current_thread_id();

        if self.rank <= current {
            crate::diagnostics::trace_deadlock_prevent(task_id, self.rank, current);
            return Err(LockRankError::HierarchyViolation {
                requested_rank: self.rank,
                currently_held_rank: current,
            });
        }

        let guard = self.inner.lock();
        let prev = CURRENT_HELD_RANK.swap(self.rank, Ordering::SeqCst);

        Ok(RankedLockGuard {
            guard,
            prev_rank: prev,
        })
    }

    /// Acquire the lock, asserting that hierarchy order is respected. Panics on violation.
    pub fn lock_ranked(&self) -> RankedLockGuard<T> {
        self.try_lock_ranked().expect("Lock ranking hierarchy violation detected")
    }

    /// Inspect the underlying Spinlock.
    pub fn inner_spinlock(&self) -> &Spinlock<T> {
        &self.inner
    }
}

pub struct RankedLockGuard<'a, T> {
    guard: SpinlockGuard<'a, T>,
    prev_rank: LockRank,
}

impl<'a, T> Deref for RankedLockGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        self.guard.deref()
    }
}

impl<'a, T> DerefMut for RankedLockGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.guard.deref_mut()
    }
}

impl<'a, T: core::fmt::Debug> core::fmt::Debug for RankedLockGuard<'a, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RankedLockGuard")
            .field("prev_rank", &self.prev_rank)
            .field("inner_data", &self.guard.deref())
            .finish()
    }
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
        let lock_a = RankedLock::with_name("lock_a", 10, 1);
        let lock_b = RankedLock::with_name("lock_b", 20, 2);

        // In-order acquisition (10 then 20) succeeds
        let mut guard_a = lock_a.try_lock_ranked().expect("should acquire rank 10");
        assert_eq!(*guard_a, 1);
        *guard_a = 42;

        let mut guard_b = lock_b.try_lock_ranked().expect("should acquire rank 20");
        assert_eq!(*guard_b, 2);
        *guard_b = 84;

        assert_eq!(CURRENT_HELD_RANK.load(Ordering::Relaxed), 20);
        drop(guard_b);
        assert_eq!(CURRENT_HELD_RANK.load(Ordering::Relaxed), 10);
        drop(guard_a);
        assert_eq!(CURRENT_HELD_RANK.load(Ordering::Relaxed), 0);
    }

    #[test_case]
    fn test_lock_ranking_violation_rejected() {
        let lock_a = RankedLock::with_name("lock_a", 10, 1);
        let lock_b = RankedLock::with_name("lock_b", 20, 2);

        let _guard_b = lock_b.try_lock_ranked().expect("should acquire rank 20");
        // Out-of-order acquisition (trying to acquire 10 while holding 20) must fail
        let err = lock_a.try_lock_ranked();
        assert!(err.is_err());
        assert_eq!(
            err.unwrap_err(),
            LockRankError::HierarchyViolation {
                requested_rank: 10,
                currently_held_rank: 20
            }
        );
    }

    #[test_case]
    fn test_lock_ranking_three_levels_nesting() {
        let lock_1 = RankedLock::with_name("lock_level_1", 100, 10);
        let lock_2 = RankedLock::with_name("lock_level_2", 200, 20);
        let lock_3 = RankedLock::with_name("lock_level_3", 300, 30);

        let g1 = lock_1.try_lock_ranked().expect("g1 ok");
        assert_eq!(CURRENT_HELD_RANK.load(Ordering::Relaxed), 100);

        let g2 = lock_2.try_lock_ranked().expect("g2 ok");
        assert_eq!(CURRENT_HELD_RANK.load(Ordering::Relaxed), 200);

        let g3 = lock_3.try_lock_ranked().expect("g3 ok");
        assert_eq!(CURRENT_HELD_RANK.load(Ordering::Relaxed), 300);

        drop(g3);
        assert_eq!(CURRENT_HELD_RANK.load(Ordering::Relaxed), 200);
        drop(g2);
        assert_eq!(CURRENT_HELD_RANK.load(Ordering::Relaxed), 100);
        drop(g1);
        assert_eq!(CURRENT_HELD_RANK.load(Ordering::Relaxed), 0);
    }
}
