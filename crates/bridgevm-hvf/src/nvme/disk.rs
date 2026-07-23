//! Host-side block backing store: in-memory and raw-file with COW overlay, range validation, flush, export.

use super::*;
use std::collections::BTreeMap;
use std::fs::File;
use std::fs::OpenOptions;
use std::io;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;
use std::io::Write;
use std::path::Path;

pub(crate) fn rounded_disk_len(bytes: usize) -> usize {
    bytes.div_ceil(LBA_SIZE) * LBA_SIZE
}

pub(crate) const FILE_OVERLAY_CHUNK_SIZE: u64 = PAGE_SIZE_U64;

pub(crate) const EXPORT_CHUNK_SIZE: usize = 1024 * 1024;

#[derive(Debug)]
pub(crate) enum DiskBackend {
    Memory(Vec<u8>),
    RawFile(RawFileDisk),
}

#[derive(Debug)]
pub(crate) struct RawFileDisk {
    pub(crate) file: File,
    pub(crate) len: u64,
    pub(crate) overlay: BTreeMap<u64, Vec<u8>>,
    pub(crate) write_back: bool,
    #[cfg(test)]
    pub(crate) sync_failure: Option<io::ErrorKind>,
    #[cfg(test)]
    pub(crate) sync_attempts: usize,
}

impl DiskBackend {
    pub(crate) fn memory(mut disk: Vec<u8>) -> Self {
        let len = rounded_disk_len(disk.len());
        disk.resize(len, 0);
        Self::Memory(disk)
    }

    pub(crate) fn raw_file(path: impl AsRef<Path>, write_back: bool) -> io::Result<Self> {
        let path = path.as_ref();
        let file = OpenOptions::new().read(true).write(write_back).open(path)?;
        let len = file.metadata()?.len();
        if len % LBA_SIZE as u64 != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "{} is {len} bytes, not a multiple of the {LBA_SIZE}-byte NVMe LBA size",
                    path.display()
                ),
            ));
        }
        Ok(Self::RawFile(RawFileDisk {
            file,
            len,
            overlay: BTreeMap::new(),
            write_back,
            #[cfg(test)]
            sync_failure: None,
            #[cfg(test)]
            sync_attempts: 0,
        }))
    }

    pub(crate) fn byte_len(&self) -> u64 {
        match self {
            Self::Memory(disk) => disk.len() as u64,
            Self::RawFile(disk) => disk.len,
        }
    }

    pub(crate) fn memory_image(&self) -> Option<&[u8]> {
        match self {
            Self::Memory(disk) => Some(disk),
            Self::RawFile(_) => None,
        }
    }

    pub(crate) fn read_at(&mut self, offset: u64, len: usize) -> io::Result<Vec<u8>> {
        let mut out = vec![0u8; len];
        self.read_at_into(offset, &mut out)?;
        Ok(out)
    }

    /// Read `dst.len()` bytes at `offset` directly into `dst`. Both backends are
    /// bounds-checked against the namespace size, so a guest-supplied range that
    /// runs past the image is rejected rather than reading adjacent bytes. This is
    /// the coalesced data-path primitive: the NVMe engine issues one call per
    /// physically-contiguous guest segment (often the whole transfer) instead of
    /// one allocation + `pread` per 4 KiB PRP page.
    pub(crate) fn read_at_into(&mut self, offset: u64, dst: &mut [u8]) -> io::Result<()> {
        self.validate_range(offset, dst.len())?;
        match self {
            Self::Memory(disk) => {
                let start = offset as usize;
                dst.copy_from_slice(&disk[start..start + dst.len()]);
                Ok(())
            }
            Self::RawFile(disk) => disk.read_at_into(offset, dst),
        }
    }

    pub(crate) fn write_at(&mut self, offset: u64, data: &[u8]) -> io::Result<()> {
        self.validate_range(offset, data.len())?;
        match self {
            Self::Memory(disk) => {
                let start = offset as usize;
                disk[start..start + data.len()].copy_from_slice(data);
                Ok(())
            }
            Self::RawFile(disk) => disk.write_at(offset, data),
        }
    }

    pub(crate) fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Memory(_) => Ok(()),
            Self::RawFile(disk) => disk.flush(),
        }
    }

    pub(crate) fn export_to_path(&mut self, path: impl AsRef<Path>) -> io::Result<u64> {
        let mut out = File::create(path)?;
        let len = self.byte_len();
        let mut offset = 0u64;
        while offset < len {
            let chunk_len = (len - offset).min(EXPORT_CHUNK_SIZE as u64) as usize;
            let chunk = self.read_at(offset, chunk_len)?;
            out.write_all(&chunk)?;
            offset += chunk_len as u64;
        }
        out.flush()?;
        Ok(len)
    }

    pub(crate) fn validate_range(&self, offset: u64, len: usize) -> io::Result<()> {
        let end = offset.checked_add(len as u64).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "NVMe disk range overflows")
        })?;
        if end > self.byte_len() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                format!(
                    "NVMe disk range {offset:#x}..{end:#x} exceeds image size {:#x}",
                    self.byte_len()
                ),
            ));
        }
        Ok(())
    }
}

