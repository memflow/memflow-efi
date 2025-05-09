use core::{convert::identity, ffi::c_void};

use ::r_efi::{system::*, *};
use x86_64::{
    registers::control::{Cr3, Cr3Flags},
    structures::paging::{PhysFrame, Size4KiB},
    PhysAddr,
};

use crate::{
    runtime_services, runtime_services_mut, utils::hook_service_pointer, vtop::virt_to_phys,
    EFI_MEM_MAPS, IDENTITY_CR3, IDENTITY_PAGE_TABLE, IDENTITY_PAGE_TABLE_BASE,
};

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

static mut KERNEL_MAPPED: u8 = 0;

static mut VAR_CALLED: usize = 0;

static mut ORIG_SET_VARIABLE: *const c_void = core::ptr::null_mut();
eficall! {fn hook_set_variable(
    variable_name: *mut crate::base::Char16,
    vendor_guid: *mut crate::base::Guid,
    attributes: u32,
    data_size: usize,
    data: *mut c_void,
) -> crate::base::Status {
    let var_called = unsafe { &mut VAR_CALLED };
    *var_called += 1;
    //info!("hook_set_variable called: orig={:x} cnt={var_called}", unsafe { ORIG_SET_VARIABLE as u64 });
    //

    if !variable_name.is_null() && !vendor_guid.is_null() && data_size > 0 && !data.is_null() {
        // compare guid
        // TODO: generate random guid at compile time
        let guid = unsafe { &*vendor_guid };
        let target_guid = "cZ53x7dyxAVJRD19";
        if guid.as_bytes() == target_guid.as_bytes() {

            let mfcmd = unsafe { &*(data as *mut MemflowCommand) };
            if mfcmd.magic == 0x2b54a004 && !mfcmd.src.is_null() && !mfcmd.dst.is_null() && mfcmd.len > 0 {

                let old_dtb = Cr3::read();

                let dtb = unsafe { IDENTITY_PAGE_TABLE_BASE };
                let dtb = unsafe{ PhysFrame::<Size4KiB>::from_start_address_unchecked(PhysAddr::new(dtb)) };

                return x86_64::instructions::interrupts::without_interrupts(|| {
                    if unsafe { KERNEL_MAPPED == 0 } {
                        unsafe {
                            core::arch::asm!(
                                // Write new dtb
                                "mov cr3, rdi",
                                // Copy kernel pages to our mapping
                                "add rdi, 2048",
                                "add rsi, 2048",
                                "rep movsq",
                                // Reset RDI to original value
                                "sub rdi, 4096",
                                // Flush TLB
                                "mov cr3, rdi",
                                // Explicit registers because movsq moves from rsi to rdi
                                inout("rdi") dtb.start_address().as_u64() => _,
                                // These registers may be clobbered upon copy
                                inout("rsi") old_dtb.0.start_address().as_u64() => _,
                                inout("rcx") 256 => _,
                            );

                            KERNEL_MAPPED = 1;
                            debug!("First time mapping");
                        }
                    } else {
                        unsafe { Cr3::write(dtb, Cr3Flags::empty()) };
                    }

                    // open a new scope so we can be sure everything is dropped by the time we swap cr3 again
                    let mut result = efi::Status::ACCESS_DENIED;
                    {
                        // Map user buffer into a free memory range
                        debug!("Identity mapping {:x}", mfcmd.dst as usize);
                        let identity = unsafe { &mut IDENTITY_PAGE_TABLE };
                        let mapping = identity.remap_range(mfcmd.dst as usize, mfcmd.len, old_dtb.0); // TODO: crashy

                        if let Some((handle, remapped_dst)) = mapping {
                            //let remapped_dst = mfcmd.dst;
                            debug!("Identity mapped {remapped_dst:x}");

                            // Fully flush TLB again now that we mapped the buffer in
                            unsafe {
                                Cr3::write(dtb, Cr3Flags::empty());
                            }

                            // iterate buffer page by page
                            let mem_maps = unsafe { &EFI_MEM_MAPS };
                            let mut offs = 0usize;
                            while offs < mfcmd.len {
                                let addr = mfcmd.src as usize + offs;
                                let addr_end = ((addr + 0x1000) - (addr + 0x1000) % 0x1000).min(mfcmd.src as usize + mfcmd.len);
                                let addr_align = addr - addr % 0x1000;
                                let len_align = addr_end - addr; // FB for first chunk

                                //trace!("Try Copy {addr_align:x}");

                                // check if 'src' is a valid physical memory region
                                if mem_maps.is_mapped(addr_align as u64) {
                                    //unsafe { core::ptr::copy_nonoverlapping(addr as *mut u8, (mfcmd.dst as usize + offs) as *mut u8, len_align) };

                                    //trace!("Copy {:x}", addr);

                                    unsafe { core::ptr::write_bytes((remapped_dst as usize + offs) as *mut u8, 2, len_align) };

                                    // unsafe { core::ptr::copy_nonoverlapping(global_buffer_addr, (mfcmd.dst as usize + offs) as *mut u8, len_align) };

                                    result = efi::Status::SUCCESS;
                                } else {
                                    // TODO: unneeded, buffers are 0-filled anyways
                                    //unsafe { core::ptr::write_bytes((remapped_dst + offs) as *mut u8, 0, len_align) };
                                }

                                offs += len_align;
                            }
                        }
                    }

                    unsafe { Cr3::write(old_dtb.0, old_dtb.1) };

                    return result;
                });
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
