#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(micro_os::test_runner)]
#![reexport_test_harness_main = "test_main"]

extern crate alloc;

use bootloader_api::config::{BootloaderConfig, Mapping};
use core::panic::PanicInfo;
use micro_os::println;
#[cfg(not(test))]
use micro_os::task::executor::Executor;
#[cfg(not(test))]
use micro_os::task::Task;
use x86_64::VirtAddr;

pub static BOOTLOADER_CONFIG: BootloaderConfig = {
    let mut config = BootloaderConfig::new_default();
    config.mappings.physical_memory = Some(Mapping::Dynamic);
    config
};

bootloader_api::entry_point!(kernel_main, config = &BOOTLOADER_CONFIG);

fn kernel_main(boot_info: &'static mut bootloader_api::BootInfo) -> ! {
    // 1. Read TSC as early as possible
    let tsc_start: u64 = unsafe { core::arch::x86_64::_rdtsc() };

    // 2. Initialize UEFI Graphical Framebuffer Console
    if let Some(framebuffer) = boot_info.framebuffer.as_mut() {
        micro_os::framebuffer::init_framebuffer(framebuffer);
    }

    println!("==================================================");
    println!("   VenOS v0.1 - x86_64 Hobby Operating System");
    println!("   UEFI 64-bit Bootloader | GOP Framebuffer Console");
    println!("==================================================");
    println!("Kernel booted successfully via UEFI!");
    micro_os::serial_println!("Serial port online.");

    // 3. Initialize CPU structures (GDT, TSS, IDT, PIC, Interrupts enabled)
    micro_os::init();

    // 4. Initialize Memory Management (Paging, Frame Allocator, Kernel Heap)
    let phys_mem_offset_val = boot_info
        .physical_memory_offset
        .into_option()
        .expect("Physical memory offset missing in BootInfo");
    let phys_mem_offset = VirtAddr::new(phys_mem_offset_val);
    let mut mapper = unsafe { micro_os::memory::init(phys_mem_offset) };
    let mut frame_allocator =
        unsafe { micro_os::memory::BootInfoFrameAllocator::init(&boot_info.memory_regions) };

    micro_os::allocator::init_heap(&mut mapper, &mut frame_allocator)
        .expect("Kernel heap initialization failed");

    #[cfg(not(feature = "ca6-core"))]
    {
        // 5. Initialize ACPI Subsystem
        if let Some(rsdp_addr) = boot_info.rsdp_addr.into_option() {
            micro_os::serial_println!("UEFI: ACPI RSDP table provided at physical {:#x}", rsdp_addr);
        }
        micro_os::acpi::init_acpi(phys_mem_offset);

        // 6. Initialize Storage & File System (ATA probe, NVMe probe, MicroFS mount)
        micro_os::init_storage();
        micro_os::storage::nvme::init_nvme(&mut mapper, &mut frame_allocator);

        // 7. Initialize LAN & 802.11 Wireless Networking (e1000 driver & smoltcp stack)
        micro_os::net::init_network(&mut mapper, &mut frame_allocator);

        // 8. Initialize Package Management Subsystem (apt / dpkg)
        micro_os::pkg::init_pkg();

        // 9. Initialize Snap Store Subsystem
        micro_os::snap::init_snap();
    }

    #[cfg(feature = "ca6-core")]
    {
        micro_os::println!("==================================================");
        micro_os::println!("   VenOS - CA-6 Core Architecture Mode Active");
        micro_os::println!("==================================================");
        micro_os::scheduler::run_ca6_demo();
    }

    // 10. Measure elapsed TSC ticks up to shell prompt readiness
    let tsc_end: u64 = unsafe { core::arch::x86_64::_rdtsc() };
    let elapsed_ticks = tsc_end.wrapping_sub(tsc_start);

    // 11. Calibrate TSC frequency against PIT Channel 2
    let tsc_freq_hz = micro_os::interrupts::calibrate_tsc();
    let boot_time_ms = elapsed_ticks.saturating_mul(1000) / tsc_freq_hz;
    let boot_time_us = (elapsed_ticks.saturating_mul(1_000_000) / tsc_freq_hz) % 1000;
    let freq_mhz = tsc_freq_hz / 1_000_000;

    micro_os::shell::set_boot_time(boot_time_ms, boot_time_us, freq_mhz);

    micro_os::serial_println!(
        "BOOT_TIME_MS={}.{:03} (TSC ticks={}, freq={} MHz)",
        boot_time_ms,
        boot_time_us,
        elapsed_ticks,
        freq_mhz
    );

    println!(
        "Boot time to shell: {}.{:03} ms (calibrated @ {} MHz)",
        boot_time_ms,
        boot_time_us,
        freq_mhz
    );
    let now = micro_os::rtc::read_rtc();
    println!("System Clock: {}", now);
    println!("Type 'help' for available commands.");
    println!();

    #[cfg(test)]
    test_main();

    #[cfg(not(test))]
    {
        let mut executor = Executor::new();
        executor.spawn(Task::new(micro_os::shell::run_shell()));
        executor.run();
    }

    #[cfg(test)]
    micro_os::hlt_loop();
}

#[cfg(not(test))]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("KERNEL PANIC: {}", info);
    micro_os::serial_println!("KERNEL PANIC: {}", info);
    micro_os::hlt_loop();
}

#[cfg(test)]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    micro_os::test_panic_handler(info)
}
