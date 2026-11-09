//! One bounded host upload read; bytes and their version must describe the same source.

pub(super) use super::host_destination::snapshot::HostSnapshot;
use super::host_destination::Destination;
use super::*;
#[cfg(test)]
use std::fs::File;
#[cfg(test)]
use std::io;

/// At most `max_bytes`: a larger file is TooLarge, unread.
pub(in crate::agent_console) fn read_share_host_snapshot(
    share: &mut ShareState,
    name: &str,
    max_bytes: u64,
) -> Result<HostSnapshot, ShareFileReadError> {
    let (source, _) = snapshot_source(share, name)?;
    source.snapshot(max_bytes)
}

#[cfg(test)]
pub(in crate::agent_console) fn read_share_host_snapshot_with(
    share: &mut ShareState,
    name: &str,
    read: impl FnOnce(&mut File, &mut Vec<u8>, u64) -> io::Result<()>,
) -> Result<HostSnapshot, ShareFileReadError> {
    let (source, max_bytes) = snapshot_source(share, name)?;
    source.snapshot_with(max_bytes, read)
}

pub(in crate::agent_console::share) fn snapshot_source(
    share: &mut ShareState,
    name: &str,
) -> Result<(Destination, u64), ShareFileReadError> {
    let root = share.checked_root().map_err(|_| ShareFileReadError::Io)?;
    let source = Destination::from_root(root, name, false).map_err(|_| ShareFileReadError::Io)?;
    Ok((source, share.engine.max_bytes()))
}

/// A Mac file read whole, or `Err(size)` when it exceeds `max_bytes` but not the share's cap
/// (`streams`: an agent that takes large files in pieces, share_big_put.rs).
pub(super) fn read_share_snapshot_or_skip(
    share: &mut ShareState,
    name: &str,
    streams: bool,
    read: &mut impl FnMut(&mut ShareState, &str, u64) -> Result<HostSnapshot, ShareFileReadError>,
) -> Option<Result<HostSnapshot, u64>> {
    let cap = share.engine.max_bytes();
    let max_bytes = if streams { cap.min(SHARE_RANGE_BYTES) } else { cap };
    match read(share, name, max_bytes) {
        Ok(snapshot) => Some(Ok(snapshot)),
        Err(ShareFileReadError::TooLarge(size)) if streams && size <= cap => Some(Err(size)),
        Err(ShareFileReadError::TooLarge(size)) => {
            if let Some(mtime_ms) = file_mtime_ms(&share.host_dir.join(name)) {
                print_host_skip_once(share, name, mtime_ms, HostSkipKind::TooLarge, size);
            }
            None
        }
        Err(ShareFileReadError::Io) => None,
    }
}
