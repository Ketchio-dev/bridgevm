//! PSCI adapter: probe vCPU state to the `bridgevm_hvf::psci` state logic.
//!
//! The specification rules live in the library so they can be tested without
//! Hypervisor.framework. This file owns only the probe-side concerns: locking
//! a target's state, tracing transitions, and assembling the topology that
//! `AFFINITY_INFO` searches.

use std::sync::{atomic::Ordering, Arc};

use bridgevm_hvf::{machine, psci};

use crate::*;

pub(crate) fn normalized_mpidr(value: u64) -> u64 {
    value & !MPIDR_RES1_BIT
}

pub(crate) fn psci_target_index(target_mpidr: u64, controls: &[Arc<VcpuControl>]) -> Option<usize> {
    let target = normalized_mpidr(target_mpidr);
    controls
        .iter()
        .position(|control| normalized_mpidr(control.mpidr) == target)
}

/// `PSCI_FEATURES` answers for the PSCI namespace only.
///
/// It previously also answered for the TRNG function IDs. Those belong to a
/// separate specification with its own status values, and a guest must query
/// them through `TRNG_FEATURES`; conflating the two namespaces means a PSCI
/// probe can report that a TRNG call exists.
pub(crate) fn psci_features(func: u64) -> u64 {
    match func {
        PSCI_VERSION
        | PSCI_CPU_OFF
        | PSCI_CPU_ON_32
        | PSCI_CPU_ON_64
        | PSCI_AFFINITY_INFO_32
        | PSCI_AFFINITY_INFO_64
        | PSCI_SYSTEM_OFF
        | PSCI_SYSTEM_RESET
        | PSCI_FEATURES => PSCI_SUCCESS,
        _ => PSCI_NOT_SUPPORTED,
    }
}

pub(crate) fn psci_cpu_on(
    controls: &[Arc<VcpuControl>],
    target_mpidr: u64,
    entry: u64,
    context: u64,
    smp_trace: Option<&SmpTrace>,
) -> u64 {
    let Some(target_index) = psci_target_index(target_mpidr, controls) else {
        return PSCI_INVALID_PARAMS;
    };
    let control = &controls[target_index];
    // The state check and the Off -> OnPending transition share one critical
    // section, so two racing CPU_ON calls cannot both receive SUCCESS.
    let mut state = lock_vcpu_state(control, smp_trace, 0, "target vCPU PSCI state mutex");
    match psci::cpu_on_decision(to_psci_state(*state)) {
        psci::CpuOnDecision::Reject(status) => status,
        psci::CpuOnDecision::Start => {
            control.entry.store(entry, Ordering::SeqCst);
            control.context.store(context, Ordering::SeqCst);
            if let Some(trace) = smp_trace {
                trace.state_transition(control.index, PsciState::Off, PsciState::OnPending);
            }
            *state = PsciState::OnPending;
            drop(state);
            control.condvar.notify_one();
            PSCI_SUCCESS
        }
    }
}

fn to_psci_state(state: PsciState) -> psci::CpuState {
    match state {
        PsciState::Off => psci::CpuState::Off,
        PsciState::OnPending => psci::CpuState::OnPending,
        PsciState::On => psci::CpuState::On,
    }
}

