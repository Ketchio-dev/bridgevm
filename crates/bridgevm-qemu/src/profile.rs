//! QemuProfile presets and manifest-to-profile/accelerator/machine selection.

use bridgevm_config::VmManifest;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QemuProfile {
    pub accelerator: String,
    pub machine: String,
    pub display: String,
    pub extra_args: Vec<String>,
}

pub(crate) fn machine_for_arch(arch: &str) -> &'static str {
    match arch {
        "arm64" | "aarch64" | "riscv64" => "virt",
        _ => "q35",
    }
}

pub(crate) fn qemu_profile_for_manifest(manifest: &VmManifest) -> QemuProfile {
    if is_windows_11_arm(manifest) {
        QemuProfile::restricted_windows_arm()
    } else {
        QemuProfile::compatibility_default()
    }
}

pub(crate) fn is_windows_11_arm(manifest: &VmManifest) -> bool {
    let os = manifest.guest.os.to_ascii_lowercase();
    let version = manifest
        .guest
        .version
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let arch = manifest.guest.arch.to_ascii_lowercase();
    os == "windows" && version.starts_with("11") && matches!(arch.as_str(), "arm64" | "aarch64")
}

pub(crate) fn accelerator_arg(profile: &QemuProfile) -> &str {
    match profile.accelerator.as_str() {
        "hvf-or-tcg" => "hvf",
        accelerator => accelerator,
    }
}

impl QemuProfile {
    pub fn restricted_windows_arm() -> Self {
        Self {
            accelerator: "hvf".to_string(),
            machine: "virt".to_string(),
            display: "metal-adapter-preferred".to_string(),
            extra_args: vec!["-device".to_string(), "virtio-rng-pci".to_string()],
        }
    }

    pub fn compatibility_default() -> Self {
        Self {
            accelerator: "hvf-or-tcg".to_string(),
            machine: "auto".to_string(),
            display: "spice-or-vnc".to_string(),
            extra_args: Vec::new(),
        }
    }
}
