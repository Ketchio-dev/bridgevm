//! Single-vCPU SMCCC/PSCI adapter for the Windows boot diagnostic.

use super::hvf::*;
use bridgevm_hvf::psci::{self, CpuState};

const SMCCC_VERSION: u64 = 0x8000_0000;
const PSCI_VERSION: u64 = 0x8400_0000;
const PSCI_CPU_OFF: u64 = 0x8400_0002;
const PSCI_CPU_ON_32: u64 = 0x8400_0003;
const PSCI_CPU_ON_64: u64 = 0xc400_0003;
const PSCI_AFFINITY_INFO_32: u64 = 0x8400_0004;
const PSCI_AFFINITY_INFO_64: u64 = 0xc400_0004;
const PSCI_SYSTEM_OFF: u64 = 0x8400_0008;
const PSCI_SYSTEM_RESET: u64 = 0x8400_0009;
const PSCI_FEATURES: u64 = 0x8400_000a;

pub(super) enum Action {
    Resume,
    SystemOff,
    SystemReset,
}

fn features(function: u64) -> u64 {
    match function {
        PSCI_VERSION
        | PSCI_CPU_OFF
        | PSCI_CPU_ON_32
        | PSCI_CPU_ON_64
        | PSCI_AFFINITY_INFO_32
        | PSCI_AFFINITY_INFO_64
        | PSCI_SYSTEM_OFF
        | PSCI_SYSTEM_RESET
        | PSCI_FEATURES => psci::status::SUCCESS,
        _ => psci::status::NOT_SUPPORTED,
    }
}

pub(super) unsafe fn handle(vcpu: HvVcpu) -> Result<Action, String> {
    let read = |register: u32, label: &str| {
        let mut value = 0;
        status(label, hv_vcpu_get_reg(vcpu, register, &mut value))?;
        Ok::<u64, String>(value)
    };
    let function = read(0, "read SMCCC function")? & 0xffff_ffff;
    let response = match function {
        SMCCC_VERSION | PSCI_VERSION => 0x0001_0001,
        PSCI_FEATURES => features(read(1, "read PSCI_FEATURES function")?),
        PSCI_CPU_ON_32 | PSCI_CPU_ON_64 | PSCI_CPU_OFF => psci::status::NOT_SUPPORTED,
        PSCI_AFFINITY_INFO_32 | PSCI_AFFINITY_INFO_64 => psci::affinity_info(
            [(0, CpuState::On)],
            read(1, "read PSCI affinity target")?,
            read(2, "read PSCI affinity level")?,
        ),
        PSCI_SYSTEM_OFF => return Ok(Action::SystemOff),
        PSCI_SYSTEM_RESET => return Ok(Action::SystemReset),
        _ => psci::status::NOT_SUPPORTED,
    };
    status("write SMCCC response", hv_vcpu_set_reg(vcpu, 0, response))?;
    Ok(Action::Resume)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn features_stays_in_the_psci_namespace() {
        assert_eq!(features(PSCI_VERSION), psci::status::SUCCESS);
        assert_eq!(features(SMCCC_VERSION), psci::status::NOT_SUPPORTED);
        assert_eq!(features(0x8400_00ff), psci::status::NOT_SUPPORTED);
    }
}
