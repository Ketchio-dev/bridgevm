//! Export, import, clone, deletion, repair, migration and live-evidence receipt types.

use serde::Deserialize;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmExportMetadata {
    pub vm: String,
    pub source: PathBuf,
    pub output: PathBuf,
    pub archive_format: String,
    pub copied_file_count: u64,
    pub copied_files: Vec<String>,
    pub manifest_preserved: bool,
    pub metadata_preserved: bool,
    pub exported_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmImportMetadata {
    pub vm: String,
    pub original_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requested_name: Option<String>,
    pub source: PathBuf,
    pub output: PathBuf,
    pub archive_format: String,
    pub copied_file_count: u64,
    pub copied_files: Vec<String>,
    pub manifest_preserved: bool,
    pub metadata_preserved: bool,
    pub manifest_identity_rewritten: bool,
    pub imported_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmCloneMetadata {
    pub vm: String,
    pub source: PathBuf,
    pub output: PathBuf,
    #[serde(default)]
    pub linked: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backing_path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backing_format: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub create_command: Option<Vec<String>>,
    pub cloned_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmDeletionMetadata {
    pub vm: String,
    pub bundle: PathBuf,
    pub manifest_backup: PathBuf,
    pub metadata_path: PathBuf,
    pub deleted_at_unix: u64,
    pub metadata_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmMetadataRepairMetadata {
    pub vm: String,
    pub bundle: PathBuf,
    pub repaired: bool,
    pub actions: Vec<MetadataRepairAction>,
    pub repaired_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmLiveEvidenceMetadata {
    pub vm: String,
    pub source: PathBuf,
    pub preserved_path: PathBuf,
    pub copied_file_count: u64,
    pub copied_files: Vec<String>,
    pub recorded_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmManifestMigrationMetadata {
    pub vm: String,
    pub bundle: PathBuf,
    pub manifest_path: PathBuf,
    pub from_schema: String,
    pub to_schema: String,
    pub dry_run: bool,
    pub migrated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backup_path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt_path: Option<PathBuf>,
    pub actions: Vec<MetadataRepairAction>,
    pub migrated_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetadataRepairAction {
    pub path: PathBuf,
    pub action: String,
    pub detail: String,
}
