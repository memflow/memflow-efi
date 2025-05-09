#![no_std]
#![no_main]
#![feature(lang_items)]
#![feature(panic_info_message)]

extern crate alloc;

#[macro_use]
mod logger;
mod hooks;
mod identity_page_table;
mod mem_maps;
mod utils;
mod vtop;

use core::arch::asm;
use core::borrow::BorrowMut;
use core::ffi::c_void;
use core::mem::MaybeUninit;
use core::ops;

use ::r_efi::{
    protocols::*,
    system::{RuntimeSetVariable, TPL_HIGH_LEVEL},
    *,
};
use identity_page_table::IdentityPageTable;
use mem_maps::EfiMemMaps;
use r_efi::{protocols::file::ProtocolOpen, system::MemoryDescriptor};

use x86_64::{
    addr::PhysAddr,
    registers::{
        self,
        control::{Cr3, Cr3Flags},
    },
    structures::paging::PhysFrame,
};

// system table
static mut SYSTEM_TABLE: MaybeUninit<efi::SystemTable> = MaybeUninit::uninit();
static mut EFI_MEM_MAPS: EfiMemMaps = EfiMemMaps::new();
static mut IDENTITY_CR3: Option<(PhysFrame, Cr3Flags)> = None;
static mut IDENTITY_PAGE_TABLE: IdentityPageTable = IdentityPageTable::new();
static mut IDENTITY_PAGE_TABLE_BASE: u64 = 0u64;

pub fn system_table() -> &'static efi::SystemTable {
    unsafe { &*SYSTEM_TABLE.as_ptr() }
}

pub fn system_table_mut() -> &'static mut efi::SystemTable {
    unsafe { &mut *SYSTEM_TABLE.as_mut_ptr() }
}

pub fn runtime_services() -> &'static efi::RuntimeServices {
    unsafe { &*system_table().runtime_services }
}

pub fn runtime_services_mut() -> &'static mut efi::RuntimeServices {
    unsafe { &mut *system_table().runtime_services }
}

pub fn boot_services() -> &'static efi::BootServices {
    unsafe { &*system_table().boot_services }
}

eficall! {fn handle_exit_boot_services(mut event: base::Event, _context: *mut c_void) {
    info!("handle_exit_boot_services called");

    // retrieve latest mem maps
    /*
    let mem_maps = unsafe { &mut EFI_MEM_MAPS };
    match mem_maps.load_maps(boot_services()) {
        Ok(_) => {
            info!("retrieved a total of {} mem_maps", mem_maps.len());
        }
        Err(err) => {
            error!("mem_maps could not be retrieved: {}", err);
        }
    }

    // create custom identity mapped pagetable
    let identity_page_table = unsafe { &mut IDENTITY_PAGE_TABLE };
    match identity_page_table.create_identity_mapping(mem_maps) {
        Ok(_) => {
            info!("identity mapping created at: {}", 0);
        }
        Err(err) => {
            error!("unable to create identity mapping: {}", err);
        }
    }

    // retrieve cr3
    unsafe { IDENTITY_CR3 = Some(Cr3::read()) };
    info!("efi identity cr3: {:?}", unsafe { IDENTITY_CR3 });
    */

    // TODO: STALL UNTIL UNPLUGGED
    // the genius method of unplugging the USB before windows boots by stalling on the exit boot services event.
    // TODO: STALL UNTIL UNPLUGGED

    event = core::ptr::null_mut();
}
}

fn convert_pointer<T>(mut p: *mut T) -> Option<*mut T> {
    match (runtime_services().convert_pointer)(0, &mut p as *mut _ as *mut *mut _) {
        efi::Status::SUCCESS => Some(p),
        _ => None,
    }
}

