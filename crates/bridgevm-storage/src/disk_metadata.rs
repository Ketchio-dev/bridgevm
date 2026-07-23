//! Active-disk and disk preparation/create/inspect/verify/compact metadata types.

use serde::Deserialize;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ActiveDiskSource {
    Primary,
    SnapshotOverlay,
    SnapshotBacking,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveDiskMetadata {
    pub source: ActiveDiskSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<String>,
    pub path: PathBuf,
    pub format: String,
    pub exists: bool,
    pub activated_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiskPreparationMetadata {
    pub path: PathBuf,
    pub format: String,
    pub size: String,
    pub size_bytes: Option<u64>,
    pub exists: bool,
    pub created: bool,
    pub create_command: Option<Vec<String>>,
    pub prepared_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiskCreateMetadata {
    pub preparation: DiskPreparationMetadata,
    pub command: Option<Vec<String>>,
    pub executed: bool,
    pub exit_status: Option<String>,
    pub stdout: String,
    pub stderr: String,
    pub created_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiskInspectMetadata {
    pub preparation: DiskPreparationMetadata,
    pub command: Vec<String>,
    pub exit_status: String,
    pub info: serde_json::Value,
    pub stdout: String,
    pub stderr: String,
    pub inspect_duration_microseconds: u64,
    pub inspected_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiskVerifyMetadata {
    pub active_disk: ActiveDiskMetadata,
    pub command: Vec<String>,
    pub exit_status: String,
    pub report: serde_json::Value,
    pub stdout: String,
    pub stderr: String,
    pub verify_duration_microseconds: u64,
    pub verified_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiskCompactMetadata {
    pub preparation: DiskPreparationMetadata,
    pub active_disk: ActiveDiskMetadata,
    pub command: Vec<String>,
    pub temp_path: PathBuf,
    pub backup_path: PathBuf,
    pub exit_status: String,
    pub stdout: String,
    pub stderr: String,
    pub original_size_bytes: u64,
    pub compacted_size_bytes: u64,
    pub compact_duration_microseconds: u64,
    pub compacted_at_unix: u64,
}

impl std::fmt::Display for ActiveDiskSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ActiveDiskSource::Primary => write!(f, "primary"),
            ActiveDiskSource::SnapshotOverlay => write!(f, "snapshot-overlay"),
            ActiveDiskSource::SnapshotBacking => write!(f, "snapshot-backing"),
        }
    }
}
