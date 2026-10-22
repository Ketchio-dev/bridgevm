//! A TPM backend whose power cycle fails during a guest-requested reset.

use super::super::*;
use crate::dtb::VirtFdtConfig;
use crate::machine;
use crate::tpm_tis::{Tpm2Backend, TpmBackendError};

#[derive(Debug)]
struct FailingResetTpm;

impl Tpm2Backend for FailingResetTpm {
    fn execute(&mut self, _locality: u8, _command: &[u8]) -> Result<Vec<u8>, TpmBackendError> {
        Ok(vec![0x80, 0x01, 0, 0, 0, 10, 0, 0, 0, 0])
    }

    fn reset(&mut self) -> Result<(), TpmBackendError> {
        Err(TpmBackendError::Protocol(
            "swtpm CMD_INIT returned control result 0x00000001".into(),
        ))
    }
}

#[test]
fn tpm_reset_failure_is_returned_before_any_device_is_reset() {
    let mut config = VirtPlatformConfig::new(VirtFdtConfig::default());
    config.devices.tpm_tis_present = true;
    let mut p =
        VirtPlatform::new_with_config_and_tpm_backend(config, Some(Box::new(FailingResetTpm)));
    let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0);
    let serial = MmioOp::Write {
        size: 1,
        value: u64::from(b'X'),
    };
    assert_eq!(
        p.on_mmio(machine::UART.base, serial, &mut mem),
        MmioOutcome::WriteAck
    );

    let error = p
        .reset()
        .expect_err("a TPM backend reset failure is returned, not a panic");

    assert!(error.to_string().contains("CMD_INIT"), "{error}");
    // The caller stops the run; its final report still sees the generation
    // that requested the reset, because no device was reset.
    assert_eq!(p.uart_output(), b"X");
}
