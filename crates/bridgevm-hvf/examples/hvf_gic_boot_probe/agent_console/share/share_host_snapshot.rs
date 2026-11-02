//! One bounded host upload read; bytes and their version must describe the same source.

pub(super) use super::host_destination::snapshot::HostSnapshot;
use super::host_destination::Destination;
use super::*;
#[cfg(test)]
use std::fs::File;
#[cfg(test)]
use std::io;

pub(in crate::agent_console) fn read_share_host_snapshot(
    share: &mut ShareState,
    name: &str,
) -> Result<HostSnapshot, ShareFileReadError> {
    let (source, max_bytes) = snapshot_source(share, name)?;
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

fn snapshot_source(
    share: &mut ShareState,
    name: &str,
) -> Result<(Destination, u64), ShareFileReadError> {
    let root = share.checked_root().map_err(|_| ShareFileReadError::Io)?;
    let source = Destination::from_root(root, name, false).map_err(|_| ShareFileReadError::Io)?;
    Ok((source, share.engine.max_bytes()))
}

pub(super) fn read_share_snapshot_or_skip(
    share: &mut ShareState,
    name: &str,
    read: &mut impl FnMut(&mut ShareState, &str) -> Result<HostSnapshot, ShareFileReadError>,
) -> Option<HostSnapshot> {
    match read(share, name) {
        Ok(snapshot) => Some(snapshot),
        Err(ShareFileReadError::TooLarge(size)) => {
            if let Some(mtime_ms) = file_mtime_ms(&share.host_dir.join(name)) {
                print_host_skip_once(share, name, mtime_ms, HostSkipKind::TooLarge, size);
            }
            None
        }
        Err(ShareFileReadError::Io) => None,
    }
}
