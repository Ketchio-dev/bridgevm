//! Unix-second and microsecond time capture used by every receipt.

use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

pub(crate) fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub(crate) fn duration_micros_u64(duration: Duration) -> u64 {
    duration.as_micros().min(u128::from(u64::MAX)) as u64
}
