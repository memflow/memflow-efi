use core::ffi::c_void;

use ::r_efi::{system::*, *};

use crate::{runtime_services, runtime_services_mut, utils::hook_service_pointer, EFI_MEM_MAPS};

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
    let prev_set_variable = &mut ORIG_SET_VARIABLE as *mut *const _ as usize;
    (runtime_services().convert_pointer)(0, &mut ORIG_SET_VARIABLE as *mut *const _ as *mut *mut _);
    info!(
        "converting ORIG_SET_VARIABLE pointer: prev={:x}; new={:x}",
        prev_set_variable, &mut ORIG_SET_VARIABLE as *mut *const _ as usize
    );

    let prev_get_time = &mut ORIG_GET_TIME as *mut *const _ as usize;
    (runtime_services().convert_pointer)(0, &mut ORIG_GET_TIME as *mut *const _ as *mut *mut _);
    info!(
        "converting ORIG_GET_TIME pointer: prev={:x}; new={:x}",
        prev_get_time, &mut ORIG_GET_TIME as *mut *const _ as usize
    );
}

#[repr(C)]
pub struct MemflowCommand {
    magic: u32,
    src: *const c_void,
    dst: *mut c_void,
    len: usize,
}
const _: [(); core::mem::size_of::<MemflowCommand>()] = [(); 32];

static mut ORIG_SET_VARIABLE: *const c_void = core::ptr::null_mut();
eficall! {fn hook_set_variable(
    variable_name: *mut crate::base::Char16,
    vendor_guid: *mut crate::base::Guid,
    attributes: u32,
    data_size: usize,
    data: *mut c_void,
) -> crate::base::Status {
    //info!("hook_set_variable called: orig={:x}", unsafe { ORIG_SET_VARIABLE as u64 });

    if !variable_name.is_null() && !vendor_guid.is_null() && data_size > 0 && !data.is_null() {
        // compare guid
        // TODO: generate random guid at compile time
        let guid = unsafe { &*vendor_guid };
        let target_guid = "cZ53x7dyxAVJRD19";
        if guid.as_bytes() == target_guid.as_bytes() {
            let mfcmd = unsafe { &*(data as *mut MemflowCommand) };
            if mfcmd.magic == 0x2b54a004 && !mfcmd.src.is_null() && !mfcmd.dst.is_null() && mfcmd.len > 0 {
                unsafe { core::ptr::write_bytes(mfcmd.dst, 0, mfcmd.len) };
                //unsafe { core::ptr::copy_nonoverlapping(mfcmd.src, mfcmd.dst, mfcmd.len) };
                //let test = alloc::boxed::Box::new(133742usize);
                //return efi::Status::from_usize(*test.as_ref());

                // test iteration
                let num_mem_maps = unsafe { (&*EFI_MEM_MAPS.as_ptr()).len() };
               unsafe { *(mfcmd.dst as *mut usize) = num_mem_maps };
                return efi::Status::SUCCESS;
            } else {
                return efi::Status::INVALID_PARAMETER;
            }
        }
    }

    let orig_func: RuntimeSetVariable = unsafe { core::mem::transmute(ORIG_SET_VARIABLE) };
    (orig_func)(variable_name, vendor_guid, attributes, data_size, data)
}
}

static mut ORIG_GET_TIME: *const c_void = core::ptr::null_mut();
eficall! {fn hook_get_time(
    time: *mut Time,
    capabilities:  *mut TimeCapabilities,
) -> crate::base::Status {
    //info!("hook_get_time called: orig={:x}", unsafe { ORIG_GET_TIME as u64 });
    let orig_func: RuntimeGetTime = unsafe { core::mem::transmute(ORIG_GET_TIME) };
    (orig_func)(time, capabilities)
}
}
