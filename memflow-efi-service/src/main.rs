#![no_std]
#![no_main]
#![feature(lang_items)]
#![feature(panic_info_message)]

#[macro_use]
mod logger;
mod utils;

use core::borrow::BorrowMut;
use core::mem::MaybeUninit;

use r_efi::system::BootCopyMem;
use r_efi::*;

static mut SYSTEM_TABLE: MaybeUninit<efi::SystemTable> = MaybeUninit::uninit();

pub fn system_table() -> &'static efi::SystemTable {
    unsafe { &*SYSTEM_TABLE.as_ptr() }
}

pub fn runtime_services() -> &'static efi::RuntimeServices {
    unsafe { &*system_table().runtime_services }
}

pub fn boot_services() -> &'static efi::BootServices {
    unsafe { &*system_table().boot_services }
}

extern "win64" fn handle_exit_boot_services(_event: base::Event, _context: *mut core::ffi::c_void) {
    info!("[~] ExitBootServices() has been called.");
}

extern "win64" fn handle_set_virtual_address_map(
    _event: base::Event,
    _context: *mut core::ffi::c_void,
) {
    info!("[~] SetVirtualAddressMap() has been called.");
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
        runtime_services() as *const _ as *mut core::ffi::c_void,
        &efi::EVENT_GROUP_VIRTUAL_ADDRESS_CHANGE,
        event_virtual_address.borrow_mut(),
    );

    if status.is_error() {
        error!(
            "[-] Creating VIRTUAL_ADDRESS_CHANGE event failed: {:#x}",
            status.as_usize()
        );
        return status;
    }

    let mut event_boot_services: base::Event = core::ptr::null_mut();
    status = (boot_services().create_event_ex)(
        efi::EVT_NOTIFY_SIGNAL,
        efi::TPL_CALLBACK,
        Some(handle_exit_boot_services),
        runtime_services() as *const _ as *mut core::ffi::c_void,
        &efi::EVENT_GROUP_EXIT_BOOT_SERVICES,
        event_boot_services.borrow_mut(),
    );

    if status.is_error() {
        error!(
            "[-] Creating EXIT_BOOT_SERVICES event failed: {:#x}",
            status.as_usize()
        );
        return status;
    }

    // Your runtime driver initialization. If the initialization fails, manually close the previously
    // created events with:
    // (boot_services().close_event)(event_virtual_address);
    // (boot_services().close_event)(event_boot_services);

    info!("[~] EFI runtime driver has been loaded and initialized.");

    let s = [
        0x0048u16, 0x0065u16, 0x006cu16, 0x006cu16, 0x006fu16, // "Hello"
        0x0020u16, //                                             " "
        0x0057u16, 0x006fu16, 0x0072u16, 0x006cu16, 0x0064u16, // "World"
        0x0021u16, //                                             "!"
        0x000au16, //                                             "\n"
        0x0000u16, //                                             NUL
    ];

    // Print "Hello World!".
    let r = unsafe {
        ((*system_table().con_out).output_string)(
            system_table().con_out,
            s.as_ptr() as *mut efi::Char16,
        )
    };
    if r.is_error() {
        return r;
    }

    //unsafe { core::ptr::copy(0 as *mut u8, 0x1000 as *mut u8, 0x1000) };

    // Wait for key input, by waiting on the `wait_for_key` event hook.
    let r = unsafe {
        let mut x: usize = 0;
        ((*system_table().boot_services).wait_for_event)(
            1,
            &mut (*system_table().con_in).wait_for_key,
            &mut x,
        )
    };
    if r.is_error() {
        return r;
    }

    efi::Status::SUCCESS
}
