//! The interval of vCPU0's steady service wake, moved out of probe_runtime.rs
//! to make room there for the HDA service tick. Every ServiceWake tick is an
//! `hv_vcpus_exit` cancel; the HDA service tick (hda_service_tick.rs) and the
//! agent service (agent_console/service_thread.rs) deliberately use none.

use crate::BootTimer;
use std::time::Duration;

/// The PPM display export's wake. It has none of its own and had this one from
/// the agent console's wake, which the agent service no longer needs.
const DISPLAY_EXPORT_WAKE: Duration = Duration::from_millis(250);

/// BOOT_TIMER's wake, else the PPM display export's, else 2 ms while a KD
/// serial bridge is attached; `None` means no steady wake.
pub(crate) fn service_wake_interval(
    boot_timer: &BootTimer,
    display_export: bool,
    kd_serial_bridge_attached: bool,
) -> Option<Duration> {
    boot_timer
        .service_wake_interval()
        .or_else(|| display_export.then_some(DISPLAY_EXPORT_WAKE))
        // A guest halted at a KD breakpoint generates no vCPU exits, so
        // the pre-run drain (and the KD serial pump with it) would stall
        // until the debugger's next byte happened to arrive. A steady
        // wake keeps the debugger<->guest byte pipe flowing while halted.
        .or_else(|| kd_serial_bridge_attached.then_some(Duration::from_millis(2)))
}
