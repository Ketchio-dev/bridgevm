//! Host-support detection and shared Hypervisor.framework status vocabulary.
//!
//! These items are consumed by the probe result renderers in the crate root and
//! by the `platform` backend, so the crate root re-exports them: the public
//! `HvfSupport`/`detect_hvf_support`/`WindowsArmVmmGateStatus` surface is
//! unchanged, and the HV status/exit names stay crate-internal.

/// `HV_SUCCESS` as returned by Hypervisor.framework entry points.
pub(crate) const HV_SUCCESS_VALUE: i32 = 0;
pub(crate) const HV_EXIT_REASON_CANCELED_VALUE: u32 = 0;
pub(crate) const HV_EXIT_REASON_EXCEPTION_VALUE: u32 = 1;
pub(crate) const HV_EXIT_REASON_VTIMER_ACTIVATED_VALUE: u32 = 2;
const HV_EXIT_REASON_UNKNOWN_VALUE: u32 = 3;

pub(crate) fn hv_exit_reason_name(reason: u32) -> &'static str {
    match reason {
        HV_EXIT_REASON_CANCELED_VALUE => "HV_EXIT_REASON_CANCELED",
        HV_EXIT_REASON_EXCEPTION_VALUE => "HV_EXIT_REASON_EXCEPTION",
        HV_EXIT_REASON_VTIMER_ACTIVATED_VALUE => "HV_EXIT_REASON_VTIMER_ACTIVATED",
        HV_EXIT_REASON_UNKNOWN_VALUE => "HV_EXIT_REASON_UNKNOWN",
        _ => "unknown",
    }
}

pub(crate) fn hv_return_name(status: i32) -> &'static str {
    match status as u32 {
        0x0000_0000 => "HV_SUCCESS",
        0xfae9_4001 => "HV_ERROR",
        0xfae9_4002 => "HV_BUSY",
        0xfae9_4003 => "HV_BAD_ARGUMENT",
        0xfae9_4004 => "HV_ILLEGAL_GUEST_STATE",
        0xfae9_4005 => "HV_NO_RESOURCES",
        0xfae9_4006 => "HV_NO_DEVICE",
        0xfae9_4007 => "HV_DENIED",
        0xfae9_4008 => "HV_EXISTS",
        0xfae9_400f => "HV_UNSUPPORTED",
        _ => "unknown",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HvfSupport {
    Available,
    Unavailable,
}

pub fn detect_hvf_support() -> HvfSupport {
    if cfg!(target_os = "macos") {
        HvfSupport::Available
    } else {
        HvfSupport::Unavailable
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowsArmVmmGateStatus {
    Pass,
    Research,
    Blocked,
}

impl WindowsArmVmmGateStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            WindowsArmVmmGateStatus::Pass => "pass",
            WindowsArmVmmGateStatus::Research => "research",
            WindowsArmVmmGateStatus::Blocked => "blocked",
        }
    }
}
