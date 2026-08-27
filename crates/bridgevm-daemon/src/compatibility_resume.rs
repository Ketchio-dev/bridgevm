//! Supervised Compatibility Mode resume and pre-ownership child cleanup.

use crate::*;
use anyhow::{Context, Result};
use bridgevm_api::{
    build_compatibility_resume_command, compat_suspend_marker_path,
    compatibility_launch_dependency_blockers, compatibility_launch_readiness_metadata,
    verify_compatibility_resume_loaded, BridgeVmResponse,
};
use bridgevm_qemu::assign_free_vnc_display;
use bridgevm_storage::{RunnerMetadata, VmRuntimeState};
use std::fs;
use std::path::Path;
use std::process::{Child, Command, Stdio};

struct PendingChild {
    child: Option<Child>,
}

impl PendingChild {
    fn new(child: Child) -> Self {
        Self { child: Some(child) }
    }

    fn child_mut(&mut self) -> &mut Child {
        self.child.as_mut().expect("pending child must exist")
    }

    fn id(&self) -> u32 {
        self.child.as_ref().expect("pending child must exist").id()
    }

    fn commit(mut self) -> Child {
        self.child.take().expect("pending child must exist")
    }
}

impl Drop for PendingChild {
    fn drop(&mut self) {
        let Some(child) = self.child.as_mut() else {
            return;
        };
        if child.try_wait().ok().flatten().is_none() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl DaemonState {
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
        let avoid = self.live_vnc_displays();
        assign_free_vnc_display(&mut command, &avoid).map_err(|error| anyhow::anyhow!(error))?;
        let log_path = bundle.join("logs").join("qemu.log");
        let guest_tools = self
            .store
            .guest_tools_runner_metadata(name)
            .context("failed to prepare guest tools runner metadata")?;
        let qmp_supervisor = self
            .store
            .qmp_supervisor_metadata(name)
            .context("failed to read QMP supervisor metadata")?;
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
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr))
            .spawn()
            .with_context(|| format!("failed to spawn {}", command.program))?;
        let mut pending_child = PendingChild::new(child);
        verify_compatibility_resume_loaded(pending_child.child_mut(), bundle, &log_path)
            .map_err(anyhow::Error::msg)?;

        let metadata = RunnerMetadata {
            engine: "fullvm".to_string(),
            pid: Some(pending_child.id()),
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
        if let Err(error) = self.store.transition_state(name, VmRuntimeState::Running) {
            let rollback = self.store.clear_runner_metadata(name);
            if let Err(rollback_error) = rollback {
                anyhow::bail!(
                    "failed to mark VM running: {error}; runner metadata rollback also failed: {rollback_error}"
                );
            }
            anyhow::bail!("failed to mark VM running: {error}");
        }

        let child = pending_child.commit();
        self.children
            .insert(name.to_string(), SupervisedBackend::new(child));
        if let Err(error) = fs::remove_file(&marker_path) {
            eprintln!(
                "bridgevmd resume: VM '{name}' is running but the consumed suspend marker could not be removed at {}: {error}",
                marker_path.display()
            );
        }

        Ok(BridgeVmResponse::RunnerStatus {
            metadata: Some(metadata),
            qmp_supervisor,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::thread;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    fn marker_path(label: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        env::temp_dir().join(format!("bridgevm-{label}-{}-{nonce}", std::process::id()))
    }

    #[test]
    fn uncommitted_resume_child_is_killed_on_drop() {
        let marker = marker_path("pending-child");
        let _ = fs::remove_file(&marker);
        let child = Command::new("sh")
            .arg("-c")
            .arg(r#"sleep 0.2; : > "$1""#)
            .arg("bridgevm-pending-child")
            .arg(&marker)
            .spawn()
            .unwrap();

        drop(PendingChild::new(child));
        thread::sleep(Duration::from_millis(300));

        assert!(!marker.exists(), "uncommitted child escaped cleanup");
    }

    #[test]
    fn committed_resume_child_is_transferred_to_the_caller() {
        let child = Command::new("sh").arg("-c").arg("exit 0").spawn().unwrap();
        let mut child = PendingChild::new(child).commit();
        assert!(child.wait().unwrap().success());
    }
}
