//! PSCI 1.1 state-table and affinity tests.

use super::*;
use crate::machine::cpu_mpidr;

fn topology(states: &[CpuState]) -> Vec<(u64, CpuState)> {
    states
        .iter()
        .enumerate()
        .map(|(index, state)| (cpu_mpidr(index as u64), *state))
        .collect()
}

#[test]
fn status_codes_match_den0022() {
    assert_eq!(status::SUCCESS, 0);
    assert_eq!(status::NOT_SUPPORTED, 0xffff_ffff_ffff_ffff);
    assert_eq!(status::INVALID_PARAMS, 0xffff_ffff_ffff_fffe);
    assert_eq!(status::DENIED, 0xffff_ffff_ffff_fffd);
    assert_eq!(status::ALREADY_ON, 0xffff_ffff_ffff_fffc);
    assert_eq!(status::ON_PENDING, 0xffff_ffff_ffff_fffb);
    assert_eq!(status::INTERNAL_FAILURE, 0xffff_ffff_ffff_fffa);
}

#[test]
fn affinity_values_are_not_error_codes() {
    // AFFINITY_INFO returns small positive integers; confusing them with the
    // negative status codes is how OFF and "invalid" got merged.
    assert_eq!(affinity::ON, 0);
    assert_eq!(affinity::OFF, 1);
    assert_eq!(affinity::ON_PENDING, 2);
}

#[test]
fn cpu_on_follows_the_specified_state_table() {
    assert_eq!(cpu_on_decision(CpuState::Off), CpuOnDecision::Start);
    assert_eq!(
        cpu_on_decision(CpuState::OnPending),
        CpuOnDecision::Reject(status::ON_PENDING),
        "a pending CPU must report ON_PENDING, not ALREADY_ON"
    );
    assert_eq!(
        cpu_on_decision(CpuState::On),
        CpuOnDecision::Reject(status::ALREADY_ON)
    );
}

#[test]
fn normalizing_mpidr_ignores_the_res1_bit() {
    assert_eq!(normalized_mpidr(MPIDR_RES1_BIT | 0x101), 0x101);
    assert_eq!(normalized_mpidr(0x101), 0x101);
    assert_eq!(
        normalized_mpidr(MPIDR_RES1_BIT | cpu_mpidr(3)),
        cpu_mpidr(3)
    );
}

#[test]
fn affinity_level_zero_addresses_a_single_cpu() {
    let cpus = topology(&[
        CpuState::On,
        CpuState::Off,
        CpuState::OnPending,
        CpuState::Off,
    ]);

    assert_eq!(affinity_info(cpus.clone(), cpu_mpidr(0), 0), affinity::ON);
    assert_eq!(affinity_info(cpus.clone(), cpu_mpidr(1), 0), affinity::OFF);
    assert_eq!(
        affinity_info(cpus.clone(), cpu_mpidr(2), 0),
        affinity::ON_PENDING,
        "a pending CPU must be distinguishable from an off one"
    );
    assert_eq!(affinity_info(cpus, cpu_mpidr(3), 0), affinity::OFF);
}

#[test]
fn cpu0_is_searched_like_any_other_cpu() {
    // CPU0 used to be answered unconditionally as ON without consulting state.
    let all_off = topology(&[CpuState::Off, CpuState::Off]);
    assert_eq!(affinity_info(all_off, cpu_mpidr(0), 0), affinity::OFF);

    let running = topology(&[CpuState::On, CpuState::Off]);
    assert_eq!(affinity_info(running, cpu_mpidr(0), 0), affinity::ON);
}

#[test]
fn unknown_target_is_invalid_not_off() {
    let cpus = topology(&[CpuState::On, CpuState::Off]);
    assert_eq!(
        affinity_info(cpus.clone(), 0xdead_beef, 0),
        status::INVALID_PARAMS,
        "a target naming no CPU is a caller error"
    );
    assert_eq!(affinity_info(cpus, cpu_mpidr(9), 0), status::INVALID_PARAMS);
}

#[test]
fn invalid_affinity_level_is_rejected() {
    let cpus = topology(&[CpuState::On]);
    for level in [4u64, 5, 64, u64::MAX] {
        assert_eq!(
            affinity_info(cpus.clone(), cpu_mpidr(0), level),
            status::INVALID_PARAMS,
            "level {level} is out of range"
        );
    }
    for level in 0..=MAX_AFFINITY_LEVEL {
        assert!(affinity_mask(level).is_some(), "level {level} is valid");
    }
}

#[test]
fn higher_affinity_levels_aggregate_a_group() {
    // Aff1 groups CPUs 0..15; CPU 16 is the first member of the next group.
    let cpus = vec![
        (cpu_mpidr(0), CpuState::Off),
        (cpu_mpidr(1), CpuState::OnPending),
        (cpu_mpidr(16), CpuState::On),
    ];

    // Level 1 keeps Aff1 and above, so CPUs 0 and 1 answer together.
    assert_eq!(
        affinity_info(cpus.clone(), cpu_mpidr(0), 1),
        affinity::ON_PENDING,
        "a group with a pending member is pending"
    );
    // The second group holds a running CPU.
    assert_eq!(affinity_info(cpus.clone(), cpu_mpidr(16), 1), affinity::ON);
    // Level 2 collapses both groups into one, and any ON wins.
    assert_eq!(affinity_info(cpus, cpu_mpidr(0), 2), affinity::ON);
}

#[test]
fn on_wins_over_pending_within_a_group() {
    let cpus = vec![
        (cpu_mpidr(0), CpuState::OnPending),
        (cpu_mpidr(1), CpuState::On),
    ];
    assert_eq!(affinity_info(cpus, cpu_mpidr(0), 1), affinity::ON);
}

#[test]
fn affinity_masks_keep_only_levels_at_or_above_the_request() {
    // Aff0 [7:0], Aff1 [15:8], Aff2 [23:16], Aff3 [39:32]; bits 24..31 are not
    // affinity and must never be compared.
    assert_eq!(affinity_mask(0), Some(0xff_00ff_ffff));
    assert_eq!(affinity_mask(1), Some(0xff_00ff_ff00));
    assert_eq!(affinity_mask(2), Some(0xff_00ff_0000));
    assert_eq!(affinity_mask(3), Some(0xff_0000_0000));
    assert_eq!(affinity_mask(4), None);

    for level in 0..=MAX_AFFINITY_LEVEL {
        let mask = affinity_mask(level).expect("valid level");
        assert_eq!(mask & 0xff00_0000, 0, "level {level} must skip bits 24..31");
        assert_eq!(
            mask & MPIDR_RES1_BIT,
            0,
            "level {level} must not compare RES1"
        );
    }
}

#[test]
fn res1_spelling_of_a_target_still_matches() {
    let cpus = topology(&[CpuState::Off, CpuState::On]);
    assert_eq!(
        affinity_info(cpus, MPIDR_RES1_BIT | cpu_mpidr(1), 0),
        affinity::ON
    );
}

#[test]
fn empty_topology_reports_invalid_params() {
    let cpus: Vec<(u64, CpuState)> = Vec::new();
    assert_eq!(affinity_info(cpus, cpu_mpidr(0), 0), status::INVALID_PARAMS);
}
