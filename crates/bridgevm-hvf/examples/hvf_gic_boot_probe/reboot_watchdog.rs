//! Reboot policy, terminal PSCI actions, and boot watchdogs.

use crate::*;

impl RebootPlan {
    pub(crate) fn from_env() -> Self {
        Self::from_env_value(
            std::env::var("BRIDGEVM_BOOT_PROBE_MAX_REBOOTS")
                .ok()
                .as_deref(),
        )
    }
    pub(crate) fn from_env_value(value: Option<&str>) -> Self {
        Self {
            max_reboots: value.and_then(parse_u64).unwrap_or(DEFAULT_MAX_REBOOTS),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RebootActions {
    pub(crate) reset_gic: bool,
    pub(crate) reset_guest_ram: bool,
    pub(crate) reset_platform: bool,
    pub(crate) reset_vcpu: bool,
    pub(crate) continue_run_loop: bool,
}

impl RebootActions {
    const SYSTEM_RESET: Self = Self {
        reset_gic: true,
        reset_guest_ram: true,
        reset_platform: true,
        reset_vcpu: true,
        continue_run_loop: true,
    };
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SystemResetDecision {
    Reboot {
        next_reboot_count: u64,
        actions: RebootActions,
    },
    Stop {
        reason: String,
    },
}

pub(crate) fn decide_system_reset(reboot_count: u64, plan: RebootPlan) -> SystemResetDecision {
    if reboot_count < plan.max_reboots {
        return SystemResetDecision::Reboot {
            next_reboot_count: reboot_count + 1,
            actions: RebootActions::SYSTEM_RESET,
        };
    }
    SystemResetDecision::Stop {
        reason: format!(
            "PSCI {PSCI_SYSTEM_RESET:#x} max reboot count {} reached",
            plan.max_reboots
        ),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PsciTerminalAction {
    SystemOff,
    SystemReset,
}

pub(crate) fn psci_terminal_action(func: u64) -> Option<PsciTerminalAction> {
    match func & 0xffff_ffff {
        PSCI_SYSTEM_OFF => Some(PsciTerminalAction::SystemOff),
        PSCI_SYSTEM_RESET => Some(PsciTerminalAction::SystemReset),
        _ => None,
    }
}

pub(crate) fn begin_watchdog_generation(generation: &AtomicU64) -> u64 {
    generation.fetch_add(1, Ordering::SeqCst).wrapping_add(1)
}

pub(crate) fn invalidate_watchdog_generation(generation: &AtomicU64) {
    generation.fetch_add(1, Ordering::SeqCst);
}

pub(crate) fn watchdog_generation_matches(generation: &AtomicU64, expected: u64) -> bool {
    generation.load(Ordering::SeqCst) == expected
}

pub(crate) fn spawn_boot_watchdog(
    vcpu: HvVcpuT,
    watchdog_ms: u64,
    generation: Arc<AtomicU64>,
    boot_generation: u64,
    watchdog_fired: Arc<AtomicBool>,
) {
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(watchdog_ms));
        if !watchdog_generation_matches(&generation, boot_generation) {
            return;
        }
        watchdog_fired.store(true, Ordering::SeqCst);
        let v = vcpu;
        // SAFETY: Category 8 - FFI boundary. `vcpu` is a live HVF vCPU
        // handle owned by the probe until shutdown, and `&v` points to one
        // initialized handle for the duration of this call.
        unsafe {
            hv_vcpus_exit(&v, 1);
        }
    });
}

#[cfg(test)]
mod reboot_plan_tests {
    use super::*;

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
}
