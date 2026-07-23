//! The launch handoff/attempt records and the readiness-gated launch entry point.

use crate::*;
use bridgevm_config::BootMode;
use serde::Deserialize;
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppleVzLaunchHandoff {
    pub backend: String,
    pub vm_name: String,
    pub bundle_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launch_spec_path: Option<String>,
    pub guest: AppleVzGuestSpec,
    pub boot_mode: BootMode,
    pub disk: AppleVzDiskSpec,
    pub resources: AppleVzResourceSpec,
    pub runner_log_path: String,
    pub serial_log_path: String,
    pub integration: AppleVzIntegrationSpec,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shares: Vec<AppleVzShareSpec>,
    pub readiness: AppleVzReadinessSpec,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppleVzLaunchAttempt {
    pub backend: String,
    pub vm_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub stdout: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub stderr: String,
}

pub fn build_launch_handoff(
    spec: &AppleVzLaunchSpec,
    launch_spec_path: Option<&Path>,
) -> AppleVzLaunchHandoff {
    AppleVzLaunchHandoff {
        backend: "apple-virtualization-framework".to_string(),
        vm_name: spec.vm_name.clone(),
        bundle_path: spec.bundle_path.clone(),
        launch_spec_path: launch_spec_path.map(|path| path.display().to_string()),
        guest: spec.guest.clone(),
        boot_mode: spec.boot.mode,
        disk: spec.disk.clone(),
        resources: spec.resources.clone(),
        runner_log_path: spec.logs.runner_log_path.clone(),
        serial_log_path: spec.devices.serial_log_path.clone(),
        integration: spec.integration.clone(),
        shares: spec.shares.clone(),
        readiness: spec.readiness.clone(),
    }
}

pub fn launch_with_apple_vz<L: AppleVzLauncher>(
    launcher: &L,
    handoff: AppleVzLaunchHandoff,
) -> Result<AppleVzLaunchAttempt, AppleVzLaunchError> {
    ensure_launch_handoff_ready(&handoff)?;
    launcher.launch(handoff)
}

pub fn ensure_launch_handoff_ready(
    handoff: &AppleVzLaunchHandoff,
) -> Result<(), AppleVzLaunchError> {
    if handoff.readiness.ready {
        Ok(())
    } else {
        Err(AppleVzLaunchError::NotReady {
            blockers: handoff.readiness.blockers.clone(),
        })
    }
}
