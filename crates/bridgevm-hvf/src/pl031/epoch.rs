//! Initial PL031 wall clock: the host clock, unless `BRIDGEVM_GUEST_RTC_EPOCH`
//! names a fixed guest start time (Unix seconds). Test images whose account
//! policy depends on the date boot at the date they were prepared for.

use std::time::{SystemTime, UNIX_EPOCH};

pub(super) const GUEST_RTC_EPOCH_ENV: &str = "BRIDGEVM_GUEST_RTC_EPOCH";

pub(super) fn host_epoch_seconds() -> u32 {
    let host = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as u32;
    guest_epoch_override(std::env::var(GUEST_RTC_EPOCH_ENV).ok().as_deref()).unwrap_or(host)
}

/// A positive decimal `u32`; anything else keeps the host clock.
pub(super) fn guest_epoch_override(value: Option<&str>) -> Option<u32> {
    value?.parse::<u32>().ok().filter(|&seconds| seconds > 0)
}

#[cfg(test)]
mod tests {
    use super::guest_epoch_override;

    #[test]
    fn only_positive_decimal_seconds_override_the_host_clock() {
        assert_eq!(
            guest_epoch_override(Some("1785542400")),
            Some(1_785_542_400)
        );
        for rejected in [
            None,
            Some(""),
            Some("0"),
            Some("-5"),
            Some("0x10"),
            Some("4294967296"),
            Some(" 7"),
        ] {
            assert_eq!(guest_epoch_override(rejected), None, "{rejected:?}");
        }
    }
}
