#![no_std]
#![no_main]
#![feature(lang_items)]
#![feature(panic_info_message)]

#[macro_use]
mod logger;
mod utils;

use core::borrow::BorrowMut;
use core::ffi::c_void;
use core::mem::MaybeUninit;

use r_efi::system::{RuntimeSetVariable, TPL_HIGH_LEVEL};
use r_efi::*;

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

pub fn boot_services() -> &'static efi::BootServices {
    unsafe { &*system_table().boot_services }
}

extern "win64" fn handle_exit_boot_services(_event: base::Event, _context: *mut c_void) {
    info!("[~] ExitBootServices() has been called.");
}

extern "win64" fn handle_set_virtual_address_map(_event: base::Event, _context: *mut c_void) {
    // Keep the runtime service loaded
}

static mut ORIG_SET_VARIABLE: *const c_void = core::ptr::null_mut();
eficall! {fn hook_set_variable(
    variable_name: *mut crate::base::Char16,
    vendor_guid: *mut crate::base::Guid,
    attributes: u32,
    data_size: usize,
    data: *mut c_void,
) -> crate::base::Status {
    let orig_func: RuntimeSetVariable = unsafe { core::mem::transmute(ORIG_SET_VARIABLE) };
    (orig_func)(variable_name, vendor_guid, attributes, data_size, data)
}
}

fn hook_service_pointer(orig_func: *mut *mut c_void, hook_func: *mut c_void) -> *mut c_void {
    let boot_services = unsafe { &*(system_table().boot_services) };
    let orig_tpl = (boot_services.raise_tpl)(TPL_HIGH_LEVEL);

    let orig_func_bak = unsafe { *orig_func };
    unsafe { *orig_func = hook_func };

    {
        let system_table_header = &mut system_table_mut().hdr;
        system_table_header.crc32 = 0;
        (boot_services.calculate_crc32)(
            system_table_header as *mut _ as *mut _,
            system_table_header.header_size as usize,
            &mut system_table_header.crc32,
        );
    }

    (boot_services.restore_tpl)(orig_tpl);

    orig_func_bak
}

/*
    // Swap the pointers
    // GNU-EFI and InterlockedCompareExchangePointer
    // are not friends
    VOID* OriginalFunction = *ServiceTableFunction;
    *ServiceTableFunction = NewFunction;

    // Change the table CRC32 signature
    ServiceTableHeader->CRC32 = 0;
    BS->CalculateCrc32((UINT8*)ServiceTableHeader, ServiceTableHeader->HeaderSize, &ServiceTableHeader->CRC32);
}
*/

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
        runtime_services() as *const _ as *mut c_void,
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

    // TODO: Unload routine?
    // Setup hooks

    // Hook SetVariable (should not fail)
    let runtime_services = unsafe { &mut *(system_table_mut().runtime_services) };
    hook_service_pointer(
        &mut runtime_services.set_variable as *mut _ as *mut *mut _,
        hook_set_variable as *mut _,
    );

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
