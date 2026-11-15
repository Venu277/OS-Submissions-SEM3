#![no_std]
#![no_main]

extern crate alloc;

use bootloader::{entry_point, BootInfo};
use core::alloc::Layout;
use core::panic::PanicInfo;
use micro_os::allocator::bump::BumpAllocator;
use micro_os::allocator::linked_list::LinkedListAllocator;
use micro_os::allocator::{benchmark_allocator_comparison, init_heap};
use micro_os::{exit_qemu, memory, serial_print, serial_println, QemuExitCode};
use x86_64::VirtAddr;

entry_point!(main);

fn main(boot_info: &'static BootInfo) -> ! {
    serial_println!("==================================================");
    serial_println!("   CA-6 Layer 8 & Track B: Allocator Benchmark");
    serial_println!("==================================================");

    // 1. Initialize CPU, IDT, Paging & Heap
    micro_os::init();
    let phys_mem_offset = VirtAddr::new(boot_info.physical_memory_offset);
    let mut mapper = unsafe { memory::init(phys_mem_offset) };
    let mut frame_allocator =
        unsafe { memory::BootInfoFrameAllocator::init(&boot_info.memory_map) };
    init_heap(&mut mapper, &mut frame_allocator).expect("heap initialization failed");

    // Test 1: Linked List Allocation, Out-of-Order Deallocation & Memory Reuse
    serial_print!("alloc::1_linked_list_allocation_and_reuse...\t");
    let mut buffer_ll = [0u8; 16 * 1024];
    let base_ll = buffer_ll.as_mut_ptr() as usize;
    let mut ll = LinkedListAllocator::new();
    unsafe {
        ll.init(base_ll, buffer_ll.len());
    }
    let initial_free = ll.free_memory();

    let layout_64 = Layout::from_size_align(64, 8).unwrap();
    let layout_128 = Layout::from_size_align(128, 8).unwrap();

    let p1 = ll.alloc(layout_64);
    let p2 = ll.alloc(layout_128);
    let p3 = ll.alloc(layout_64);
    assert!(!p1.is_null());
    assert!(!p2.is_null());
    assert!(!p3.is_null());
    assert!(ll.free_memory() < initial_free);

    // Free p2 (the middle block)
    unsafe {
        ll.dealloc(p2, layout_128);
    }
    // Re-allocating a block of size 128 should reuse the freed region
    let p4 = ll.alloc(layout_128);
    assert_eq!(p4, p2, "Linked list allocator must immediately reuse freed middle block");

    unsafe {
        ll.dealloc(p1, layout_64);
        ll.dealloc(p3, layout_64);
        ll.dealloc(p4, layout_128);
    }
    serial_println!("[ok]");

    // Test 2: Bump Allocator Sequential Allocation & Total Reset
    serial_print!("alloc::2_bump_allocator_sequential_and_reset...\t");
    let mut buffer_bump = [0u8; 16 * 1024];
    let base_bump = buffer_bump.as_mut_ptr() as usize;
    let mut bump = BumpAllocator::new();
    unsafe {
        bump.init(base_bump, buffer_bump.len());
    }
    assert_eq!(bump.active_allocations(), 0);

    let b1 = bump.alloc(layout_64);
    let b2 = bump.alloc(layout_128);
    assert_eq!(b1 as usize, base_bump);
    assert!(b2 as usize >= (b1 as usize + 64));
    assert_eq!(bump.active_allocations(), 2);

    // Freeing b1 cannot reclaim memory because b2 is active
    let free_before_dealloc = bump.free_memory();
    unsafe {
        bump.dealloc(b1, layout_64);
    }
    assert_eq!(bump.free_memory(), free_before_dealloc, "Bump allocator cannot reclaim non-tail block");
    assert_eq!(bump.active_allocations(), 1);

    // Freeing b2 drops allocations to 0 -> trigger full reset
    unsafe {
        bump.dealloc(b2, layout_128);
    }
    assert_eq!(bump.active_allocations(), 0);
    assert_eq!(bump.free_memory(), buffer_bump.len(), "Bump allocator must reset upon all allocations freed");

    let b3 = bump.alloc(layout_64);
    assert_eq!(b3 as usize, base_bump, "After reset, next allocation must start at base");
    serial_println!("[ok]");

    // Test 3: Graceful OOM Handling on Oversized Requests
    serial_print!("alloc::3_graceful_oom_handling...\t");
    let huge_layout = Layout::from_size_align(1024 * 1024, 8).unwrap(); // 1 MiB on 16 KiB heap
    let oom_ll = ll.alloc(huge_layout);
    assert!(oom_ll.is_null(), "Linked list allocator must return null on OOM without panicking");

    let oom_bump = bump.alloc(huge_layout);
    assert!(oom_bump.is_null(), "Bump allocator must return null on OOM without panicking");
    serial_println!("[ok]");

    // Test 4: Empirical Benchmark (100 Blocks, Latency, Deallocation & Fragmentation)
    serial_print!("alloc::4_benchmark_100_blocks_comparison...\t");
    let (ll_m, bump_m) = benchmark_allocator_comparison();

    assert_eq!(ll_m.total_allocations, 100);
    assert_eq!(bump_m.total_allocations, 100);
    assert!(ll_m.min_latency_tsc > 0);
    assert!(bump_m.min_latency_tsc > 0);
    assert!(ll_m.avg_latency_tsc > 0);
    assert!(bump_m.avg_latency_tsc > 0);

    // Linked list must successfully reclaim memory after 50% freed
    assert!(
        ll_m.free_after_50_free > ll_m.free_after_100_alloc,
        "Linked list must reclaim freed memory"
    );

    // Bump allocator cannot reclaim non-tail freed blocks
    assert_eq!(
        bump_m.free_after_50_free, bump_m.free_after_100_alloc,
        "Bump allocator cannot reclaim non-tail memory"
    );

    assert!(ll_m.oom_handled_ok, "Linked list OOM must be handled gracefully");
    assert!(bump_m.oom_handled_ok, "Bump allocator OOM must be handled gracefully");

    serial_println!(
        "[ok] (LL_avg={} cyc, Bump_avg={} cyc, LL_reused={} B, Bump_reused=0 B)",
        ll_m.avg_latency_tsc,
        bump_m.avg_latency_tsc,
        ll_m.free_after_50_free - ll_m.free_after_100_alloc
    );

    serial_println!("==================================================");
    serial_println!("CA-6 Layer 8 Allocator Benchmark: ALL PASSED");
    serial_println!("==================================================");

    exit_qemu(QemuExitCode::Success);
    micro_os::hlt_loop();
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    micro_os::test_panic_handler(info)
}
