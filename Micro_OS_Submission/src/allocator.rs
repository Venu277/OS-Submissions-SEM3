pub mod bump;
pub mod linked_list;

use core::alloc::{GlobalAlloc, Layout};
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use bump::BumpAllocator;
use linked_list::LinkedListAllocator;
use spin::Mutex;
use x86_64::{
    structures::paging::{
        mapper::MapToError, FrameAllocator, Mapper, Page, PageTableFlags, Size4KiB,
    },
    VirtAddr,
};

extern crate alloc;

/// Start address of the kernel heap.
pub const HEAP_START: usize = 0x_4444_4444_0000;
/// Minimum heap size: 256 KiB.
pub const MIN_HEAP_SIZE: usize = 256 * 1024;
/// Maximum adaptive heap size: 4 MiB.
pub const MAX_HEAP_SIZE: usize = 4 * 1024 * 1024;
/// Baseline default heap size: 256 KiB.
pub const HEAP_SIZE: usize = MIN_HEAP_SIZE;
/// Actual initialized heap size in bytes.
pub static HEAP_ACTUAL_SIZE: AtomicUsize = AtomicUsize::new(HEAP_SIZE);

/// Dynamically determine an appropriate kernel heap size based on available system RAM.
///
/// Allocates 1/64th of usable RAM, clamped between MIN_HEAP_SIZE (256 KiB) and MAX_HEAP_SIZE (4 MiB).
pub fn calculate_adaptive_heap_size(usable_ram_bytes: u64) -> usize {
    if usable_ram_bytes == 0 {
        return MIN_HEAP_SIZE;
    }
    let target = (usable_ram_bytes / 64) as usize;
    target.clamp(MIN_HEAP_SIZE, MAX_HEAP_SIZE)
}

/// Global flag controlling structured allocation trace emission (Layer 2 & Layer 8).
pub static ALLOC_TRACING_ENABLED: AtomicBool = AtomicBool::new(false);

/// Enable or disable structured allocation/deallocation tracing over serial.
pub fn enable_alloc_tracing(enable: bool) {
    ALLOC_TRACING_ENABLED.store(enable, Ordering::SeqCst);
}

/// Query whether allocation tracing is enabled.
pub fn is_alloc_tracing_enabled() -> bool {
    ALLOC_TRACING_ENABLED.load(Ordering::SeqCst)
}

/// A wrapper around spin::Mutex to permit trait implementations.
pub struct Locked<A> {
    inner: Mutex<A>,
}

impl<A> Locked<A> {
    pub const fn new(inner: A) -> Self {
        Locked {
            inner: Mutex::new(inner),
        }
    }

    pub fn lock(&self) -> spin::MutexGuard<A> {
        self.inner.lock()
    }
}

unsafe impl GlobalAlloc for Locked<LinkedListAllocator> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = self.inner.lock().alloc(layout);
        if is_alloc_tracing_enabled() {
            crate::diagnostics::trace_alloc(layout.size(), !ptr.is_null());
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.inner.lock().dealloc(ptr, layout);
        if is_alloc_tracing_enabled() {
            crate::diagnostics::trace_dealloc(layout.size());
        }
    }
}

#[global_allocator]
static ALLOCATOR: Locked<LinkedListAllocator> = Locked::new(LinkedListAllocator::new());

/// Map heap virtual memory pages to physical frames and initialize the heap allocator
/// with a customized or adaptive heap size.
pub fn init_heap_adaptive(
    mapper: &mut impl Mapper<Size4KiB>,
    frame_allocator: &mut impl FrameAllocator<Size4KiB>,
    heap_size: usize,
) -> Result<(), MapToError<Size4KiB>> {
    let aligned_size = (heap_size + 4095) & !4095;
    let page_range = {
        let heap_start = VirtAddr::new(HEAP_START as u64);
        let heap_end = heap_start + aligned_size - 1u64;
        let heap_start_page = Page::containing_address(heap_start);
        let heap_end_page = Page::containing_address(heap_end);
        Page::range_inclusive(heap_start_page, heap_end_page)
    };

    for page in page_range {
        let frame = frame_allocator
            .allocate_frame()
            .ok_or(MapToError::FrameAllocationFailed)?;
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
        unsafe {
            mapper.map_to(page, frame, flags, frame_allocator)?.flush();
        }
    }

    unsafe {
        ALLOCATOR.lock().init(HEAP_START, aligned_size);
    }

    HEAP_ACTUAL_SIZE.store(aligned_size, Ordering::SeqCst);
    Ok(())
}

