//! Host file mapping, clock, and serial stop helpers.

use crate::*;

pub(crate) fn serial_reached_shell(serial: &[u8], scans: &mut SerialStopScans) -> bool {
    scans
        .shell_prompt
        .contains_new(serial, b"UEFI Interactive Shell")
        || scans.shell_short_prompt.contains_new(serial, b"Shell>")
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
    let data = read_bounded_file(path, region_bytes)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let layout = Layout::from_size_align(region_bytes, 0x1_0000).unwrap();
    unsafe {
        let mem = alloc_zeroed(layout);
        std::ptr::copy_nonoverlapping(data.as_ptr(), mem, data.len());
        assert_eq!(
            hv_vm_map(mem as *mut c_void, ipa, region_bytes, flags),
            0,
            "map {}",
            path.display()
        );
    }
}

/// Read the host's virtual counter (Apple Silicon system counter, ~24 MHz).
pub(crate) fn host_cntvct() -> u64 {
    let v: u64;
    unsafe { std::arch::asm!("mrs {}, cntvct_el0", out(reg) v) };
    v
}
