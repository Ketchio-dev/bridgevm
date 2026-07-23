//! StorageError: every failure of the VM store.

use crate::*;
use bridgevm_config::ConfigError;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("VM already exists: {0}")]
    AlreadyExists(String),
    #[error("VM not found: {0}")]
    NotFound(String),
    #[error("metadata file {path:?} is {actual} bytes, exceeding the {maximum}-byte limit")]
    MetadataTooLarge {
        path: PathBuf,
        actual: u64,
        maximum: u64,
    },
    #[error("snapshot already exists for {vm}: {snapshot}")]
    SnapshotAlreadyExists { vm: String, snapshot: String },
    #[error("snapshot not found for {vm}: {snapshot}")]
    SnapshotNotFound { vm: String, snapshot: String },
    #[error("disk snapshot metadata not found for {vm}: {snapshot}")]
    SnapshotDiskMetadataNotFound { vm: String, snapshot: String },
    #[error("suspend image metadata not found for {vm}: {snapshot}")]
    SnapshotSuspendImageMetadataNotFound { vm: String, snapshot: String },
    #[error("suspend image is missing: {0}")]
    SnapshotSuspendImageMissing(PathBuf),
    #[error("disk snapshot backing image is missing: {0}")]
    SnapshotDiskBackingMissing(PathBuf),
    #[error("disk snapshot overlay image is missing: {0}")]
    SnapshotDiskOverlayMissing(PathBuf),
    #[error("snapshot disk creation command failed ({status}): {command:?}: {stderr}")]
    SnapshotDiskCreateFailed {
        command: Vec<String>,
        status: String,
        stderr: String,
    },
    #[error("failed to execute snapshot disk creation command {command:?}: {source}")]
    SnapshotDiskCreateIo {
        command: Vec<String>,
        #[source]
        source: std::io::Error,
    },
    #[error("export output already exists: {0}")]
    ExportAlreadyExists(PathBuf),
    #[error("export output must not be the source bundle or inside it: source={source_bundle:?}, output={output:?}")]
    ExportOutputInsideSource {
        source_bundle: PathBuf,
        output: PathBuf,
    },
    #[error("import input is not a valid VM bundle: {0}")]
    InvalidImportBundle(PathBuf),
    #[error(
        "import input conflicts with the destination store: input={input:?}, output={output:?}"
    )]
    ImportPathConflict { input: PathBuf, output: PathBuf },
    #[error("unsupported VM bundle archive format: {0}")]
    UnsupportedArchiveFormat(PathBuf),
    #[error("VM bundle archive contains an unsafe path: {0}")]
    UnsafeArchiveEntry(PathBuf),
    #[error("VM bundle copy rejected unsupported file type: {0}")]
    UnsupportedBundleEntry(PathBuf),
    #[error("invalid VM state transition from {from:?} to {to:?}")]
    InvalidStateTransition {
        from: VmRuntimeState,
        to: VmRuntimeState,
    },
    #[error("disk creation command failed ({status}): {command:?}: {stderr}")]
    DiskCreateFailed {
        command: Vec<String>,
        status: String,
        stderr: String,
    },
    #[error("failed to execute disk creation command {command:?}: {source}")]
    DiskCreateIo {
        command: Vec<String>,
        #[source]
        source: std::io::Error,
    },
    #[error("primary disk is missing: {0}")]
    DiskMissing(PathBuf),
    #[error(
        "disk inspection requires qemu-img-managed formats; raw disk is prepared directly: {0}"
    )]
    DiskInspectUnsupportedRaw(PathBuf),
    #[error("disk inspection command failed ({status}): {command:?}: {stderr}")]
    DiskInspectFailed {
        command: Vec<String>,
        status: String,
        stderr: String,
    },
    #[error("failed to execute disk inspection command {command:?}: {source}")]
    DiskInspectIo {
        command: Vec<String>,
        #[source]
        source: std::io::Error,
    },
    #[error(
        "disk verification requires qemu-img-managed formats; raw disk is prepared directly: {0}"
    )]
    DiskVerifyUnsupportedRaw(PathBuf),
    #[error("disk verification command failed ({status}): {command:?}: {stderr}")]
    DiskVerifyFailed {
        command: Vec<String>,
        status: String,
        stderr: String,
    },
    #[error("failed to execute disk verification command {command:?}: {source}")]
    DiskVerifyIo {
        command: Vec<String>,
        #[source]
        source: std::io::Error,
    },
    #[error("linked clone disk creation command failed ({status}): {command:?}: {stderr}")]
    LinkedCloneDiskCreateFailed {
        command: Vec<String>,
        status: String,
        stderr: String,
    },
    #[error("failed to execute linked clone disk creation command {command:?}: {source}")]
    LinkedCloneDiskCreateIo {
        command: Vec<String>,
        #[source]
        source: std::io::Error,
    },
    #[error(
        "disk compaction requires qemu-img-managed formats; raw disk is prepared directly: {0}"
    )]
    DiskCompactUnsupportedRaw(PathBuf),
    #[error("disk compaction command failed ({status}): {command:?}: {stderr}")]
    DiskCompactFailed {
        command: Vec<String>,
        status: String,
        stderr: String,
    },
    #[error("failed to execute disk compaction command {command:?}: {source}")]
    DiskCompactIo {
        command: Vec<String>,
        #[source]
        source: std::io::Error,
    },
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("config error: {0}")]
    Config(#[from] ConfigError),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}