impl RawFileDisk {
    /// Make prior write-through writes durable on the backing storage. A
    /// read-only raw backend keeps guest writes in its volatile COW overlay, so
    /// it has no host-file data to synchronize.
    pub(crate) fn flush(&mut self) -> io::Result<()> {
        if !self.write_back {
            return Ok(());
        }

        #[cfg(test)]
        {
            self.sync_attempts += 1;
            if let Some(kind) = self.sync_failure {
                return Err(io::Error::new(kind, "injected raw-file sync failure"));
            }
        }

        self.file.sync_data()
    }

    /// Read `dst.len()` bytes at `offset`: one `pread` of the whole span from the
    /// backing file, then the sparse write overlay merged on top. Merging over the
    /// whole span (rather than page-by-page) keeps the coalesced read a single
    /// syscall while preserving the COW-overlay read semantics exactly.
    pub(crate) fn read_at_into(&mut self, offset: u64, dst: &mut [u8]) -> io::Result<()> {
        if dst.is_empty() {
            return Ok(());
        }
        let end = offset + dst.len() as u64;
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.read_exact(dst)?;
        let overlay_start = offset.saturating_sub(FILE_OVERLAY_CHUNK_SIZE - 1);
        for (&chunk_base, chunk) in self.overlay.range(overlay_start..end) {
            let chunk_end = chunk_base + chunk.len() as u64;
            if chunk_end <= offset {
                continue;
            }
            let copy_start = offset.max(chunk_base);
            let copy_end = end.min(chunk_end);
            let src = (copy_start - chunk_base) as usize;
            let dst_off = (copy_start - offset) as usize;
            let copy_len = (copy_end - copy_start) as usize;
            dst[dst_off..dst_off + copy_len].copy_from_slice(&chunk[src..src + copy_len]);
        }
        Ok(())
    }

    pub(crate) fn write_at(&mut self, offset: u64, data: &[u8]) -> io::Result<()> {
        if data.is_empty() {
            return Ok(());
        }
        if self.write_back {
            self.file.seek(SeekFrom::Start(offset))?;
            self.file.write_all(data)?;
            return Ok(());
        }

        let mut copied = 0usize;
        while copied < data.len() {
            let abs = offset + copied as u64;
            let chunk_base = (abs / FILE_OVERLAY_CHUNK_SIZE) * FILE_OVERLAY_CHUNK_SIZE;
            let chunk_len = self.chunk_len(chunk_base)?;
            let chunk_off = (abs - chunk_base) as usize;
            let copy_len = (data.len() - copied).min(chunk_len - chunk_off);

            if !self.overlay.contains_key(&chunk_base) {
                let mut chunk = vec![0u8; chunk_len];
                self.file.seek(SeekFrom::Start(chunk_base))?;
                self.file.read_exact(&mut chunk)?;
                self.overlay.insert(chunk_base, chunk);
            }
            let chunk = self.overlay.get_mut(&chunk_base).unwrap();
            chunk[chunk_off..chunk_off + copy_len]
                .copy_from_slice(&data[copied..copied + copy_len]);
            copied += copy_len;
        }
        Ok(())
    }

    pub(crate) fn chunk_len(&self, chunk_base: u64) -> io::Result<usize> {
        if chunk_base >= self.len {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "overlay chunk starts past the NVMe image",
            ));
        }
        Ok((self.len - chunk_base).min(FILE_OVERLAY_CHUNK_SIZE) as usize)
    }
}
