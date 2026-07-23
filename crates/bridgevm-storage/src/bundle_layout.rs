//! Where each metadata file lives inside a bundle.

use bridgevm_config::slug;
use std::path::Path;
use std::path::PathBuf;

pub(crate) fn snapshot_disk_metadata_path(bundle: &Path, snapshot_name: &str) -> PathBuf {
    bundle
        .join("metadata")
        .join("snapshot-disks")
        .join(format!("{}.json", slug(snapshot_name)))
}

pub(crate) fn snapshot_disk_create_metadata_path(bundle: &Path, snapshot_name: &str) -> PathBuf {
    bundle
        .join("metadata")
        .join("snapshot-disks")
        .join(format!("{}-create.json", slug(snapshot_name)))
}

pub(crate) fn snapshot_suspend_image_metadata_path(bundle: &Path, snapshot_name: &str) -> PathBuf {
    bundle
        .join("metadata")
        .join("suspend-images")
        .join(format!("{}.json", slug(snapshot_name)))
}

pub(crate) fn fast_suspend_image_metadata_path(bundle: &Path, vm_name: &str) -> PathBuf {
    bundle
        .join("metadata")
        .join("suspend-images")
        .join(format!("{}.fast.json", slug(vm_name)))
}

pub(crate) fn application_consistent_snapshot_preflight_path(
    bundle: &Path,
    snapshot_name: &str,
) -> PathBuf {
    bundle
        .join("metadata")
        .join("application-consistent-snapshots")
        .join(format!("{}.json", slug(snapshot_name)))
}

pub(crate) fn guest_tools_token_path(bundle: &Path) -> PathBuf {
    bundle.join("metadata").join("guest-tools-token.json")
}

pub(crate) fn guest_tools_runtime_path(bundle: &Path) -> PathBuf {
    bundle.join("metadata").join("guest-tools-runtime.json")
}

pub(crate) fn runtime_resource_policy_path(bundle: &Path) -> PathBuf {
    bundle.join("metadata").join("runtime-resources.json")
}

pub(crate) fn deletion_metadata_path(bundle: &Path) -> PathBuf {
    bundle.join("metadata").join("deletion.json")
}

pub(crate) fn qmp_supervisor_path(bundle: &Path) -> PathBuf {
    bundle.join("metadata").join("qmp-supervisor.json")
}

pub(crate) fn live_evidence_metadata_path(bundle: &Path) -> PathBuf {
    bundle.join("metadata").join("live-evidence.json")
}

pub(crate) fn live_evidence_preserved_path(bundle: &Path) -> PathBuf {
    bundle.join("metadata").join("live-evidence").join("latest")
}
