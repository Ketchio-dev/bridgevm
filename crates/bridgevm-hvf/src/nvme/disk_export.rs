//! Exporting a disk, and the ceiling on how much of one may be held in RAM.
//! Split from disk.rs: what leaves or is retained by the backend, rather
//! than how a request is decoded.

use super::{DiskBackend, EXPORT_CHUNK_SIZE};
use std::fs::File;
use std::io::{self, Write};
use std::path::Path;

/// Default copy-on-write overlay ceiling: 2 GiB. Larger than any read-only
/// boot writes in practice (injector and firstboot write tens of megabytes)
/// and far below a run's host RAM, so hitting it means something is wrong
/// rather than merely busy.
pub(crate) const DEFAULT_OVERLAY_QUOTA_BYTES: u64 = 2 * 1024 * 1024 * 1024;

impl DiskBackend {
    /// Export through a temp file and rename, so the destination is either the
    /// previous export or a complete new one.
    ///
    /// `flush` alone was not enough: it pushes the process buffer to the
    /// kernel and returns, leaving the bytes unwritten to disk. An export
    /// interrupted after `File::create` also left a truncated file where a
    /// good one had been.
    pub(crate) fn export_to_path(&mut self, path: impl AsRef<Path>) -> io::Result<u64> {
        let path = path.as_ref();
        let parent = path.parent().unwrap_or(Path::new("."));
        let tmp = parent.join(format!(
            ".{}.export",
            path.file_name().unwrap_or_default().to_string_lossy()
        ));
        let len = self.byte_len();
        {
            let mut out = File::create(&tmp)?;
            let mut offset = 0u64;
            while offset < len {
                let chunk_len = (len - offset).min(EXPORT_CHUNK_SIZE as u64) as usize;
                let chunk = self.read_at(offset, chunk_len)?;
                out.write_all(&chunk)?;
                offset += chunk_len as u64;
            }
            out.sync_all()?;
        }
        std::fs::rename(&tmp, path)?;
        File::open(parent)?.sync_all()?;
        Ok(len)
    }
}

#[cfg(test)]
#[path = "overlay_quota_tests.rs"]
mod overlay_quota_tests;
#[cfg(test)]
#[path = "positional_io_tests.rs"]
mod positional_io_tests;
