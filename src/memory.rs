use bootloader_api::info::{MemoryRegionKind, MemoryRegions};
use core::sync::atomic::{AtomicU64, Ordering};
use x86_64::{
    registers::control::Cr3,
    structures::paging::{
        FrameAllocator, OffsetPageTable, PageTable, PhysFrame, Size4KiB,
    },
    PhysAddr, VirtAddr,
};

pub static TOTAL_RAM_BYTES: AtomicU64 = AtomicU64::new(0);
pub static USABLE_RAM_BYTES: AtomicU64 = AtomicU64::new(0);

/// Query system physical memory metrics: (total_bytes, usable_bytes).
pub fn system_ram_metrics() -> (u64, u64) {
    (
        TOTAL_RAM_BYTES.load(Ordering::Relaxed),
        USABLE_RAM_BYTES.load(Ordering::Relaxed),
    )
}

/// Initialize a new OffsetPageTable given the physical memory offset provided by the bootloader.
///
/// # Safety
/// The caller must guarantee that the complete physical memory is mapped to virtual
/// memory at the passed `physical_memory_offset`.
pub unsafe fn init(physical_memory_offset: VirtAddr) -> OffsetPageTable<'static> {
    let level_4_table = active_level_4_table(physical_memory_offset);
    OffsetPageTable::new(level_4_table, physical_memory_offset)
}

/// Returns a mutable reference to the active level 4 page table.
unsafe fn active_level_4_table(physical_memory_offset: VirtAddr) -> &'static mut PageTable {
    let (level_4_table_frame, _) = Cr3::read();
    let phys = level_4_table_frame.start_address();
    let virt = physical_memory_offset + phys.as_u64();
    let page_table_ptr: *mut PageTable = virt.as_mut_ptr();
    &mut *page_table_ptr
}

/// Source of physical memory regions: UEFI or legacy BIOS.
#[derive(Clone, Copy)]
pub enum FrameSource {
    Uefi(&'static MemoryRegions),
    Bios(&'static bootloader::bootinfo::MemoryMap),
}

/// Trait converting various bootloader memory maps into a uniform `FrameSource`.
pub trait IntoFrameSource {
    fn into_source(self) -> FrameSource;
}

impl IntoFrameSource for &'static MemoryRegions {
    fn into_source(self) -> FrameSource {
        FrameSource::Uefi(self)
    }
}

impl IntoFrameSource for &'static bootloader::bootinfo::MemoryMap {
    fn into_source(self) -> FrameSource {
        FrameSource::Bios(self)
    }
}

/// A FrameAllocator that returns usable frames from either UEFI MemoryRegions or BIOS MemoryMap.
pub struct BootInfoFrameAllocator {
    source: FrameSource,
    next: usize,
}

impl BootInfoFrameAllocator {
    /// Create a FrameAllocator from the passed memory map or regions.
    ///
    /// # Safety
    /// The caller must guarantee that the passed memory map is valid. All frames
    /// marked as `Usable` must really be unused.
    pub unsafe fn init<T: IntoFrameSource>(map: T) -> Self {
        let source = map.into_source();
        match source {
            FrameSource::Uefi(memory_regions) => {
                let mut total = 0u64;
                let mut usable = 0u64;
                for r in memory_regions.iter() {
                    let sz = r.end.saturating_sub(r.start);
                    total += sz;
                    if r.kind == MemoryRegionKind::Usable {
                        usable += sz;
                    }
                }
                TOTAL_RAM_BYTES.store(total, Ordering::SeqCst);
                USABLE_RAM_BYTES.store(usable, Ordering::SeqCst);
            }
            FrameSource::Bios(memory_map) => {
                let mut total = 0u64;
                let mut usable = 0u64;
                for r in memory_map.iter() {
                    let sz = r.range.end_addr().saturating_sub(r.range.start_addr());
                    total += sz;
                    if r.region_type == bootloader::bootinfo::MemoryRegionType::Usable {
                        usable += sz;
                    }
                }
                TOTAL_RAM_BYTES.store(total, Ordering::SeqCst);
                USABLE_RAM_BYTES.store(usable, Ordering::SeqCst);
            }
        }

        BootInfoFrameAllocator {
            source,
            next: 0,
        }
    }
}

unsafe impl FrameAllocator<Size4KiB> for BootInfoFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        let frame = match self.source {
            FrameSource::Uefi(regions) => {
                let usable_regions = regions.iter().filter(|r| r.kind == MemoryRegionKind::Usable);
                let addr_ranges = usable_regions.map(|r| r.start..r.end);
                let frame_addresses =
                    addr_ranges.flat_map(|r| (r.start as usize..r.end as usize).step_by(4096));
                frame_addresses
                    .map(|addr| PhysFrame::containing_address(PhysAddr::new(addr as u64)))
                    .nth(self.next)
            }
            FrameSource::Bios(map) => {
                let usable_regions = map
                    .iter()
                    .filter(|r| r.region_type == bootloader::bootinfo::MemoryRegionType::Usable);
                let addr_ranges = usable_regions.map(|r| r.range.start_addr()..r.range.end_addr());
                let frame_addresses =
                    addr_ranges.flat_map(|r| (r.start as usize..r.end as usize).step_by(4096));
                frame_addresses
                    .map(|addr| PhysFrame::containing_address(PhysAddr::new(addr as u64)))
                    .nth(self.next)
            }
        };
        self.next += 1;
        frame
    }
}
