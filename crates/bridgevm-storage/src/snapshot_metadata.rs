//! Snapshot catalog, disk-snapshot, suspend-image and preflight metadata types.

use crate::*;
use serde::Deserialize;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotMetadata {
    pub name: String,
    pub kind: SnapshotKind,
    pub created_at_unix: u64,
    pub vm_state: VmRuntimeState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotDiskMetadata {
    pub snapshot: String,
    pub overlay_path: PathBuf,
    pub overlay_format: String,
    pub overlay_exists: bool,
    pub backing_path: PathBuf,
    pub backing_format: String,
    pub backing_exists: bool,
    pub create_command: Vec<String>,
    pub prepared_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotDiskCreateMetadata {
    pub snapshot: String,
    pub disk: SnapshotDiskMetadata,
    pub command: Vec<String>,
    pub executed: bool,
    pub exit_status: Option<String>,
    pub stdout: String,
    pub stderr: String,
    pub created_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotSuspendImageMetadata {
    pub snapshot: String,
    pub image_path: PathBuf,
    pub image_format: String,
    pub image_exists: bool,
    pub prepared_at_unix: u64,
}

/// Metadata describing a VM-scoped Fast Mode suspend image (the Apple VZ saved
/// machine state written when a Fast VM is suspended). Distinct from
/// [`SnapshotSuspendImageMetadata`], which is keyed by snapshot name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FastSuspendImageMetadata {
    pub vm: String,
    pub image_path: PathBuf,
    pub image_format: String,
    pub image_exists: bool,
    pub updated_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplicationConsistentSnapshotPreflightMetadata {
    pub snapshot: String,
    pub connected: bool,
    pub required_capabilities: Vec<String>,
    pub available_capabilities: Vec<String>,
    pub missing_capabilities: Vec<String>,
    pub ready: bool,
    pub planned_freeze_semantics: String,
    pub planned_thaw_semantics: String,
    pub runtime_updated_at_unix: Option<u64>,
    pub prepared_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotChainMetadata {
    pub active_disk: ActiveDiskMetadata,
    pub disks: Vec<SnapshotDiskMetadata>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotRestoreMetadata {
    pub snapshot: String,
    pub restored_at_unix: u64,
    pub restored_state: VmRuntimeState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_disk: Option<ActiveDiskMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suspend_image: Option<SnapshotSuspendImageMetadata>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SnapshotKind {
    Disk,
    Suspend,
    ApplicationConsistent,
}

impl std::fmt::Display for SnapshotKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SnapshotKind::Disk => write!(f, "disk"),
            SnapshotKind::Suspend => write!(f, "suspend"),
            SnapshotKind::ApplicationConsistent => write!(f, "application-consistent"),
        }
    }
}
