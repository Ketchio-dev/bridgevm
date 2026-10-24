//! The interval of vCPU0's steady service wake, moved out of probe_runtime.rs
//! unchanged to make room there for the HDA service tick. Every ServiceWake
//! tick is an `hv_vcpus_exit` cancel; the HDA service tick
//! (hda_service_tick.rs) deliberately uses none.

use crate::{env_u64, AgentConsoleHarness, BootTimer};
use std::time::Duration;

/// BOOT_TIMER's wake, else the agent console's when its harness needs one,
/// else 2 ms while a KD serial bridge is attached; `None` means no steady wake.
pub(crate) fn service_wake_interval(
    boot_timer: &BootTimer,
    agent_console: Option<&AgentConsoleHarness>,
    kd_serial_bridge_attached: bool,
) -> Option<Duration> {
    boot_timer
        .service_wake_interval()
        .or_else(|| {
            agent_console
                .is_some_and(|harness| harness.service_wake_needed())
                // Every ServiceWake tick is an hv_vcpus_exit cancel,
                // and each cancel rolls the A1 swallow race (measured
                // 3516/3517 wakes were agent-console in a wedged A2
                // run). The default stays 250ms; a longer interval
                // trades reply latency for an 8x lower cancel rate
                // during measurement windows (A2's fps gate).
                .then(|| {
                    Duration::from_millis(env_u64("BRIDGEVM_AGENT_WAKE_MS", 250).clamp(50, 10_000))
                })
        })
        // A guest halted at a KD breakpoint generates no vCPU exits, so
        // the pre-run drain (and the KD serial pump with it) would stall
        // until the debugger's next byte happened to arrive. A steady
        // wake keeps the debugger<->guest byte pipe flowing while halted.
        .or_else(|| kd_serial_bridge_attached.then_some(Duration::from_millis(2)))
}
