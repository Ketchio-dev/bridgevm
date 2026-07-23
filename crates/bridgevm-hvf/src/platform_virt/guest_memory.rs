//! Host-side flat guest-RAM view and the firmware/DTB/RAM address-space layout.

use super::*;
use crate::fwcfg::GuestMemoryMut;
use crate::machine;
use crate::machine::Region;

/// Where the firmware, device tree and RAM live in the guest address space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuestMemoryLayout {
    /// pflash bank 0 — firmware code (read-only).
    pub flash_code: Region,
    /// pflash bank 1 — UEFI variable store (writable).
    pub flash_vars: Region,
    /// System RAM.
    pub ram: Region,
    /// Address the flattened device tree is loaded at (inside RAM).
    pub dtb_load: u64,
}

/// A flat span of guest RAM implementing [`GuestMemoryMut`]. In live use the run
/// loop supplies a view over the HVF-mapped guest memory instead; this is the
/// host-side stand-in used for tests and offline pipeline exercises.
#[derive(Debug)]
pub struct FlatGuestRam {
    pub(crate) base: u64,
    pub(crate) bytes: Vec<u8>,
}

impl VirtPlatform {
    /// The guest memory layout. The DTB is placed at the base of RAM, where the
    /// firmware looks for it; the kernel/initrd are loaded above it.
    pub fn memory_layout(&self) -> GuestMemoryLayout {
        GuestMemoryLayout {
            flash_code: machine::FLASH_CODE,
            flash_vars: machine::FLASH_VARS,
            ram: Region::new(machine::RAM_BASE, self.cfg.ram_size),
            dtb_load: machine::RAM_BASE,
        }
    }
}

impl FlatGuestRam {
    pub fn new(base: u64, len: usize) -> Self {
        Self {
            base,
            bytes: vec![0u8; len],
        }
    }

    pub(crate) fn offset(&self, gpa: u64) -> Option<usize> {
        gpa.checked_sub(self.base)
            .and_then(|value| usize::try_from(value).ok())
    }
}

impl GuestMemoryMut for FlatGuestRam {
    fn write_bytes(&mut self, gpa: u64, data: &[u8]) -> bool {
        let Some(start) = self.offset(gpa) else {
            return false;
        };
        let Some(end) = start.checked_add(data.len()) else {
            return false;
        };
        if end > self.bytes.len() {
            return false;
        }
        self.bytes[start..end].copy_from_slice(data);
        true
    }

    fn read_bytes(&self, gpa: u64, len: usize) -> Option<Vec<u8>> {
        let start = self.offset(gpa)?;
        let end = start.checked_add(len)?;
        if end > self.bytes.len() {
            return None;
        }
        Some(self.bytes[start..end].to_vec())
    }

    fn read_into(&self, gpa: u64, dst: &mut [u8]) -> bool {
        let Some(start) = self.offset(gpa) else {
            return false;
        };
        let Some(end) = start.checked_add(dst.len()) else {
            return false;
        };
        if end > self.bytes.len() {
            return false;
        }
        dst.copy_from_slice(&self.bytes[start..end]);
        true
    }

    fn host_ptr(&self, gpa: u64, len: usize) -> Option<*mut u8> {
        let start = self.offset(gpa)?;
        let end = start.checked_add(len)?;
        if end > self.bytes.len() {
            return None;
        }
        Some(self.bytes.as_ptr().wrapping_add(start) as *mut u8)
    }
}
