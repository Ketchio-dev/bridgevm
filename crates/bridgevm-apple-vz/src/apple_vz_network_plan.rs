//! Split out of lib.rs to keep files under 800 lines.

use crate::*;
use bridgevm_config::Boot;
use bridgevm_config::BootMode;
use bridgevm_config::VmManifest;
use bridgevm_network::plan_network;
use bridgevm_network::NetworkBackend;
use bridgevm_network::NetworkMode;
use bridgevm_network::NetworkPlan;
use bridgevm_network::NetworkPlanError;
use bridgevm_network::PortForwardRule;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;

pub(crate) fn apple_vz_network_plan(manifest: &VmManifest) -> Result<NetworkPlan, AppleVzError> {
    let mode = NetworkMode::from_str(&manifest.network.mode)
        .map_err(|_| AppleVzError::UnsupportedNetworkMode(manifest.network.mode.clone()))?;
    let port_forwards = manifest
        .network
        .forwards
        .iter()
        .map(|forward| PortForwardRule {
            host: forward.host,
            guest: forward.guest,
        })
        .collect();
    let plan = plan_network(
        NetworkBackend::AppleVz,
        mode,
        manifest.network.hostname.clone(),
        port_forwards,
    )
    .map_err(|error| match error {
        NetworkPlanError::UnsupportedMode { mode, .. }
        | NetworkPlanError::UnsupportedPortForwarding { mode } => {
            AppleVzError::UnsupportedNetworkMode(mode.to_string())
        }
        other => AppleVzError::NetworkPlan(other),
    })?;

    if plan.mode != NetworkMode::Nat {
        return Err(AppleVzError::UnsupportedNetworkMode(plan.mode.to_string()));
    }

    Ok(plan)
}

pub(crate) fn validate_boot(boot: Option<&Boot>, guest_os: &str) -> Result<(), AppleVzError> {
    let Some(boot) = boot else {
        return Ok(());
    };
    let mode = boot.mode;
    for (field, value) in [
        ("installerImage", boot.installer_image.as_deref()),
        ("kernelPath", boot.kernel_path.as_deref()),
        ("initrdPath", boot.initrd_path.as_deref()),
        ("kernelCommandLine", boot.kernel_command_line.as_deref()),
        ("macosRestoreImage", boot.macos_restore_image.as_deref()),
    ] {
        if value.is_some_and(|value| value.trim().is_empty()) {
            return Err(AppleVzError::EmptyBootInput { field });
        }
    }

    let linux_guest = matches!(guest_os, "ubuntu" | "fedora" | "debian" | "linux");
    match mode {
        BootMode::ExistingDisk if boot.macos_restore_image.is_some() => {
            Err(AppleVzError::UnsupportedBootInput {
                mode,
                field: "macosRestoreImage",
            })
        }
        BootMode::ExistingDisk if boot.installer_image.is_some() => {
            Err(AppleVzError::UnsupportedBootInput {
                mode,
                field: "installerImage",
            })
        }
        BootMode::ExistingDisk if boot.kernel_path.is_some() => {
            Err(AppleVzError::UnsupportedBootInput {
                mode,
                field: "kernelPath",
            })
        }
        BootMode::ExistingDisk => Ok(()),
        BootMode::LinuxKernel if !linux_guest => Err(AppleVzError::InvalidBootModeForGuest {
            guest_os: guest_os.to_string(),
            mode,
        }),
        BootMode::LinuxKernel if boot.kernel_path.is_none() => {
            Err(AppleVzError::MissingBootInput {
                mode,
                field: "kernelPath",
            })
        }
        BootMode::LinuxKernel if boot.installer_image.is_some() => {
            Err(AppleVzError::UnsupportedBootInput {
                mode,
                field: "installerImage",
            })
        }
        BootMode::LinuxKernel if boot.macos_restore_image.is_some() => {
            Err(AppleVzError::UnsupportedBootInput {
                mode,
                field: "macosRestoreImage",
            })
        }
        BootMode::LinuxKernel => Ok(()),
        BootMode::LinuxInstaller if !linux_guest => Err(AppleVzError::InvalidBootModeForGuest {
            guest_os: guest_os.to_string(),
            mode,
        }),
        BootMode::LinuxInstaller if boot.installer_image.is_none() => {
            Err(AppleVzError::MissingBootInput {
                mode,
                field: "installerImage",
            })
        }
        BootMode::LinuxInstaller if boot.kernel_path.is_some() => {
            Err(AppleVzError::UnsupportedBootInput {
                mode,
                field: "kernelPath",
            })
        }
        BootMode::LinuxInstaller if boot.macos_restore_image.is_some() => {
            Err(AppleVzError::UnsupportedBootInput {
                mode,
                field: "macosRestoreImage",
            })
        }
        BootMode::LinuxInstaller => Ok(()),
        // Apple VZ (Fast Mode) cannot run Windows guests; windows-installer is a
        // Compatibility Mode (QEMU) boot mode only.
        BootMode::WindowsInstaller => Err(AppleVzError::InvalidBootModeForGuest {
            guest_os: guest_os.to_string(),
            mode,
        }),
        BootMode::MacosRestore if guest_os != "macos" => {
            Err(AppleVzError::InvalidBootModeForGuest {
                guest_os: guest_os.to_string(),
                mode,
            })
        }
        BootMode::MacosRestore if boot.macos_restore_image.is_none() => {
            Err(AppleVzError::MissingBootInput {
                mode,
                field: "macosRestoreImage",
            })
        }
        BootMode::MacosRestore if boot.installer_image.is_some() => {
            Err(AppleVzError::UnsupportedBootInput {
                mode,
                field: "installerImage",
            })
        }
        BootMode::MacosRestore if boot.kernel_path.is_some() => {
            Err(AppleVzError::UnsupportedBootInput {
                mode,
                field: "kernelPath",
            })
        }
        BootMode::MacosRestore => Ok(()),
    }
}

