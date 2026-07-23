//! Application-consistent snapshot freeze/snapshot/thaw sequencing and preflight status.

use crate::*;
use anyhow::Context;
use anyhow::Result;
use bridgevm_api::guest_tools_freeze_filesystem_envelope;
use bridgevm_api::guest_tools_thaw_filesystem_envelope;
use bridgevm_api::handle_request;
use bridgevm_api::ApplicationConsistentSnapshotExecutionRecord;
use bridgevm_api::BridgeVmRequest;
use bridgevm_api::BridgeVmResponse;
use bridgevm_api::SnapshotConsistency;
use bridgevm_storage::SnapshotKind;

impl DaemonState {
    pub(crate) fn execute_application_consistent_snapshot(
        &mut self,
        vm: &str,
        snapshot: &str,
        freeze_timeout_millis: Option<u64>,
    ) -> Result<BridgeVmResponse> {
        let BridgeVmResponse::SnapshotPreflightStatus { preflight } = self
            .owned_backend_snapshot_preflight_status(
                vm,
                SnapshotConsistency::ApplicationConsistent,
            )?
        else {
            anyhow::bail!("snapshot preflight request returned unexpected response");
        };
        if !preflight.ready {
            anyhow::bail!(
                "application-consistent snapshot preflight is not ready: {}",
                preflight
                    .blockers
                    .iter()
                    .map(|blocker| blocker.code.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }

        let freeze_request_id = format!("application-consistent-snapshot:{snapshot}:freeze");
        let thaw_request_id = format!("application-consistent-snapshot:{snapshot}:thaw");

        self.send_guest_tools_command_record(
            vm,
            guest_tools_freeze_filesystem_envelope(
                freeze_request_id.clone(),
                freeze_timeout_millis,
            ),
        )?;
        let freeze_result = self.wait_for_guest_tools_command_result(
            vm,
            &freeze_request_id,
            command_result_timeout(freeze_timeout_millis),
        )?;
        if !freeze_result.ok {
            // Freeze did not enter the boundary (the agent rejected it), so the
            // guest is not quiesced and there is nothing to thaw. Still issue a
            // best-effort thaw so a partially-frozen agent cannot get stuck.
            let thaw_attempted = self.dispatch_and_await_thaw(vm, &thaw_request_id).is_ok();
            anyhow::bail!(
                "guest tools freeze failed for application-consistent snapshot '{}': {}; thaw attempted: {}",
                snapshot,
                freeze_result
                    .error_code
                    .as_deref()
                    .unwrap_or("command-result-not-ok"),
                thaw_attempted
            );
        }

        // The guest is now frozen. From here on the filesystem MUST be thawed no
        // matter what happens to the snapshot, so we capture the snapshot result
        // WITHOUT propagating it, then unconditionally dispatch + await the thaw,
        // and only afterwards surface any errors. This guarantees the thaw is
        // always sent even when the snapshot fails.
        let snapshot_result =
            self.store
                .create_snapshot(vm, snapshot, SnapshotKind::ApplicationConsistent);
        let thaw_result = self.dispatch_and_await_thaw(vm, &thaw_request_id);

        let snapshot_metadata = snapshot_result.with_context(|| {
            format!("failed to create application-consistent snapshot '{snapshot}'")
        })?;
        let thaw_result = thaw_result.with_context(|| {
            format!("snapshot '{snapshot}' was recorded, but thaw dispatch failed")
        })?;
        if !thaw_result.ok {
            anyhow::bail!(
                "snapshot '{}' was recorded, but guest tools thaw failed: {}",
                snapshot,
                thaw_result
                    .error_code
                    .as_deref()
                    .unwrap_or("command-result-not-ok")
            );
        }

        Ok(BridgeVmResponse::ApplicationConsistentSnapshotExecution {
            execution: ApplicationConsistentSnapshotExecutionRecord {
                vm: vm.to_string(),
                snapshot: snapshot.to_string(),
                freeze_request_id,
                thaw_request_id,
                pending_commands_after_freeze: freeze_result.pending_commands,
                pending_commands_after_thaw: thaw_result.pending_commands,
                snapshot_created_at_unix: snapshot_metadata.created_at_unix,
                freeze_result: freeze_result.into_record(),
                thaw_result: thaw_result.into_record(),
                preflight_ready: true,
                note: "Received successful guest-tools freeze/thaw CommandResult frames around snapshot creation; with the agent's Real fsfreeze backend this enters the OS fsfreeze boundary, but this still does not prove OS-level application consistency (it depends on guest applications flushing their own state).".to_string(),
            },
        })
    }

    /// Dispatches a ThawFilesystem command and waits for its CommandResult.
    ///
    /// This is the single thaw step used by [`execute_application_consistent_snapshot`]
    /// so that the freeze boundary is always closed exactly once, regardless of
    /// whether the snapshot succeeded or failed.
    pub(crate) fn dispatch_and_await_thaw(
        &mut self,
        vm: &str,
        thaw_request_id: &str,
    ) -> Result<CompletedGuestToolsCommand> {
        self.send_guest_tools_command_record(
            vm,
            guest_tools_thaw_filesystem_envelope(thaw_request_id.to_string()),
        )?;
        self.wait_for_guest_tools_command_result(
            vm,
            thaw_request_id,
            GUEST_TOOLS_COMMAND_RESULT_TIMEOUT,
        )
    }

    pub(crate) fn owned_backend_snapshot_preflight_status(
        &self,
        name: &str,
        consistency: bridgevm_api::SnapshotConsistency,
    ) -> Result<BridgeVmResponse> {
        let response = handle_request(
            &self.store,
            BridgeVmRequest::SnapshotPreflightStatus {
                name: name.to_string(),
                consistency,
            },
        )
        .into_result()
        .map_err(anyhow::Error::msg)?;
        let BridgeVmResponse::SnapshotPreflightStatus { mut preflight } = response else {
            anyhow::bail!("snapshot preflight request returned unexpected response");
        };

        preflight.backend_freeze_thaw_supported = true;
        preflight
            .blockers
            .retain(|blocker| blocker.code != "backend-freeze-thaw-unavailable");
        preflight.ready = preflight.blockers.is_empty();

        Ok(BridgeVmResponse::SnapshotPreflightStatus { preflight })
    }
}
