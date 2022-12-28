use core::ffi::c_void;

use ::r_efi::{
    protocols::file::ProtocolOpen,
    protocols::*,
    system::MemoryDescriptor,
    system::{RuntimeSetVariable, TPL_HIGH_LEVEL},
    *,
};
use alloc::{format, string::String};
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
    page_table: PageTable, // TODO: align?
    allocator: StaticFrameAllocator<10000>,
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

        // loop through each page in each mapping and create new entries
        for mem_map in mem_maps.iter() {
            //.filter(|m| m.r#type <= 7) {
            let mut bytes_offset = 0;
            let mut bytes_left = mem_map.number_of_pages * Size4KiB::SIZE;

            let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;

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
                    info!("mapping 2mib page");
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
                            return Err(format!("could not add 2mib page_table entry, for mem_map at {:x} with size {:x}: {:?}", mem_map.physical_start, mem_map.number_of_pages* Size4KiB::SIZE, err));
                        }
                    }
                    bytes_offset += Size2MiB::SIZE;
                    bytes_left -= Size2MiB::SIZE;
                } else*/
                {
                    match unsafe {
                        pt_mapper.identity_map(
                            PhysFrame::<Size4KiB>::from_start_address_unchecked(PhysAddr::new(
                                mem_map.physical_start + bytes_offset,
                            )),
                            flags,
                            &mut self.allocator,
                        )
                    } {
                        Ok(_) => {
                            /*debug!(
                                "4kib page table entry for address {:x} created",
                                mem_map.physical_start + bytes_offset
                            )*/
                        }
                        Err(err) => {
                            return Err(format!("could not add 4kib page_table entry, for mem_map at {:x} with size {:x}: {:?}", mem_map.physical_start, mem_map.number_of_pages* Size4KiB::SIZE, err));
                        }
                    }

                    bytes_offset += Size4KiB::SIZE;
                    bytes_left -= Size4KiB::SIZE;
                };
            }
        }

        Ok(())
    }

    // copies high mem pml4 entries from the given dtb
    pub fn copy_pml4_entries(&mut self, dtb: u64) -> Result<(), String> {
        let page_table_ptr = &mut self.page_table as *mut _ as *mut c_void as u64;
        unsafe { core::ptr::copy_nonoverlapping((dtb + 8 * 256) as *mut u8, (page_table_ptr + 8 * 256) as *mut u8, 8 * 256) };
        Ok(())
    }

    pub fn dtb_addr(&self) -> u64 {
        &self.page_table as *const _ as *const c_void as u64
    }

    pub fn dtb(&self) -> PhysFrame {
        let addr = self.dtb_addr();
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
            let aligned_addr = align_addr_forward(addr);
            self.used_frames += 1;
            PhysFrame::from_start_address(PhysAddr::new(aligned_addr)).ok()
        } else {
            None
        }
    }
}

fn align_addr_forward(addr: u64) -> u64 {
    if addr % Size4KiB::SIZE == 0 {
        addr
    } else {
        (addr + Size4KiB::SIZE) - (addr + Size4KiB::SIZE) % Size4KiB::SIZE
    }
}

// TODO: create dynamic alloc
#[derive(Default)]
pub struct DynamicFrameAllocator;

unsafe impl FrameAllocator<Size4KiB> for DynamicFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        let mut addr = core::ptr::null_mut();
        let status = (boot_services().allocate_pool)(
            LOADER_DATA,
            2 * Size4KiB::SIZE as usize,
            &mut addr as *mut *mut _ as *mut *mut _,
        );
        if status == efi::Status::SUCCESS {
            let addr = addr as u64;
            let aligned_addr = align_addr_forward(addr);
            PhysFrame::from_start_address(PhysAddr::new(aligned_addr)).ok()
        } else {
            None
        }
    }
}
