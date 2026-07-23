//! Guest-tools capability preflight for application-consistent snapshots.

use crate::*;
use std::path::Path;

pub(crate) const APPLICATION_CONSISTENT_FREEZE_SEMANTICS: &str =
    "daemon-owned guest-tools fs-freeze request before disk snapshot when the preflight is ready";

pub(crate) const APPLICATION_CONSISTENT_THAW_SEMANTICS: &str =
    "daemon-owned guest-tools fs-thaw request after the snapshot attempt when freeze was dispatched";

pub(crate) fn application_consistent_snapshot_required_capabilities() -> Vec<String> {
    vec!["fs-freeze".to_string(), "fs-thaw".to_string()]
}

impl VmStore {
    pub fn application_consistent_snapshot_preflight_metadata(
        &self,
        vm_name: &str,
        snapshot_name: &str,
    ) -> Result<Option<ApplicationConsistentSnapshotPreflightMetadata>, StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        let path = application_consistent_snapshot_preflight_path(&bundle, snapshot_name);
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(read_json_required(&path)?))
    }

    pub(crate) fn prepare_application_consistent_snapshot_preflight_at(
        &self,
        bundle: &Path,
        snapshot_name: &str,
    ) -> Result<ApplicationConsistentSnapshotPreflightMetadata, StorageError> {
        let runtime_path = guest_tools_runtime_path(bundle);
        let runtime: Option<GuestToolsRuntimeMetadata> = if runtime_path.exists() {
            Some(read_json_required(&runtime_path)?)
        } else {
            None
        };
        let required_capabilities = application_consistent_snapshot_required_capabilities();
        let available_capabilities = runtime
            .as_ref()
            .map(|runtime| runtime.capabilities.clone())
            .unwrap_or_default();
        let missing_capabilities = required_capabilities
            .iter()
            .filter(|required| {
                !available_capabilities
                    .iter()
                    .any(|available| available == *required)
            })
            .cloned()
            .collect::<Vec<_>>();
        let connected = runtime.as_ref().is_some_and(|runtime| runtime.connected);
        let ready = connected && missing_capabilities.is_empty();
        let metadata = ApplicationConsistentSnapshotPreflightMetadata {
            snapshot: snapshot_name.to_string(),
            connected,
            required_capabilities,
            available_capabilities,
            missing_capabilities,
            ready,
            planned_freeze_semantics: APPLICATION_CONSISTENT_FREEZE_SEMANTICS.to_string(),
            planned_thaw_semantics: APPLICATION_CONSISTENT_THAW_SEMANTICS.to_string(),
            runtime_updated_at_unix: runtime.as_ref().map(|runtime| runtime.updated_at_unix),
            prepared_at_unix: now_unix(),
        };
        write_json_pretty_atomic(
            &application_consistent_snapshot_preflight_path(bundle, snapshot_name),
            &metadata,
        )?;
        Ok(metadata)
    }
}
