//! Cold-start spawn of the Fast Mode and Compatibility backends, including readiness gating.

use crate::*;
use anyhow::Context;
use anyhow::Result;
use bridgevm_api::add_fast_spawn_runner_required_blocker;
use bridgevm_api::apply_power_aware_fast_resources;
use bridgevm_api::compatibility_launch_dependency_blockers;
use bridgevm_api::compatibility_launch_readiness_metadata;
use bridgevm_api::fast_spawn_runner_required_error;
use bridgevm_api::handle_request;
use bridgevm_api::launch_readiness_metadata;
use bridgevm_api::BridgeVmRequest;
use bridgevm_api::BridgeVmResponse;
use bridgevm_api::CurrentRuntimeEngine;
use bridgevm_apple_vz::build_fast_plan;
use bridgevm_apple_vz::write_launch_spec_artifact;
use bridgevm_qemu::assign_free_vnc_display;
use bridgevm_qemu::build_compatibility_command;
use bridgevm_storage::LaunchReadinessMetadata;
use bridgevm_storage::RunnerMetadata;
use bridgevm_storage::VmRuntimeState;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::process::Stdio;

pub(crate) fn launch_readiness_blocker_summary(readiness: &LaunchReadinessMetadata) -> String {
    if readiness.blockers.is_empty() {
        return "unknown blocker".to_string();
    }
    readiness
        .blockers
        .iter()
        .map(|blocker| {
            let mut summary = format!("{}: {}", blocker.code, blocker.message);
            if let Some(path) = &blocker.path {
                summary.push_str(&format!(" ({})", path.display()));
            } else if let Some(capability) = &blocker.capability {
                summary.push_str(&format!(" ({capability})"));
            }
            summary
        })
        .collect::<Vec<_>>()
        .join(", ")
}

impl DaemonState {
    pub(crate) fn spawn_fast_backend(
        &mut self,
        name: &str,
        bundle: PathBuf,
        manifest: bridgevm_config::VmManifest,
        config: FastModeSpawnConfig,
    ) -> Result<BridgeVmResponse> {
        self.spawn_fast_backend_with_restore(name, bundle, manifest, config, None)
    }

