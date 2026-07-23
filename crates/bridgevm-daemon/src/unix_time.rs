//! Unix-epoch second and nanosecond helpers shared by the metadata writers.

use std::time::SystemTime;
use std::time::UNIX_EPOCH;

pub(crate) fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub(crate) fn now_unix_nanos() -> Option<u64> {
    system_time_unix_nanos(SystemTime::now())
}

pub(crate) fn system_time_unix_nanos(time: SystemTime) -> Option<u64> {
    let nanos = time.duration_since(UNIX_EPOCH).ok()?.as_nanos();
    u64::try_from(nanos).ok()
}
