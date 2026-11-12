//! Reboot-policy and PSCI terminal-action tests.

use crate::*;

#[test]
fn a_run_has_no_exit_cap_unless_one_is_requested() {
    // 50,000,000 was the default: about 4 h at 3,450 exits/s, a day at 560.
    assert_eq!(exit_cap(None), u64::MAX);
    assert_eq!(exit_cap(Some(150_000_000)), 150_000_000);
    assert_eq!(exit_cap(Some(0)), 1);
}

#[test]
fn reboot_plan_advances_the_count_for_system_reset() {
    assert_eq!(
        psci_terminal_action(PSCI_SYSTEM_OFF),
        Some(PsciTerminalAction::SystemOff)
    );
    assert_eq!(
        psci_terminal_action(PSCI_SYSTEM_RESET),
        Some(PsciTerminalAction::SystemReset)
    );
    assert_eq!(
        decide_system_reset(0, RebootPlan { max_reboots: 2, exit_on_reset: false }),
        SystemResetDecision::Reboot {
            next_reboot_count: 1,
        }
    );
}

#[test]
fn in_process_reboot_preserves_the_fixed_reset_order() {
    let run = include_str!("../probe_runtime.rs");
    let reboot = &run[run.find("SystemResetDecision::Reboot {").unwrap()..];
    let mut remaining = reboot;
    for step in [
        "crate::usgic_bridge::reset()",
        "hv_gic_reset()",
        "system_reset_failure(gic_reset_status, &platform)",
        "reset_guest_ram_for_boot(&mut guest_ram, &boot_dtb)",
        "reset_vcpu_for_boot(vcpu)",
        "arm_watchpoint_for_boot(vcpu, watch_addr)",
        "continue 'reboot;",
    ] {
        let position = remaining.find(step).unwrap_or_else(|| panic!("missing or reordered {step}"));
        remaining = &remaining[position + step.len()..];
    }
}

#[test]
fn reboot_guard_parses_env_and_caps_system_reset_loop() {
    assert_eq!(
        RebootPlan::from_env_value(Some("0x2")),
        RebootPlan { max_reboots: 2, exit_on_reset: false }
    );
    assert_eq!(
        RebootPlan::from_env_value(Some("bad")),
        RebootPlan {
            max_reboots: DEFAULT_MAX_REBOOTS,
            exit_on_reset: false
        }
    );
    assert!(matches!(
        decide_system_reset(0, RebootPlan { max_reboots: 0, exit_on_reset: false }),
        SystemResetDecision::Stop { .. }
    ));
    assert!(matches!(
        decide_system_reset(1, RebootPlan { max_reboots: 1, exit_on_reset: false }),
        SystemResetDecision::Stop { .. }
    ));

    let generation = AtomicU64::new(7);
    assert!(watchdog_generation_matches(&generation, 7));
    assert!(!watchdog_generation_matches(&generation, 6));
    let boot_generation = begin_watchdog_generation(&generation);
    assert!(watchdog_generation_matches(&generation, boot_generation));
    invalidate_watchdog_generation(&generation);
    assert!(!watchdog_generation_matches(&generation, boot_generation));
}

#[test]
fn exit_on_reset_overrides_the_in_process_reboot_path_entirely() {
    // Even with reboot budget remaining, the product mode exits for
    // process recreation: PLAN.md R1 makes in-process reset diagnostic-only.
    assert_eq!(
        decide_system_reset(
            0,
            RebootPlan {
                max_reboots: 8,
                exit_on_reset: true
            }
        ),
        SystemResetDecision::ExitForRecreate
    );
}

