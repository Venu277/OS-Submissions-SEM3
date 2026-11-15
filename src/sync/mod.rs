//! CA-6 Layer 7 & Track C: Kernel Synchronization & Lock Ranking
//!
//! Specification: CA-6 Section 9 (Layer 7), Section 10 & Section 11 (Track C)
//!
//! Provides:
//! - Hand-written interrupt-safe spinlocks implemented via atomic test-and-set
//! - Lock ranking hierarchy to prevent Coffman circular wait deadlocks
//! - Lock contention statistics (wait ticks, acquisition counts, contention rate)

pub mod spinlock;
pub mod ranked;
