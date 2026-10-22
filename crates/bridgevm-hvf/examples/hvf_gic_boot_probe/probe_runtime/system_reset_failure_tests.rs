use super::*;
use crate::probe_exit_code;
use bridgevm_hvf::dtb::VirtFdtConfig;
use bridgevm_hvf::platform_virt::VirtPlatformConfig;
use bridgevm_hvf::tpm_tis::{Tpm2Backend, TpmBackendError};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::process::ExitCode;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

const CMD_INIT_REJECTED: &str = "swtpm CMD_INIT returned control result 0x00000001";

#[derive(Debug)]
struct CountingTpm {
    resets: Arc<AtomicUsize>,
    fail_reset: bool,
}

impl Tpm2Backend for CountingTpm {
    fn execute(&mut self, _locality: u8, _command: &[u8]) -> Result<Vec<u8>, TpmBackendError> {
        Ok(vec![0x80, 0x01, 0, 0, 0, 10, 0, 0, 0, 0])
    }

    fn reset(&mut self) -> Result<(), TpmBackendError> {
        self.resets.fetch_add(1, Ordering::SeqCst);
        if self.fail_reset {
            return Err(TpmBackendError::Protocol(CMD_INIT_REJECTED.into()));
        }
        Ok(())
    }
}

fn platform_with_tpm(fail_reset: bool) -> (Mutex<VirtPlatform>, Arc<AtomicUsize>) {
    let resets = Arc::new(AtomicUsize::new(0));
    let mut config = VirtPlatformConfig::new(VirtFdtConfig::default());
    config.devices.tpm_tis_present = true;
    let backend = CountingTpm {
        resets: resets.clone(),
        fail_reset,
    };
    let platform = VirtPlatform::new_with_config_and_tpm_backend(config, Some(Box::new(backend)));
    (Mutex::new(platform), resets)
}

#[test]
fn tpm_reset_failure_stops_the_run_without_panicking_or_poisoning_the_platform() {
    let (platform, resets) = platform_with_tpm(true);

    let outcome = catch_unwind(AssertUnwindSafe(|| {
        system_reset_failure(0, true, &platform)
    }));

    assert!(
        !platform.is_poisoned(),
        "the final report must still be able to lock the platform"
    );
    let reason = outcome
        .expect("a TPM backend reset failure must not panic the VMM")
        .expect("a TPM backend reset failure must stop the run");
    assert_eq!(
        reason,
        format!("TPM backend reset failed during PSCI SYSTEM_RESET: {CMD_INIT_REJECTED}")
    );
    assert_eq!(resets.load(Ordering::SeqCst), 1);
    // The reboot arm turns the reason into fatal_reset_error.
    assert_eq!(probe_exit_code(false, true, false), ExitCode::FAILURE);
}

#[test]
fn gic_reset_failure_stops_before_the_platform_is_reset() {
    let (platform, resets) = platform_with_tpm(false);

    assert_eq!(
        system_reset_failure(0x1, true, &platform).as_deref(),
        Some("hv_gic_reset failed during PSCI SYSTEM_RESET: 0x1")
    );
    assert_eq!(resets.load(Ordering::SeqCst), 0);
}

#[test]
fn successful_resets_let_the_reboot_continue() {
    let (platform, resets) = platform_with_tpm(false);

    assert_eq!(system_reset_failure(0, true, &platform), None);
    assert_eq!(resets.load(Ordering::SeqCst), 1);
    assert_eq!(system_reset_failure(0, false, &platform), None);
    assert_eq!(
        resets.load(Ordering::SeqCst),
        1,
        "no platform reset requested"
    );
}
