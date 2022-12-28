// bit mask macros
pub const fn make_bit_mask(a: u32, b: u32) -> u64 {
    (0xffff_ffff_ffff_ffff >> (63 - b)) & !(((1 as u64) << a) - 1)
}

#[macro_export]
macro_rules! get_bit {
    ($a:expr, $b:expr) => {
        ($a & ((1 as u64) << $b)) != 0
    };
}

// page test macros
#[macro_export]
macro_rules! is_large_page {
    ($a:expr) => {
        get_bit!($a, 7)
    };
}

#[macro_export]
macro_rules! is_transition_page {
    ($a:expr) => {
        get_bit!($a, 11)
    };
}

#[macro_export]
macro_rules! is_writeable_page {
    ($a:expr) => {
        get_bit!($a, 1)
    };
}

#[allow(clippy::all)]
macro_rules! is_prototype_page {
    ($a:expr) => {
        get_bit!($a, 10)
    };
}

// TODO: tests
#[macro_export]
macro_rules! check_entry {
    ($a:expr) => {
        get_bit!($a, 0) || (is_transition_page!($a) && !is_prototype_page!($a))
    };
}

// TODO: write tests for these macros
// pagetable indizes
#[macro_export]
macro_rules! pml4_index_bits {
    ($a:expr) => {
        ($a & make_bit_mask(39, 47)) >> 36
    };
}

#[macro_export]
macro_rules! pdpte_index_bits {
    ($a:expr) => {
        ($a & make_bit_mask(30, 38)) >> 27
    };
}

#[macro_export]
macro_rules! pd_index_bits {
    ($a:expr) => {
        ($a & make_bit_mask(21, 29)) >> 18
    };
}

#[macro_export]
macro_rules! pt_index_bits {
    ($a:expr) => {
        ($a & make_bit_mask(12, 20)) >> 9
    };
}

// assume a 4kb page-table page for pt reads
fn read_pt_address(addr: u64) -> u64 {
    unsafe { *(addr as *const u64) }
}

// TODO: return page size
pub fn virt_to_phys(dtb: u64, addr: u64) -> Option<u64> {
    let pml4e = read_pt_address((dtb & make_bit_mask(12, 51)) | pml4_index_bits!(addr));
    if !check_entry!(pml4e) {
        //return Err(Error::new("unable to read pml4e"));
        return None;
    }

    let pdpte = read_pt_address((pml4e & make_bit_mask(12, 51)) | pdpte_index_bits!(addr));
    if !check_entry!(pdpte) {
        //return Err(Error::new("unable to read pdpte"));
        return None;
    }

    if is_large_page!(pdpte) {
        //trace!("found 1gb page");
        let phys_addr = (pdpte & make_bit_mask(30, 51)) | (addr & make_bit_mask(0, 29));
        //let page_size = Length::from_gb(1);
        return Some(phys_addr);
    }

    let pgd = read_pt_address((pdpte & make_bit_mask(12, 51)) | pd_index_bits!(addr));
    if !check_entry!(pgd) {
        //return Err(Error::new("unable to read pgd"));
        return None;
    }

    if is_large_page!(pgd) {
        //trace!("found 2mb page");
        let phys_addr = (pgd & make_bit_mask(21, 51)) | (addr & make_bit_mask(0, 20));
        //let page_size = Length::from_mb(2);
        return Some(phys_addr);
    }

    let pte = read_pt_address((pgd & make_bit_mask(12, 51)) | pt_index_bits!(addr));
    if !check_entry!(pte) {
        //return Err(Error::new("unable to read pte"));
        return None;
    }

    //trace!("found 4kb page");
    let phys_addr = (pte & make_bit_mask(12, 51)) | (addr & make_bit_mask(0, 11));
    //let page_size = Length::from_kb(4);
    Some(phys_addr)
}

fn as_page_aligned(val: u64) -> u64 {
    val - val % 0x1000
}
