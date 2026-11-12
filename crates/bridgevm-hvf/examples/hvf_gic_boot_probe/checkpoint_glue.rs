use std::env;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use bridgevm_hvf::checkpoint::{request_vcpu_exit, HvVcpu, VmCheckpoint};
use bridgevm_hvf::platform_virt::VirtPlatform;

const CHECKPOINT_ENV: &str = "BRIDGEVM_CHECKPOINT_STATE";
const RESTORE_ENV: &str = "BRIDGEVM_RESTORE_STATE";

static CHECKPOINT_WRITTEN: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Default)]
pub struct CheckpointEnv {
    pub checkpoint: Option<PathBuf>,
    pub restore: Option<PathBuf>,
}

impl CheckpointEnv {
    pub(crate) fn validate_cpu_count(&self, cpus: u64) -> io::Result<()> {
        if cpus > 1 && (self.checkpoint.is_some() || self.restore.is_some()) {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "checkpoints need a single-CPU run (BRIDGEVM_SMP_CPUS=1)",
            ));
        }
        Ok(())
    }

    pub fn from_env() -> Self {
        Self {
            checkpoint: nonempty_path(CHECKPOINT_ENV),
            restore: nonempty_path(RESTORE_ENV),
        }
    }
}

/// Stop the supplied vCPUs, capture RAM/HVF/GIC/device state, and write it.
///
/// The current probe creates secondary vCPUs on separate owner threads, while
/// Apple's register APIs must be called by the owning thread. Until the probe
/// gains an owner-thread checkpoint rendezvous, this glue deliberately accepts
/// only the default single-vCPU configuration.
pub fn quiesce_and_capture(
    path: &Path,
    vcpus: &[HvVcpu],
    guest_ram: &[u8],
    platform: &mut VirtPlatform,
) -> io::Result<()> {
    require_probe_owned_vcpus(vcpus)?;
    request_vcpu_exit(vcpus)?;

    let device_state = platform.snapshot_state();
    let checkpoint = VmCheckpoint::capture(vcpus, guest_ram, device_state)?;
    checkpoint.write_to_path(path)
}

/// Restore RAM, platform devices, Apple's GIC state, and vCPU registers.
///
/// The VM, GIC, vCPU, media backends, and guest RAM mapping must already exist,
/// but no restored vCPU may have entered hv_vcpu_run.
pub fn restore(
    path: &Path,
    vcpus: &[HvVcpu],
    guest_ram: &mut [u8],
    platform: &mut VirtPlatform,
) -> io::Result<()> {
    require_probe_owned_vcpus(vcpus)?;

    let checkpoint = VmCheckpoint::read_from_path(path)?;
    platform.restore_state(&checkpoint.device_state);
    checkpoint.restore_hvf(vcpus, guest_ram)?;
    // A later pause adds to the restored offset, not to zero.
    let offset = checkpoint.vcpus.first().map_or(0, |vcpu| vcpu.vtimer_offset);
    crate::pause_gate::GATE.seed_offset(offset);
    Ok(())
}

pub fn restore_if_requested(
    vcpus: &[HvVcpu],
    guest_ram: &mut [u8],
    platform: &mut VirtPlatform,
) -> io::Result<bool> {
    let Some(path) = CheckpointEnv::from_env().restore else {
        return Ok(false);
    };

    restore(&path, vcpus, guest_ram, platform)?;
    println!("VM checkpoint restored: {}", path.display());
    Ok(true)
}

/// One-shot desktop-ready checkpoint hook.
pub fn checkpoint_if_requested(
    vcpus: &[HvVcpu],
    guest_ram: &[u8],
    platform: &mut VirtPlatform,
) -> io::Result<bool> {
    let Some(path) = CheckpointEnv::from_env().checkpoint else {
        return Ok(false);
    };

    if CHECKPOINT_WRITTEN
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Ok(false);
    }

    match quiesce_and_capture(&path, vcpus, guest_ram, platform) {
        Ok(()) => {
            println!("VM checkpoint written: {}", path.display());
            Ok(true)
        }
        Err(error) => {
            CHECKPOINT_WRITTEN.store(false, Ordering::SeqCst);
            Err(error)
        }
    }
}

fn require_probe_owned_vcpus(vcpus: &[HvVcpu]) -> io::Result<()> {
    if vcpus.len() == 1 {
        return Ok(());
    }

    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        concat!(
            "the current probe's secondary HVF vCPUs are owned by separate threads; ",
            "checkpoint/restore currently requires BRIDGEVM_SMP_CPUS=1"
        ),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkpoint_and_restore_refuse_smp_before_vm_creation() {
        for request in [
            CheckpointEnv { checkpoint: Some("capture".into()), restore: None },
            CheckpointEnv { checkpoint: None, restore: Some("restore".into()) },
        ] {
            let error = request.validate_cpu_count(4).expect_err("SMP checkpoints must be refused");
            assert_eq!(error.to_string(), "checkpoints need a single-CPU run (BRIDGEVM_SMP_CPUS=1)");
            assert!(request.validate_cpu_count(1).is_ok());
        }
        assert!(CheckpointEnv::default().validate_cpu_count(4).is_ok());
    }
}

fn nonempty_path(name: &str) -> Option<PathBuf> {
    env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}
