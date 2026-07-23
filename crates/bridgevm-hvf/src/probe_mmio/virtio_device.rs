//! Split out of probe_mmio.rs by responsibility.

use super::*;
use crate::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VirtioMmioBlockDevice {
    pub(crate) base_ipa: u64,
    pub(crate) device_features: u64,
    pub(crate) driver_features: u64,
    pub(crate) queue_select: u64,
    pub(crate) queue_num_max: u64,
    pub(crate) queue_num: u64,
    pub(crate) queue_ready: u64,
    pub(crate) queue_notify: u64,
    pub(crate) queue_desc: u64,
    pub(crate) queue_driver: u64,
    pub(crate) queue_device: u64,
    pub(crate) interrupt_status: u64,
    pub(crate) interrupt_ack: u64,
    pub(crate) last_avail_idx: u16,
    pub(crate) completed_requests: u64,
    pub(crate) status: u64,
    pub(crate) config_generation: u64,
    pub(crate) capacity_sectors: u64,
}

impl VirtioMmioBlockDevice {
    pub(crate) fn new(base_ipa: u64) -> Self {
        Self::new_with_features_and_capacity(
            base_ipa,
            VIRTIO_MMIO_BLOCK_DEVICE_FEATURES_VALUE,
            VIRTIO_MMIO_BLOCK_CAPACITY_SECTORS,
        )
    }

    pub(crate) fn from_metadata(device: &WindowsArmVirtioBlockDeviceMetadata) -> Self {
        Self::new_with_features_and_capacity(
            device.base_ipa,
            device.device_features,
            device.capacity_sectors,
        )
    }

    pub(crate) fn new_with_features_and_capacity(
        base_ipa: u64,
        device_features: u64,
        capacity_sectors: u64,
    ) -> Self {
        Self {
            base_ipa,
            device_features,
            driver_features: 0,
            queue_select: 0,
            queue_num_max: VIRTIO_MMIO_BLOCK_QUEUE_NUM_MAX_VALUE,
            queue_num: 0,
            queue_ready: 0,
            queue_notify: 0,
            queue_desc: 0,
            queue_driver: 0,
            queue_device: 0,
            interrupt_status: VIRTIO_MMIO_BLOCK_INTERRUPT_STATUS_VALUE,
            interrupt_ack: 0,
            last_avail_idx: 0,
            completed_requests: 0,
            status: 0,
            config_generation: VIRTIO_MMIO_BLOCK_CONFIG_GENERATION_VALUE,
            capacity_sectors,
        }
    }

    pub(crate) fn mask_value(value: u64, width: u8) -> u64 {
        if width >= 8 {
            value
        } else {
            value & ((1_u64 << (u64::from(width) * 8)) - 1)
        }
    }

    pub(crate) fn replace_low_32(current: u64, value: u64, width: u8) -> u64 {
        let value = Self::mask_value(value, width) & 0xffff_ffff;
        (current & 0xffff_ffff_0000_0000) | value
    }

    pub(crate) fn replace_high_32(current: u64, value: u64, width: u8) -> u64 {
        let value = Self::mask_value(value, width) & 0xffff_ffff;
        (current & 0x0000_0000_ffff_ffff) | (value << 32)
    }

    pub(crate) fn reset_driver_state(&mut self) {
        self.driver_features = 0;
        self.queue_select = 0;
        self.queue_num = 0;
        self.queue_ready = 0;
        self.queue_notify = 0;
        self.queue_desc = 0;
        self.queue_driver = 0;
        self.queue_device = 0;
        self.interrupt_status = VIRTIO_MMIO_BLOCK_INTERRUPT_STATUS_VALUE;
        self.interrupt_ack = 0;
        self.last_avail_idx = 0;
        self.completed_requests = 0;
        self.status = 0;
    }

    pub(crate) fn complete_next_available_block_request(
        &mut self,
        memory: &mut VirtioGuestMemory<'_>,
    ) -> Result<VirtioBlockRequestCompletion, VirtioBlockRequestError> {
        let mut backend = SyntheticBlockStorageBackend;
        self.complete_next_available_block_request_from_backend(memory, &mut backend)
    }

