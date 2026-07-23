//! Boot-mode validation for Apple VZ guests and boot spec construction.

use crate::*;
use bridgevm_config::Boot;
use bridgevm_config::BootMode;
use bridgevm_config::VmManifest;
use std::path::Path;

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
