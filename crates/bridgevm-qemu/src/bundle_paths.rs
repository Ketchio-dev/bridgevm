//! Per-bundle socket and firmware file locations, and bundle-relative resolution.

use std::path::Path;
use std::path::PathBuf;

pub fn qmp_socket_path(bundle_path: &Path) -> PathBuf {
    bundle_path.join("metadata").join("qmp.sock")
}

pub fn guest_tools_socket_path(bundle_path: &Path) -> PathBuf {
    bundle_path.join("metadata").join("guest-tools.sock")
}

/// Socket an external `swtpm` process must listen on for the emulated TPM 2.0
/// (`-tpmdev emulator,...,chardev`). Per-bundle so concurrent VMs don't collide.
pub fn swtpm_socket_path(bundle_path: &Path) -> PathBuf {
    bundle_path.join("metadata").join("swtpm.sock")
}

/// Per-bundle writable edk2 UEFI variable store used when Secure Boot is enabled
/// (the `if=pflash,unit=1` device). Must be seeded from an edk2 secure-boot vars
/// template with Microsoft keys enrolled before first boot.
pub fn secure_boot_vars_path(bundle_path: &Path) -> PathBuf {
    bundle_path.join("metadata").join("edk2-vars.fd")
}

pub(crate) fn resolve_bundle_path(bundle_path: &Path, relative_or_absolute: &str) -> PathBuf {
    let path = PathBuf::from(relative_or_absolute);
    if path.is_absolute() {
        path
    } else {
        bundle_path.join(path)
    }
}
