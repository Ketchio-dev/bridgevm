//! Host file mapping, clock, and serial stop helpers.

use crate::*;
use guest_ram_backing::{hv_vm_allocate, HV_ALLOCATE_DEFAULT};

pub(crate) fn serial_reached_shell(serial: &[u8], scans: &mut SerialStopScans) -> bool {
    scans
        .shell_prompt
        .contains_new(serial, b"UEFI Interactive Shell")
        || scans.shell_short_prompt.contains_new(serial, b"Shell>")
}

/// The firmware found nothing to start: BdsDxe prints "No bootable option or device was found."
/// and waits for a key, so the window shows only its lines (measured with a blank disk). One line
/// tells the app; `loader=other-cpu` when it refused a boot loader as built for another processor
/// (EDK2's "Unsupported": Windows for x64 PCs). A Windows restart starts a new process.
#[derive(Default)]
pub(crate) struct NoBootableDisk {
    none: IncrementalSerialScan,
    other_cpu: IncrementalSerialScan,
}

impl NoBootableDisk {
    pub(crate) fn line(&mut self, serial: &[u8]) -> Option<&'static str> {
        if self.none.found {
            return None;
        }
        let other_cpu = self.other_cpu.contains_new(serial, b".efi: Unsupported");
        if !self.none.contains_new(serial, b"No bootable option or device was found") {
            return None;
        }
        Some(if other_cpu {
            "BVBOOT no-bootable-disk loader=other-cpu"
        } else {
            "BVBOOT no-bootable-disk"
        })
    }
}

pub(crate) fn serial_reached_linux_early_boot(serial: &[u8], scans: &mut SerialStopScans) -> bool {
    scans
        .linux_boot_cpu
        .contains_new(serial, b"Booting Linux on physical CPU")
        || scans.linux_version.contains_new(serial, b"Linux version")
}

pub(crate) fn serial_reached_linux_panic(serial: &[u8], scans: &mut SerialStopScans) -> bool {
    scans.linux_panic.contains_new(serial, b"Kernel panic")
}

pub(crate) fn map_file(path: &Path, ipa: u64, region_bytes: usize, flags: u64) {
    unsafe {
        // Zero pages that stay unbacked until touched, as for guest RAM, and the file read
        // straight in: the aligned alloc_zeroed this replaced wrote all 64 MiB of the code
        // region for a 3 MiB firmware image.
        let mut mem = null_mut();
        assert_eq!(
            hv_vm_allocate(&mut mem, region_bytes, HV_ALLOCATE_DEFAULT),
            0,
            "allocate {}",
            path.display()
        );
        let region = std::slice::from_raw_parts_mut(mem.cast::<u8>(), region_bytes);
        read_file_into(path, region).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        assert_eq!(
            hv_vm_map(mem, ipa, region_bytes, flags),
            0,
            "map {}",
            path.display()
        );
    }
}

extern "C" {
    fn mach_absolute_time() -> u64;
}

/// Host timebase ticks (mach_absolute_time).
pub(crate) fn host_time_ticks() -> u64 {
    unsafe { mach_absolute_time() }
}

/// Read the host's virtual counter (Apple Silicon system counter, ~24 MHz).
pub(crate) fn host_cntvct() -> u64 {
    let v: u64;
    unsafe { std::arch::asm!("mrs {}, cntvct_el0", out(reg) v) };
    v
}

#[cfg(test)]
#[path = "host_support_tests.rs"]
mod tests;
