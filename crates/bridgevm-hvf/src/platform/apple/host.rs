//! Host-capability queries (IPA sizes, EL2 support) and raw vCPU register reads.
//!
//! Split out of the single 12,111-line apple.rs backend.

use super::*;
use crate::*;

pub fn query_hvf_host_capabilities() -> HvfHostCapabilities {
    let mut blockers = Vec::new();
    let default_ipa_bits = query_u32(
        hv_vm_config_get_default_ipa_size,
        "default IPA size",
        &mut blockers,
    );
    let max_ipa_bits = query_u32(hv_vm_config_get_max_ipa_size, "max IPA size", &mut blockers);
    let el2_supported = query_bool(hv_vm_config_get_el2_supported, "EL2 support", &mut blockers);
    HvfHostCapabilities {
        available: blockers.is_empty() || default_ipa_bits.is_some() || max_ipa_bits.is_some(),
        host: "macos-aarch64",
        default_ipa_bits,
        max_ipa_bits,
        el2_supported,
        blockers,
    }
}

pub(crate) fn query_u32(
    query: unsafe extern "C" fn(*mut u32) -> HvReturn,
    label: &str,
    blockers: &mut Vec<String>,
) -> Option<u32> {
    let mut value = 0;
    let status = unsafe { query(&mut value) };
    if status == HV_SUCCESS {
        Some(value)
    } else {
        blockers.push(format!(
            "Hypervisor.framework {label} query failed: {status:#x}"
        ));
        None
    }
}

pub(crate) fn query_bool(
    query: unsafe extern "C" fn(*mut bool) -> HvReturn,
    label: &str,
    blockers: &mut Vec<String>,
) -> Option<bool> {
    let mut value = false;
    let status = unsafe { query(&mut value) };
    if status == HV_SUCCESS {
        Some(value)
    } else {
        blockers.push(format!(
            "Hypervisor.framework {label} query failed: {status:#x}"
        ));
        None
    }
}

pub(crate) fn read_vcpu_reg(vcpu: HvVcpu, register: u32) -> Option<u64> {
    let mut value = 0;
    let status = unsafe { hv_vcpu_get_reg(vcpu, register, &mut value) };
    (status == HV_SUCCESS).then_some(value)
}

pub(crate) fn read_vcpu_sys_reg(vcpu: HvVcpu, register: HvSysReg) -> Option<u64> {
    let mut value = 0;
    let status = unsafe { hv_vcpu_get_sys_reg(vcpu, register, &mut value) };
    (status == HV_SUCCESS).then_some(value)
}
