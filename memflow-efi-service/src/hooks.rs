use core::ffi::c_void;

use ::r_efi::{system::*, *};

use crate::{runtime_services, runtime_services_mut, utils::hook_service_pointer};

pub unsafe fn init_hooks() {
    ORIG_SET_VARIABLE = hook_service_pointer(
        &mut runtime_services_mut().set_variable as *mut _ as *mut *mut _,
        hook_set_variable as *mut _,
    );

    ORIG_GET_TIME = hook_service_pointer(
        &mut runtime_services_mut().get_time as *mut _ as *mut *mut _,
        hook_get_time as *mut _,
    );
}

pub unsafe fn convert_hook_pointers() {
    (runtime_services().convert_pointer)(0, &mut ORIG_SET_VARIABLE as *mut *const _ as *mut *mut _);
    info!(
        "convert pointer: new_orig_set_variable={:x}",
        ORIG_SET_VARIABLE as usize
    );

    (runtime_services().convert_pointer)(0, &mut ORIG_GET_TIME as *mut *const _ as *mut *mut _);
    info!(
        "convert pointer: new_orig_get_time={:x}",
        ORIG_GET_TIME as usize
    );
}

static mut ORIG_SET_VARIABLE: *const c_void = core::ptr::null_mut();
eficall! {fn hook_set_variable(
    variable_name: *mut crate::base::Char16,
    vendor_guid: *mut crate::base::Guid,
    attributes: u32,
    data_size: usize,
    data: *mut c_void,
) -> crate::base::Status {
    info!("hook_set_variable called2: orig={:x}", unsafe { ORIG_SET_VARIABLE as u64 });
    let orig_func: RuntimeSetVariable = unsafe { core::mem::transmute(ORIG_SET_VARIABLE) };
    (orig_func)(variable_name, vendor_guid, attributes, data_size, data)
}
}

static mut ORIG_GET_TIME: *const c_void = core::ptr::null_mut();
eficall! {fn hook_get_time(
    time: *mut Time,
    capabilities:  *mut TimeCapabilities,
) -> crate::base::Status {
    info!("hook_get_time called: orig={:x}", unsafe { ORIG_GET_TIME as u64 });
        let orig_func: RuntimeGetTime = unsafe { core::mem::transmute(ORIG_GET_TIME) };
    (orig_func)(time, capabilities)
}
}