pub(crate) fn build_boot_spec(
    manifest: &VmManifest,
    bundle_path: &Path,
) -> Result<AppleVzBootSpec, AppleVzError> {
    let Some(boot) = manifest.boot.as_ref() else {
        return Ok(AppleVzBootSpec {
            mode: BootMode::ExistingDisk,
            installer_image: None,
            kernel: None,
            initrd: None,
            kernel_command_line: None,
            macos_restore_image: None,
        });
    };
    validate_boot(Some(boot), &manifest.guest.os.to_ascii_lowercase())?;
    Ok(AppleVzBootSpec {
        mode: boot.mode,
        installer_image: boot
            .installer_image
            .as_deref()
            .map(|path| resolved_path_spec(bundle_path, path)),
        kernel: boot
            .kernel_path
            .as_deref()
            .map(|path| resolved_path_spec(bundle_path, path)),
        initrd: boot
            .initrd_path
            .as_deref()
            .map(|path| resolved_path_spec(bundle_path, path)),
        kernel_command_line: boot.kernel_command_line.clone(),
        macos_restore_image: boot
            .macos_restore_image
            .as_deref()
            .map(|path| resolved_path_spec(bundle_path, path)),
    })
}

pub(crate) fn resolved_path_spec(
    bundle_path: &Path,
    relative_or_absolute: &str,
) -> AppleVzPathSpec {
    let path = resolve_bundle_path(bundle_path, relative_or_absolute);
    AppleVzPathSpec {
        exists: path.exists(),
        path: path.display().to_string(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AppleVzHostCapability {
    pub(crate) os: String,
    pub(crate) arch: String,
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

pub(crate) fn resolve_bundle_path(bundle_path: &Path, relative_or_absolute: &str) -> PathBuf {
    let path = PathBuf::from(relative_or_absolute);
    if path.is_absolute() {
        path
    } else {
        bundle_path.join(path)
    }
}
