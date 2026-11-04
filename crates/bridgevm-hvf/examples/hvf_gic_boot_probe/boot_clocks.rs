//! The clock domain of each boot's time consumers. A VM pause takes paused time
//! out of VM time (bridgevm_hvf::vm_clock) for the whole helper, so a later
//! boot in the same helper starts with VM time behind host time; a consumer
//! that mixes the two sees its origin in the future and its timeouts late.
//!
//! - The agent console runs on VM time: its origin at each boot and every
//!   tick's `now`, so a pause does not make in-flight requests overdue and an
//!   earlier pause does not delay a later boot's READY timeout.
//! - Boot telemetry (BootTimer) runs on host time: its origin, its ticks and
//!   the agent's desktop milestone.
//! - The platform takes host time and keeps VM time inside (set_host_now in,
//!   xhci_pointer_report_deadline out); the NAT and HDA WALCLK are VM time
//!   throughout; triggers and live input are host time throughout.

use std::time::Instant;

pub(crate) trait Clocks {
    fn host(&self) -> Instant;
    fn vm(&self) -> Instant;
}

pub(crate) struct Live;

impl Clocks for Live {
    fn host(&self) -> Instant {
        Instant::now()
    }

    fn vm(&self) -> Instant {
        bridgevm_hvf::vm_clock::active_now()
    }
}

/// A new boot's agent console origin.
pub(crate) fn agent_origin(clocks: &impl Clocks) -> Instant {
    clocks.vm()
}

/// One automation tick's times: the agent console's and boot telemetry's.
pub(crate) struct TickTimes {
    pub(crate) agent: Instant,
    pub(crate) telemetry: Instant,
}

pub(crate) fn tick_times(clocks: &impl Clocks) -> TickTimes {
    TickTimes {
        agent: clocks.vm(),
        telemetry: clocks.host(),
    }
}

