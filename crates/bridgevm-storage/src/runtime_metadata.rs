//! Runtime state, runner, resource-policy and QMP-supervisor metadata types.

use crate::*;
use bridgevm_qemu::QmpEvent;
use serde::Deserialize;
use serde::Serialize;
use std::path::PathBuf;

pub(crate) fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VmRuntimeState {
    Running,
    Suspended,
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmRuntimeMetadata {
    pub state: VmRuntimeState,
    pub updated_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunnerMetadata {
    pub engine: String,
    pub pid: Option<u32>,
    pub command: Vec<String>,
    pub log_path: PathBuf,
    pub started_at_unix: u64,
    pub dry_run: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launch_spec_path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guest_tools: Option<GuestToolsRunnerMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disk: Option<DiskPreparationMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_disk: Option<ActiveDiskMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launch_readiness: Option<LaunchReadinessMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_control: Option<RuntimeControlMetadata>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeControlMetadata {
    pub kind: String,
    pub socket_path: PathBuf,
    pub commands: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeResourceVisibility {
    Foreground,
    Background,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeResourcePolicyMetadata {
    pub vm: String,
    pub mode: String,
    pub profile: String,
    pub visibility: RuntimeResourceVisibility,
    pub state: VmRuntimeState,
    pub on_battery: bool,
    pub memory: String,
    pub cpu: String,
    pub display_fps_cap: String,
    pub rationale: String,
    pub live_applied: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub runtime_control_acknowledged: bool,
    pub live_apply_blockers: Vec<RuntimeResourcePolicyBlocker>,
    pub updated_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeResourcePolicyBlocker {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchReadinessMetadata {
    pub ready: bool,
    pub blockers: Vec<LaunchReadinessBlockerMetadata>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchReadinessBlockerMetadata {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QmpSupervisorMetadata {
    pub events: Vec<QmpEvent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_event: Option<QmpEvent>,
    pub envelopes_read: usize,
    pub limit_reached: bool,
    pub updated_at_unix: u64,
}

impl std::fmt::Display for VmRuntimeState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VmRuntimeState::Running => write!(f, "running"),
            VmRuntimeState::Suspended => write!(f, "suspended"),
            VmRuntimeState::Stopped => write!(f, "stopped"),
        }
    }
}

impl std::fmt::Display for RuntimeResourceVisibility {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuntimeResourceVisibility::Foreground => write!(f, "foreground"),
            RuntimeResourceVisibility::Background => write!(f, "background"),
        }
    }
}
