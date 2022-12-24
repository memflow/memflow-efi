#![no_std]
#![no_main]
#![feature(lang_items)]
#![feature(panic_info_message)]

#[macro_use]
mod logger;
mod hooks;
mod utils;

use core::borrow::BorrowMut;
use core::ffi::c_void;
use core::mem::MaybeUninit;

use ::r_efi::{
    system::{RuntimeSetVariable, TPL_HIGH_LEVEL},
    *,
};

static mut SYSTEM_TABLE: MaybeUninit<efi::SystemTable> = MaybeUninit::uninit();

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

eficall! {fn handle_exit_boot_services(_event: base::Event, _context: *mut c_void) {
    info!("handle_exit_boot_services called");
}
}

eficall! {fn handle_set_virtual_address_map(_event: base::Event, _context: *mut c_void) {
    info!("handle_set_virtual_address_map called");

    unsafe {
        hooks::convert_hook_pointers();

        (runtime_services().convert_pointer)(0, &mut SYSTEM_TABLE as *mut _ as *mut *mut _);
        info!("convert pointer: new_system_table={:x}", SYSTEM_TABLE.as_mut_ptr() as usize);
    }
}
}

#[export_name = "efi_main"]
pub extern "C" fn main(
    _image_handle: efi::Handle,
    raw_system_table: *mut efi::SystemTable,
) -> efi::Status {
    #[cfg(debug_assertions)]
    {
        utils::wait_for_debugger();
    }

    unsafe { SYSTEM_TABLE = MaybeUninit::new(raw_system_table.read()) };

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

    //unsafe { core::ptr::copy(0 as *mut u8, 0x1000 as *mut u8, 0x1000) };

    // Wait for key input, by waiting on the `wait_for_key` event hook.
    let r = unsafe {
        let mut x: usize = 0;
        (boot_services().wait_for_event)(1, &mut (*system_table().con_in).wait_for_key, &mut x)
    };
    if r.is_error() {
        return r;
    }

    efi::Status::SUCCESS
}
