//! Semantic validation: schema, name, bundle-relative paths, boot inputs, shared folders.

use crate::*;
use std::collections::BTreeSet;
use std::path::Path;

pub(crate) fn validate_shared_folders(shared_folders: &[SharedFolder]) -> Result<(), ConfigError> {
    let mut names = BTreeSet::new();
    let mut tokens = BTreeSet::new();
    for (index, folder) in shared_folders.iter().enumerate() {
        let name = folder.name.trim();
        if name.is_empty() {
            return Err(ConfigError::EmptySharedFolderField {
                index,
                field: "name",
            });
        }
        if folder.host_path.trim().is_empty() {
            return Err(ConfigError::EmptySharedFolderField {
                index,
                field: "hostPath",
            });
        }
        if !names.insert(name.to_string()) {
            return Err(ConfigError::DuplicateSharedFolderName {
                name: name.to_string(),
            });
        }

        let token = folder.resolved_host_path_token();
        if token.trim().is_empty() {
            return Err(ConfigError::EmptySharedFolderField {
                index,
                field: "hostPathToken",
            });
        }
        if !tokens.insert(token.clone()) {
            return Err(ConfigError::DuplicateSharedFolderToken { token });
        }
    }

    Ok(())
}

/// Reject a path that is absolute or contains a `..`/root/prefix component, so a
/// manifest-supplied path can only ever resolve inside the VM bundle.
pub(crate) fn ensure_bundle_relative(field: &'static str, value: &str) -> Result<(), ConfigError> {
    use std::path::Component;
    let path = Path::new(value);
    let unsafe_path = path.components().any(|component| {
        matches!(
            component,
            Component::RootDir | Component::ParentDir | Component::Prefix(_)
        )
    });
    if unsafe_path {
        return Err(ConfigError::UnsafePath {
            field,
            value: value.to_string(),
        });
    }
    Ok(())
}

pub(crate) fn validate_boot(boot: Option<&Boot>) -> Result<(), ConfigError> {
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
            return Err(ConfigError::EmptyBootInput { field });
        }
    }

    match mode {
        BootMode::ExistingDisk if boot.installer_image.is_some() => {
            Err(ConfigError::UnsupportedBootInput {
                mode,
                field: "installerImage",
            })
        }
        BootMode::ExistingDisk if boot.kernel_path.is_some() => {
            Err(ConfigError::UnsupportedBootInput {
                mode,
                field: "kernelPath",
            })
        }
        BootMode::ExistingDisk if boot.macos_restore_image.is_some() => {
            Err(ConfigError::UnsupportedBootInput {
                mode,
                field: "macosRestoreImage",
            })
        }
        BootMode::ExistingDisk => Ok(()),
        BootMode::LinuxKernel if boot.kernel_path.is_none() => Err(ConfigError::MissingBootInput {
            mode,
            field: "kernelPath",
        }),
        BootMode::LinuxKernel if boot.installer_image.is_some() => {
            Err(ConfigError::UnsupportedBootInput {
                mode,
                field: "installerImage",
            })
        }
        BootMode::LinuxKernel if boot.macos_restore_image.is_some() => {
            Err(ConfigError::UnsupportedBootInput {
                mode,
                field: "macosRestoreImage",
            })
        }
        BootMode::LinuxKernel => Ok(()),
        BootMode::LinuxInstaller if boot.installer_image.is_none() => {
            Err(ConfigError::MissingBootInput {
                mode,
                field: "installerImage",
            })
        }
        BootMode::LinuxInstaller if boot.kernel_path.is_some() => {
            Err(ConfigError::UnsupportedBootInput {
                mode,
                field: "kernelPath",
            })
        }
        BootMode::LinuxInstaller if boot.macos_restore_image.is_some() => {
            Err(ConfigError::UnsupportedBootInput {
                mode,
                field: "macosRestoreImage",
            })
        }
        BootMode::LinuxInstaller => Ok(()),
        BootMode::WindowsInstaller if boot.installer_image.is_none() => {
            Err(ConfigError::MissingBootInput {
                mode,
                field: "installerImage",
            })
        }
        BootMode::WindowsInstaller if boot.kernel_path.is_some() => {
            Err(ConfigError::UnsupportedBootInput {
                mode,
                field: "kernelPath",
            })
        }
        BootMode::WindowsInstaller if boot.macos_restore_image.is_some() => {
            Err(ConfigError::UnsupportedBootInput {
                mode,
                field: "macosRestoreImage",
            })
        }
        BootMode::WindowsInstaller => Ok(()),
        BootMode::MacosRestore if boot.macos_restore_image.is_none() => {
            Err(ConfigError::MissingBootInput {
                mode,
                field: "macosRestoreImage",
            })
        }
        BootMode::MacosRestore if boot.installer_image.is_some() => {
            Err(ConfigError::UnsupportedBootInput {
                mode,
                field: "installerImage",
            })
        }
        BootMode::MacosRestore if boot.kernel_path.is_some() => {
            Err(ConfigError::UnsupportedBootInput {
                mode,
                field: "kernelPath",
            })
        }
        BootMode::MacosRestore => Ok(()),
    }
}

impl VmManifest {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(ConfigError::UnsupportedSchema {
                expected: SCHEMA_VERSION,
                actual: self.schema_version.clone(),
            });
        }
        if self.name.trim().is_empty() {
            return Err(ConfigError::EmptyName);
        }
        // A name that survives the trim but slugs to empty (e.g. "!!!", "···")
        // would map onto the shared `vms/.vmbridge` bundle path -> collisions.
        if slug(&self.name).is_empty() {
            return Err(ConfigError::UnusableName {
                name: self.name.clone(),
            });
        }
        // The primary disk is created/truncated under the VM bundle. An absolute
        // or `..`-escaping path would let a hostile/imported manifest read, write,
        // or truncate an arbitrary host file. Confine it to the bundle.
        ensure_bundle_relative("storage.primary.path", &self.storage.primary.path)?;
        validate_boot(self.boot.as_ref())?;
        validate_shared_folders(&self.shared_folders)?;
        Ok(())
    }
}
