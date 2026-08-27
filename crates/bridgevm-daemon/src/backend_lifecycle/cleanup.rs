//! Transactional termination and persistence for daemon-owned backends.

use super::*;
use anyhow::Context;
use bridgevm_qemu::qmp_socket_path;
use bridgevm_qemu::quit as qmp_quit;
use bridgevm_storage::VmRuntimeState;
use std::io::ErrorKind;
use std::process::Child;
use std::thread;
use std::time::Duration;

fn terminate_child(child: &mut Child, name: &str) -> Result<()> {
    let mut exited = false;
    for _ in 0..40 {
        if child
            .try_wait()
            .with_context(|| format!("failed to poll backend '{name}'"))?
            .is_some()
        {
            exited = true;
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    if exited {
        return Ok(());
    }
    match child.kill() {
        Ok(()) => {}
        // The child can exit between our poll and the kill. Reap below.
        Err(error) if error.kind() == ErrorKind::InvalidInput => {}
        Err(error) => {
            let _ = child.wait();
            return Err(error).with_context(|| format!("failed to terminate backend '{name}'"));
        }
    }
    let _ = child.wait();
    Ok(())
}

impl DaemonState {
    pub(super) fn terminate_owned_child(&mut self, name: &str) -> Result<()> {
        let backend = self
            .children
            .get_mut(name)
            .with_context(|| format!("backend is not owned by this daemon for '{name}'"))?;
        terminate_child(&mut backend.child, name)
    }

    pub(crate) fn cleanup_owned_backend(
        &mut self,
        name: &str,
        send_qmp_quit: bool,
    ) -> Result<BridgeVmResponse> {
        let (bundle, _) = self.store.get_vm(name).context("failed to read VM")?;
        let qmp_supervisor = self
            .store
            .qmp_supervisor_metadata(name)
            .context("failed to read QMP supervisor metadata")?;
        let child_exited = self
            .children
            .get_mut(name)
            .with_context(|| format!("backend is not owned by this daemon for '{name}'"))?
            .child
            .try_wait()
            .with_context(|| format!("failed to poll backend '{name}'"))?
            .is_some();
        let socket_path = qmp_socket_path(&bundle);
        if send_qmp_quit && !child_exited && socket_path.exists() {
            qmp_quit(&socket_path).context("failed to send QMP quit")?;
        }

        self.terminate_owned_child(name)?;
        self.store
            .force_transition_state(name, VmRuntimeState::Stopped)
            .context("failed to mark VM stopped")?;
        self.store
            .clear_runner_metadata(name)
            .context("failed to clear runner metadata")?;
        self.children.remove(name);
        Ok(BridgeVmResponse::RunnerStatus {
            metadata: None,
            qmp_supervisor,
        })
    }

    pub(crate) fn stop_owned_backend(&mut self, name: &str) -> Result<BridgeVmResponse> {
        self.cleanup_owned_backend(name, true)
    }

    pub(crate) fn restart_owned_backend(&mut self, name: &str) -> Result<BridgeVmResponse> {
        self.cleanup_owned_backend(name, true)?;
        self.spawn_backend(name)
    }
}
