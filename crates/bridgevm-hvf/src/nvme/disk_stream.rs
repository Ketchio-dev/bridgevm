//! Bounded current-image export, including overlays, without owning publication.

use super::{DiskBackend, EXPORT_CHUNK_SIZE};
use std::io::{self, Write};

impl DiskBackend {
    pub(crate) fn export_into(&mut self, out: &mut dyn Write) -> io::Result<u64> {
        let len = self.byte_len();
        let mut offset = 0u64;
        while offset < len {
            let chunk_len = (len - offset).min(EXPORT_CHUNK_SIZE as u64) as usize;
            let chunk = self.read_at(offset, chunk_len)?;
            out.write_all(&chunk)?;
            offset += chunk_len as u64;
        }
        Ok(len)
    }
}
