use core::ffi::c_void;

use ::r_efi::{
    protocols::file::ProtocolOpen,
    protocols::*,
    system::MemoryDescriptor,
    system::{RuntimeSetVariable, TPL_HIGH_LEVEL},
    *,
};
use alloc::string::String;
use r_efi::system::{
    ALLOCATE_ADDRESS, ALLOCATE_ANY_PAGES, ALLOCATE_MAX_ADDRESS, CONVENTIONAL_MEMORY, LOADER_DATA,
    RUNTIME_SERVICES_DATA,
};
use x86_64::{
    structures::paging::{self, OffsetPageTable, Page},
    structures::paging::{
        mapper::Mapper,
        page::{PageSize, Size1GiB, Size2MiB, Size4KiB},
        page_table::{PageTable, PageTableFlags},
        FrameAllocator, PhysFrame, Translate,
    },
    PhysAddr, VirtAddr,
};

use crate::{boot_services, mem_maps::EfiMemMaps};

pub struct IdentityPageTable {
    page_table: PageTable, // TODO: align
    allocator: StaticFrameAllocator<1024>,
}

impl IdentityPageTable {
    pub const fn new() -> Self {
        Self {
            page_table: PageTable::new(),
            allocator: StaticFrameAllocator::new(),
        }
    }

    pub fn create_identity_mapping(&mut self, mem_maps: &EfiMemMaps) -> Result<(), String> {
        let mut pt_mapper = unsafe { OffsetPageTable::new(&mut self.page_table, VirtAddr::new(0)) };
        //let mut pt_allocator = IdentityPageTableAllocator {};

        // loop through each page in each mapping and create new entries
        for mem_map in mem_maps.iter() {
            //.filter(|m| m.r#type == 7 /* TODO: */) {
            let mut bytes_offset = 0;
            let mut bytes_left = mem_map.number_of_pages * Size4KiB::SIZE;

            let flags = PageTableFlags::PRESENT
                | PageTableFlags::WRITABLE
                | PageTableFlags::USER_ACCESSIBLE; // TODO: HUGE_PAGE ?

            info!("bytes_left: {}", bytes_left);
            while bytes_left > 0 {
                /*if bytes_left >= Size1GiB::SIZE {
                    match unsafe {
                        pt_mapper.map_to(
                            Page::<Size1GiB>::from_start_address_unchecked(VirtAddr::new(
                                mem_map.physical_start + bytes_offset,
                            )),
                            PhysFrame::from_start_address_unchecked(PhysAddr::new(
                                mem_map.physical_start + bytes_offset,
                            )),
                            flags | PageTableFlags::HUGE_PAGE,
                            &mut self.allocator,
                        )
                    } {
                        Ok(_) => debug!(
                            "1gib page table entry for address {:x} created",
                            mem_map.physical_start + bytes_offset
                        ),
                        Err(err) => {
                            error!("could not add 1gib page_table entry, {:?}", err);
                        }
                    }
                    bytes_offset += Size1GiB::SIZE;
                    bytes_left -= Size1GiB::SIZE;
                } else if bytes_left >= Size2MiB::SIZE {
                    match unsafe {
                        pt_mapper.map_to(
                            Page::<Size2MiB>::from_start_address_unchecked(VirtAddr::new(
                                mem_map.physical_start + bytes_offset,
                            )),
                            PhysFrame::from_start_address_unchecked(PhysAddr::new(
                                mem_map.physical_start + bytes_offset,
                            )),
                            flags | PageTableFlags::HUGE_PAGE,
                            &mut self.allocator,
                        )
                    } {
                        Ok(_) => debug!(
                            "2mib page table entry for address {:x} created",
                            mem_map.physical_start + bytes_offset
                        ),
                        Err(err) => {
                            error!("could not add 2mib page_table entry, {:?}", err);
                        }
                    }
                    bytes_offset += Size2MiB::SIZE;
                    bytes_left -= Size2MiB::SIZE;
                } else*/ {
                    match unsafe {
                        pt_mapper.map_to(
                            Page::<Size4KiB>::from_start_address_unchecked(VirtAddr::new(
                                mem_map.physical_start + bytes_offset,
                            )),
                            PhysFrame::from_start_address_unchecked(PhysAddr::new(
                                mem_map.physical_start + bytes_offset,
                            )),
                            flags,
                            &mut self.allocator,
                        )
                    } {
                        Ok(_) => { /*debug!(
                                 "4kib page table entry for address {:x} created",
                                 mem_map.physical_start + bytes_offset
                             )*/
                        }
                        Err(err) => {
                            error!("could not add 4kib page_table entry, {:?}", err);
                        }
                    }

                    bytes_offset += Size4KiB::SIZE;
                    bytes_left -= Size4KiB::SIZE;
                };
            }

            /*
            for page_num in 0..mem_map.number_of_pages {
                // TODO: handle bigger pages here as well if it fits
                unsafe {
                    match pt_mapper.map_to(
                        Page::<Size4KiB>::from_start_address_unchecked(VirtAddr::new(
                            mem_map.physical_start + page_num * 0x1000,
                        )),
                        PhysFrame::from_start_address_unchecked(PhysAddr::new(
                            mem_map.physical_start + page_num * 0x1000,
                        )),
                        flags,
                        &mut self.allocator,
                    ) {
                        Ok(_) =>
                        /*debug!(
                            "page table entry for address {} created",
                            mem_map.physical_start + page_num * 0x1000
                        )*/
                        {
                            ()
                        }
                        Err(err) => {
                            error!("could not add page_table entry, {:?}", err)
                        }
                    }
                }
            }
                */
        }

        Ok(())
    }

    pub fn dtb(&self) -> PhysFrame {
        let addr = &self.page_table as *const _ as *const c_void as u64;
        unsafe { PhysFrame::from_start_address_unchecked(PhysAddr::new(addr)) }
    }
}

