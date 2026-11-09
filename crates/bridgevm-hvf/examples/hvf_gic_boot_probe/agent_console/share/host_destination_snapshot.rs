//! Upload bytes and metadata come from one regular descriptor, with observed changes refused.

use super::super::{file_mtime_ms_from_meta, ShareFileReadError};
use super::Destination;
use std::fs::{File, Metadata};
use std::io::{self, Read};
use std::os::unix::fs::MetadataExt;

pub(in crate::agent_console) struct HostSnapshot {
    pub(in crate::agent_console) bytes: Vec<u8>,
    pub(in crate::agent_console) mtime_ms: u128,
}

/// What changes with any write to a file: size, modification and change times.
pub(in crate::agent_console) type SourceVersion = (u64, i64, i64, i64, i64);

pub(in crate::agent_console) fn version(metadata: &Metadata) -> SourceVersion {
    (
        metadata.len(),
        metadata.mtime(),
        metadata.mtime_nsec(),
        metadata.ctime(),
        metadata.ctime_nsec(),
    )
}

impl Destination {
    /// The regular file at the destination, never through a symlink, with its version.
    pub(in crate::agent_console::share) fn open_source(&self) -> Result<(File, Metadata), ShareFileReadError> {
        let file = self
            .existing(libc::O_RDONLY)
            .map_err(|_| ShareFileReadError::Io)?
            .ok_or(ShareFileReadError::Io)?;
        let meta = file.metadata().map_err(|_| ShareFileReadError::Io)?;
        Ok((file, meta))
    }

    pub(in crate::agent_console::share) fn snapshot(
        &self,
        max_bytes: u64,
    ) -> Result<HostSnapshot, ShareFileReadError> {
        self.snapshot_with(max_bytes, |file, bytes, limit| {
            file.take(limit).read_to_end(bytes).map(|_| ())
        })
    }

    pub(in crate::agent_console::share) fn snapshot_with(
        &self,
        max_bytes: u64,
        read: impl FnOnce(&mut File, &mut Vec<u8>, u64) -> io::Result<()>,
    ) -> Result<HostSnapshot, ShareFileReadError> {
        let mut file = self
            .existing(libc::O_RDONLY)
            .map_err(|_| ShareFileReadError::Io)?
            .ok_or(ShareFileReadError::Io)?;
        let before = file.metadata().map_err(|_| ShareFileReadError::Io)?;
        let size = before.len();
        if size > max_bytes {
            return Err(ShareFileReadError::TooLarge(size));
        }
        let limit = max_bytes.checked_add(1).ok_or(ShareFileReadError::Io)?;
        let capacity = usize::try_from(size).map_err(|_| ShareFileReadError::TooLarge(size))?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(capacity)
            .map_err(|_| ShareFileReadError::Io)?;
        read(&mut file, &mut bytes, limit).map_err(|_| ShareFileReadError::Io)?;
        if bytes.len() as u64 > max_bytes {
            return Err(ShareFileReadError::TooLarge(bytes.len() as u64));
        }
        let after = file.metadata().map_err(|_| ShareFileReadError::Io)?;
        if version(&before) != version(&after) || bytes.len() as u64 != after.len() {
            return Err(ShareFileReadError::Io);
        }
        let mtime_ms = file_mtime_ms_from_meta(&after).ok_or(ShareFileReadError::Io)?;
        Ok(HostSnapshot { bytes, mtime_ms })
    }
}