/// Map heap virtual memory pages to physical frames and initialize the heap allocator with baseline size.
pub fn init_heap(
    mapper: &mut impl Mapper<Size4KiB>,
    frame_allocator: &mut impl FrameAllocator<Size4KiB>,
) -> Result<(), MapToError<Size4KiB>> {
    init_heap_adaptive(mapper, frame_allocator, HEAP_SIZE)
}

/// Returns (used_bytes, free_bytes, total_bytes) of the kernel heap.
pub fn memory_usage() -> (usize, usize, usize) {
    let free = ALLOCATOR.lock().free_memory();
    let total = HEAP_ACTUAL_SIZE.load(Ordering::Relaxed);
    let used = total.saturating_sub(free);
    (used, free, total)
}

/// Align the given address `addr` upwards to alignment `align`.
///
/// Requires that `align` is a power of two.
pub fn align_up(addr: usize, align: usize) -> usize {
    (addr + align - 1) & !(align - 1)
}

/// Benchmark results comparing allocator performance metrics.
#[derive(Debug, Clone, Copy, Default)]
pub struct AllocatorBenchmarkMetrics {
    pub total_allocations: usize,
    pub total_bytes_allocated: usize,
    pub min_latency_tsc: u64,
    pub max_latency_tsc: u64,
    pub avg_latency_tsc: u64,
    pub free_before: usize,
    pub free_after_100_alloc: usize,
    pub free_after_50_free: usize,
    pub oom_handled_ok: bool,
}

#[repr(align(16))]
pub struct BenchmarkBuffer(pub [u8; 32 * 1024]);

/// Dedicated static buffer in BSS section for allocator benchmarking.
/// Prevents fragmentation or competition with the primary kernel heap.
static TEST_HEAP_BUFFER: spin::Mutex<BenchmarkBuffer> = spin::Mutex::new(BenchmarkBuffer([0u8; 32 * 1024]));

