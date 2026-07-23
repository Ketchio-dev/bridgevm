//! The AppleVzLaunchSpec JSON contract handed to the Swift AppleVzRunner.

use crate::*;
use bridgevm_config::BootMode;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppleVzLaunchSpec {
    pub vm_name: String,
    pub bundle_path: String,
    pub guest: AppleVzGuestSpec,
    pub boot: AppleVzBootSpec,
    pub disk: AppleVzDiskSpec,
    pub resources: AppleVzResourceSpec,
    pub devices: AppleVzDeviceSpec,
    pub integration: AppleVzIntegrationSpec,
    pub logs: AppleVzLogSpec,
    pub readiness: AppleVzReadinessSpec,
    /// Virtio-FS shared directories handed to the AppleVzRunner helper. The Swift
    /// side attaches each as a `VZSharedDirectory` (one `VZSingleDirectoryShare`
    /// for a single entry, a `VZMultipleDirectoryShare` for 2+); see
    /// [`build_fast_plan`] for how the manifest's approved folders are mapped.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shares: Vec<AppleVzShareSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppleVzGuestSpec {
    pub os: String,
    pub arch: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppleVzBootSpec {
    pub mode: BootMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installer_image: Option<AppleVzPathSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kernel: Option<AppleVzPathSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initrd: Option<AppleVzPathSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kernel_command_line: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub macos_restore_image: Option<AppleVzPathSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppleVzPathSpec {
    pub path: String,
    pub exists: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppleVzDiskSpec {
    pub path: String,
    pub format: String,
    pub read_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppleVzResourceSpec {
    pub memory: String,
    pub cpu: String,
    pub display_fps_cap: String,
    pub rationale: String,
    pub balloon_device: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppleVzDeviceSpec {
    pub entropy_device: bool,
    pub network: String,
    pub serial_log_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppleVzIntegrationSpec {
    pub clipboard: bool,
    pub dynamic_resolution: bool,
    pub shared_folders: bool,
    pub virtiofs: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppleVzLogSpec {
    pub runner_log_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppleVzReadinessSpec {
    pub ready: bool,
    pub blockers: Vec<AppleVzReadinessBlocker>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppleVzReadinessBlocker {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability: Option<String>,
}
