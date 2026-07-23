//! Read-only raw-file media backing store and its capacity/read primitives.

use super::*;
use std::fs::File;
use std::io;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;
use std::path::Path;

#[derive(Debug)]
pub(crate) struct RawFileBackend {
    pub(crate) file: File,
    pub(crate) len: u64,
}

impl RawFileBackend {
    pub(crate) fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let file = File::open(path)?;
        let len = file.metadata()?.len();
        Ok(Self { file, len })
    }

    pub(crate) fn capacity_sectors(&self) -> u64 {
        self.len.div_ceil(SECTOR_SIZE)
    }

    pub(crate) fn read_at_into(&mut self, byte_offset: u64, dst: &mut [u8]) -> io::Result<()> {
        let end = byte_offset
            .checked_add(dst.len() as u64)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "read offset overflow"))?;
        if end > self.capacity_sectors() * SECTOR_SIZE {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "virtio block read past media",
            ));
        }
        if byte_offset >= self.len {
            dst.fill(0);
            return Ok(());
        }
        let readable = (self.len - byte_offset).min(dst.len() as u64) as usize;
        self.file.seek(SeekFrom::Start(byte_offset))?;
        self.file.read_exact(&mut dst[..readable])?;
        if readable < dst.len() {
            dst[readable..].fill(0);
        }
        Ok(())
    }
}
