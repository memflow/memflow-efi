use core::ffi::c_void;

use ::r_efi::system::{RuntimeSetVariable, TPL_HIGH_LEVEL};

use crate::{boot_services, error, system_table_mut};

static mut GDB_ATTACHED: bool = false;

pub fn wait_for_debugger() {
    unsafe {
        while !GDB_ATTACHED {
            core::arch::asm!("pause");
        }
    }
}

#[lang = "eh_personality"]
fn eh_personality() {}

#[panic_handler]
fn panic_handler(info: &core::panic::PanicInfo) -> ! {
    if let Some(location) = info.location() {
        error!(
            "[-] Panic in {} at ({}, {}):",
            location.file(),
            location.line(),
            location.column()
        );
        if let Some(message) = info.message() {
            error!("[-] {}", message);
        }
    }

    loop {}
}

pub fn hook_service_pointer(orig_func: *mut *mut c_void, hook_func: *mut c_void) -> *mut c_void {
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
            "recomputing crc32 of system_table: old_crc32={:x} new_crc32={:x}",
            prev_crc32, system_table_header.crc32
        );
    }

    (boot_services().restore_tpl)(orig_tpl);

    orig_func_bak
}

pub struct Mutex<T> {
    lock: AtomicBool,
    inner: T,
}

impl<T> Mutex<T> {
    pub const fn new(value: T) -> Self {
        Self {
            lock: AtomicBool::new(false),
            inner: value,
        }
    }

    pub fn lock<'a>(&'a mut self) -> MutexGuard<'a, T> {
        while !self.lock.compare_and_swap(false, true, Ordering::SeqCst) {
            // TODO: yield thread?
        }
        MutexGuard { parent: self }
    }
}

pub struct MutexGuard<'a, T> {
    parent: &'a mut Mutex<T>,
}

impl<'a, T> Deref for MutexGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.parent.inner
    }
}

impl<'a, T> DerefMut for MutexGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.parent.inner
    }
}

impl<'a, T> Drop for MutexGuard<'a, T> {
    fn drop(&mut self) {
        self.parent.lock.store(false, Ordering::SeqCst);
    }
}
