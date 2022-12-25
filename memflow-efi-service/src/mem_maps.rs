use ::alloc::{string::String, vec::Vec};
use ::r_efi::{
    protocols::file::ProtocolOpen,
    protocols::*,
    system::MemoryDescriptor,
    system::{RuntimeSetVariable, TPL_HIGH_LEVEL},
    *,
};
use alloc::format;

/// Reads and stores the memory mappings returned by EFI boot services
pub struct EfiMemMaps {
    mem_maps: Vec<MemoryDescriptor>,
}

impl EfiMemMaps {
    pub fn load(boot_services: &efi::BootServices) -> Result<Self, String> {
        let mut tmp_mem_maps = [0u8; 1];
        let mut mem_maps_size = 0usize;
        let mut map_key = 0usize;
        let mut descriptor_size = 0usize;
        let mut descriptor_version = 0u32;

        // retrieve mapping size by calling get_memory_map with a size of 1
        let status = (boot_services.get_memory_map)(
            &mut mem_maps_size as *mut _,
            &mut tmp_mem_maps as *mut _ as *mut _,
            &mut map_key as *mut _,
            &mut descriptor_size as *mut _,
            &mut descriptor_version as *mut _,
        );
        if status != efi::Status::BUFFER_TOO_SMALL {
            return Err(format!(
                "get_memory_map returned status `{:?}` but `{:?}` was expected",
                status,
                efi::Status::BUFFER_TOO_SMALL
            ));
        }

        mem_maps_size += 0x1000; // #define EFI_PAGE_SIZE SIZE_4KB
        info!(
            "get_memory_map requires a buffer size of {:x} bytes",
            mem_maps_size
        );

        // allocate required buffer and convert it into a slice
        let mut mem_maps_ptr = core::ptr::null_mut();
        let status = (boot_services.allocate_pool)(
            2, /* EfiLoaderData */
            mem_maps_size,
            &mut mem_maps_ptr as *mut *mut _ as *mut *mut _,
        );
        if status != efi::Status::SUCCESS {
            return Err(format!("allocate_pool failed with status: `{:?}`", status));
        }

        // retrieve final memory mappings
        let status = (boot_services.get_memory_map)(
            &mut mem_maps_size as *mut _,
            mem_maps_ptr as *mut _,
            &mut map_key as *mut _,
            &mut descriptor_size as *mut _,
            &mut descriptor_version as *mut _,
        );
        if status != efi::Status::SUCCESS {
            return Err(format!("get_memory_map failed with status: `{:?}`", status));
        }

        // convert this oddity in a regular rust slice
        let mut mem_maps = Vec::new();

        let num_mem_maps = mem_maps_size / descriptor_size;
        info!("found a total of {} mem_maps:", num_mem_maps);
        let mut mem_map: &mut MemoryDescriptor = unsafe { core::mem::transmute(mem_maps_ptr) };
        for i in 0..num_mem_maps {
            info!(
                "map: type={:x}; vstart={:x}; pstart={:x}, pagecnt={:x}",
                mem_map.r#type,
                mem_map.virtual_start,
                mem_map.physical_start,
                mem_map.number_of_pages
            );
            mem_maps.push(mem_map.clone());

            mem_map = unsafe {
                core::mem::transmute((mem_map as *mut _ as usize + descriptor_size) as *mut u8)
            };
        }

        let status = (boot_services.free_pool)(mem_maps_ptr);
        if status != efi::Status::SUCCESS {
            return Err(format!("free_pool failed with status: `{:?}`", status));
        }

        Ok(Self { mem_maps })
    }

    pub fn len(&self) -> usize {
        self.mem_maps.len()
    }
}