    pub(crate) fn complete_next_available_block_request_from_backend(
        &mut self,
        memory: &mut VirtioGuestMemory<'_>,
        backend: &mut impl VirtioBlockStorageBackend,
    ) -> Result<VirtioBlockRequestCompletion, VirtioBlockRequestError> {
        if self.queue_ready != VIRTIO_MMIO_BLOCK_QUEUE_READY_VALUE {
            return Err(VirtioBlockRequestError::QueueNotReady);
        }

        let queue_size = u16::try_from(self.queue_num)
            .ok()
            .filter(|value| *value > 0)
            .ok_or(VirtioBlockRequestError::InvalidQueueSize(self.queue_num))?;
        if u64::from(queue_size) > self.queue_num_max {
            return Err(VirtioBlockRequestError::InvalidQueueSize(self.queue_num));
        }

        let avail_idx = memory.read_u16(self.queue_driver + 2)?;
        if avail_idx == self.last_avail_idx {
            return Err(VirtioBlockRequestError::NoAvailableRequest);
        }

        let avail_slot = u64::from(self.last_avail_idx % queue_size);
        let descriptor_index = memory.read_u16(self.queue_driver + 4 + (avail_slot * 2))?;
        let header_desc =
            VirtqDescriptor::read(memory, self.queue_desc, descriptor_index, queue_size)?;
        if header_desc.len < VIRTIO_BLOCK_REQUEST_HEADER_BYTES {
            return Err(VirtioBlockRequestError::DescriptorTooSmall {
                role: "request header",
                len: header_desc.len,
            });
        }
        if header_desc.flags & VIRTQ_DESC_F_WRITE != 0 {
            return Err(VirtioBlockRequestError::UnexpectedDescriptorFlags {
                role: "request header",
                flags: header_desc.flags,
            });
        }
        if header_desc.flags & VIRTQ_DESC_F_NEXT == 0 {
            return Err(VirtioBlockRequestError::MissingNextDescriptor(
                "request header",
            ));
        }

        let request_type = memory.read_u32(header_desc.addr)?;
        if !matches!(
            request_type,
            VIRTIO_BLK_T_IN | VIRTIO_BLK_T_OUT | VIRTIO_BLK_T_FLUSH
        ) {
            return Err(VirtioBlockRequestError::UnsupportedRequestType(
                request_type,
            ));
        }
        let sector = memory.read_u64(header_desc.addr + 8)?;

        let (status_desc, data_bytes, used_len, status) = match request_type {
            VIRTIO_BLK_T_FLUSH => {
                let status_desc =
                    VirtqDescriptor::read(memory, self.queue_desc, header_desc.next, queue_size)?;
                if status_desc.len < VIRTIO_BLOCK_STATUS_BYTES {
                    return Err(VirtioBlockRequestError::DescriptorTooSmall {
                        role: "status",
                        len: status_desc.len,
                    });
                }
                if status_desc.flags & VIRTQ_DESC_F_WRITE == 0 {
                    return Err(VirtioBlockRequestError::UnexpectedDescriptorFlags {
                        role: "status",
                        flags: status_desc.flags,
                    });
                }
                let status = match backend.flush() {
                    Ok(()) => VIRTIO_BLK_S_OK,
                    Err(VirtioBlockRequestError::StorageWriteRejected { .. }) => VIRTIO_BLK_S_IOERR,
                    Err(error) => return Err(error),
                };
                (status_desc, 0, VIRTIO_BLOCK_STATUS_BYTES, status)
            }
            VIRTIO_BLK_T_IN | VIRTIO_BLK_T_OUT => {
                let data_desc =
                    VirtqDescriptor::read(memory, self.queue_desc, header_desc.next, queue_size)?;
                if data_desc.len == 0 || data_desc.len > VIRTIO_BLOCK_MAX_SYNTHETIC_IO_BYTES {
                    return Err(VirtioBlockRequestError::InvalidDataLength(data_desc.len));
                }
                match request_type {
                    VIRTIO_BLK_T_IN => {
                        if data_desc.flags & VIRTQ_DESC_F_WRITE == 0
                            || data_desc.flags & VIRTQ_DESC_F_NEXT == 0
                        {
                            return Err(VirtioBlockRequestError::UnexpectedDescriptorFlags {
                                role: "data",
                                flags: data_desc.flags,
                            });
                        }
                    }
                    VIRTIO_BLK_T_OUT => {
                        if data_desc.flags & VIRTQ_DESC_F_WRITE != 0
                            || data_desc.flags & VIRTQ_DESC_F_NEXT == 0
                        {
                            return Err(VirtioBlockRequestError::UnexpectedDescriptorFlags {
                                role: "data",
                                flags: data_desc.flags,
                            });
                        }
                    }
                    _ => unreachable!("request_type checked above"),
                }

                let status_desc =
                    VirtqDescriptor::read(memory, self.queue_desc, data_desc.next, queue_size)?;
                if status_desc.len < VIRTIO_BLOCK_STATUS_BYTES {
                    return Err(VirtioBlockRequestError::DescriptorTooSmall {
                        role: "status",
                        len: status_desc.len,
                    });
                }
                if status_desc.flags & VIRTQ_DESC_F_WRITE == 0 {
                    return Err(VirtioBlockRequestError::UnexpectedDescriptorFlags {
                        role: "status",
                        flags: status_desc.flags,
                    });
                }

                let byte_offset = sector
                    .checked_mul(VIRTIO_BLOCK_SECTOR_BYTES)
                    .ok_or(VirtioBlockRequestError::StorageOffsetOverflow { sector })?;
                let status = match request_type {
                    VIRTIO_BLK_T_IN => {
                        let mut data = vec![0_u8; data_desc.len as usize];
                        backend.read_exact_at(byte_offset, &mut data)?;
                        memory.write_bytes(data_desc.addr, &data)?;
                        VIRTIO_BLK_S_OK
                    }
                    VIRTIO_BLK_T_OUT => {
                        let data = memory.read_slice(data_desc.addr, data_desc.len as usize)?;
                        match backend.write_exact_at(byte_offset, data) {
                            Ok(()) => VIRTIO_BLK_S_OK,
                            Err(VirtioBlockRequestError::StorageWriteRejected { .. }) => {
                                VIRTIO_BLK_S_IOERR
                            }
                            Err(error) => return Err(error),
                        }
                    }
                    _ => unreachable!("request_type checked above"),
                };
                let used_len = match request_type {
                    VIRTIO_BLK_T_IN => data_desc.len + VIRTIO_BLOCK_STATUS_BYTES,
                    VIRTIO_BLK_T_OUT => VIRTIO_BLOCK_STATUS_BYTES,
                    _ => unreachable!("request_type checked above"),
                };
                (status_desc, data_desc.len, used_len, status)
            }
            _ => unreachable!("request_type checked above"),
        };

        memory.write_u8(status_desc.addr, status)?;

        let used_idx = memory.read_u16(self.queue_device + 2)?;
        let used_slot = u64::from(used_idx % queue_size);
        let used_elem = self.queue_device + 4 + (used_slot * 8);
        memory.write_u32(used_elem, u32::from(descriptor_index))?;
        memory.write_u32(used_elem + 4, used_len)?;
        memory.write_u16(self.queue_device + 2, used_idx.wrapping_add(1))?;

        self.last_avail_idx = self.last_avail_idx.wrapping_add(1);
        self.completed_requests = self.completed_requests.saturating_add(1);
        self.interrupt_status |= VIRTIO_MMIO_INTERRUPT_USED_BUFFER_VALUE;

        Ok(VirtioBlockRequestCompletion {
            descriptor_index,
            request_type,
            sector,
            data_bytes,
            status,
            used_index: used_idx.wrapping_add(1),
            interrupt_status: self.interrupt_status,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mmio_bus_typed_lookup_skips_overlapping_wrong_type() {
        let mut bus = MmioBus::default();
        let block_base = 0x5000_2000;
        bus.attach(Box::new(Pl011UartDevice::new(block_base, 0x90)));
        bus.attach(Box::new(VirtioMmioBlockDevice::new(block_base)));

        assert!(bus
            .find_device_mut_at::<VirtioMmioBlockDevice>(block_base)
            .is_some());
    }
}