/// `AFFINITY_INFO` over the whole topology, including CPU0.
///
/// `level` is the caller's `lowest_affinity_level` from X2. The primary CPU is
/// always running when it can ask this question, so it is reported as `On`;
/// secondaries report their tracked state.
pub(crate) fn psci_affinity_info(
    controls: &[Arc<VcpuControl>],
    target_mpidr: u64,
    level: u64,
) -> u64 {
    let mut topology = vec![(machine::cpu_mpidr(0), psci::CpuState::On)];
    for control in controls {
        let state = control.state.lock().expect("target vCPU PSCI state mutex");
        topology.push((control.mpidr, to_psci_state(*state)));
    }
    psci::affinity_info(topology, target_mpidr, level)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hvf_abi::PsciState;
    use std::sync::atomic::Ordering;
    #[test]
    fn psci_target_index_masks_mpidr_res1_bit() {
        let controls: Vec<_> = (1..4)
            .map(|index| Arc::new(VcpuControl::new(index)))
            .collect();

        assert_eq!(psci_target_index(machine::cpu_mpidr(1), &controls), Some(0));
        assert_eq!(
            psci_target_index(0x8000_0000 | machine::cpu_mpidr(2), &controls),
            Some(1)
        );
        assert_eq!(psci_target_index(machine::cpu_mpidr(0), &controls), None);
        assert_eq!(psci_target_index(machine::cpu_mpidr(4), &controls), None);
    }

    #[test]
    fn psci_cpu_on_sets_pending_and_returns_codes() {
        let controls: Vec<_> = (1..3)
            .map(|index| Arc::new(VcpuControl::new(index)))
            .collect();

        assert_eq!(
            psci_cpu_on(&controls, machine::cpu_mpidr(1), 0x1234, 0x5678, None),
            PSCI_SUCCESS
        );
        assert_eq!(*controls[0].state.lock().unwrap(), PsciState::OnPending);
        assert_eq!(controls[0].entry.load(Ordering::SeqCst), 0x1234);
        assert_eq!(controls[0].context.load(Ordering::SeqCst), 0x5678);

        // A CPU that has been asked to start but has not yet run reports
        // ON_PENDING. It previously reported ALREADY_ON, which tells the caller
        // the CPU is executing when it has not taken its first instruction.
        assert_eq!(
            psci_cpu_on(
                &controls,
                0x8000_0000 | machine::cpu_mpidr(1),
                0x9,
                0xa,
                None
            ),
            bridgevm_hvf::psci::status::ON_PENDING
        );
        assert_eq!(
            controls[0].entry.load(Ordering::SeqCst),
            0x1234,
            "a rejected CPU_ON must not overwrite the pending entry point"
        );

        *controls[0].state.lock().unwrap() = PsciState::On;
        assert_eq!(
            psci_cpu_on(&controls, machine::cpu_mpidr(1), 0x9, 0xa, None),
            bridgevm_hvf::psci::status::ALREADY_ON
        );

        assert_eq!(
            psci_cpu_on(&controls, machine::cpu_mpidr(3), 0x9, 0xa, None),
            PSCI_INVALID_PARAMS
        );
    }

    #[test]
    fn psci_affinity_info_reads_the_requested_level_and_sees_cpu0() {
        use bridgevm_hvf::psci::affinity;

        let controls: Vec<_> = (1..4)
            .map(|index| Arc::new(VcpuControl::new(index)))
            .collect();

        // CPU0 is the caller and is always running.
        assert_eq!(
            psci_affinity_info(&controls, machine::cpu_mpidr(0), 0),
            affinity::ON
        );
        // Secondaries start Off.
        assert_eq!(
            psci_affinity_info(&controls, machine::cpu_mpidr(1), 0),
            affinity::OFF
        );

        *controls[0].state.lock().unwrap() = PsciState::OnPending;
        assert_eq!(
            psci_affinity_info(&controls, machine::cpu_mpidr(1), 0),
            affinity::ON_PENDING,
            "pending must be distinguishable from off"
        );

        // Level 1 aggregates the cluster, which contains a running CPU0.
        assert_eq!(
            psci_affinity_info(&controls, machine::cpu_mpidr(1), 1),
            affinity::ON
        );

        assert_eq!(
            psci_affinity_info(&controls, machine::cpu_mpidr(9), 0),
            PSCI_INVALID_PARAMS,
            "an unknown target is a caller error, not OFF"
        );
        assert_eq!(
            psci_affinity_info(&controls, machine::cpu_mpidr(0), 4),
            PSCI_INVALID_PARAMS,
            "affinity levels above 3 are invalid"
        );
    }

    #[test]
    fn psci_cpu_on_defers_hvf_vcpu_creation_to_secondary_thread() {
        let controls: Vec<_> = (1..2)
            .map(|index| Arc::new(VcpuControl::new(index)))
            .collect();

        assert_eq!(
            psci_cpu_on(&controls, machine::cpu_mpidr(1), 0x8000, 0xfeed, None),
            PSCI_SUCCESS
        );

        let control = &controls[0];
        assert_eq!(*control.state.lock().unwrap(), PsciState::OnPending);
        assert_eq!(control.entry.load(Ordering::SeqCst), 0x8000);
        assert_eq!(control.context.load(Ordering::SeqCst), 0xfeed);
        assert_eq!(*control.vcpu.lock().unwrap(), None);
    }

    #[test]
    fn psci_features_only_reports_implemented_functions() {
        for func in [
            PSCI_VERSION,
            PSCI_CPU_OFF,
            PSCI_CPU_ON_32,
            PSCI_CPU_ON_64,
            PSCI_AFFINITY_INFO_32,
            PSCI_AFFINITY_INFO_64,
            PSCI_SYSTEM_OFF,
            PSCI_SYSTEM_RESET,
            PSCI_FEATURES,
        ] {
            assert_eq!(psci_features(func), PSCI_SUCCESS, "func {func:#x}");
        }
        assert_eq!(psci_features(0x8400_00ff), PSCI_NOT_SUPPORTED);
        assert_eq!(psci_features(SMCCC_VERSION), PSCI_NOT_SUPPORTED);
    }

    #[test]
    fn psci_features_does_not_answer_for_the_trng_namespace() {
        // TRNG has its own specification, its own status values and its own
        // TRNG_FEATURES discovery call. Answering for it here told a guest
        // that a TRNG function existed based on a PSCI query.
        for func in [
            bridgevm_hvf::smccc_trng::func::VERSION,
            bridgevm_hvf::smccc_trng::func::FEATURES,
            bridgevm_hvf::smccc_trng::func::GET_UUID,
            bridgevm_hvf::smccc_trng::func::RND32,
            bridgevm_hvf::smccc_trng::func::RND64,
        ] {
            assert_eq!(
                psci_features(func),
                PSCI_NOT_SUPPORTED,
                "TRNG func {func:#x} must not be discoverable through PSCI_FEATURES"
            );
        }
    }
}
