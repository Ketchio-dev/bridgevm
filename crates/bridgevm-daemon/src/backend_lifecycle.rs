//! Suspend, resume, stop, restart and cleanup of daemon-owned backends, and VNC display arbitration.

use crate::*;
use anyhow::Context;
use anyhow::Result;
use bridgevm_api::build_compatibility_resume_command;
use bridgevm_api::compat_suspend_marker_path;
use bridgevm_api::compatibility_launch_dependency_blockers;
use bridgevm_api::compatibility_launch_readiness_metadata;
use bridgevm_api::fast_suspend_state_path;
use bridgevm_api::resume_backend;
use bridgevm_api::suspend_backend;
use bridgevm_api::verify_compatibility_resume_loaded;
use bridgevm_api::BridgeVmResponse;
use bridgevm_api::CurrentRuntimeEngine;
use bridgevm_qemu::assign_free_vnc_display;
use bridgevm_qemu::qmp_socket_path;
use bridgevm_qemu::quit as qmp_quit;
use bridgevm_qemu::vnc_display_in_command;
use bridgevm_storage::RunnerMetadata;
use bridgevm_storage::VmRuntimeState;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;
use std::process::Command;
use std::process::Stdio;
use std::thread;
use std::time::Duration;

impl DaemonState {
    /// Suspend a backend through the daemon.
    ///
    /// Suspend is synchronous (pause -> save state -> quit). If the daemon owns
    /// the child, drop our `Child`/QMP handles first (without killing) so the
    /// api suspend path can drive QMP and terminate the recorded pid without the
    /// reconcile loop racing it. The api suspend path leaves the VM `suspended`.
    pub(crate) fn suspend_backend_supervised(&mut self, name: &str) -> Result<BridgeVmResponse> {
        // Release the owned handles before the synchronous suspend so the
        // supervisor does not poll/clear state underneath it. The api suspend
        // path is responsible for terminating the recorded pid.
        self.children.remove(name);
        let metadata = suspend_backend(&self.store, name).map_err(anyhow::Error::msg)?;
        Ok(BridgeVmResponse::RunnerStatus {
            metadata: Some(metadata),
            qmp_supervisor: self
                .store
                .qmp_supervisor_metadata(name)
                .context("failed to read QMP supervisor metadata")?,
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

    pub(crate) fn resume_compatibility_supervised(
        &mut self,
        name: &str,
        bundle: &Path,
        manifest: &bridgevm_config::VmManifest,
    ) -> Result<BridgeVmResponse> {
        let marker_path = compat_suspend_marker_path(bundle, name);
        if !marker_path.exists() {
            anyhow::bail!(
                "no saved Compatibility Mode state to resume from at {}; suspend the VM first",
                marker_path.display()
            );
        }
        let (disk, active_disk) = self
            .store
            .prepare_active_disk(name)
            .context("failed to prepare active disk")?;
        if !disk.exists {
            anyhow::bail!("active disk is not ready: {}", disk.path.display());
        }
        let readiness = compatibility_launch_readiness_metadata(
            &disk,
            compatibility_launch_dependency_blockers(manifest, bundle),
        );
        if !readiness.ready {
            anyhow::bail!(
                "Compatibility Mode launch readiness failed: {}",
                launch_readiness_blocker_summary(&readiness)
            );
        }

        let mut command = build_compatibility_resume_command(manifest, bundle)
            .map_err(|error| anyhow::anyhow!("{error}"))?;
        // Pin a free VNC display so a resumed Compat VM doesn't collide on 5900,
        // avoiding displays already owned by this daemon's live children.
        let avoid = self.live_vnc_displays();
        assign_free_vnc_display(&mut command, &avoid).map_err(|error| anyhow::anyhow!(error))?;
        let log_path = bundle.join("logs").join("qemu.log");
        let guest_tools = self
            .store
            .guest_tools_runner_metadata(name)
            .context("failed to prepare guest tools runner metadata")?;
        fs::create_dir_all(bundle.join("logs")).context("failed to create VM log directory")?;
        let stdout = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .context("failed to open QEMU log file")?;
        let stderr = stdout
            .try_clone()
            .context("failed to clone QEMU log file")?;
        let mut child = Command::new(&command.program)
            .args(&command.args)
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr))
            .spawn()
            .with_context(|| format!("failed to spawn {}", command.program))?;
        verify_compatibility_resume_loaded(&mut child, bundle, &log_path)
            .map_err(anyhow::Error::msg)?;

        let metadata = RunnerMetadata {
            engine: "fullvm".to_string(),
            pid: Some(child.id()),
            command: command.render_shell_words(),
            log_path,
            started_at_unix: now_unix(),
            dry_run: false,
            launch_spec_path: None,
            guest_tools: Some(guest_tools),
            disk: Some(disk),
            active_disk: Some(active_disk),
            launch_readiness: None,
            runtime_control: None,
        };
        self.store
            .write_runner_metadata(name, &metadata)
            .context("failed to write runner metadata")?;
        // Resume marker consumed.
        let _ = fs::remove_file(&marker_path);
        self.store
            .transition_state(name, VmRuntimeState::Running)
            .context("failed to mark VM running")?;
        self.children
            .insert(name.to_string(), SupervisedBackend::new(child));

        Ok(BridgeVmResponse::RunnerStatus {
            metadata: Some(metadata),
            qmp_supervisor: self
                .store
                .qmp_supervisor_metadata(name)
                .context("failed to read QMP supervisor metadata")?,
        })
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

    pub(crate) fn stop_owned_backend(&mut self, name: &str) -> Result<BridgeVmResponse> {
        self.cleanup_owned_backend(name, true)
    }

    pub(crate) fn restart_owned_backend(&mut self, name: &str) -> Result<BridgeVmResponse> {
        self.cleanup_owned_backend(name, true)?;
        Ok(BridgeVmResponse::State {
            name: name.to_string(),
            metadata: self
                .store
                .transition_state(name, VmRuntimeState::Running)
                .context("failed to mark VM running after restart")?,
        })
    }
}
