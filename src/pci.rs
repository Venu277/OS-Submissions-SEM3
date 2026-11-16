use alloc::vec::Vec;
use x86_64::instructions::port::Port;

const CONFIG_ADDRESS: u16 = 0xCF8;
const CONFIG_DATA: u16 = 0xCFC;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PciDevice {
    pub bus: u8,
    pub device: u8,
    pub function: u8,
    pub vendor_id: u16,
    pub device_id: u16,
    pub class_id: u8,
    pub subclass_id: u8,
    pub prog_if: u8,
}

impl PciDevice {
    pub fn class_name(&self) -> &'static str {
        match self.class_id {
            0x00 => "Unclassified",
            0x01 => match self.subclass_id {
                0x01 => "IDE Controller",
                0x06 => "SATA / AHCI Controller",
                0x08 => "NVMe Controller",
                _ => "Mass Storage Controller",
            },
            0x02 => match self.subclass_id {
                0x00 => "Ethernet Controller",
                0x80 => "Network Controller",
                _ => "Network Controller",
            },
            0x03 => "Display Controller",
            0x06 => "Bridge Device (Host/PCI/ISA)",
            0x0C => match self.subclass_id {
                0x03 => "USB Controller",
                _ => "Serial Bus Controller",
            },
            _ => "Other Peripheral",
        }
    }

    pub fn vendor_name(&self) -> &'static str {
        match self.vendor_id {
            0x8086 => "Intel Corporation",
            0x14C3 => "MediaTek Inc.",
            0x15B7 => "Western Digital",
            0x10DE => "NVIDIA Corporation",
            0x1002 => "AMD / ATI",
            0x10EC => "Realtek Semiconductor",
            0x1234 => "QEMU / Bochs Virtual Device",
            0x1B36 => "Red Hat, Inc. (QEMU)",
            0x1AF4 => "Red Hat Virtio",
            _ => "Unknown Vendor",
        }
    }

    pub fn device_name(&self) -> &'static str {
        match (self.vendor_id, self.device_id) {
            (0x8086, 0x100E) => "82540EM Gigabit Ethernet Controller (QEMU)",
            (0x8086, 0x1004) => "82543GC Gigabit Ethernet Controller",
            (0x8086, 0x100F) => "82545EM Gigabit Ethernet Controller",
            (0x8086, 0x1237) => "440FX - 82441FX PMC [Natoma]",
            (0x8086, 0x7000) => "82371SB PIIX3 ISA [Natoma/Triton II]",
            (0x8086, 0x7010) => "82371SB PIIX3 IDE [Natoma/Triton II]",
            (0x8086, 0x7113) => "82371AB/EB/MB PIIX4 ACPI",
            (0x8086, 0x2918) => "LPC Interface Controller",
            (0x8086, 0x29C0) => "82G33/G31/P35/P31 Express DRAM Controller",
            (0x8086, 0xA7A0)
            | (0x8086, 0xA7A1)
            | (0x8086, 0x46A8)
            | (0x8086, 0x46A6)
            | (0x8086, 0x4690)
            | (0x8086, 0x4680) => "Iris Xe Graphics (Raptor Lake / Alder Lake-U)",
            (0x8086, 0x51ED) | (0x8086, 0x464F) | (0x8086, 0x7A60) => {
                "Alder Lake PCH USB 3.2 xHCI Host Controller"
            }
            (0x14C3, 0x7961) | (0x14C3, 0x7921) | (0x14C3, 0x7922) | (0x14C3, 0x0608) => {
                "MT7921 / MT7922 Wi-Fi 6 802.11ax PCIe Adapter"
            }
            (0x15B7, 0x5041) | (0x15B7, 0x5040) | (0x15B7, 0x5017) | (0x15B7, 0x5030) => {
                "WD Blue SN5000 / SN580 NVMe SSD"
            }
            (0x1234, 0x1111) => "Bochs / QEMU Standard VGA Display Adapter",
            (0x1B36, 0x0010) => "QEMU PCIe Root Port",
            _ => self.class_name(),
        }
    }
}