/// Run an empirical benchmark evaluating allocation latency, fragmentation, and memory reuse.
pub fn benchmark_allocator_comparison() -> (AllocatorBenchmarkMetrics, AllocatorBenchmarkMetrics) {
    const BLOCK_COUNT: usize = 100;
    let mut block_sizes = [0usize; BLOCK_COUNT];
    for i in 0..BLOCK_COUNT {
        block_sizes[i] = match i % 5 {
            0 => 32,
            1 => 64,
            2 => 128,
            3 => 256,
            _ => 512,
        };
    }

    const TEST_HEAP_SIZE: usize = 32 * 1024; // 32 KiB dedicated buffer
    let mut buffer_guard = TEST_HEAP_BUFFER.lock();
    for b in buffer_guard.0.iter_mut() {
        *b = 0;
    }
    let buffer_base = buffer_guard.0.as_mut_ptr() as usize;

    // =========================================================================
    // 1. Benchmark Intrusive Linked List Allocator
    // =========================================================================
    let mut ll = LinkedListAllocator::new();
    unsafe {
        ll.init(buffer_base, TEST_HEAP_SIZE);
    }
    let ll_free_before = ll.free_memory();

    let mut ll_ptrs = [core::ptr::null_mut::<u8>(); BLOCK_COUNT];
    let mut ll_latencies = [0u64; BLOCK_COUNT];
    let mut ll_total_bytes = 0usize;

    for i in 0..BLOCK_COUNT {
        let size = block_sizes[i];
        let layout = Layout::from_size_align(size, 8).unwrap();
        ll_total_bytes += size;

        let t_start = unsafe { core::arch::x86_64::_rdtsc() };
        let ptr = ll.alloc(layout);
        let t_end = unsafe { core::arch::x86_64::_rdtsc() };

        ll_ptrs[i] = ptr;
        ll_latencies[i] = t_end.saturating_sub(t_start);
        if is_alloc_tracing_enabled() {
            crate::diagnostics::trace_alloc(size, !ptr.is_null());
        }
    }

    let ll_min = *ll_latencies.iter().min().unwrap_or(&0);
    let ll_max = *ll_latencies.iter().max().unwrap_or(&0);
    let ll_sum: u64 = ll_latencies.iter().sum();
    let ll_avg = ll_sum / (BLOCK_COUNT as u64);
    let ll_free_after_100 = ll.free_memory();

    // Free 50% (every even index)
    for i in (0..BLOCK_COUNT).step_by(2) {
        if !ll_ptrs[i].is_null() {
            let size = block_sizes[i];
            let layout = Layout::from_size_align(size, 8).unwrap();
            unsafe {
                ll.dealloc(ll_ptrs[i], layout);
            }
            if is_alloc_tracing_enabled() {
                crate::diagnostics::trace_dealloc(size);
            }
        }
    }
    let ll_free_after_50_free = ll.free_memory();

    // Graceful OOM check: request 64 KiB on 32 KiB heap
    let oom_layout = Layout::from_size_align(64 * 1024, 8).unwrap();
    let oom_ptr = ll.alloc(oom_layout);
    let ll_oom_ok = oom_ptr.is_null();

    let ll_metrics = AllocatorBenchmarkMetrics {
        total_allocations: BLOCK_COUNT,
        total_bytes_allocated: ll_total_bytes,
        min_latency_tsc: ll_min,
        max_latency_tsc: ll_max,
        avg_latency_tsc: ll_avg,
        free_before: ll_free_before,
        free_after_100_alloc: ll_free_after_100,
        free_after_50_free: ll_free_after_50_free,
        oom_handled_ok: ll_oom_ok,
    };

    // =========================================================================
    // 2. Benchmark Bump Allocator (using fresh reinitialized buffer)
    // =========================================================================
    for b in buffer_guard.0.iter_mut() {
        *b = 0;
    }

    let mut bump = BumpAllocator::new();
    unsafe {
        bump.init(buffer_base, TEST_HEAP_SIZE);
    }
    let bump_free_before = bump.free_memory();

    let mut bump_ptrs = [core::ptr::null_mut::<u8>(); BLOCK_COUNT];
    let mut bump_latencies = [0u64; BLOCK_COUNT];
    let mut bump_total_bytes = 0usize;

    for i in 0..BLOCK_COUNT {
        let size = block_sizes[i];
        let layout = Layout::from_size_align(size, 8).unwrap();
        bump_total_bytes += size;

        let t_start = unsafe { core::arch::x86_64::_rdtsc() };
        let ptr = bump.alloc(layout);
        let t_end = unsafe { core::arch::x86_64::_rdtsc() };

        bump_ptrs[i] = ptr;
        bump_latencies[i] = t_end.saturating_sub(t_start);
        if is_alloc_tracing_enabled() {
            crate::diagnostics::trace_alloc(size, !ptr.is_null());
        }
    }

    let bump_min = *bump_latencies.iter().min().unwrap_or(&0);
    let bump_max = *bump_latencies.iter().max().unwrap_or(&0);
    let bump_sum: u64 = bump_latencies.iter().sum();
    let bump_avg = bump_sum / (BLOCK_COUNT as u64);
    let bump_free_after_100 = bump.free_memory();

    // Free 50%
    for i in (0..BLOCK_COUNT).step_by(2) {
        if !bump_ptrs[i].is_null() {
            let size = block_sizes[i];
            let layout = Layout::from_size_align(size, 8).unwrap();
            unsafe {
                bump.dealloc(bump_ptrs[i], layout);
            }
            if is_alloc_tracing_enabled() {
                crate::diagnostics::trace_dealloc(size);
            }
        }
    }
    let bump_free_after_50_free = bump.free_memory();

    // Graceful OOM check
    let bump_oom_ptr = bump.alloc(oom_layout);
    let bump_oom_ok = bump_oom_ptr.is_null();

    let bump_metrics = AllocatorBenchmarkMetrics {
        total_allocations: BLOCK_COUNT,
        total_bytes_allocated: bump_total_bytes,
        min_latency_tsc: bump_min,
        max_latency_tsc: bump_max,
        avg_latency_tsc: bump_avg,
        free_before: bump_free_before,
        free_after_100_alloc: bump_free_after_100,
        free_after_50_free: bump_free_after_50_free,
        oom_handled_ok: bump_oom_ok,
    };

    (ll_metrics, bump_metrics)
}

