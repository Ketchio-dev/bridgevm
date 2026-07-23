//! Split out of probe_mmio.rs by responsibility.

use super::*;
use crate::*;

pub(crate) trait VirtioBlockStorageBackend {
    fn kind(&self) -> &'static str;
    fn read_exact_at(
        &mut self,
        byte_offset: u64,
        buffer: &mut [u8],
    ) -> Result<(), VirtioBlockRequestError>;

    fn write_exact_at(
        &mut self,
        byte_offset: u64,
        buffer: &[u8],
    ) -> Result<(), VirtioBlockRequestError> {
        Err(VirtioBlockRequestError::StorageWriteRejected {
            backing_kind: self.kind(),
            offset: byte_offset,
            len: buffer.len(),
        })
    }

    fn flush(&mut self) -> Result<(), VirtioBlockRequestError> {
        Ok(())
    }
}

pub(crate) struct SyntheticBlockStorageBackend;

impl VirtioBlockStorageBackend for SyntheticBlockStorageBackend {
    fn kind(&self) -> &'static str {
        "synthetic-sector-pattern"
    }

    fn read_exact_at(
        &mut self,
        byte_offset: u64,
        buffer: &mut [u8],
    ) -> Result<(), VirtioBlockRequestError> {
        let len = buffer.len();
        let sector = byte_offset / VIRTIO_BLOCK_SECTOR_BYTES;
        let sector_offset = byte_offset % VIRTIO_BLOCK_SECTOR_BYTES;
        for (index, byte) in buffer.iter_mut().enumerate() {
            let offset = sector_offset
                .checked_add(u64::try_from(index).map_err(|_| {
                    VirtioBlockRequestError::StorageReadOutOfRange {
                        offset: byte_offset,
                        len,
                        capacity: u64::MAX,
                    }
                })?)
                .ok_or(VirtioBlockRequestError::StorageOffsetOverflow { sector })?;
            *byte = synthetic_block_byte(sector, offset as u32);
        }
        Ok(())
    }
}

pub(crate) struct FileBlockStorageBackend {
    pub(crate) file: File,
    pub(crate) capacity: u64,
}

impl FileBlockStorageBackend {
    pub(crate) fn open(path: &PathBuf) -> Result<Self, VirtioBlockRequestError> {
        let file =
            File::open(path).map_err(|error| VirtioBlockRequestError::StorageOpenFailed {
                path: path.clone(),
                error: error.to_string(),
            })?;
        let capacity = file
            .metadata()
            .map_err(|error| VirtioBlockRequestError::StorageOpenFailed {
                path: path.clone(),
                error: error.to_string(),
            })?
            .len();
        Ok(Self { file, capacity })
    }
}

impl VirtioBlockStorageBackend for FileBlockStorageBackend {
    fn kind(&self) -> &'static str {
        "host-file"
    }

    fn read_exact_at(
        &mut self,
        byte_offset: u64,
        buffer: &mut [u8],
    ) -> Result<(), VirtioBlockRequestError> {
        let len = buffer.len();
        let end = byte_offset.checked_add(len as u64).ok_or(
            VirtioBlockRequestError::StorageReadOutOfRange {
                offset: byte_offset,
                len,
                capacity: self.capacity,
            },
        )?;
        if end > self.capacity {
            return Err(VirtioBlockRequestError::StorageReadOutOfRange {
                offset: byte_offset,
                len,
                capacity: self.capacity,
            });
        }
        self.file
            .seek(SeekFrom::Start(byte_offset))
            .map_err(|error| VirtioBlockRequestError::StorageReadFailed {
                offset: byte_offset,
                len,
                error: error.to_string(),
            })?;
        self.file
            .read_exact(buffer)
            .map_err(|error| VirtioBlockRequestError::StorageReadFailed {
                offset: byte_offset,
                len,
                error: error.to_string(),
            })
    }
}

pub(crate) struct WritableHostFileBlockStorageBackend {
    pub(crate) file: File,
    pub(crate) capacity: u64,
}

impl WritableHostFileBlockStorageBackend {
    pub(crate) fn open(path: &PathBuf) -> Result<Self, VirtioBlockRequestError> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .map_err(|error| VirtioBlockRequestError::StorageOpenFailed {
                path: path.clone(),
                error: error.to_string(),
            })?;
        let capacity = file
            .metadata()
            .map_err(|error| VirtioBlockRequestError::StorageOpenFailed {
                path: path.clone(),
                error: error.to_string(),
            })?
            .len();
        Ok(Self { file, capacity })
    }

    pub(crate) fn checked_range(
        &self,
        byte_offset: u64,
        len: usize,
    ) -> Result<(), VirtioBlockRequestError> {
        let end = byte_offset.checked_add(len as u64).ok_or(
            VirtioBlockRequestError::StorageWriteOutOfRange {
                offset: byte_offset,
                len,
                capacity: self.capacity,
            },
        )?;
        if end > self.capacity {
            return Err(VirtioBlockRequestError::StorageWriteOutOfRange {
                offset: byte_offset,
                len,
                capacity: self.capacity,
            });
        }
        Ok(())
    }
}

impl VirtioBlockStorageBackend for WritableHostFileBlockStorageBackend {
    fn kind(&self) -> &'static str {
        "host-file-writable"
    }

    fn read_exact_at(
        &mut self,
        byte_offset: u64,
        buffer: &mut [u8],
    ) -> Result<(), VirtioBlockRequestError> {
        let len = buffer.len();
        self.checked_range(byte_offset, len)?;
        self.file
            .seek(SeekFrom::Start(byte_offset))
            .map_err(|error| VirtioBlockRequestError::StorageReadFailed {
                offset: byte_offset,
                len,
                error: error.to_string(),
            })?;
        self.file
            .read_exact(buffer)
            .map_err(|error| VirtioBlockRequestError::StorageReadFailed {
                offset: byte_offset,
                len,
                error: error.to_string(),
            })
    }

    fn write_exact_at(
        &mut self,
        byte_offset: u64,
        buffer: &[u8],
    ) -> Result<(), VirtioBlockRequestError> {
        let len = buffer.len();
        self.checked_range(byte_offset, len)?;
        self.file
            .seek(SeekFrom::Start(byte_offset))
            .map_err(|error| VirtioBlockRequestError::StorageWriteFailed {
                offset: byte_offset,
                len,
                error: error.to_string(),
            })?;
        self.file
            .write_all(buffer)
            .map_err(|error| VirtioBlockRequestError::StorageWriteFailed {
                offset: byte_offset,
                len,
                error: error.to_string(),
            })
    }

    fn flush(&mut self) -> Result<(), VirtioBlockRequestError> {
        self.file
            .sync_data()
            .map_err(|error| VirtioBlockRequestError::StorageFlushFailed {
                error: error.to_string(),
            })
    }
}

pub(crate) struct ReadOnlyIsoBlockStorageBackend {
    pub(crate) inner: FileBlockStorageBackend,
}

impl ReadOnlyIsoBlockStorageBackend {
    pub(crate) fn open(path: &PathBuf) -> Result<Self, VirtioBlockRequestError> {
        Ok(Self {
            inner: FileBlockStorageBackend::open(path)?,
        })
    }
}

impl VirtioBlockStorageBackend for ReadOnlyIsoBlockStorageBackend {
    fn kind(&self) -> &'static str {
        "host-iso-readonly"
    }

    fn read_exact_at(
        &mut self,
        byte_offset: u64,
        buffer: &mut [u8],
    ) -> Result<(), VirtioBlockRequestError> {
        self.inner.read_exact_at(byte_offset, buffer)
    }
}