    pub(crate) fn spawn_fast_backend_with_restore(
        &mut self,
        name: &str,
        bundle: PathBuf,
        mut manifest: bridgevm_config::VmManifest,
        config: FastModeSpawnConfig,
        restore_state: Option<PathBuf>,
    ) -> Result<BridgeVmResponse> {
        config.validate()?;
        // Battery-adaptive `auto` resources on a fresh cold start (the app's
        // primary path goes through the daemon). Not on resume: a restored VM
        // must reuse the exact saved-state config.
        if restore_state.is_none() {
            apply_power_aware_fast_resources(&mut manifest);
        }
        let (disk, active_disk) = self
            .store
            .prepare_active_disk(name)
            .context("failed to prepare active disk")?;
        let plan = build_fast_plan(&manifest, &bundle).context("failed to build Apple VZ plan")?;
        let launch_spec_path = write_launch_spec_artifact(&bundle, plan.launch_spec())
            .context("failed to write Fast Mode Apple VZ launch spec")?;
        let readiness = launch_readiness_metadata(&plan.launch_spec().readiness);
        if !readiness.ready {
            anyhow::bail!(
                "Fast Mode launch readiness failed: {}",
                launch_readiness_blocker_summary(&readiness)
            );
        }

        fs::create_dir_all(bundle.join("logs")).context("failed to create VM log directory")?;
        let log_path: PathBuf = plan.launch_spec().logs.runner_log_path.clone().into();
        let stdout = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .context("failed to open Apple VZ runner log file")?;
        let stderr = stdout
            .try_clone()
            .context("failed to clone Apple VZ runner log file")?;

        let args = config.runner_args_with_restore(&launch_spec_path, restore_state.as_deref());
        let mut child = Command::new(&config.lightvm_runner);
        child.args(&args);
        child.env("BRIDGEVM_APPLE_VZ_ALLOW_REAL_START", "1");
        let child = child
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr))
            .spawn()
            .with_context(|| {
                format!(
                    "failed to spawn Fast Mode runner {}",
                    config.lightvm_runner.display()
                )
            })?;

        let mut command = vec![config.lightvm_runner.display().to_string()];
        command.extend(args);
        let metadata = RunnerMetadata {
            engine: "lightvm".to_string(),
            pid: Some(child.id()),
            command,
            log_path,
            started_at_unix: now_unix(),
            dry_run: false,
            launch_spec_path: Some(launch_spec_path),
            guest_tools: None,
            disk: Some(disk),
            active_disk: Some(active_disk),
            launch_readiness: Some(readiness),
            runtime_control: None,
        };
        self.store
            .write_runner_metadata(name, &metadata)
            .context("failed to write Fast Mode runner metadata")?;
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

    pub(crate) fn spawn_backend(&mut self, name: &str) -> Result<BridgeVmResponse> {
        if self.children.contains_key(name) {
            anyhow::bail!("backend is already running for '{name}'");
        }

        let (bundle, manifest, _) = self
            .store
            .get_vm_with_active_disk(name)
            .context("failed to read VM")?;
        let runtime_engine = CurrentRuntimeEngine::for_manifest(&manifest);
        if runtime_engine == CurrentRuntimeEngine::AppleVz {
            if let Some(config) = FastModeSpawnConfig::from_env()? {
                return self.spawn_fast_backend(name, bundle, manifest, config);
            }

            let response = handle_request(
                &self.store,
                BridgeVmRequest::RunBackend {
                    name: name.to_string(),
                    spawn: false,
                },
            )
            .into_result()
            .map_err(anyhow::Error::msg)?;
            let BridgeVmResponse::RunnerStatus {
                metadata: Some(mut metadata),
                ..
            } = response
            else {
                anyhow::bail!("Fast Mode dry-run planning did not return runner metadata");
            };
            let readiness =
                metadata
                    .launch_readiness
                    .get_or_insert_with(|| LaunchReadinessMetadata {
                        ready: false,
                        blockers: Vec::new(),
                    });
            add_fast_spawn_runner_required_blocker(readiness);
            let spawn_error = fast_spawn_runner_required_error(readiness);
            self.store
                .write_runner_metadata(name, &metadata)
                .context("failed to write Fast Mode runner metadata")?;
            anyhow::bail!("{}", spawn_error);
        }
        let (disk, active_disk) = self
            .store
            .prepare_active_disk(name)
            .context("failed to prepare active disk")?;
        if !disk.exists {
            if let Some(command) = &disk.create_command {
                anyhow::bail!(
                    "active disk is not ready: {}; create it with: {}",
                    disk.path.display(),
                    command.join(" ")
                );
            }
            anyhow::bail!("active disk is not ready: {}", disk.path.display());
        }

        let mut command = build_compatibility_command(&manifest, &bundle)
            .map_err(|error| anyhow::anyhow!("{}", compatibility_qemu_command_error(error)))?;
        let readiness = compatibility_launch_readiness_metadata(
            &disk,
            compatibility_launch_dependency_blockers(&manifest, &bundle),
        );
        if !readiness.ready {
            anyhow::bail!(
                "Compatibility Mode launch readiness failed: {}",
                launch_readiness_blocker_summary(&readiness)
            );
        }
        // Pin this VM to a free VNC display so concurrent Compat VMs don't
        // collide on TCP 5900. Avoid displays already handed to live children
        // (their QEMU may not have bound the port yet, so a bare probe would
        // hand the same :0 to two back-to-back launches).
        let avoid = self.live_vnc_displays();
        assign_free_vnc_display(&mut command, &avoid).map_err(|error| anyhow::anyhow!(error))?;
        // Test-only escape hatch (mirrors BRIDGEVM_APPLE_VZ_RUNNER): append extra
        // QEMU args without touching the product command builder. The
        // application-consistent live opt-in smoke uses this to attach a NoCloud
        // cidata seed ISO so a daemon-owned guest can boot the agent. Args are
        // shell-word split; empty/unset means no change.
        let extra_compat_args = compat_extra_qemu_args();
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
        let child = Command::new(&command.program)
            .args(&command.args)
            .args(&extra_compat_args)
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr))
            .spawn()
            .with_context(|| format!("failed to spawn {}", command.program))?;

        let metadata = RunnerMetadata {
            engine: runtime_engine.runner_metadata_engine().to_string(),
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
}
