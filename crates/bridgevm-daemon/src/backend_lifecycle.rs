//! Suspend, resume, stop, restart and cleanup of daemon-owned backends, and VNC display arbitration.

use crate::*;
use anyhow::Context;
use anyhow::Result;
use bridgevm_api::fast_suspend_state_path;
use bridgevm_api::resume_backend;
use bridgevm_api::suspend_backend;
use bridgevm_api::BridgeVmResponse;
use bridgevm_api::CurrentRuntimeEngine;
use bridgevm_qemu::qmp_socket_path;
use bridgevm_qemu::quit as qmp_quit;
use bridgevm_qemu::vnc_display_in_command;
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
    /// Keep daemon ownership until synchronous suspend has committed. A failed
    /// QMP or runner preflight therefore leaves the original backend supervised.
    pub(crate) fn suspend_backend_supervised(&mut self, name: &str) -> Result<BridgeVmResponse> {
        let owned = self.children.contains_key(name);
        let qmp_supervisor = self
            .store
            .qmp_supervisor_metadata(name)
            .context("failed to read QMP supervisor metadata")?;
        let metadata = suspend_backend(&self.store, name).map_err(anyhow::Error::msg)?;
        if owned {
            self.terminate_owned_child(name)?;
        }
        Ok(BridgeVmResponse::RunnerStatus {
            metadata: Some(metadata),
            qmp_supervisor,
        })
    }

    /// Resume a backend through the daemon, tracking the new child in the
    /// supervisor exactly like cold-start `run` so reconcile/stop see it.
    ///
    /// Fast Mode: relaunch `lightvm-runner` with `--apple-vz-restore-state`.
    /// Compatibility Mode: relaunch QEMU with `-loadvm <tag>`. In both cases the
    /// child is inserted into `self.children`. When the Fast Mode real-start
    /// env is not configured, fall back to the daemon-less api resume (which is
    /// detached, matching legacy behavior).
    pub(crate) fn resume_backend_supervised(&mut self, name: &str) -> Result<BridgeVmResponse> {
        if self.children.contains_key(name) {
            anyhow::bail!("backend is already running for '{name}'");
        }
        let (bundle, manifest, _) = self
            .store
            .get_vm_with_active_disk(name)
            .context("failed to read VM")?;

        match CurrentRuntimeEngine::for_manifest(&manifest) {
            CurrentRuntimeEngine::AppleVz => {
                let state_path = fast_suspend_state_path(&bundle, name);
                if !state_path.exists() {
                    anyhow::bail!(
                        "no saved Fast Mode state to resume from at {}; suspend the VM first",
                        state_path.display()
                    );
                }
                if let Some(config) = FastModeSpawnConfig::from_env()? {
                    return self.spawn_fast_backend_with_restore(
                        name,
                        bundle,
                        manifest,
                        config,
                        Some(state_path),
                    );
                }
                // Real-start env not configured: fall back to detached api resume.
                let metadata = resume_backend(&self.store, name).map_err(anyhow::Error::msg)?;
                Ok(BridgeVmResponse::RunnerStatus {
                    metadata: Some(metadata),
                    qmp_supervisor: self
                        .store
                        .qmp_supervisor_metadata(name)
                        .context("failed to read QMP supervisor metadata")?,
                })
            }
            CurrentRuntimeEngine::QemuCompatibility => {
                self.resume_compatibility_supervised(name, &bundle, &manifest)
            }
        }
    }

    fn terminate_owned_child(&mut self, name: &str) -> Result<()> {
        {
            let backend = self
                .children
                .get_mut(name)
                .with_context(|| format!("backend is not owned by this daemon for '{name}'"))?;
            terminate_child(&mut backend.child, name)?;
        }
        self.children.remove(name);
        Ok(())
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

        self.terminate_owned_child(name)?;
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

    pub(crate) fn stop_owned_backend(&mut self, name: &str) -> Result<BridgeVmResponse> {
        self.cleanup_owned_backend(name, true)
    }

    pub(crate) fn restart_owned_backend(&mut self, name: &str) -> Result<BridgeVmResponse> {
        self.cleanup_owned_backend(name, true)?;
        self.spawn_backend(name)
    }
}
