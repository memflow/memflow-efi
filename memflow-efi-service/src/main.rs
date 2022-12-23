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

pub fn runtime_services_mut() -> &'static mut efi::RuntimeServices {
    unsafe { &mut *system_table().runtime_services }
}

pub fn boot_services() -> &'static efi::BootServices {
    unsafe { &*system_table().boot_services }
}

eficall! {fn handle_exit_boot_services(_event: base::Event, _context: *mut c_void) {
    info!("handle_exit_boot_services called");

    unsafe {
        //((*system_table().con_out).set_attribute)(system_table().con_out, 0x1 | 0x2 | 0x4 | 0x8 | 0x10);
        //((*system_table().con_out).clear_screen)(system_table().con_out);
    }
}
}

eficall! {fn handle_set_virtual_address_map(_event: base::Event, _context: *mut c_void) {
    info!("handle_set_virtual_address_map called");

    unsafe {
        (runtime_services().convert_pointer)(0, &mut ORIG_SET_VARIABLE as *mut *const _ as *mut *mut _);
        info!("convert pointer: new_orig_set_variable={:x}", ORIG_SET_VARIABLE as usize);

        (runtime_services().convert_pointer)(0, &mut SYSTEM_TABLE as *mut _ as *mut *mut _);
        info!("convert pointer: new_system_table={:x}", SYSTEM_TABLE.as_mut_ptr() as usize);
    }
}
}

static mut ORIG_SET_VARIABLE: *const c_void = core::ptr::null_mut();
eficall! {fn hook_set_variable(
    variable_name: *mut crate::base::Char16,
    vendor_guid: *mut crate::base::Guid,
    attributes: u32,
    data_size: usize,
    data: *mut c_void,
) -> crate::base::Status {
    info!("hook_set_variable called: orig_set_variable={:x}", unsafe { ORIG_SET_VARIABLE as u64 });
    let orig_func: RuntimeSetVariable = unsafe { core::mem::transmute(ORIG_SET_VARIABLE) };
    (orig_func)(variable_name, vendor_guid, attributes, data_size, data)
}
}

fn hook_service_pointer(orig_func: *mut *mut c_void, hook_func: *mut c_void) -> *mut c_void {
    let orig_tpl = (boot_services().raise_tpl)(TPL_HIGH_LEVEL);

    info!(
        "hooking function: orig_func={:?}; hook_func={:?}",
        unsafe { *orig_func } as usize,
        hook_func
    );
    let orig_func_bak = unsafe { *orig_func };
    unsafe { *orig_func = hook_func };

    {
        let system_table_header = &mut system_table_mut().hdr;
        let prev_crc32 = system_table_header.crc32;
        system_table_header.crc32 = 0;
        (boot_services().calculate_crc32)(
            system_table_header as *mut _ as *mut _,
            system_table_header.header_size as usize,
            &mut system_table_header.crc32,
        );
        info!(
            "recomputing crc32 of system_table: old_crc32={}, new_crc32={}",
            prev_crc32, system_table_header.crc32
        );
    }

    (boot_services().restore_tpl)(orig_tpl);

    orig_func_bak
}

#[export_name = "efi_main"]
pub extern "C" fn main(
    _image_handle: efi::Handle,
    raw_system_table: *mut efi::SystemTable,
) -> efi::Status {
    
    let mut uefi_system_table = unsafe {
        ::uefi::table::SystemTable::<::uefi::table::Boot>::from_ptr(raw_system_table as *mut _)
            .expect("Pointer must not be null!")
    };
    ::uefi_services::init(&mut uefi_system_table).unwrap();
    log::set_max_level(log::LevelFilter::Trace);

    /*
    #[cfg(debug_assertions)]
    {
        utils::wait_for_debugger();
    }
    */

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
        info!(
            "[-] Creating EXIT_BOOT_SERVICES event failed: {:#x}",
            status.as_usize()
        );
        return status;
    }

    // Your runtime driver initialization. If the initialization fails, manually close the previously
    // created events with:
    // (boot_services().close_event)(event_virtual_address);
    // (boot_services().close_event)(event_boot_services);

    info!("memflow efi runtime driver has been initialized.");

    // TODO: Unload routine?
    // Setup hooks

    // Hook SetVariable (should not fail)
    info!("setting up runtime_services hooks");

    let orig_func = hook_service_pointer(
        &mut runtime_services_mut().set_variable as *mut _ as *mut *mut _,
        hook_set_variable as *mut _,
    );
    unsafe {
        ORIG_SET_VARIABLE = orig_func;
    }

    info!("hooks set successfully, exiting.");

    //info!("hooks set successfully, press any key to boot.");

    //unsafe { core::ptr::copy(0 as *mut u8, 0x1000 as *mut u8, 0x1000) };

    // Wait for key input, by waiting on the `wait_for_key` event hook.
    /*
    let r = unsafe {
        let mut x: usize = 0;
        (boot_services().wait_for_event)(
            1,
            &mut (*system_table().con_in).wait_for_key,
            &mut x,
        )
    };
    if r.is_error() {
        return r;
    }
    */

    efi::Status::SUCCESS
}
