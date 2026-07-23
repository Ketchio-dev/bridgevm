//! Split out of probe_mmio.rs by responsibility.

use super::*;
use crate::*;

#[derive(Debug)]
pub(crate) struct VirtioGuestMemory<'a> {
    pub(crate) base_ipa: u64,
    pub(crate) bytes: &'a mut [u8],
}

impl<'a> VirtioGuestMemory<'a> {
    pub(crate) fn new(base_ipa: u64, bytes: &'a mut [u8]) -> Self {
        Self { base_ipa, bytes }
    }

    pub(crate) fn range(
        &self,
        ipa: u64,
        len: usize,
    ) -> Result<std::ops::Range<usize>, VirtioBlockRequestError> {
        let offset = ipa
            .checked_sub(self.base_ipa)
            .ok_or(VirtioBlockRequestError::MemoryOutOfRange { ipa, len })?;
        let offset = usize::try_from(offset)
            .map_err(|_| VirtioBlockRequestError::MemoryOutOfRange { ipa, len })?;
        let end = offset
            .checked_add(len)
            .ok_or(VirtioBlockRequestError::MemoryOutOfRange { ipa, len })?;
        if end > self.bytes.len() {
            return Err(VirtioBlockRequestError::MemoryOutOfRange { ipa, len });
        }
        Ok(offset..end)
    }

    pub(crate) fn read_bytes(
        &self,
        ipa: u64,
        len: usize,
    ) -> Result<Vec<u8>, VirtioBlockRequestError> {
        Ok(self.read_slice(ipa, len)?.to_vec())
    }

    pub(crate) fn read_slice(
        &self,
        ipa: u64,
        len: usize,
    ) -> Result<&[u8], VirtioBlockRequestError> {
        let range = self.range(ipa, len)?;
        Ok(&self.bytes[range])
    }

    pub(crate) fn read_array<const N: usize>(
        &self,
        ipa: u64,
    ) -> Result<[u8; N], VirtioBlockRequestError> {
        let mut bytes = [0u8; N];
        bytes.copy_from_slice(self.read_slice(ipa, N)?);
        Ok(bytes)
    }

    pub(crate) fn read_u16(&self, ipa: u64) -> Result<u16, VirtioBlockRequestError> {
        Ok(u16::from_le_bytes(self.read_array(ipa)?))
    }

    pub(crate) fn read_u32(&self, ipa: u64) -> Result<u32, VirtioBlockRequestError> {
        Ok(u32::from_le_bytes(self.read_array(ipa)?))
    }

    pub(crate) fn read_u64(&self, ipa: u64) -> Result<u64, VirtioBlockRequestError> {
        Ok(u64::from_le_bytes(self.read_array(ipa)?))
    }

    pub(crate) fn write_bytes(
        &mut self,
        ipa: u64,
        bytes: &[u8],
    ) -> Result<(), VirtioBlockRequestError> {
        let range = self.range(ipa, bytes.len())?;
        self.bytes[range].copy_from_slice(bytes);
        Ok(())
    }

    pub(crate) fn write_u8(&mut self, ipa: u64, value: u8) -> Result<(), VirtioBlockRequestError> {
        self.write_bytes(ipa, &[value])
    }

    pub(crate) fn write_u16(
        &mut self,
        ipa: u64,
        value: u16,
    ) -> Result<(), VirtioBlockRequestError> {
        self.write_bytes(ipa, &value.to_le_bytes())
    }

    pub(crate) fn write_u32(
        &mut self,
        ipa: u64,
        value: u32,
    ) -> Result<(), VirtioBlockRequestError> {
        self.write_bytes(ipa, &value.to_le_bytes())
    }