/// Run the interactive CA-6 Memory Allocator Demonstration (Layer 8 & Track B).
pub fn demo_memory_allocator() {
    crate::println!("==================================================");
    crate::println!("   CA-6 Layer 8 & Track B: Memory Allocator Evaluation");
    crate::println!("==================================================");
    crate::println!("Specification: CA-6 Section 9 (Layer 8) & Section 11 (Track B)");
    crate::println!("Comparing: Intrusive Linked List vs. Sequential Bump Allocator");
    crate::println!("Workload: 100 blocks (32B - 512B), 50% freed, OOM resilience");
    crate::println!("--------------------------------------------------");

    enable_alloc_tracing(true);
    let (ll, bump) = benchmark_allocator_comparison();
    enable_alloc_tracing(false);

    crate::println!("Empirical Benchmark Results (TSC Cycles):");
    crate::println!("  Metric                     Linked List        Bump Allocator");
    crate::println!("  -------------------------------------------------------------");
    crate::println!("  Allocated Blocks:          {:<18} {:<18}", ll.total_allocations, bump.total_allocations);
    crate::println!("  Total Volume:              {:<15} B  {:<15} B", ll.total_bytes_allocated, bump.total_bytes_allocated);
    crate::println!("  Min Latency:               {:<15} cyc{:<15} cyc", ll.min_latency_tsc, bump.min_latency_tsc);
    crate::println!("  Max Latency:               {:<15} cyc{:<15} cyc", ll.max_latency_tsc, bump.max_latency_tsc);
    crate::println!("  Avg Latency:               {:<15} cyc{:<15} cyc", ll.avg_latency_tsc, bump.avg_latency_tsc);
    crate::println!("  Free Memory (Initial):     {:<15} B  {:<15} B", ll.free_before, bump.free_before);
    crate::println!("  Free Memory (100 Blocks):  {:<15} B  {:<15} B", ll.free_after_100_alloc, bump.free_after_100_alloc);
    crate::println!("  Free Memory (50% Freed):   {:<15} B  {:<15} B", ll.free_after_50_free, bump.free_after_50_free);
    crate::println!("  Memory Reclaimed (Reuse):  {:<15} B  {:<15} B", ll.free_after_50_free.saturating_sub(ll.free_after_100_alloc), bump.free_after_50_free.saturating_sub(bump.free_after_100_alloc));
    crate::println!("  Graceful OOM Resilience:   {:<18} {:<18}", if ll.oom_handled_ok { "[OK] (Null)" } else { "[FAIL]" }, if bump.oom_handled_ok { "[OK] (Null)" } else { "[FAIL]" });
    crate::println!("--------------------------------------------------");

    crate::serial_println!(
        "[CA6-ALLOC] Benchmark: LL_avg={} cyc, Bump_avg={} cyc, LL_reused={} B, Bump_reused={} B",
        ll.avg_latency_tsc,
        bump.avg_latency_tsc,
        ll.free_after_50_free.saturating_sub(ll.free_after_100_alloc),
        bump.free_after_50_free.saturating_sub(bump.free_after_100_alloc)
    );
}
