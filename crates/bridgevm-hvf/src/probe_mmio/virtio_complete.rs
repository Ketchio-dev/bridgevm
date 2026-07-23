//! Split out of probe_mmio.rs by responsibility.

use super::*;
use crate::*;

pub(crate) fn complete_probe_virtio_block_request(
    block: &mut VirtioMmioBlockDevice,
    memory: &mut VirtioGuestMemory<'_>,
    backing: VirtioBlockProbeBackingRef<'_>,
) -> Result<VirtioBlockProbeCompletion, VirtioBlockRequestError> {
    let (completion, backing_kind) = match backing {
        VirtioBlockProbeBackingRef::HostFile(path) => {
            let mut backend = FileBlockStorageBackend::open(path)?;
            let backing_kind = backend.kind();
            let completion =
                block.complete_next_available_block_request_from_backend(memory, &mut backend)?;
            (completion, backing_kind)
        }
        VirtioBlockProbeBackingRef::HostIsoReadOnly(path) => {
            let mut backend = ReadOnlyIsoBlockStorageBackend::open(path)?;
            let backing_kind = backend.kind();
            let completion =
                block.complete_next_available_block_request_from_backend(memory, &mut backend)?;
            (completion, backing_kind)
        }
        VirtioBlockProbeBackingRef::HostFileWritable(path) => {
            let mut backend = WritableHostFileBlockStorageBackend::open(path)?;
            let backing_kind = backend.kind();
            let completion =
                block.complete_next_available_block_request_from_backend(memory, &mut backend)?;
            (completion, backing_kind)
        }
        VirtioBlockProbeBackingRef::Synthetic => {
            let mut backend = SyntheticBlockStorageBackend;
            let backing_kind = backend.kind();
            let completion =
                block.complete_next_available_block_request_from_backend(memory, &mut backend)?;
            (completion, backing_kind)
        }
    };
    let byte_offset = completion
        .sector
        .checked_mul(VIRTIO_BLOCK_SECTOR_BYTES)
        .ok_or(VirtioBlockRequestError::StorageOffsetOverflow {
            sector: completion.sector,
        })?;
    let used_len = memory.read_u32(VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS + 8)?;
    let data_prefix = memory.read_bytes(VIRTIO_BLOCK_SYNTHETIC_REQUEST_DATA_ADDRESS, 8)?;
    let status = memory.read_bytes(VIRTIO_BLOCK_SYNTHETIC_REQUEST_STATUS_ADDRESS, 1)?[0];

    Ok(VirtioBlockProbeCompletion {
        completion,
        backing_kind,
        byte_offset,
        used_len,
        data_prefix,
        status,
    })
}

pub(crate) fn complete_probe_virtio_block_writable_file_requests(
    block: &mut VirtioMmioBlockDevice,
    memory: &mut VirtioGuestMemory<'_>,
    path: &PathBuf,
) -> Result<VirtioBlockWritableProbeCompletion, VirtioBlockRequestError> {
    let mut backend = WritableHostFileBlockStorageBackend::open(path)?;
    let backing_kind = backend.kind();
    let initial_completion =
        block.complete_next_available_block_request_from_backend(memory, &mut backend)?;
    let initial_byte_offset = initial_completion
        .sector
        .checked_mul(VIRTIO_BLOCK_SECTOR_BYTES)
        .ok_or(VirtioBlockRequestError::StorageOffsetOverflow {
            sector: initial_completion.sector,
        })?;
    let initial_used_len = memory.read_u32(VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS + 8)?;
    let initial_read = VirtioBlockProbeCompletion {
        completion: initial_completion,
        backing_kind,
        byte_offset: initial_byte_offset,
        used_len: initial_used_len,
        data_prefix: memory.read_bytes(VIRTIO_BLOCK_SYNTHETIC_REQUEST_DATA_ADDRESS, 8)?,
        status: memory.read_bytes(VIRTIO_BLOCK_SYNTHETIC_REQUEST_STATUS_ADDRESS, 1)?[0],
    };

    seed_synthetic_virtio_block_write_request(memory)?;
    let write_completion =
        block.complete_next_available_block_request_from_backend(memory, &mut backend)?;
    let write_used_len = memory.read_u32(VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS + 16)?;
    let write_byte_offset = write_completion
        .sector
        .checked_mul(VIRTIO_BLOCK_SECTOR_BYTES)
        .ok_or(VirtioBlockRequestError::StorageOffsetOverflow {
            sector: write_completion.sector,
        })?;
    let write_data_prefix = memory.read_bytes(VIRTIO_BLOCK_SYNTHETIC_WRITE_DATA_ADDRESS, 8)?;
    let write_status = memory.read_bytes(VIRTIO_BLOCK_SYNTHETIC_WRITE_STATUS_ADDRESS, 1)?[0];

    seed_synthetic_virtio_block_flush_request(memory)?;
    let flush_completion =
        block.complete_next_available_block_request_from_backend(memory, &mut backend)?;
    let flush_used_len = memory.read_u32(VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS + 24)?;
    let flush_status = memory.read_bytes(VIRTIO_BLOCK_SYNTHETIC_FLUSH_STATUS_ADDRESS, 1)?[0];
    drop(backend);

    let mut persisted_data_prefix = vec![0_u8; 8];
    let mut reopened = FileBlockStorageBackend::open(path)?;
    reopened.read_exact_at(write_byte_offset, &mut persisted_data_prefix)?;

    Ok(VirtioBlockWritableProbeCompletion {
        initial_read,
        write_completion,
        write_byte_offset,
        write_used_len,
        write_data_prefix,
        write_status,
        flush_completion,
        flush_used_len,
        flush_status,
        persisted_data_prefix,
    })
}

