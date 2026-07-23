//! Manifest-level gate on arch, backend, network, disk format and guest OS.

use crate::*;
use bridgevm_config::VmManifest;

pub(crate) fn preflight_apple_vz_launch(manifest: &VmManifest) -> Result<(), AppleVzError> {
    let guest_arch = manifest.guest.arch.to_ascii_lowercase();
    if !matches!(guest_arch.as_str(), "arm64" | "aarch64") {
        return Err(AppleVzError::UnsupportedGuestArch(
            manifest.guest.arch.clone(),
        ));
    }

    if let Some(preferred) = &manifest.backend.preferred {
        if preferred != "apple-vz" {
            return Err(AppleVzError::UnsupportedPreferredBackend(preferred.clone()));
        }
    }

    let _network_plan = apple_vz_network_plan(manifest)?;

    if !matches!(manifest.storage.primary.format.as_str(), "raw" | "qcow2") {
        return Err(AppleVzError::UnsupportedPrimaryDiskFormat(
            manifest.storage.primary.format.clone(),
        ));
    }

    let guest_os = manifest.guest.os.to_ascii_lowercase();
    if !matches!(
        guest_os.as_str(),
        "ubuntu" | "fedora" | "debian" | "linux" | "macos"
    ) {
        return Err(AppleVzError::UnsupportedGuestOs(manifest.guest.os.clone()));
    }

    validate_boot(manifest.boot.as_ref(), &guest_os)?;

    Ok(())
}
