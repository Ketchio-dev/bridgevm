//! Initial PL031 wall clock: the Mac's local time, unless `BRIDGEVM_GUEST_RTC_EPOCH`
//! names a fixed guest start time (Unix seconds). Test images whose account
//! policy depends on the date boot at the date they were prepared for.
//!
//! Windows keeps the hardware clock in local time (RealTimeIsUniversal is not
//! set) and reads it at boot. Started at UTC, Windows' clock came up off by the
//! zone offset (measured: Korea 9 h behind, Eastern 4 h ahead) until Guest Tools
//! set it after the user signed in (agent_console time_resync.rs): the lock
//! screen, Setup and everything before sign-in ran on the wrong time. QEMU gives
//! Windows guests the same with `-rtc base=localtime`.

use std::time::{SystemTime, UNIX_EPOCH};

pub(super) const GUEST_RTC_EPOCH_ENV: &str = "BRIDGEVM_GUEST_RTC_EPOCH";

pub(super) fn host_epoch_seconds() -> u32 {
    let utc = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let host = (utc as i64).saturating_add(utc_offset_seconds(utc)) as u32;
    guest_epoch_override(std::env::var(GUEST_RTC_EPOCH_ENV).ok().as_deref()).unwrap_or(host)
}

/// The Mac's offset from UTC at `utc` (Unix seconds), daylight time included.
fn utc_offset_seconds(utc: u64) -> i64 {
    let time = utc as libc::time_t;
    // SAFETY: localtime_r only writes the tm it is given.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    if unsafe { libc::localtime_r(&time, &mut tm) }.is_null() {
        return 0;
    }
    tm.tm_gmtoff
}

/// A positive decimal `u32`; anything else keeps the host clock.
pub(super) fn guest_epoch_override(value: Option<&str>) -> Option<u32> {
    value?.parse::<u32>().ok().filter(|&seconds| seconds > 0)
}

#[cfg(test)]
mod tests {
    use super::{guest_epoch_override, host_epoch_seconds, GUEST_RTC_EPOCH_ENV};

    /// The Mac's UTC offset as `date +%z` prints it (+0900, -0500), in seconds.
    fn date_offset_seconds() -> i64 {
        let out = std::process::Command::new("/bin/date")
            .arg("+%z")
            .output()
            .unwrap();
        let text = String::from_utf8(out.stdout).unwrap();
        let text = text.trim();
        let sign = if text.starts_with('-') { -1 } else { 1 };
        let hours: i64 = text[1..3].parse().unwrap();
        let minutes: i64 = text[3..5].parse().unwrap();
        sign * (hours * 3600 + minutes * 60)
    }

    #[test]
    fn the_clock_starts_at_the_macs_local_time() {
        // Windows reads the hardware clock as local time: in Seoul it must start 9 h past UTC.
        if std::env::var_os(GUEST_RTC_EPOCH_ENV).is_some() {
            return;
        }
        let utc = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let ahead = i64::from(host_epoch_seconds()) - utc;
        let offset = date_offset_seconds();
        assert!(
            (ahead - offset).abs() <= 2,
            "started {ahead} s from UTC, the Mac is {offset} s"
        );
    }

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
