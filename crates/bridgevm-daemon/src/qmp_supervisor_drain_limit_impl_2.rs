//! Continuation of the `qmp_supervisor_drain_limit` impl block, split for the 1000-line rule.

use super::*;

use anyhow::Context;
use anyhow::Result;
use bridgevm_agent_protocol::AgentEnvelope;
use bridgevm_agent_protocol::AgentMessage;
use bridgevm_agent_protocol::DEFAULT_BENCHMARK_DURATION_MILLIS;
use bridgevm_agent_protocol::MAX_BENCHMARK_DURATION_MILLIS;
use bridgevm_agentd::write_envelope_line;
use bridgevm_api::build_compatibility_resume_command;
use bridgevm_api::compat_suspend_marker_path;
use bridgevm_api::compatibility_launch_dependency_blockers;
use bridgevm_api::compatibility_launch_readiness_metadata;
use bridgevm_api::create_performance_sample;
use bridgevm_api::fast_suspend_state_path;
use bridgevm_api::guest_tools_freeze_filesystem_envelope;
use bridgevm_api::inspect_guest_tools_status;
use bridgevm_api::resume_backend;
use bridgevm_api::suspend_backend;
use bridgevm_api::verify_compatibility_resume_loaded;
use bridgevm_api::ApplicationConsistentSnapshotExecutionRecord;
use bridgevm_api::BridgeVmResponse;
use bridgevm_api::CurrentRuntimeEngine;
use bridgevm_api::GuestToolsCommandRecord;
use bridgevm_api::SnapshotConsistency;
use bridgevm_qemu::assign_free_vnc_display;
use bridgevm_storage::RunnerMetadata;
use bridgevm_storage::SnapshotKind;
use bridgevm_storage::VmRuntimeState;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Stdio;
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

    pub(crate) fn send_guest_tools_command(
        &mut self,
        name: &str,
        envelope: AgentEnvelope,
    ) -> Result<BridgeVmResponse> {
        Ok(BridgeVmResponse::GuestToolsCommand {
            command: self.send_guest_tools_command_record(name, envelope)?,
        })
    }

    pub(crate) fn send_guest_tools_command_record(
        &mut self,
        name: &str,
        envelope: AgentEnvelope,
    ) -> Result<GuestToolsCommandRecord> {
        let backend = self
            .children
            .get_mut(name)
            .with_context(|| format!("backend is not owned by this daemon for '{name}'"))?;
        let session = backend
            .guest_tools
            .as_ref()
            .with_context(|| format!("guest tools session is not connected for '{name}'"))?;
        let stream = backend
            .guest_tools_stream
            .as_mut()
            .with_context(|| format!("guest tools stream is not connected for '{name}'"))?;

        backend
            .guest_tools_commands
            .begin_host_command(session, &envelope)
            .map_err(|error| anyhow::anyhow!("guest tools command rejected: {error:?}"))?;
        write_envelope_line(stream.get_mut(), &envelope)
            .map_err(|error| anyhow::anyhow!("failed to write guest tools command: {error:?}"))?;

        Ok(GuestToolsCommandRecord {
            vm: name.to_string(),
            request_id: envelope.request_id,
            pending_commands: backend.guest_tools_commands.pending_count(),
        })
    }

    pub(crate) fn create_performance_sample_with_optional_guest_benchmark(
        &mut self,
        name: &str,
        output: PathBuf,
        artifact_bytes: Option<u64>,
        iterations: Option<u16>,
        sync: bool,
    ) -> Result<BridgeVmResponse> {
        let mut sample =
            create_performance_sample(&self.store, name, output, artifact_bytes, iterations, sync)
                .map_err(anyhow::Error::msg)?;

        match self.run_guest_benchmark_for_sample(name, sample.created_at_unix) {
            Ok(Some(completed)) => record_guest_benchmark_result(&mut sample, &completed),
            Ok(None) => sample.notes.push(
                "guest benchmark skipped because no benchmark-capable guest-tools session was connected"
                    .to_string(),
            ),
            Err(error) => sample
                .notes
                .push(format!("guest benchmark skipped: {error}")),
        }

        if let Ok(status) = inspect_guest_tools_status(&self.store, name) {
            sample.metrics = status
                .runtime
                .as_ref()
                .and_then(|runtime| runtime.metrics.clone());
            sample.guest_tools = status;
        }
        fs::write(
            &sample.artifact,
            serde_json::to_string_pretty(&sample).context("failed to serialize sample")?,
        )
        .with_context(|| {
            format!(
                "failed to update performance sample metadata at {}",
                sample.artifact.display()
            )
        })?;

        Ok(BridgeVmResponse::PerformanceSample { sample })
    }

    pub(crate) fn run_guest_benchmark_for_sample(
        &mut self,
        name: &str,
        created_at_unix: u64,
    ) -> Result<Option<CompletedGuestToolsCommand>> {
        let supports_benchmark = self
            .children
            .get(name)
            .and_then(|backend| backend.guest_tools.as_ref())
            .is_some_and(|session| session.supports("benchmark"));
        if !supports_benchmark {
            return Ok(None);
        }

        let request_id = format!("performance-sample:{created_at_unix}:guest-benchmark");
        let envelope = AgentEnvelope::with_request_id(
            AgentMessage::RunBenchmark {
                duration_millis: Some(DEFAULT_BENCHMARK_DURATION_MILLIS),
            },
            request_id.clone(),
        );
        self.send_guest_tools_command_record(name, envelope)?;
        self.wait_for_guest_tools_command_result(
            name,
            &request_id,
            Duration::from_millis(MAX_BENCHMARK_DURATION_MILLIS.saturating_add(5_000)),
        )
        .map(Some)
    }

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
}
