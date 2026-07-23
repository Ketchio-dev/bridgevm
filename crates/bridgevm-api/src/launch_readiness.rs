//! Split out of lib.rs by responsibility.

use crate::*;

pub fn compatibility_launch_readiness_metadata(
    disk: &bridgevm_storage::DiskPreparationMetadata,
    additional_blockers: Vec<LaunchReadinessBlockerMetadata>,
) -> LaunchReadinessMetadata {
    let mut blockers = Vec::new();
    if !disk.exists {
        blockers.push(LaunchReadinessBlockerMetadata {
            code: "missing-primary-disk".to_string(),
            message: missing_disk_message(disk),
            path: Some(disk.path.clone()),
            capability: Some("qemu".to_string()),
        });
    }
    blockers.extend(additional_blockers);
    LaunchReadinessMetadata {
        ready: blockers.is_empty(),
        blockers,
    }
}

pub fn compatibility_launch_dependency_blockers(
    manifest: &VmManifest,
    bundle: &Path,
) -> Vec<LaunchReadinessBlockerMetadata> {
    let mut blockers = Vec::new();

    if manifest.boot.as_ref().is_some_and(|boot| {
        boot.mode == BootMode::WindowsInstaller && boot.installer_image.is_some()
    }) {
        let installer = manifest
            .boot
            .as_ref()
            .and_then(|boot| boot.installer_image.as_deref())
            .expect("checked installer image presence");
        let path = resolve_bundle_path(bundle, installer);
        if !path.exists() {
            blockers.push(LaunchReadinessBlockerMetadata {
                code: "missing-windows-installer-image".to_string(),
                message: format!("Windows installer image is missing: {}", path.display()),
                path: Some(path),
                capability: Some("qemu-boot-media".to_string()),
            });
        }
    }

    if manifest.firmware.tpm {
        let path = swtpm_socket_path(bundle);
        if !path.exists() {
            blockers.push(LaunchReadinessBlockerMetadata {
                code: "missing-tpm-socket".to_string(),
                message: format!("firmware.tpm requires swtpm socket: {}", path.display()),
                path: Some(path),
                capability: Some("qemu-tpm".to_string()),
            });
        }
    }

    if manifest.firmware.secure_boot {
        let path = secure_boot_vars_path(bundle);
        if !path.exists() {
            blockers.push(LaunchReadinessBlockerMetadata {
                code: "missing-secure-boot-vars".to_string(),
                message: format!(
                    "firmware.secureBoot requires seeded edk2 variable store: {}",
                    path.display()
                ),
                path: Some(path),
                capability: Some("qemu-secure-boot".to_string()),
            });
        }
    }

    blockers.extend(compatibility_network_privilege_blockers(manifest));

    blockers
}

pub(crate) fn compatibility_network_privilege_blockers(
    manifest: &VmManifest,
) -> Vec<LaunchReadinessBlockerMetadata> {
    let Ok(mode) = manifest.network.mode.parse::<NetworkMode>() else {
        return Vec::new();
    };
    if !matches!(mode, NetworkMode::HostOnly | NetworkMode::Bridged) {
        return Vec::new();
    }
    let port_forwards = manifest
        .network
        .forwards
        .iter()
        .map(|forward| PortForwardRule {
            host: forward.host,
            guest: forward.guest,
        })
        .collect::<Vec<_>>();

    let Ok(plan) = plan_network(
        NetworkBackend::Qemu,
        mode,
        manifest.network.hostname.clone(),
        port_forwards,
    ) else {
        return Vec::new();
    };

    plan.requirements
        .into_iter()
        .map(|requirement| LaunchReadinessBlockerMetadata {
            code: requirement.blocker,
            message: requirement.requirement,
            path: None,
            capability: Some("qemu-network".to_string()),
        })
        .collect()
}

pub fn compatibility_launch_readiness_blocker_from_qemu_error(
    error: QemuError,
) -> LaunchReadinessBlockerMetadata {
    match error {
        QemuError::UnsupportedNetworkRequirement {
            mode,
            blocker,
            requirement,
        } => LaunchReadinessBlockerMetadata {
            code: blocker,
            message: format!(
                "{mode} networking requires an advanced Compatibility Mode QEMU schema before args can be generated; requirement: {requirement}"
            ),
            path: None,
            capability: Some("qemu-network".to_string()),
        },
        QemuError::NetworkPlan(error) => LaunchReadinessBlockerMetadata {
            code: "qemu-network-plan-invalid".to_string(),
            message: error.to_string(),
            path: None,
            capability: Some("qemu-network".to_string()),
        },
        QemuError::UnsupportedMode(mode) => LaunchReadinessBlockerMetadata {
            code: "qemu-unsupported-mode".to_string(),
            message: format!("QEMU command builder only supports Compatibility Mode manifests, got {mode}"),
            path: None,
            capability: Some("qemu".to_string()),
        },
        QemuError::UnsupportedNetworkMode(mode) => LaunchReadinessBlockerMetadata {
            code: "qemu-network-mode-unsupported".to_string(),
            message: format!("QEMU launch does not support {mode} networking yet"),
            path: None,
            capability: Some("qemu-network".to_string()),
        },
        QemuError::QmpIo(error) => LaunchReadinessBlockerMetadata {
            code: "qmp-io-error".to_string(),
            message: error.to_string(),
            path: None,
            capability: Some("qmp".to_string()),
        },
        QemuError::QmpJson(error) => LaunchReadinessBlockerMetadata {
            code: "qmp-json-error".to_string(),
            message: error.to_string(),
            path: None,
            capability: Some("qmp".to_string()),
        },
        QemuError::QmpProtocol(error) => LaunchReadinessBlockerMetadata {
            code: "qmp-protocol-error".to_string(),
            message: error,
            path: None,
            capability: Some("qmp".to_string()),
        },
        QemuError::MissingInstallerImage => LaunchReadinessBlockerMetadata {
            code: "qemu-missing-installer-image".to_string(),
            message: "windows-installer boot mode requires boot.installerImage".to_string(),
            path: None,
            capability: Some("qemu".to_string()),
        },
    }
}

