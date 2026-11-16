#![no_std]
#![no_main]

extern crate alloc;

use bootloader::{entry_point, BootInfo};
use core::panic::PanicInfo;
use micro_os::diagnostics::{self, LogLevel};
use micro_os::{exit_qemu, serial_print, serial_println, QemuExitCode};

entry_point!(main);

fn main(_boot_info: &'static BootInfo) -> ! {
    serial_println!("==================================================");
    serial_println!("   CA-6 Layer 2 Diagnostics & Trace Test Suite");
    serial_println!("==================================================");

    // 1. Initialize CPU & Interrupts
    micro_os::init();

    // 2. Test Log Level Representations
    serial_print!("diag::1_log_levels...\t");
    assert_eq!(LogLevel::Debug.as_str(), "DEBUG");
    assert_eq!(LogLevel::Info.as_str(), "INFO ");
    assert_eq!(LogLevel::Warn.as_str(), "WARN ");
    assert_eq!(LogLevel::Error.as_str(), "ERROR");
    serial_println!("[ok]");

    // 3. Test Diagnostic Macros
    serial_print!("diag::2_log_dispatch_macros...\t");
    micro_os::kinfo!("System boot diagnostics test message: info");
    micro_os::kwarn!("System resource warning threshold test message: warn");
    micro_os::kerror!("Controlled error diagnostic test message: error");
    micro_os::kdebug!("Low-level register trace test message: debug");
    serial_println!("[ok]");

    // 4. Test Machine-Parseable Tracing Events
    serial_print!("diag::3_structured_machine_traces...\t");
    diagnostics::trace_switch(1, 2, "quantum");
    diagnostics::trace_task_state(2, "RUNNING");
    diagnostics::trace_alloc(128, true);
    diagnostics::trace_lock_wait(2, "SPINLOCK_0", 3);
    diagnostics::trace_lock_acq(2, "SPINLOCK_0");
    diagnostics::trace_lock_rel(2, "SPINLOCK_0");
    diagnostics::trace_deadlock_prevent(2, 10, 20);
    diagnostics::trace_dealloc(128);
    serial_println!("[ok]");

    // 5. Test In-Kernel Trace Ring Buffer (Zero-Allocation Inspection)
    serial_print!("diag::4_trace_ring_buffer...\t");
    let mut entries = [diagnostics::TraceRecord::default(); 16];
    let count = diagnostics::TRACE_BUFFER.lock().copy_to_slice(&mut entries);
    assert!(count >= 8, "Expected at least 8 events in ring buffer");
    let total = diagnostics::TRACE_BUFFER.lock().total();
    assert!(total >= 8, "Total events written should be >= 8");

    // Test FIFO wrap-around on a dedicated TraceRingBuffer instance
    let mut rb = diagnostics::TraceRingBuffer::new();
    for i in 0..(diagnostics::TRACE_BUFFER_CAPACITY + 10) {
        rb.push(diagnostics::TraceRecord {
            tick: i as u64,
            tsc: i as u64 * 100,
            kind: diagnostics::TraceKind::Alloc,
            task_id: i as u64,
            param1: 64,
            param2: 1,
        });
    }
    assert_eq!(rb.count(), diagnostics::TRACE_BUFFER_CAPACITY);
    assert_eq!(rb.total(), diagnostics::TRACE_BUFFER_CAPACITY + 10);
    let mut wrap_entries = [diagnostics::TraceRecord::default(); diagnostics::TRACE_BUFFER_CAPACITY];
    let wrap_count = rb.copy_to_slice(&mut wrap_entries);
    assert_eq!(wrap_count, diagnostics::TRACE_BUFFER_CAPACITY);
    assert_eq!(wrap_entries[0].task_id, 10);
    serial_println!("[ok]");

    serial_println!("==================================================");
    serial_println!("CA-6 Diagnostics & Tracing Test: ALL PASSED");
    serial_println!("==================================================");

    exit_qemu(QemuExitCode::Success);
    micro_os::hlt_loop();
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    micro_os::test_panic_handler(info)
}
