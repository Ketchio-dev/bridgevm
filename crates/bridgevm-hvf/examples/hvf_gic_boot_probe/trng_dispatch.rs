//! One SMCCC TRNG dispatch shared by the primary and secondary run loops.
//!
//! Both loops previously carried their own copy of the handler, and both
//! copies derived guest randomness from the vCPU exit counter. Having a single
//! entry point is what makes that class of divergence impossible: the protocol
//! lives in `bridgevm_hvf::smccc_trng` and the entropy comes from the host
//! CSPRNG in `bridgevm_hvf::host_entropy`.

use bridgevm_hvf::host_entropy::HostEntropy;
use bridgevm_hvf::smccc_trng;

use crate::hvf_abi::{hv_vcpu_get_reg, hv_vcpu_set_reg, HV_REG_X0};

/// Serve a TRNG call if `function_id` is one, writing X0..=X3.
///
/// Returns `false` when the function is not part of the TRNG range, leaving
/// the caller's existing dispatch untouched.
///
/// # Safety
///
/// `vcpu` must be a live vCPU owned by the calling thread.
pub(crate) unsafe fn handle_trng_hvc(vcpu: u64, function_id: u64) -> bool {
    let mut x1 = 0u64;
    // X1 carries the requested bit count for RND32/RND64 and the queried
    // function ID for TRNG_FEATURES, so it must be read before dispatching.
    hv_vcpu_get_reg(vcpu, HV_REG_X0 + 1, &mut x1);

    let Some(ret) = smccc_trng::handle_call(function_id, x1, &mut HostEntropy::new()) else {
        return false;
    };

    hv_vcpu_set_reg(vcpu, HV_REG_X0, ret.x0);
    hv_vcpu_set_reg(vcpu, HV_REG_X0 + 1, ret.x1);
    hv_vcpu_set_reg(vcpu, HV_REG_X0 + 2, ret.x2);
    hv_vcpu_set_reg(vcpu, HV_REG_X0 + 3, ret.x3);
    true
}

/// Whether a function ID belongs to the TRNG service.
pub(crate) fn is_trng_function(function_id: u64) -> bool {
    matches!(
        function_id,
        smccc_trng::func::VERSION
            | smccc_trng::func::FEATURES
            | smccc_trng::func::GET_UUID
            | smccc_trng::func::RND32
            | smccc_trng::func::RND64
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn function_ids_match_the_specification() {
        // Written out rather than referencing the constants under test, so a
        // wrong value in the library is visible here.
        assert_eq!(smccc_trng::func::VERSION, 0x8400_0050);
        assert_eq!(smccc_trng::func::FEATURES, 0x8400_0051);
        assert_eq!(smccc_trng::func::GET_UUID, 0x8400_0052);
        assert_eq!(smccc_trng::func::RND32, 0x8400_0053);
        assert_eq!(smccc_trng::func::RND64, 0xc400_0053);
    }

    #[test]
    fn trng_range_is_recognised_and_psci_is_not() {
        for id in [
            smccc_trng::func::VERSION,
            smccc_trng::func::FEATURES,
            smccc_trng::func::GET_UUID,
            smccc_trng::func::RND32,
            smccc_trng::func::RND64,
        ] {
            assert!(is_trng_function(id), "{id:#x} is a TRNG function");
        }
        for id in [0x8400_0000u64, 0x8400_0003, 0xc400_0003, 0x8400_000A] {
            assert!(!is_trng_function(id), "{id:#x} must stay with PSCI");
        }
    }
}
