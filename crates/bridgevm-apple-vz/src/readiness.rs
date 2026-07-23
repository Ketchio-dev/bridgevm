//! Host capability probe and the readiness blockers that gate a live launch.

use crate::*;
use bridgevm_config::BootMode;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AppleVzHostCapability {
    pub(crate) os: String,
    pub(crate) arch: String,
}

pub(crate) fn build_readiness_spec(
    boot: &AppleVzBootSpec,
    disk_path: &str,
    disk_format: &str,
    host: &AppleVzHostCapability,
) -> AppleVzReadinessSpec {
    let mut blockers = Vec::new();
    append_host_readiness_blockers(&mut blockers, host);
    append_current_runner_readiness_blockers(&mut blockers, boot, disk_format);

    let disk = Path::new(disk_path);
    if !disk.exists() {
        blockers.push(AppleVzReadinessBlocker {
            code: "missing-primary-disk".to_string(),
            message: "Primary disk is missing; prepare or create the disk before Fast Mode launch."
                .to_string(),
            path: Some(disk_path.to_string()),
            capability: None,
        });
    }

    for (code, label, media) in [
        (
            "missing-installer-image",
            "Installer image",
            boot.installer_image.as_ref(),
        ),
        ("missing-kernel", "Kernel", boot.kernel.as_ref()),
        ("missing-initrd", "Initrd", boot.initrd.as_ref()),
        (
            "missing-macos-restore-image",
            "macOS restore image",
            boot.macos_restore_image.as_ref(),
        ),
    ] {
        if let Some(media) = media {
            if !media.exists {
                blockers.push(AppleVzReadinessBlocker {
                    code: code.to_string(),
                    message: format!(
                        "{label} is missing; import, verify, or download boot media before launch."
                    ),
                    path: Some(media.path.clone()),
                    capability: None,
                });
            }
        }
    }

    AppleVzReadinessSpec {
        ready: blockers.is_empty(),
        blockers,
    }
}

pub(crate) fn append_current_runner_readiness_blockers(
    blockers: &mut Vec<AppleVzReadinessBlocker>,
    boot: &AppleVzBootSpec,
    disk_format: &str,
) {
    if boot.mode != BootMode::LinuxKernel {
        blockers.push(AppleVzReadinessBlocker {
            code: "unsupported-live-boot-mode".to_string(),
            message: format!(
                "Current AppleVzRunner live launch supports linux-kernel boot only; this plan uses {}.",
                boot.mode
            ),
            path: None,
            capability: Some("apple-vz-runner".to_string()),
        });
    }

    if disk_format != "raw" {
        blockers.push(AppleVzReadinessBlocker {
            code: "unsupported-live-disk-format".to_string(),
            message: format!(
                "Current AppleVzRunner live launch supports raw primary disks only; this plan uses {disk_format}."
            ),
            path: None,
            capability: Some("apple-vz-runner".to_string()),
        });
    }
}

pub(crate) fn append_host_readiness_blockers(
    blockers: &mut Vec<AppleVzReadinessBlocker>,
    host: &AppleVzHostCapability,
) {
    if !host.is_macos() {
        blockers.push(AppleVzReadinessBlocker {
            code: "unsupported-host-os".to_string(),
            message: format!(
                "Apple Virtualization Fast Mode launch requires macOS; current host reports {}.",
                host.os
            ),
            path: None,
            capability: Some("apple-virtualization-framework".to_string()),
        });
    }

    if !host.is_apple_silicon() {
        blockers.push(AppleVzReadinessBlocker {
            code: "unsupported-host-arch".to_string(),
            message: format!(
                "Apple Virtualization Fast Mode launch requires Apple Silicon; current host arch is {}.",
                host.arch
            ),
            path: None,
            capability: Some("apple-silicon".to_string()),
        });
    }
}

impl AppleVzHostCapability {
    pub(crate) fn current() -> Self {
        Self {
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
        }
    }

    pub(crate) fn is_macos(&self) -> bool {
        self.os == "macos"
    }

    pub(crate) fn is_apple_silicon(&self) -> bool {
        matches!(self.arch.as_str(), "aarch64" | "arm64")
    }
}
