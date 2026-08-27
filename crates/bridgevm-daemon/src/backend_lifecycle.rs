//! Suspend, resume, stop, restart and cleanup of daemon-owned backends.

use crate::*;
use anyhow::Context;
use anyhow::Result;
use bridgevm_api::fast_suspend_state_path;
use bridgevm_api::suspend_backend;
use bridgevm_api::BridgeVmResponse;
use bridgevm_api::CurrentRuntimeEngine;

#[path = "backend_lifecycle/cleanup.rs"]
mod cleanup;
#[path = "backend_lifecycle/vnc_displays.rs"]
mod vnc_displays;

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
            self.children.remove(name);
        }
        Ok(BridgeVmResponse::RunnerStatus {
            metadata: Some(metadata),
            qmp_supervisor,
        })
    }

    /// Resume a backend through the daemon, tracking the new child in the
    /// supervisor exactly like cold-start `run` so reconcile/stop see it.
    ///
    /// Fast Mode relaunches `lightvm-runner` with `--apple-vz-restore-state`
    /// only when real start is explicitly enabled. Compatibility Mode relaunches
    /// QEMU with `-loadvm <tag>`. Both children stay daemon-owned.
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
                let Some(config) = FastModeSpawnConfig::from_env()? else {
                    anyhow::bail!(
                        "Fast Mode resume requires explicit real-start opt-in \
                         (BRIDGEVM_APPLE_VZ_ALLOW_REAL_START=1); refusing to launch a detached backend"
                    );
                };
                self.spawn_fast_backend_with_restore(
                    name,
                    bundle,
                    manifest,
                    config,
                    Some(state_path),
                )
            }
            CurrentRuntimeEngine::QemuCompatibility => {
                self.resume_compatibility_supervised(name, &bundle, &manifest)
            }
        }
    }
}