    pub(crate) fn write_u64(
        &mut self,
        ipa: u64,
        value: u64,
    ) -> Result<(), VirtioBlockRequestError> {
        self.write_bytes(ipa, &value.to_le_bytes())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VirtqDescriptor {
    pub(crate) addr: u64,
    pub(crate) len: u32,
    pub(crate) flags: u16,
    pub(crate) next: u16,
}

impl VirtqDescriptor {
    pub(crate) fn read(
        memory: &VirtioGuestMemory<'_>,
        table_ipa: u64,
        index: u16,
        queue_size: u16,
    ) -> Result<Self, VirtioBlockRequestError> {
        if index >= queue_size {
            return Err(VirtioBlockRequestError::DescriptorIndexOutOfRange { index, queue_size });
        }
        let descriptor_ipa = table_ipa + (u64::from(index) * VIRTQ_DESC_SIZE);
        Ok(Self {
            addr: memory.read_u64(descriptor_ipa)?,
            len: memory.read_u32(descriptor_ipa + 8)?,
            flags: memory.read_u16(descriptor_ipa + 12)?,
            next: memory.read_u16(descriptor_ipa + 14)?,
        })
    }

    pub(crate) fn write(
        &self,
        memory: &mut VirtioGuestMemory<'_>,
        table_ipa: u64,
        index: u16,
    ) -> Result<(), VirtioBlockRequestError> {
        let descriptor_ipa = table_ipa + (u64::from(index) * VIRTQ_DESC_SIZE);
        memory.write_u64(descriptor_ipa, self.addr)?;
        memory.write_u32(descriptor_ipa + 8, self.len)?;
        memory.write_u16(descriptor_ipa + 12, self.flags)?;
        memory.write_u16(descriptor_ipa + 14, self.next)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VirtioBlockRequestCompletion {
    pub(crate) descriptor_index: u16,
    pub(crate) request_type: u32,
    pub(crate) sector: u64,
    pub(crate) data_bytes: u32,
    pub(crate) status: u8,
    pub(crate) used_index: u16,
    pub(crate) interrupt_status: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum VirtioBlockRequestError {
    QueueNotReady,
    InvalidQueueSize(u64),
    NoAvailableRequest,
    MemoryOutOfRange {
        ipa: u64,
        len: usize,
    },
    DescriptorIndexOutOfRange {
        index: u16,
        queue_size: u16,
    },
    DescriptorTooSmall {
        role: &'static str,
        len: u32,
    },
    MissingNextDescriptor(&'static str),
    UnexpectedDescriptorFlags {
        role: &'static str,
        flags: u16,
    },
    UnsupportedRequestType(u32),
    InvalidDataLength(u32),
    StorageOffsetOverflow {
        sector: u64,
    },
    StorageReadOutOfRange {
        offset: u64,
        len: usize,
        capacity: u64,
    },
    StorageOpenFailed {
        path: PathBuf,
        error: String,
    },
    StorageReadFailed {
        offset: u64,
        len: usize,
        error: String,
    },
    StorageWriteRejected {
        backing_kind: &'static str,
        offset: u64,
        len: usize,
    },
    StorageWriteOutOfRange {
        offset: u64,
        len: usize,
        capacity: u64,
    },
    StorageWriteFailed {
        offset: u64,
        len: usize,
        error: String,
    },
    StorageFlushFailed {
        error: String,
    },
    MissingBlockDeviceMetadata {
        ipa: u64,
    },
    MissingBlockBackingPath {
        role: &'static str,
        backing_kind: &'static str,
    },
    UnsupportedBlockBackingKind {
        role: &'static str,
        backing_kind: &'static str,
    },
    UnsupportedQueueNotifyValue {
        role: &'static str,
        value: u64,
    },
    UnexpectedQueueNotifyIpa {
        role: &'static str,
        ipa: u64,
    },
    MissingMmioDevice(&'static str),
    UnexpectedMmioAction {
        register: &'static str,
        action: MmioAction,
    },
}

impl VirtioBlockRequestError {
    pub(crate) fn render_blocker(&self) -> String {
        match self {
            Self::MissingMmioDevice(device) => format!("missing MMIO device: {device}"),
            Self::UnexpectedMmioAction { register, action } => {
                format!("unexpected MMIO action for {register}: {action:?}")
            }
            Self::StorageOpenFailed { path, error } => {
                format!(
                    "could not open host block backing {}: {error}",
                    path.display()
                )
            }
            Self::StorageReadFailed { offset, len, error } => {
                format!("host block backing read failed at {offset:#x} for {len:#x} bytes: {error}")
            }
            Self::StorageWriteRejected {
                backing_kind,
                offset,
                len,
            } => {
                format!("{backing_kind} rejected block write at {offset:#x} for {len:#x} bytes")
            }
            Self::StorageWriteFailed { offset, len, error } => {
                format!(
                    "host block backing write failed at {offset:#x} for {len:#x} bytes: {error}"
                )
            }
            Self::StorageWriteOutOfRange {
                offset,
                len,
                capacity,
            } => {
                format!(
                    "host block backing write out of range at {offset:#x} for {len:#x} bytes against capacity {capacity:#x}"
                )
            }
            Self::StorageFlushFailed { error } => {
                format!("host block backing flush failed: {error}")
            }
            Self::MissingBlockDeviceMetadata { ipa } => {
                format!("missing firmware block-device metadata for MMIO IPA {ipa:#x}")
            }
            Self::MissingBlockBackingPath { role, backing_kind } => {
                format!("missing {backing_kind} backing path for firmware block device {role}")
            }
            Self::UnsupportedBlockBackingKind { role, backing_kind } => {
                format!("unsupported backing kind {backing_kind} for firmware block device {role}")
            }
            Self::UnsupportedQueueNotifyValue { role, value } => {
                format!(
                    "unsupported queue_notify value {value:#x} for firmware block device {role}"
                )
            }
            Self::UnexpectedQueueNotifyIpa { role, ipa } => {
                format!("unexpected queue_notify IPA {ipa:#x} for firmware block device {role}")
            }
            error => format!("{error:?}"),
        }
    }
}
