//! Probe environment parsing and report interval configuration.

use crate::*;

pub(crate) fn parse_u64(s: &str) -> Option<u64> {
    let s = s.trim();
    if let Some(h) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u64::from_str_radix(h, 16).ok()
    } else {
        s.parse().ok()
    }
}

pub(crate) fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|s| parse_u64(&s))
        .unwrap_or(default)
}

pub(crate) fn env_optional_u64(name: &str) -> Option<u64> {
    std::env::var(name).ok().and_then(|s| parse_u64(&s))
}

pub(crate) fn env_flag(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .and_then(|value| parse_flag(&value))
        .unwrap_or(false)
}

pub(crate) fn env_flag_default(name: &str, default: bool) -> bool {
    std::env::var(name)
        .ok()
        .and_then(|value| parse_flag(&value))
        .unwrap_or(default)
}

pub(crate) fn parse_flag(value: &str) -> Option<bool> {
    let value = value.trim();
    if value == "1"
        || value.eq_ignore_ascii_case("true")
        || value.eq_ignore_ascii_case("yes")
        || value.eq_ignore_ascii_case("on")
    {
        Some(true)
    } else if value == "0"
        || value.eq_ignore_ascii_case("false")
        || value.eq_ignore_ascii_case("no")
        || value.eq_ignore_ascii_case("off")
    {
        Some(false)
    } else {
        None
    }
}

pub(crate) fn trace_msix_enabled() -> bool {
    static TRACE_MSIX: OnceLock<bool> = OnceLock::new();
    *TRACE_MSIX.get_or_init(|| env_flag("BRIDGEVM_TRACE_MSIX"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RebootPlan {
    pub(crate) max_reboots: u64,
}

pub(crate) const XHCI_REPORT_INTERVAL_ENV: &str = "BRIDGEVM_XHCI_REPORT_INTERVAL_MS";

pub(crate) const XHCI_REPORT_INTERVAL_DEFAULT_MS: u64 = 30;

pub(crate) const XHCI_REPORT_INTERVAL_MAX_MS: u64 = 10_000;

/// Minimum host-time spacing between consecutive HID interrupt-IN reports, from
/// `BRIDGEVM_XHCI_REPORT_INTERVAL_MS` (default 30 ms; `0` disables pacing).
/// Windows drops keystrokes when a burst of reports lands microseconds apart, so
/// live runs throttle emission. Parsed leniently like the other optional
/// `BRIDGEVM_XHCI_*` knobs: a missing/invalid value falls back to the default.
pub(crate) fn parse_xhci_report_interval_env() -> std::time::Duration {
    let ms = match std::env::var(XHCI_REPORT_INTERVAL_ENV) {
        Ok(value) => match value.trim().parse::<u64>() {
            Ok(ms) if ms <= XHCI_REPORT_INTERVAL_MAX_MS => ms,
            Ok(ms) => {
                println!(
                    "{XHCI_REPORT_INTERVAL_ENV}={ms} exceeds max {XHCI_REPORT_INTERVAL_MAX_MS}; clamping"
                );
                XHCI_REPORT_INTERVAL_MAX_MS
            }
            Err(_) => {
                println!(
                    "{XHCI_REPORT_INTERVAL_ENV}='{}' invalid; using default {XHCI_REPORT_INTERVAL_DEFAULT_MS}",
                    value.trim()
                );
                XHCI_REPORT_INTERVAL_DEFAULT_MS
            }
        },
        Err(_) => XHCI_REPORT_INTERVAL_DEFAULT_MS,
    };
    std::time::Duration::from_millis(ms)
}