pub fn pci_read_u32(bus: u8, device: u8, function: u8, offset: u8) -> u32 {
    let address = ((bus as u32) << 16)
        | ((device as u32) << 11)
        | ((function as u32) << 8)
        | ((offset as u32) & 0xFC)
        | 0x8000_0000;

    unsafe {
        let mut addr_port: Port<u32> = Port::new(CONFIG_ADDRESS);
        let mut data_port: Port<u32> = Port::new(CONFIG_DATA);
        addr_port.write(address);
        data_port.read()
    }
}

pub fn pci_write_u32(bus: u8, device: u8, function: u8, offset: u8, value: u32) {
    let address = ((bus as u32) << 16)
        | ((device as u32) << 11)
        | ((function as u32) << 8)
        | ((offset as u32) & 0xFC)
        | 0x8000_0000;

    unsafe {
        let mut addr_port: Port<u32> = Port::new(CONFIG_ADDRESS);
        let mut data_port: Port<u32> = Port::new(CONFIG_DATA);
        addr_port.write(address);
        data_port.write(value);
    }
}

pub fn enumerate_pci() -> Vec<PciDevice> {
    let mut devices = Vec::new();

    for bus in 0..=31 {
        for device in 0..32 {
            let vendor_and_device = pci_read_u32(bus, device, 0, 0x00);
            let vendor_id = (vendor_and_device & 0xFFFF) as u16;
            if vendor_id == 0xFFFF || vendor_id == 0x0000 {
                continue;
            }

            let header_type_reg = pci_read_u32(bus, device, 0, 0x0C);
            let is_multi_func = ((header_type_reg >> 16) & 0x80) != 0;
            let max_func = if is_multi_func { 8 } else { 1 };

            for function in 0..max_func {
                let v_and_d = pci_read_u32(bus, device, function, 0x00);
                let v_id = (v_and_d & 0xFFFF) as u16;
                if v_id == 0xFFFF || v_id == 0x0000 {
                    continue;
                }

                let device_id = ((v_and_d >> 16) & 0xFFFF) as u16;
                let class_rev = pci_read_u32(bus, device, function, 0x08);
                let class_id = ((class_rev >> 24) & 0xFF) as u8;
                let subclass_id = ((class_rev >> 16) & 0xFF) as u8;
                let prog_if = ((class_rev >> 8) & 0xFF) as u8;

                devices.push(PciDevice {
                    bus,
                    device,
                    function,
                    vendor_id: v_id,
                    device_id,
                    class_id,
                    subclass_id,
                    prog_if,
                });
            }
        }
    }

    devices
}

impl PciDevice {
    pub fn read_bar(&self, bar_idx: u8) -> u32 {
        if bar_idx > 5 {
            return 0;
        }
        pci_read_u32(self.bus, self.device, self.function, 0x10 + bar_idx * 4)
    }

    pub fn enable_bus_mastering(&self) {
        let cmd = pci_read_u32(self.bus, self.device, self.function, 0x04);
        // Bit 0: I/O Space, Bit 1: Memory Space, Bit 2: Bus Master
        pci_write_u32(self.bus, self.device, self.function, 0x04, cmd | 0x07);
    }
}

pub fn find_e1000() -> Option<PciDevice> {
    enumerate_pci().into_iter().find(|dev| {
        dev.vendor_id == 0x8086
            && matches!(
                dev.device_id,
                0x100E // 82540EM (QEMU default)
                    | 0x1004 // 82543GC
                    | 0x100F // 82545EM (VMware Workstation default)
                    | 0x1079 // 82546GB
                    | 0x107C // 82541PI
            )
    })
}

pub fn find_nvme() -> Option<PciDevice> {
    enumerate_pci().into_iter().find(|dev| {
        dev.class_id == 0x01 && dev.subclass_id == 0x08 && dev.prog_if == 0x02
    })
}
