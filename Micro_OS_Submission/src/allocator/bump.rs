//! Hand-written Bump Allocator
//!
//! Specification: CA-6 Section 9 (Layer 8: Memory Management) & Track B
//!
//! Fast, O(1) sequential allocator that linearly advances a bump pointer.
//! Demonstrates the extreme speed and fragmentation trade-offs compared
//! to intrusive free-list allocators.

use super::align_up;
use core::alloc::Layout;
use core::ptr;

pub struct BumpAllocator {
    heap_start: usize,
    heap_end: usize,
    next: usize,
    allocations: usize,
}

impl BumpAllocator {
    /// Creates a new empty bump allocator.
    pub const fn new() -> Self {
        BumpAllocator {
            heap_start: 0,
            heap_end: 0,
            next: 0,
            allocations: 0,
        }
    }

    /// Initializes the bump allocator with the given heap bounds.
    ///
    /// # Safety
    /// The caller must ensure that the given memory range is unused and mapped.
    pub unsafe fn init(&mut self, heap_start: usize, heap_size: usize) {
        self.heap_start = heap_start;
        self.heap_end = heap_start.saturating_add(heap_size);
        self.next = heap_start;
        self.allocations = 0;
    }

    /// Allocates memory with the requested layout.
    /// Returns null pointer on OOM without panicking.
    pub fn alloc(&mut self, layout: Layout) -> *mut u8 {
        let alloc_start = align_up(self.next, layout.align());
        let alloc_end = match alloc_start.checked_add(layout.size()) {
            Some(end) => end,
            None => return ptr::null_mut(),
        };

        if alloc_end > self.heap_end {
            // Out of memory: handle gracefully by returning null pointer
            ptr::null_mut()
        } else {
            self.next = alloc_end;
            self.allocations += 1;
            alloc_start as *mut u8
        }
    }

    /// Deallocates memory. Bump allocators can only reclaim memory when
    /// all active allocations have been freed.
    pub unsafe fn dealloc(&mut self, _ptr: *mut u8, _layout: Layout) {
        if self.allocations > 0 {
            self.allocations -= 1;
        }
        if self.allocations == 0 {
            // All allocations freed: reset bump pointer to heap start
            self.next = self.heap_start;
        }
    }

    /// Returns remaining contiguous bytes available before heap exhaustion.
    pub fn free_memory(&self) -> usize {
        self.heap_end.saturating_sub(self.next)
    }

    /// Returns number of active allocations.
    pub fn active_allocations(&self) -> usize {
        self.allocations
    }
}
