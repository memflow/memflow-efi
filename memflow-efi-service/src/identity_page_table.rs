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
        FrameAllocator, FrameDeallocator, PhysFrame, Translate,
    },
    PhysAddr, VirtAddr,
};

use crate::{boot_services, mem_maps::EfiMemMaps};

const REMAP_SIZE: usize = (Size1GiB::SIZE as usize) << 9;
const REMAP_ALIGN: usize = REMAP_SIZE - 1;

use core::cell::UnsafeCell;
use core::mem::{ManuallyDrop, MaybeUninit};
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicIsize, Ordering};

pub struct ConcurrentStaticVec<T, const N: usize> {
    buf: UnsafeCell<MaybeUninit<[T; N]>>,
    len: AtomicIsize,
}

impl<T, const N: usize> ConcurrentStaticVec<T, N> {
    pub const fn new() -> Self {
        Self {
            buf: UnsafeCell::new(MaybeUninit::uninit()),
            len: AtomicIsize::new(0),
        }
    }

    pub const fn new_from_const_array(buf: [T; N]) -> Self {
        Self {
            buf: UnsafeCell::new(MaybeUninit::new(buf)),
            len: AtomicIsize::new(N as isize),
        }
    }

    pub fn push(&self, val: T) {
        let idx = loop {
            let idx = self.len.fetch_add(1, Ordering::Relaxed);
            // Pop may decrement us into negatives, undo that
            if idx >= 0 {
                break idx as usize;
            }
        };
        assert!(idx < N);
        let cell = self.buf.get() as *mut [MaybeUninit<T>; N];
        // Safety: index is incremented atomically, thus we have exclusive access
        unsafe {
            (*cell)[idx].write(val);
        }
    }

    pub fn pop(&self) -> Option<T> {
        let idx = self.len.fetch_sub(1, Ordering::Relaxed);
        if idx <= 0 {
            None
        } else {
            let idx = idx as usize - 1;
            assert!(idx < N);
            let cell = self.buf.get() as *mut [MaybeUninit<T>; N];
            // Safety: index is decremented atomically, thus we have exclusive access.
            // In addition, the prior code ensures that the value is initialized.
            unsafe { Some((*cell)[idx].assume_init_read()) }
        }
    }

    pub fn len(&self) -> usize {
        core::cmp::max(self.len.load(Ordering::Relaxed), 0) as usize
    }
}

pub struct DropPush<'a, T, const N: usize>(&'a ConcurrentStaticVec<T, N>, ManuallyDrop<T>);

impl<'a, T, const N: usize> DropPush<'a, T, N> {
    fn pop(vec: &'a ConcurrentStaticVec<T, N>) -> Option<Self> {
        vec.pop().map(|elem| Self(vec, ManuallyDrop::new(elem)))
    }
}

impl<'a, T, const N: usize> Deref for DropPush<'a, T, N> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.1
    }
}

impl<'a, T, const N: usize> DerefMut for DropPush<'a, T, N> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.1
    }
}

impl<'a, T, const N: usize> Drop for DropPush<'a, T, N> {
    fn drop(&mut self) {
        // Safety: this is the only place where we take this value.
        let value = unsafe { ManuallyDrop::take(&mut self.1) };
        self.0.push(value);
    }
}

#[repr(align(4096))]
pub struct IdentityPageTable {
    page_table: PageTable, // TODO: align?
    allocator: StaticFrameAllocator<10000>,
    free_virt_remaps: ConcurrentStaticVec<usize, 256>,
}

impl IdentityPageTable {
    pub const fn new() -> Self {
        Self {
            page_table: PageTable::new(),
            allocator: StaticFrameAllocator::new(),
            free_virt_remaps: ConcurrentStaticVec::new(),
        }
    }

    pub fn map_to_virt(&mut self, phys: u64, virt: u64, size: u64) -> Result<(), &'static str> {
        let mut remap_off = virt - phys;

        let mut pt_mapper = unsafe { OffsetPageTable::new(&mut self.page_table, VirtAddr::new(0)) };

        type Alignment = Size4KiB;

        let alignment = Alignment::SIZE;
        let alignment_mask = alignment - 1;

        let start = phys & !alignment_mask;
        let end = (phys + size + alignment_mask) & !alignment_mask;

        if (remap_off & alignment_mask) != 0 {
            return Err("Incompatible alignment");
        }

        info!("{start:x}-{end:x}");
        for addr in (start..end).step_by(alignment as usize) {
            match unsafe {
                pt_mapper.map_to(
                    Page::<Alignment>::from_start_address_unchecked(VirtAddr::new(
                        addr + remap_off,
                    )),
                    PhysFrame::from_start_address_unchecked(PhysAddr::new(addr)),
                    PageTableFlags::PRESENT | PageTableFlags::WRITABLE,
                    &mut self.allocator,
                )
            } {
                Ok(_) => debug!(
                    "4kb page table entry for address {:x} created on {addr:x}",
                    addr + remap_off
                ),
                Err(err) => {
                    error!("could not add 4kb page_table entry, {err:?}");
                }
            }
        }

