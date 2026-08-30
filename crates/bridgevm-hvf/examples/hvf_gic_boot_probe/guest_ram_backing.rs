//! Hypervisor-owned backing for the primary guest RAM region.

use crate::*;

const HV_ALLOCATE_DEFAULT: u64 = 0;

#[link(name = "Hypervisor", kind = "framework")]
extern "C" {
    fn hv_vm_allocate(address: *mut *mut c_void, size: usize, flags: u64) -> HvReturn;
    fn hv_vm_deallocate(address: *mut c_void, size: usize) -> HvReturn;
}

pub(crate) struct GuestRamBacking {
    pointer: *mut u8,
    ipa: u64,
    len: usize,
}

impl GuestRamBacking {
    pub(crate) unsafe fn allocate_and_map(ipa: u64, len: usize) -> Self {
        assert_ne!(len, 0, "guest RAM must not be empty");
        let mut address = null_mut();
        let status = hv_vm_allocate(&mut address, len, HV_ALLOCATE_DEFAULT);
        assert_eq!(status, 0, "hv_vm_allocate guest RAM");
        assert!(!address.is_null(), "hv_vm_allocate returned a null address");
        let status = hv_vm_map(
            address,
            ipa,
            len,
            HV_MEMORY_READ | HV_MEMORY_WRITE | HV_MEMORY_EXEC,
        );
        if status != 0 {
            let deallocate_status = hv_vm_deallocate(address, len);
            assert_eq!(deallocate_status, 0, "deallocate guest RAM after map failure");
            panic!("hv_vm_map guest RAM failed: {status:#x}");
        }
        Self {
            pointer: address.cast(),
            ipa,
            len,
        }
    }

    pub(crate) fn pointer(&self) -> *mut u8 {
        self.pointer
    }
}

pub(crate) unsafe fn zero_mapped_memory(pointer: *mut u8, len: usize) {
    // SAFETY: the caller supplies the live guest-RAM allocation and its exact
    // mapped length. MADV_ZERO preserves the mapping while logically zeroing
    // pages without first faulting every page into the process.
    let page_aligned = (pointer as usize & 0x3fff) == 0 && (len & 0x3fff) == 0;
    let status = if page_aligned {
        unsafe { libc::madvise(pointer.cast(), len, libc::MADV_ZERO) }
    } else {
        -1
    };
    if status != 0 {
        // Older macOS releases can reject MADV_ZERO. Preserve the established
        // full-zero behavior instead of weakening reset isolation.
        // SAFETY: the same live allocation and checked length apply here.
        unsafe { std::ptr::write_bytes(pointer, 0, len) };
    }
}

impl Drop for GuestRamBacking {
    fn drop(&mut self) {
        unsafe {
            let unmap_status = hv_vm_unmap(self.ipa, self.len);
            if unmap_status != 0 {
                eprintln!("hv_vm_unmap guest RAM failed during teardown: {unmap_status:#x}");
                return;
            }
            let deallocate_status = hv_vm_deallocate(self.pointer.cast(), self.len);
            if deallocate_status != 0 {
                eprintln!(
                    "hv_vm_deallocate guest RAM failed during teardown: {deallocate_status:#x}"
                );
            }
        }
    }
}
