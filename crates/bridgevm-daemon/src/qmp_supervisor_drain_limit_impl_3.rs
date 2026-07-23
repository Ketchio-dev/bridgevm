//! Continuation of the `qmp_supervisor_drain_limit` impl block, split for the 1000-line rule.

use super::*;

use anyhow::Context;
use anyhow::Result;
use bridgevm_agentd::read_envelope_line;
use bridgevm_api::guest_tools_thaw_filesystem_envelope;
use bridgevm_api::handle_request;
use bridgevm_api::BridgeVmRequest;
use bridgevm_api::BridgeVmResponse;
use bridgevm_qemu::qmp_socket_path;
use bridgevm_qemu::quit as qmp_quit;
use bridgevm_qemu::vnc_display_in_command;
use bridgevm_storage::VmRuntimeState;
use std::io::ErrorKind;
use std::thread;
use std::time::Duration;
use std::time::Instant;

impl DaemonState {
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

    pub(crate) fn wait_for_guest_tools_command_result(
        &mut self,
        name: &str,
        request_id: &str,
        timeout: Duration,
    ) -> Result<CompletedGuestToolsCommand> {
        let deadline = Instant::now() + timeout;
        loop {
            let backend = self
                .children
                .get_mut(name)
                .with_context(|| format!("backend is not owned by this daemon for '{name}'"))?;
            let session = backend
                .guest_tools
                .clone()
                .with_context(|| format!("guest tools session is not connected for '{name}'"))?;
            let Some(reader) = backend.guest_tools_stream.as_mut() else {
                anyhow::bail!("guest tools stream is not connected for '{name}'");
            };

            let envelope = match read_envelope_line(reader) {
                Ok(Some(envelope)) => envelope,
                Ok(None) => {
                    backend.guest_tools = None;
                    backend.guest_tools_stream = None;
                    anyhow::bail!("guest tools stream closed while waiting for '{request_id}'");
                }
                Err(error) if error.is_idle_io() => {
                    if Instant::now() >= deadline {
                        anyhow::bail!(
                            "timed out waiting for guest tools command result '{request_id}'"
                        );
                    }
                    thread::sleep(Duration::from_millis(10));
                    continue;
                }
                Err(error) => {
                    backend.guest_tools = None;
                    backend.guest_tools_stream = None;
                    anyhow::bail!("failed to read guest tools frame: {error:?}");
                }
            };

            if let Some(completed) =
                process_guest_tools_envelope(&self.store, name, backend, &session, envelope)?
            {
                if completed.request_id == request_id {
                    return Ok(completed);
                }
            }

            if Instant::now() >= deadline {
                anyhow::bail!("timed out waiting for guest tools command result '{request_id}'");
            }
        }
    }

    pub(crate) fn cleanup_owned_backend(
        &mut self,
        name: &str,
        send_qmp_quit: bool,
    ) -> Result<BridgeVmResponse> {
        let (bundle, _) = self.store.get_vm(name).context("failed to read VM")?;
        let socket_path = qmp_socket_path(&bundle);
        if send_qmp_quit && socket_path.exists() {
            qmp_quit(&socket_path).context("failed to send QMP quit")?;
        }

        let mut backend = self
            .children
            .remove(name)
            .with_context(|| format!("backend is not owned by this daemon for '{name}'"))?;
        let mut exited = false;
        for _ in 0..40 {
            if backend
                .child
                .try_wait()
                .with_context(|| format!("failed to poll backend '{name}'"))?
                .is_some()
            {
                exited = true;
                break;
            }
            thread::sleep(Duration::from_millis(50));
        }

        if !exited {
            match backend.child.kill() {
                Ok(()) => {}
                // The child can exit between our poll and the kill; Rust returns
                // InvalidInput for an already-exited child. Fine -- reap below.
                Err(error) if error.kind() == ErrorKind::InvalidInput => {}
                // A genuine kill failure: still reap what we can so the child can
                // never orphan, then surface the error.
                Err(error) => {
                    let _ = backend.child.wait();
                    return Err(error)
                        .with_context(|| format!("failed to terminate backend '{name}'"));
                }
            }
            let _ = backend.child.wait();
        }

        self.store
            .transition_state(name, VmRuntimeState::Stopped)
            .context("failed to mark VM stopped")?;
        self.store
            .clear_runner_metadata(name)
            .context("failed to clear runner metadata")?;
        Ok(BridgeVmResponse::RunnerStatus {
            metadata: None,
            qmp_supervisor: self
                .store
                .qmp_supervisor_metadata(name)
                .context("failed to read QMP supervisor metadata")?,
        })
    }

    /// VNC display numbers currently owned by this daemon's live supervised
    /// backends, read back from their recorded launch commands. A newly launched
    /// Compat VM avoids these so it doesn't collide on an in-use VNC port even
    /// before the owning VM's QEMU has finished binding it.
    pub(crate) fn live_vnc_displays(&self) -> Vec<u16> {
        self.children
            .keys()
            .filter_map(|name| self.store.runner_metadata(name).ok().flatten())
            .filter(|metadata| !metadata.dry_run && metadata.pid.is_some())
            .filter_map(|metadata| vnc_display_in_command(&metadata.command))
            .collect()
    }

    /// Tear down every backend this daemon spawned — gracefully (QMP `quit` for
    /// Compatibility Mode, then `SIGTERM`/`SIGKILL`) — so no QEMU/AppleVzRunner
    /// child is orphaned when `bridgevmd` exits. The daemon has no re-adoption
    /// path (a restarted daemon does not reclaim children by pid), so a child it
    /// leaves behind is a pure leak that keeps holding its ports. Best-effort:
    /// failing to reap one backend is logged and does not block the rest, and
    /// any child that somehow survives a failed cleanup is force-killed.
    pub(crate) fn shutdown_reap_children(&mut self) {
        let names: Vec<String> = self.children.keys().cloned().collect();
        for name in names {
            if let Err(error) = self.cleanup_owned_backend(&name, true) {
                // The graceful path bailed (e.g. an unresponsive QMP socket).
                // If the child is still owned here, cleanup failed before
                // killing it, so force-kill so it cannot orphan; otherwise it
                // was already killed and only a later metadata step failed.
                if let Some(mut backend) = self.children.remove(&name) {
                    eprintln!(
                        "bridgevmd shutdown: graceful reap of '{name}' failed ({error:#}); force-killing"
                    );
                    let _ = backend.child.kill();
                    let _ = backend.child.wait();
                } else {
                    eprintln!(
                        "bridgevmd shutdown: reaped backend '{name}' but post-kill cleanup failed: {error:#}"
                    );
                }
            }
        }
    }
}
