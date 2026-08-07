//! Spawning the VM helper process from a validated manifest -- no shell.
//!
//! The wrapper script builds the helper's environment from CLI flags and
//! scrubs inherited `BRIDGEVM_*` as a policy boundary. The product runtime
//! goes further: `env_clear()` plus an explicit allowlist derived only from
//! the parsed manifest and the host launch facts, so the helper's entire
//! configuration is this function's return value. Product mode always sets
//! `BRIDGEVM_EXIT_ON_RESET=1` -- in-process reboot is diagnostic-only
//! (PLAN.md R1); a guest SYSTEM_RESET must end the process for recreation.

use crate::manifest::LaunchManifest;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

/// Host facts a launch needs beyond the manifest: which helper binary,
/// which firmware code image, how long a boot may stall, and (optionally)
/// the agent console control file through which the host drives the guest.
pub struct HelperLaunch {
    pub helper: PathBuf,
    pub firmware_code: PathBuf,
    pub watchdog_ms: u64,
    /// When set, the helper runs the resident agent console service and
    /// reads guest commands appended to this file. This is how a supervisor
    /// asks the guest for a reset without any shell in between.
    pub agent_control: Option<PathBuf>,
}

/// The helper's complete environment. Nothing else reaches the child.
pub fn helper_env(manifest: &LaunchManifest, launch: &HelperLaunch) -> Vec<(&'static str, String)> {
    let mut env = vec![
        ("BRIDGEVM_NVME_DISK", manifest.disk().to_string()),
        ("BRIDGEVM_NVME_DISK_WRITABLE", "1".to_string()),
        (
            "BRIDGEVM_AARCH64_UEFI_VARS",
            manifest.uefi_vars().to_string(),
        ),
        ("BRIDGEVM_AARCH64_UEFI_VARS_WRITABLE", "1".to_string()),
        (
            "BRIDGEVM_AARCH64_UEFI_CODE",
            launch.firmware_code.display().to_string(),
        ),
        ("BRIDGEVM_RAM_MIB", manifest.ram_mib().to_string()),
        ("BRIDGEVM_SMP_CPUS", manifest.vcpus().to_string()),
        (
            "BRIDGEVM_BOOT_PROBE_WATCHDOG_MS",
            launch.watchdog_ms.to_string(),
        ),
        ("BRIDGEVM_EXIT_ON_RESET", "1".to_string()),
    ];
    if let Some(control) = &launch.agent_control {
        env.push(("BRIDGEVM_VIRTIO_CONSOLE", "1".to_string()));
        env.push(("BRIDGEVM_VIRTIO_CONSOLE_TEST", "1".to_string()));
        env.push(("BRIDGEVM_VIRTIO_CONSOLE_TEST_PERIODIC", "1".to_string()));
        env.push(("BRIDGEVM_VIRTIO_CONSOLE_CMDS", "whoami".to_string()));
        env.push((
            "BRIDGEVM_VIRTIO_CONSOLE_TEST_TIMEOUT_MS",
            launch.watchdog_ms.to_string(),
        ));
        env.push(("BRIDGEVM_VIRTIO_CONSOLE_SERVICE", "1".to_string()));
        env.push(("BRIDGEVM_VIRTIO_CONSOLE_CTL", control.display().to_string()));
    }
    env
}

/// Spawn one helper generation: argv only, cleared environment, stdio
/// inherited so boot evidence lands in the supervisor's own transcript.
pub fn spawn_helper(
    manifest: &LaunchManifest,
    launch: &HelperLaunch,
    generation: u64,
) -> std::io::Result<Child> {
    let mut command = Command::new(&launch.helper);
    command
        .env_clear()
        .envs(
            helper_env(manifest, launch)
                .into_iter()
                .map(|(k, v)| (k.to_string(), v)),
        )
        .env("BRIDGEVM_RESET_GENERATION", generation.to_string())
        .stdin(Stdio::null());
    command.spawn()
}

#[cfg(test)]
#[path = "vm_process_tests.rs"]
mod tests;
