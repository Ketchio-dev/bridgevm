//! Continuation of the `qmp_supervisor_drain_limit` impl block, split for the 1000-line rule.

use super::*;

use anyhow::Context;
use anyhow::Result;
use bridgevm_api::apply_power_aware_fast_resources;
use bridgevm_api::launch_readiness_metadata;
use bridgevm_api::BridgeVmResponse;
use bridgevm_apple_vz::build_fast_plan;
use bridgevm_apple_vz::write_launch_spec_artifact;
use bridgevm_storage::RunnerMetadata;
use bridgevm_storage::VmRuntimeState;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::process::Stdio;

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
