//! Bounded control-file ingestion.

use super::*;

/// Read at most one bounded chunk of control-file bytes from `offset` into
/// caller-owned storage. Later polls continue from the updated caller offset.
/// Returns false on any IO error, which the caller treats as "nothing new yet".
pub(super) fn read_ctl_appended_into(path: &str, offset: u64, out: &mut Vec<u8>) -> bool {
    use std::io::{Read, Seek, SeekFrom};
    out.clear();
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    if file.seek(SeekFrom::Start(offset)).is_err() {
        return false;
    }
    file.take(MAX_CTL_READ_BYTES_PER_POLL)
        .read_to_end(out)
        .is_ok()
}