pub(crate) fn compatibility_launch_readiness_blocker_summary(
    readiness: &LaunchReadinessMetadata,
) -> String {
    let summary = launch_readiness_blocker_summary(readiness);
    if summary.is_empty() {
        "Compatibility Mode launch readiness failed".to_string()
    } else {
        format!("Compatibility Mode launch readiness failed: {summary}")
    }
}

pub(crate) fn missing_disk_message(disk: &bridgevm_storage::DiskPreparationMetadata) -> String {
    if let Some(command) = &disk.create_command {
        format!(
            "primary disk is not ready: {}; create it with: {}",
            disk.path.display(),
            command.join(" ")
        )
    } else {
        format!("primary disk is not ready: {}", disk.path.display())
    }
}

pub(crate) fn apply_active_disk_to_manifest(
    manifest: &mut VmManifest,
    active_disk: &bridgevm_storage::ActiveDiskMetadata,
) {
    manifest.storage.primary.path = active_disk.path.display().to_string();
    manifest.storage.primary.format = active_disk.format.clone();
}

pub fn fast_spawn_runner_required_message() -> &'static str {
    "Fast Mode spawn requires BRIDGEVM_APPLE_VZ_RUNNER to point at a signed AppleVzRunner; dry-run planning metadata was updated"
}

pub fn fast_spawn_runner_required_error(readiness: &LaunchReadinessMetadata) -> String {
    let mut message = fast_spawn_runner_required_message().to_string();
    if !readiness.blockers.is_empty() {
        message.push_str("; launch blockers: ");
        message.push_str(&launch_readiness_blocker_summary(readiness));
    }
    message
}

pub fn launch_readiness_metadata(readiness: &AppleVzReadinessSpec) -> LaunchReadinessMetadata {
    LaunchReadinessMetadata {
        ready: readiness.ready,
        blockers: readiness
            .blockers
            .iter()
            .map(|blocker| LaunchReadinessBlockerMetadata {
                code: blocker.code.clone(),
                message: blocker.message.clone(),
                path: blocker.path.as_ref().map(PathBuf::from),
                capability: blocker.capability.clone(),
            })
            .collect(),
    }
}

pub fn add_fast_spawn_runner_required_blocker(readiness: &mut LaunchReadinessMetadata) {
    readiness.ready = false;
    if readiness
        .blockers
        .iter()
        .any(|blocker| blocker.code == "apple-vz-runner-unavailable")
    {
        return;
    }
    readiness.blockers.push(LaunchReadinessBlockerMetadata {
        code: "apple-vz-runner-unavailable".to_string(),
        message:
            "Fast Mode spawn needs BRIDGEVM_APPLE_VZ_RUNNER to point at a signed AppleVzRunner"
                .to_string(),
        path: None,
        capability: Some("apple-virtualization-framework".to_string()),
    });
}

pub(crate) fn launch_readiness_blocker_summary(readiness: &LaunchReadinessMetadata) -> String {
    readiness
        .blockers
        .iter()
        .map(|blocker| {
            let mut summary = format!("{}: {}", blocker.code, blocker.message);
            if let Some(path) = &blocker.path {
                summary.push_str(&format!(" ({})", path.display()));
            } else if let Some(capability) = &blocker.capability {
                summary.push_str(&format!(" ({capability})"));
            }
            summary
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Number of seconds a recorded backend process is given to exit gracefully
/// after `SIGTERM` (or a graceful QMP `quit`) before it is force-killed with
/// `SIGKILL`.
pub(crate) const STOP_TERMINATION_GRACE_SECONDS: u64 = 5;

/// Outcome of attempting to terminate a recorded backend process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ProcessTerminationOutcome {
    /// No live process existed for the recorded pid (already gone).
    AlreadyGone,
    /// The process exited within the grace period after `SIGTERM`.
    ExitedAfterTerm,
    /// The process did not exit after `SIGTERM` and was force-killed with `SIGKILL`.
    Killed,
}