eficall! {fn handle_set_virtual_address_map(mut event: base::Event, _context: *mut c_void) {
    info!("handle_set_virtual_address_map called");

    unsafe {
        hooks::convert_hook_pointers();

        let new_system_table = convert_pointer(SYSTEM_TABLE.as_mut_ptr());
        info!("convert pointer: prev_system_table={:?}; new_system_table={:?}", SYSTEM_TABLE.as_mut_ptr(), new_system_table);

        let new_ags = convert_pointer(AGS.as_mut_ptr());
        info!("convert pointer: prev_ags={:?}; new_ags={:?}", AGS.as_mut_ptr(), new_ags);

        // let prev_port = &logger::PORT as *const _ as usize;
        // (runtime_services().convert_pointer)(0, &mut logger::PORT as *mut _ as *mut *mut _);
        // info!("convert pointer: prev_port={:x}; new_port={:x}", prev_port, &logger::PORT as *const _ as usize);
    }

    // Map our image into
    if let Some(image) = unsafe { LOADED_IMAGE.as_ref() } {
        let new_base = convert_pointer(image.image_base).unwrap();
        let identity_page_table = unsafe { &mut IDENTITY_PAGE_TABLE };
        identity_page_table.map_to_virt(image.image_base as u64, new_base as u64, image.image_size as u64).unwrap();
    }

    //// cr3 of ntoskrnl
    //let kernel_dtb = Cr3::read();
    //info!("kernel cr3: {:?}", kernel_dtb);

    //let identity_page_table = unsafe { &mut IDENTITY_PAGE_TABLE };
    //let dtb = identity_page_table.dtb();

    //info!("switching to cr3 for identity map: {:?}", dtb);
    //unsafe { Cr3::write(dtb, kernel_dtb.1) };

    //identity_page_table.copy_pml4_entries(kernel_dtb.0.start_address().as_u64());

    //test_phys_read();

    //info!("switching to original cr3: {:?}", kernel_dtb.0);
    //unsafe { Cr3::write(kernel_dtb.0, kernel_dtb.1) };

    /*
    let kernel_dtb = Cr3::read();
    info!("kernel cr3: {:?}", kernel_dtb);

    let identity_page_table = unsafe { &mut IDENTITY_PAGE_TABLE };
    identity_page_table.copy_pml4_entries(kernel_dtb.0.start_address().as_u64());

    let mem_maps = unsafe { &mut EFI_MEM_MAPS };
    match identity_page_table.create_identity_mapping(mem_maps) {
        Ok(_) => {
            info!("identity mapping created at: {}", 0);
        }
        Err(err) => {
            error!("unable to create identity mapping: {}", err);
        }
    }
    unsafe { IDENTITY_PAGE_TABLE_BASE = identity_page_table.dtb_addr() };
    test_phys_read();
    */

    event = core::ptr::null_mut();
}
}

eficall! {fn efi_unload(
    _image_handle: crate::base::Handle,
) -> crate::base::Status {
    info!("efi_unload called");
    efi::Status::ACCESS_DENIED
}}

static mut LOADED_IMAGE: *mut loaded_image::Protocol = core::ptr::null_mut();

fn init_dummy_protocol(image_handle: efi::Handle) -> efi::Status {
    let status = (boot_services().open_protocol)(
        image_handle,
        &mut loaded_image::PROTOCOL_GUID as *mut _,
        unsafe { &mut LOADED_IMAGE } as *mut _ as *mut *mut _,
        image_handle,
        core::ptr::null_mut(),
        efi::OPEN_PROTOCOL_GET_PROTOCOL,
    );
    if status.is_error() {
        info!(
            "unable to open protocol for loaded_image: {:#x}",
            status.as_usize()
        );
        return status;
    }

    // create protocol?

    unsafe { (&mut *LOADED_IMAGE).unload = efi_unload };

    efi::Status::SUCCESS

    /*
        // Install our protocol interface
        // This is needed to keep our driver loaded
        DummyProtocalData dummy = { 0 };
        status = LibInstallProtocolInterfaces(
          &ImageHandle, &ProtocolGuid,
          &dummy, NULL);

        // Return if interface failed to register
        if (EFI_ERROR(status))
        {
            Print(L"Can't register interface: %d\n", status);
            return status;
        }

        // Set our image unload routine
        LoadedImage->Unload = (EFI_IMAGE_UNLOAD)efi_unload;
    */
}

fn test_phys_read() {
    let mut buffer = [0u8; 0x1000];

    let old_cr3 = Cr3::read();

    let identity_page_table = unsafe { &mut IDENTITY_PAGE_TABLE };
    let dtb = identity_page_table.dtb();

    info!("switching to cr3 for identity map: {:?}", dtb);
    unsafe { Cr3::write(dtb, old_cr3.1) };

    unsafe { core::ptr::copy_nonoverlapping(0 as *mut u8, buffer.as_mut_ptr(), 0x1000) };

    info!("switching to original cr3: {:?}", old_cr3.0);
    unsafe { Cr3::write(old_cr3.0, old_cr3.1) };

    info!("data={:X?}", buffer);
}

