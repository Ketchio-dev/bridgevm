//! Reboot-policy and PSCI terminal-action tests.

use crate::*;

#[test]
fn reboot_plan_resets_gic_guest_ram_platform_and_vcpu() {
    assert_eq!(
        psci_terminal_action(PSCI_SYSTEM_OFF),
        Some(PsciTerminalAction::SystemOff)
    );
    assert_eq!(
        psci_terminal_action(PSCI_SYSTEM_RESET),
        Some(PsciTerminalAction::SystemReset)
    );
    assert_eq!(
        decide_system_reset(0, RebootPlan { max_reboots: 2 }),
        SystemResetDecision::Reboot {
            next_reboot_count: 1,
            actions: RebootActions {
                reset_gic: true,
                reset_guest_ram: true,
                reset_platform: true,
                reset_vcpu: true,
                continue_run_loop: true,
            },
        }
    );
}

#[test]
fn reboot_guard_parses_env_and_caps_system_reset_loop() {
    assert_eq!(
        RebootPlan::from_env_value(Some("0x2")),
        RebootPlan { max_reboots: 2 }
    );
    assert_eq!(
        RebootPlan::from_env_value(Some("bad")),
        RebootPlan {
            max_reboots: DEFAULT_MAX_REBOOTS
        }
    );
    assert!(matches!(
        decide_system_reset(0, RebootPlan { max_reboots: 0 }),
        SystemResetDecision::Stop { .. }
    ));
    assert!(matches!(
        decide_system_reset(1, RebootPlan { max_reboots: 1 }),
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