        Ok(())
    }

    pub fn create_identity_mapping(&mut self, mem_maps: &EfiMemMaps) -> Result<(), String> {
        let mut pt_mapper = unsafe { OffsetPageTable::new(&mut self.page_table, VirtAddr::new(0)) };

        let mut largest_identity_mapping = 0;

        // loop through each page in each mapping and create new entries
        for mem_map in mem_maps.iter() {
            //.filter(|m| m.r#type <= 7) {
            let mut bytes_offset = 0;
            let mut bytes_left = mem_map.number_of_pages * Size4KiB::SIZE;

            let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;

            info!("bytes_left: {}", bytes_left);

            while bytes_left > 0 {
                let map_addr = mem_map.physical_start + bytes_offset;

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
                                map_addr,
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
                    largest_identity_mapping =
                        core::cmp::max(largest_identity_mapping, map_addr + Size4KiB::SIZE);
                };
            }
        }

        // Align largest mapping address to the next PML4 entry
        let remap_pml4_id = (largest_identity_mapping as usize + REMAP_ALIGN) / REMAP_SIZE;

        for i in remap_pml4_id..512 {
            self.free_virt_remaps.push(i);
        }

        info!("Remappable entries: {}", self.free_virt_remaps.len());

        Ok(())
    }

    /// Remaps a virtual address range
    ///
    /// # Parameters
    ///
    /// * `virt_addr` - Virtual address to remap.
    /// * `size` - Size of the virtual mapping.
    /// * `from_cr3` - Virtual address space to remap from.
    ///
    /// # Returns
    ///
    /// `Some((handle, addr))` - remapped virtual address if successful.
    ///
    /// `None` if not successful. This can occur when there are no free PML4 entries left,
    /// or whenever virtual address range overlaps multiple PML4 entries.
    pub fn remap_range(
        &mut self,
        virt_addr: usize,
        size: usize,
        from_cr3: PhysFrame,
    ) -> Option<(impl Drop + '_, usize)> {
        if virt_addr & !REMAP_ALIGN != (virt_addr + size) & !REMAP_ALIGN {
            return None;
        }

        let from_pml4_id = virt_addr & !REMAP_ALIGN;
        let to_pml4_id = DropPush::pop(&self.free_virt_remaps)?;

        let from_cr3 = from_cr3.start_address().as_u64() as *const PageTable;
        // Safety: not very safe.
        let entry = unsafe { (*from_cr3)[from_pml4_id].clone() };
        self.page_table[*to_pml4_id] = entry;

        let remapped_addr = (*to_pml4_id * REMAP_SIZE) + (virt_addr & !REMAP_ALIGN);

        Some((to_pml4_id, remapped_addr))
    }

    // copies high mem pml4 entries from the given dtb
    pub fn copy_pml4_entries(&mut self, dtb: u64) -> Result<(), String> {
        let page_table_ptr = &mut self.page_table as *mut _ as *mut c_void as u64;
        unsafe {
            core::ptr::copy_nonoverlapping(
                (dtb + 8 * 256) as *mut u8,
                (page_table_ptr + 8 * 256) as *mut u8,
                8 * 256,
            )
        };
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

#[repr(align(4096))]
pub struct StaticFrameAllocator<const N: usize> {
    frames: [[u8; 0x1000]; N],
    free_frames: ConcurrentStaticVec<usize, N>,
}

impl<const N: usize> StaticFrameAllocator<N> {
    pub const fn new() -> Self {
        Self {
            frames: [[0u8; 0x1000]; N],
            free_frames: {
                let mut cnt = 0;
                let mut ret = [0; N];
                while cnt < N {
                    ret[cnt] = cnt;
                    cnt += 1;
                }
                ConcurrentStaticVec::new_from_const_array(ret)
            },
        }
    }
}

unsafe impl<const N: usize> FrameAllocator<Size4KiB> for StaticFrameAllocator<N> {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        self.free_frames
            .pop()
            .map(|frame| self.frames[frame].as_ptr() as u64)
            .map(PhysAddr::new)
            .map(PhysFrame::from_start_address)
            .transpose()
            .ok()
            .flatten()
    }
}

impl<const N: usize> FrameDeallocator<Size4KiB> for StaticFrameAllocator<N> {
    unsafe fn deallocate_frame(&mut self, frame: PhysFrame<Size4KiB>) {
        let off = frame.start_address().as_u64() - self.frames[0].as_ptr() as u64;
        let idx = (off / Size4KiB::SIZE) as usize;
        self.free_frames.push(idx);
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
