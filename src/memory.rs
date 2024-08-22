use x86_64::structures::paging::{Mapper, Page, PhysFrame, FrameAllocator, OffsetPageTable, PageTable, Size4KiB};
use x86_64::{PhysAddr, VirtAddr};


/// A FrameAllocator that always returns `None`.
pub struct EmptyFrameAllocator;

// allocator must guarantee that allocator yields only unused frames.
unsafe impl FrameAllocator<Size4KiB> for EmptyFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        None
    }
}

/// Initialize a new OffsetPageTable.
/// 
/// This function is unsafe because the caller must guarantee that the 
/// complete physical memory is mapped to virtual memory at the passed 
/// `physical_memory_offset`. Also, this function must be called only once
/// to avoid aliasing `&mut` reference.
pub unsafe fn init(physical_memory_offset: VirtAddr) -> OffsetPageTable<'static> {
    let level_4_table = active_level_4_table(physical_memory_offset);
    // instance stays valid for complete runtime of kernel 
    OffsetPageTable::new(level_4_table, physical_memory_offset)
}

/// returns a mutable reference to the active level 4 table
///
/// this function is unsafe because the caller must guarantee that the 
/// complete physical memory is mapped to the passed `physical_memory_offset`.
/// Also, this function must be only called once to avoid aliasing `&mut` references
unsafe fn active_level_4_table(physical_memory_offset: VirtAddr) -> &'static mut PageTable {
    use x86_64::registers::control::Cr3;

    let (level_4_table_frame, _) = Cr3::read();

    let phys = level_4_table_frame.start_address();
    let virt = physical_memory_offset + phys.as_u64();
    let page_table_ptr: *mut PageTable = virt.as_mut_ptr();

    &mut *page_table_ptr // unsafe
}

// create an example mapping for the given page to frame `0xb8000`.
pub fn create_example_mapping(
    page: Page,
    mapper: &mut OffsetPageTable,
    frame_allocator: &mut impl FrameAllocator<Size4KiB>,
) {
    use x86_64::structures::paging::PageTableFlags as Flags;

    let frame = PhysFrame::containing_address(PhysAddr::new(0xb8000));
    let flags = Flags::PRESENT | Flags::WRITABLE;

    // caller must enure frame is not already in use
    let map_to_result = unsafe {
        mapper.map_to(page, frame, flags, frame_allocator)
    };
    map_to_result.expect("map_to failed").flush();

}


/// Translates the given virtual address to the mapped physical address, or
/// `None` if the address is not mapped.
pub unsafe fn translate_addr(addr: VirtAddr, physical_memory_offset: VirtAddr) -> Option<PhysAddr> {
    translate_addr_inner(addr, physical_memory_offset)
}

fn translate_addr_inner(addr: VirtAddr, physical_memory_offset: VirtAddr) -> Option<PhysAddr> {
    use x86_64::structures::paging::page_table::FrameError;
    use x86_64::registers::control::Cr3;

    // read the active level 4 frames from the CE3 register
    let (level_4_table_frame, _) = Cr3::read();

    let table_indexes = [
        addr.p4_index(), addr.p3_index(), addr.p2_index(), addr.p1_index()
    ];
    let mut frame = level_4_table_frame;

    // traverse the muti-level page table
    for &index in &table_indexes {
        // convert the frame into a page table reference
        let virt = physical_memory_offset + frame.start_address().as_u64();
        let table_ptr: *const PageTable = virt.as_ptr();
        let table = unsafe { &*table_ptr };

        // read the page table entry and updae `frame`
        let entry = &table[index];
        frame = match entry.frame() {
            Ok(frame) => frame,
            Err(FrameError::FrameNotPresent) => return None,
            Err(FrameError::HugeFrame) => panic!("huge pages not supported"),
        };
    };

    // calculate the phyiscal address by adding the page offset
    Some(frame.start_address() + u64::from(addr.page_offset()))
}