#[export_name = "efi_main"]
pub extern "C" fn main(
    image_handle: efi::Handle,
    raw_system_table: *mut efi::SystemTable,
) -> efi::Status {
    #[cfg(debug_assertions)]
    {
        utils::wait_for_debugger();
    }

    // setup system_table
    unsafe { SYSTEM_TABLE = MaybeUninit::new(raw_system_table.read()) };

    // setup allocator
    init_allocator();

    info!("enter main()");

    init_dummy_protocol(image_handle);

    // TODO: move to exit boot
    let mem_maps = unsafe { &mut EFI_MEM_MAPS };
    mem_maps.load_maps(boot_services()); // TODO: temp
    let identity_page_table = unsafe { &mut IDENTITY_PAGE_TABLE };
    match identity_page_table.create_identity_mapping(mem_maps) {
        Ok(_) => {
            info!("identity mapping created at: {}", 0);
        }
        Err(err) => {
            error!("unable to create identity mapping: {}", err);
            return efi::Status::ABORTED;
        }
    }
    unsafe { IDENTITY_PAGE_TABLE_BASE = identity_page_table.dtb_addr() };
    //test_phys_read();

    // Register to events relevant for runtime drivers.
    let mut event_virtual_address: base::Event = core::ptr::null_mut();
    let mut status = (boot_services().create_event_ex)(
        efi::EVT_NOTIFY_SIGNAL,
        efi::TPL_CALLBACK,
        Some(handle_set_virtual_address_map),
        runtime_services() as *const _ as *mut c_void,
        &efi::EVENT_GROUP_VIRTUAL_ADDRESS_CHANGE,
        event_virtual_address.borrow_mut(),
    );

    if status.is_error() {
        error!(
            "creating VIRTUAL_ADDRESS_CHANGE event failed: {:#x}",
            status.as_usize()
        );
        return status;
    }

    let mut event_boot_services: base::Event = core::ptr::null_mut();
    status = (boot_services().create_event_ex)(
        efi::EVT_NOTIFY_SIGNAL,
        efi::TPL_CALLBACK,
        Some(handle_exit_boot_services),
        runtime_services() as *const _ as *mut c_void,
        &efi::EVENT_GROUP_EXIT_BOOT_SERVICES,
        event_boot_services.borrow_mut(),
    );

    if status.is_error() {
        info!(
            "creating EXIT_BOOT_SERVICES event failed: {:#x}",
            status.as_usize()
        );
        return status;
    }

    // Your runtime driver initialization. If the initialization fails, manually close the previously
    // created events with:
    // (boot_services().close_event)(event_virtual_address);
    // (boot_services().close_event)(event_boot_services);

    info!("memflow efi runtime driver has been initialized.");

    // Setup Hooks
    info!("setting up runtime_services hooks");
    unsafe { hooks::init_hooks() };
    info!("hooks set successfully, exiting.");

    efi::Status::SUCCESS
}

#[macro_use]
extern crate alloc_no_stdlib;
use alloc_no_stdlib::{
    bzero, uninitialized, AllocatedStackMemory, Allocator, SliceWrapper, SliceWrapperMut,
    StackAllocator,
};

use ::core::alloc::{GlobalAlloc, Layout};

// allocator
declare_stack_allocator_struct!(GlobalAllocatedFreelist, 16, global);
define_allocator_memory_pool!(16, u8, [0; 1024 * 1024 * 25], global, global_buffer);

static mut AGS: MaybeUninit<StackAllocator<u8, GlobalAllocatedFreelist<u8>>> =
    MaybeUninit::uninit();
fn ags_mut() -> &'static mut StackAllocator<'static, u8, GlobalAllocatedFreelist<'static, u8>> {
    unsafe { &mut *AGS.as_mut_ptr() }
}

fn init_allocator() {
    unsafe { AGS = MaybeUninit::new(GlobalAllocatedFreelist::<u8>::new_allocator(bzero)) };
    unsafe {
        bind_global_buffers_to_allocator!((&mut *AGS.as_mut_ptr()), global_buffer, u8);
    }
}

pub struct EfiAllocator;

unsafe impl GlobalAlloc for EfiAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        {
            let size = layout.size();
            let align = layout.align();

            if align > 8 {
                // allocate more space for alignment
                let ptr = ags_mut().alloc_cell(size + align).as_mut_ptr();

                // calculate align offset
                let mut offset = ptr.align_offset(align);
                if offset == 0 {
                    offset = align;
                }
                let return_ptr = ptr.add(offset);
                // store allocated pointer before the struct
                (return_ptr.cast::<*mut u8>()).sub(1).write(ptr);
                return_ptr
            } else {
                ags_mut().alloc_cell(size).as_mut_ptr()
            }
        }
    }

    unsafe fn dealloc(&self, mut ptr: *mut u8, layout: Layout) {
        if layout.align() > 8 {
            ptr = (ptr as *const *mut u8).sub(1).read();
        }
        let buf = unsafe { core::slice::from_raw_parts_mut(ptr, layout.size()) };
        ags_mut().free_cell(AllocatedStackMemory { mem: buf })
    }
}

#[global_allocator]
static ALLOCATOR: EfiAllocator = EfiAllocator;
