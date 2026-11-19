#![no_std]
#![no_main]

extern crate alloc;

use bootloader::{entry_point, BootInfo};
use core::panic::PanicInfo;
use micro_os::allocator::{calculate_adaptive_heap_size, MAX_HEAP_SIZE, MIN_HEAP_SIZE};
use micro_os::framebuffer::{calculate_aspect_ratio, AspectRatio};
use micro_os::pci::PciDevice;
use micro_os::{exit_qemu, serial_print, serial_println, QemuExitCode};

entry_point!(main);

fn main(_boot_info: &'static BootInfo) -> ! {
    serial_print!("hardware_adaptation::aspect_ratio_classification...\t");
    test_aspect_ratio();
    serial_println!("[ok]");

    serial_print!("hardware_adaptation::adaptive_heap_calculation...\t");
    test_adaptive_heap();
    serial_println!("[ok]");

    serial_print!("hardware_adaptation::pci_friendly_names...\t");
    test_pci_names();
    serial_println!("[ok]");

    exit_qemu(QemuExitCode::Success);
    micro_os::hlt_loop();
}

fn test_aspect_ratio() {
    // 16:10 Laptop resolution (Intel Iris Xe 1920x1200)
    assert_eq!(calculate_aspect_ratio(1920, 1200), AspectRatio::Ratio16_10);
    assert_eq!(calculate_aspect_ratio(2560, 1600), AspectRatio::Ratio16_10);

    // 16:9 Standard widescreen (1920x1080, 1280x720, 1366x768)
    assert_eq!(calculate_aspect_ratio(1920, 1080), AspectRatio::Ratio16_9);
    assert_eq!(calculate_aspect_ratio(1280, 720), AspectRatio::Ratio16_9);
    assert_eq!(calculate_aspect_ratio(1366, 768), AspectRatio::Ratio16_9);

    // 4:3 Legacy display
    assert_eq!(calculate_aspect_ratio(1024, 768), AspectRatio::Ratio4_3);
    assert_eq!(calculate_aspect_ratio(800, 600), AspectRatio::Ratio4_3);

    // 5:4 Standard monitor
    assert_eq!(calculate_aspect_ratio(1280, 1024), AspectRatio::Ratio5_4);

    // 3:2 Tablet / Surface / Modern laptop
    assert_eq!(calculate_aspect_ratio(2256, 1504), AspectRatio::Ratio3_2);
    assert_eq!(calculate_aspect_ratio(3000, 2000), AspectRatio::Ratio3_2);
}

fn test_adaptive_heap() {
    // Zero RAM edge case: must return minimum heap
    assert_eq!(calculate_adaptive_heap_size(0), MIN_HEAP_SIZE);

    // Small RAM (16 MiB): 16 MiB / 64 = 256 KiB -> matches minimum clamp
    let ram_16mib = 16 * 1024 * 1024;
    assert_eq!(calculate_adaptive_heap_size(ram_16mib), MIN_HEAP_SIZE);

    // Very low RAM (4 MiB): 4 MiB / 64 = 64 KiB -> clamped up to MIN_HEAP_SIZE (256 KiB)
    let ram_4mib = 4 * 1024 * 1024;
    assert_eq!(calculate_adaptive_heap_size(ram_4mib), MIN_HEAP_SIZE);

    // 64 MiB RAM: 64 MiB / 64 = 1 MiB
    let ram_64mib = 64 * 1024 * 1024;
    assert_eq!(calculate_adaptive_heap_size(ram_64mib), 1024 * 1024);

    // 128 MiB RAM: 128 MiB / 64 = 2 MiB
    let ram_128mib = 128 * 1024 * 1024;
    assert_eq!(calculate_adaptive_heap_size(ram_128mib), 2 * 1024 * 1024);

    // 256 MiB RAM: 256 MiB / 64 = 4 MiB -> reaches maximum clamp
    let ram_256mib = 256 * 1024 * 1024;
    assert_eq!(calculate_adaptive_heap_size(ram_256mib), MAX_HEAP_SIZE);

    // 16 GiB RAM (real laptop): 16 GiB / 64 = 256 MiB -> clamped down to MAX_HEAP_SIZE (4 MiB)
    let ram_16gib = 16u64 * 1024 * 1024 * 1024;
    assert_eq!(calculate_adaptive_heap_size(ram_16gib), MAX_HEAP_SIZE);
}

fn test_pci_names() {
    // Intel Iris Xe GPU
    let dev_gpu = PciDevice {
        bus: 0,
        device: 2,
        function: 0,
        vendor_id: 0x8086,
        device_id: 0xA7A0,
        class_id: 0x03,
        subclass_id: 0x00,
        prog_if: 0x00,
    };
    assert_eq!(dev_gpu.vendor_name(), "Intel Corporation");
    assert!(dev_gpu.device_name().contains("Iris Xe Graphics"));

    // MediaTek MT7921 WiFi
    let dev_wifi = PciDevice {
        bus: 1,
        device: 0,
        function: 0,
        vendor_id: 0x14C3,
        device_id: 0x7961,
        class_id: 0x02,
        subclass_id: 0x80,
        prog_if: 0x00,
    };
    assert_eq!(dev_wifi.vendor_name(), "MediaTek Inc.");
    assert!(dev_wifi.device_name().contains("MT7921"));

    // Intel 82540EM Ethernet (QEMU default)
    let dev_nic = PciDevice {
        bus: 0,
        device: 3,
        function: 0,
        vendor_id: 0x8086,
        device_id: 0x100E,
        class_id: 0x02,
        subclass_id: 0x00,
        prog_if: 0x00,
    };
    assert_eq!(dev_nic.vendor_name(), "Intel Corporation");
    assert!(dev_nic.device_name().contains("82540EM"));
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    micro_os::test_panic_handler(info)
}