impl MmioDevice for VirtioMmioBlockDevice {
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn range(&self) -> MmioRange {
        MmioRange {
            start: self.base_ipa,
            bytes: VIRTIO_MMIO_REGISTER_WINDOW_BYTES,
        }
    }

    fn handle(&mut self, access: MmioAccess) -> MmioAction {
        let offset = access.ipa.saturating_sub(self.base_ipa);
        match (access.kind, offset, access.value) {
            (MmioAccessKind::Read, VIRTIO_MMIO_MAGIC_VALUE_OFFSET, None) => {
                MmioAction::ReadValue(VIRTIO_MMIO_MAGIC_VALUE)
            }
            (MmioAccessKind::Read, VIRTIO_MMIO_VERSION_OFFSET, None) => {
                MmioAction::ReadValue(VIRTIO_MMIO_VERSION_VALUE)
            }
            (MmioAccessKind::Read, VIRTIO_MMIO_DEVICE_ID_OFFSET, None) => {
                MmioAction::ReadValue(VIRTIO_MMIO_BLOCK_DEVICE_ID_VALUE)
            }
            (MmioAccessKind::Read, VIRTIO_MMIO_VENDOR_ID_OFFSET, None) => {
                MmioAction::ReadValue(VIRTIO_MMIO_VENDOR_ID_VALUE)
            }
            (MmioAccessKind::Read, VIRTIO_MMIO_DEVICE_FEATURES_OFFSET, None) => {
                MmioAction::ReadValue(self.device_features)
            }
            (MmioAccessKind::Read, VIRTIO_MMIO_DRIVER_FEATURES_OFFSET, None) => {
                MmioAction::ReadValue(self.driver_features)
            }
            (MmioAccessKind::Read, VIRTIO_MMIO_QUEUE_SEL_OFFSET, None) => {
                MmioAction::ReadValue(self.queue_select)
            }
            (MmioAccessKind::Read, VIRTIO_MMIO_QUEUE_NUM_MAX_OFFSET, None) => {
                MmioAction::ReadValue(self.queue_num_max)
            }
            (MmioAccessKind::Read, VIRTIO_MMIO_QUEUE_NUM_OFFSET, None) => {
                MmioAction::ReadValue(self.queue_num)
            }
            (MmioAccessKind::Read, VIRTIO_MMIO_QUEUE_READY_OFFSET, None) => {
                MmioAction::ReadValue(self.queue_ready)
            }
            (MmioAccessKind::Read, VIRTIO_MMIO_INTERRUPT_STATUS_OFFSET, None) => {
                MmioAction::ReadValue(self.interrupt_status)
            }
            (MmioAccessKind::Read, VIRTIO_MMIO_STATUS_OFFSET, None) => {
                MmioAction::ReadValue(self.status)
            }
            (MmioAccessKind::Read, VIRTIO_MMIO_QUEUE_DESC_LOW_OFFSET, None) => {
                MmioAction::ReadValue(self.queue_desc & 0xffff_ffff)
            }
            (MmioAccessKind::Read, VIRTIO_MMIO_QUEUE_DESC_HIGH_OFFSET, None) => {
                MmioAction::ReadValue(self.queue_desc >> 32)
            }
            (MmioAccessKind::Read, VIRTIO_MMIO_QUEUE_DRIVER_LOW_OFFSET, None) => {
                MmioAction::ReadValue(self.queue_driver & 0xffff_ffff)
            }
            (MmioAccessKind::Read, VIRTIO_MMIO_QUEUE_DRIVER_HIGH_OFFSET, None) => {
                MmioAction::ReadValue(self.queue_driver >> 32)
            }
            (MmioAccessKind::Read, VIRTIO_MMIO_QUEUE_DEVICE_LOW_OFFSET, None) => {
                MmioAction::ReadValue(self.queue_device & 0xffff_ffff)
            }
            (MmioAccessKind::Read, VIRTIO_MMIO_QUEUE_DEVICE_HIGH_OFFSET, None) => {
                MmioAction::ReadValue(self.queue_device >> 32)
            }
            (MmioAccessKind::Read, VIRTIO_MMIO_CONFIG_GENERATION_OFFSET, None) => {
                MmioAction::ReadValue(self.config_generation)
            }
            (MmioAccessKind::Read, VIRTIO_MMIO_BLOCK_CAPACITY_LOW_OFFSET, None) => {
                MmioAction::ReadValue(self.capacity_sectors & 0xffff_ffff)
            }
            (MmioAccessKind::Read, VIRTIO_MMIO_BLOCK_CAPACITY_HIGH_OFFSET, None) => {
                MmioAction::ReadValue(self.capacity_sectors >> 32)
            }
            (MmioAccessKind::Write, VIRTIO_MMIO_DRIVER_FEATURES_OFFSET, Some(value)) => {
                let value = Self::mask_value(value, access.width);
                self.driver_features = value;
                MmioAction::WriteAccepted {
                    value,
                    byte: (value & 0xff) as u8,
                }
            }
            (MmioAccessKind::Write, VIRTIO_MMIO_QUEUE_SEL_OFFSET, Some(value)) => {
                let value = Self::mask_value(value, access.width);
                self.queue_select = value;
                MmioAction::WriteAccepted {
                    value,
                    byte: (value & 0xff) as u8,
                }
            }
            (MmioAccessKind::Write, VIRTIO_MMIO_QUEUE_NUM_OFFSET, Some(value)) => {
                let value = Self::mask_value(value, access.width);
                self.queue_num = value;
                MmioAction::WriteAccepted {
                    value,
                    byte: (value & 0xff) as u8,
                }
            }
            (MmioAccessKind::Write, VIRTIO_MMIO_QUEUE_READY_OFFSET, Some(value)) => {
                let value = Self::mask_value(value, access.width);
                self.queue_ready = value;
                MmioAction::WriteAccepted {
                    value,
                    byte: (value & 0xff) as u8,
                }
            }
            (MmioAccessKind::Write, VIRTIO_MMIO_QUEUE_NOTIFY_OFFSET, Some(value)) => {
                let value = Self::mask_value(value, access.width);
                self.queue_notify = value;
                MmioAction::WriteAccepted {
                    value,
                    byte: (value & 0xff) as u8,
                }
            }
            (MmioAccessKind::Write, VIRTIO_MMIO_INTERRUPT_ACK_OFFSET, Some(value)) => {
                let value = Self::mask_value(value, access.width);
                self.interrupt_ack = value;
                self.interrupt_status &= !value;
                MmioAction::WriteAccepted {
                    value,
                    byte: (value & 0xff) as u8,
                }
            }
            (MmioAccessKind::Write, VIRTIO_MMIO_STATUS_OFFSET, Some(value)) => {
                let value = Self::mask_value(value, access.width);
                if value == 0 {
                    self.reset_driver_state();
                } else {
                    self.status = value;
                }
                MmioAction::WriteAccepted {
                    value,
                    byte: (value & 0xff) as u8,
                }
            }
            (MmioAccessKind::Write, VIRTIO_MMIO_QUEUE_DESC_LOW_OFFSET, Some(value)) => {
                let value = Self::mask_value(value, access.width);
                self.queue_desc = Self::replace_low_32(self.queue_desc, value, access.width);
                MmioAction::WriteAccepted {
                    value,
                    byte: (value & 0xff) as u8,
                }
            }
            (MmioAccessKind::Write, VIRTIO_MMIO_QUEUE_DESC_HIGH_OFFSET, Some(value)) => {
                let value = Self::mask_value(value, access.width);
                self.queue_desc = Self::replace_high_32(self.queue_desc, value, access.width);
                MmioAction::WriteAccepted {
                    value,
                    byte: (value & 0xff) as u8,
                }
            }
            (MmioAccessKind::Write, VIRTIO_MMIO_QUEUE_DRIVER_LOW_OFFSET, Some(value)) => {
                let value = Self::mask_value(value, access.width);
                self.queue_driver = Self::replace_low_32(self.queue_driver, value, access.width);
                MmioAction::WriteAccepted {
                    value,
                    byte: (value & 0xff) as u8,
                }
            }
            (MmioAccessKind::Write, VIRTIO_MMIO_QUEUE_DRIVER_HIGH_OFFSET, Some(value)) => {
                let value = Self::mask_value(value, access.width);
                self.queue_driver = Self::replace_high_32(self.queue_driver, value, access.width);
                MmioAction::WriteAccepted {
                    value,
                    byte: (value & 0xff) as u8,
                }
            }
            (MmioAccessKind::Write, VIRTIO_MMIO_QUEUE_DEVICE_LOW_OFFSET, Some(value)) => {
                let value = Self::mask_value(value, access.width);
                self.queue_device = Self::replace_low_32(self.queue_device, value, access.width);
                MmioAction::WriteAccepted {
                    value,
                    byte: (value & 0xff) as u8,
                }
            }
            (MmioAccessKind::Write, VIRTIO_MMIO_QUEUE_DEVICE_HIGH_OFFSET, Some(value)) => {
                let value = Self::mask_value(value, access.width);
                self.queue_device = Self::replace_high_32(self.queue_device, value, access.width);
                MmioAction::WriteAccepted {
                    value,
                    byte: (value & 0xff) as u8,
                }
            }
            _ => MmioAction::Unhandled,
        }
    }
}