pub struct StaticFrameAllocator<const N: usize> {
    frames: [[u8; 0x1000]; N],
    used_frames: usize,
}

impl<const N: usize> StaticFrameAllocator<N> {
    pub const fn new() -> Self {
        Self {
            frames: [[0u8; 0x1000]; N],
            used_frames: 0,
        }
    }
}

unsafe impl<const N: usize> FrameAllocator<Size4KiB> for StaticFrameAllocator<N> {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        if self.used_frames < self.frames.len() {
            let addr = self.frames[self.used_frames].as_ptr() as u64;
            let aligned_addr = (addr + Size4KiB::SIZE) & (addr + Size4KiB::SIZE) % Size4KiB::SIZE;
            self.used_frames += 1;
            PhysFrame::from_start_address(PhysAddr::new(aligned_addr)).ok()
        } else {
            None
        }
    }
}

// TODO: create dynamic alloc
#[derive(Default)]
pub struct IdentityPageTableAllocator;

unsafe impl FrameAllocator<Size4KiB> for IdentityPageTableAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        /*
        PRESENT | WRITABLE | USER_ACCESSIBLE
        if present in the PageTableFlags.
        Depending on the used mapper implementation
        the PRESENT and WRITABLE flags might be set for parent tables,
        even if they are not set in PageTableFlags.
        */
        /*
        let mut allocated_frame = 0u64;
        let status = (boot_services().allocate_pages)(
            ALLOCATE_ANY_PAGES,    // TODO: ?
            CONVENTIONAL_MEMORY, // TODO: ? BOOT_SERVICES_DATA
            Size4KiB::SIZE as usize,
            &mut allocated_frame as *mut _,
        );
        */
        let mut allocated_frame = 0u64;
        let status = (boot_services().allocate_pages)(
            ALLOCATE_ADDRESS,
            LOADER_DATA,
            Size4KiB::SIZE as usize,
            &mut allocated_frame as *mut _,
        );
        if status == efi::Status::SUCCESS {
            // TODO: not page aligned
            Some(unsafe { PhysFrame::from_start_address_unchecked(PhysAddr::new(allocated_frame)) })
        } else {
            //info!("frame allocation failed {:x}", status.as_usize());
            None
        }
    }
}
