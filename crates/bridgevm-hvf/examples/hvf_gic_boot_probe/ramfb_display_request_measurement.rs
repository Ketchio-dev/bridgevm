//! Match each request to the existing post-publication completion receipt.
//! Observer scheduling and the earlier header stamp remain diagnostic only.

use std::path::Path;
use std::time::{Duration, Instant};

use super::{read_fb, timing, RamfbDisplayThread};

/// The published sequence once no frame is being written.
pub(super) fn settled(path: &Path) -> Option<u64> {
    let len = std::fs::metadata(path).ok()?.len();
    let sequence = (len >= 64).then(|| read_fb(path).1)?;
    (sequence & 1 == 0).then_some(sequence)
}

pub(super) fn field(record: &str, name: &str) -> u64 {
    let value = record.split(' ').find_map(|field| field.strip_prefix(name));
    value
        .and_then(|value| value.parse().ok())
        .unwrap_or_else(|| panic!("{name} in {record}"))
}

pub(super) fn receipt(
    thread: &RamfbDisplayThread,
    path: &Path,
    before: u64,
    posted: Instant,
    request_stamp: u64,
    observed: Duration,
) -> (Duration, Duration, Duration) {
    let bytes = std::fs::read(path).unwrap();
    let header_stamp = u64::from_le_bytes(bytes[48..56].try_into().unwrap());
    let expected = before / 2 + 1;
    let done = loop {
        let (ticks, published, done) = timing::lock(&thread.timing).publication_receipt();
        assert!(
            ticks <= expected && published <= expected,
            "unexpected extra publication"
        );
        if ticks == expected && published == expected {
            break done.expect("matching publication completion");
        }
        assert!(
            posted.elapsed() < Duration::from_secs(1),
            "completion receipt missing"
        );
        std::thread::yield_now();
    };
    (
        observed,
        done.checked_duration_since(posted)
            .expect("stale completion"),
        Duration::from_nanos(
            header_stamp
                .checked_sub(request_stamp)
                .expect("stale header"),
        ),
    )
}
