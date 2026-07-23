//! virtio-MMIO register read/write behaviour, device-config space, queue reset.

use super::*;
use crate::fwcfg::GuestMemoryMut;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VirtioMmioBlockResult {
    ReadValue(u64),
    WriteAck,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VirtioPciBlockOp {
    Read { size: u8 },
    Write { size: u8, value: u64 },
}

pub(crate) fn set_low(current: u64, value: u64) -> u64 {
    (current & !0xffff_ffff) | (value & 0xffff_ffff)
}

pub(crate) fn set_high(current: u64, value: u64) -> u64 {
    (current & 0xffff_ffff) | ((value & 0xffff_ffff) << 32)
}

pub(crate) fn mask_to_size(value: u64, size: u8) -> u64 {
    match size {
        1 => value & 0xff,
        2 => value & 0xffff,
        4 => value & 0xffff_ffff,
        _ => value,
    }
}

pub(crate) fn read_le_from_bytes(bytes: &[u8], offset: u64, size: u8) -> Option<u64> {
    let offset = usize::try_from(offset).ok()?;
    let size = usize::from(size);
    if offset.checked_add(size)? > bytes.len() || size > 8 {
        return None;
    }
    let mut buf = [0u8; 8];
    buf[..size].copy_from_slice(&bytes[offset..offset + size]);
    Some(u64::from_le_bytes(buf))
}

impl VirtioMmioBlock {
    pub fn access(
        &mut self,
        offset: u64,
        is_write: bool,
        size: u8,
        value: u64,
        mem: &mut dyn GuestMemoryMut,
    ) -> VirtioMmioBlockResult {
        if !is_write {
            return VirtioMmioBlockResult::ReadValue(self.read(offset, size));
        }
        self.write(offset, size, value, mem);
        VirtioMmioBlockResult::WriteAck
    }

    pub(crate) fn read(&self, offset: u64, size: u8) -> u64 {
        let value = match offset {
            REG_MAGIC => u64::from(MAGIC_VALUE),
            REG_VERSION => u64::from(self.transport.version()),
            REG_DEVICE_ID => u64::from(DEVICE_ID_BLOCK),
            REG_VENDOR_ID => u64::from(VENDOR_ID_QEMU),
            REG_DEVICE_FEATURES => u64::from(self.device_features()),
            REG_DRIVER_FEATURES => {
                u64::from(self.driver_features[self.driver_features_sel.min(1) as usize])
            }
            REG_QUEUE_NUM_MAX => {
                if self.queue_sel == 0 {
                    u64::from(QUEUE_MAX)
                } else {
                    0
                }
            }
            REG_QUEUE_NUM => u64::from(self.queue_num),
            REG_QUEUE_READY => u64::from(self.queue_ready as u32),
            REG_QUEUE_PFN if self.transport == VirtioMmioTransport::Legacy => {
                if self.queue_ready && self.guest_page_size != 0 {
                    self.queue_desc / u64::from(self.guest_page_size)
                } else {
                    0
                }
            }
            REG_INTERRUPT_STATUS => u64::from(self.interrupt_status),
            REG_STATUS => u64::from(self.status),
            REG_QUEUE_DESC_LOW => self.queue_desc & 0xffff_ffff,
            REG_QUEUE_DESC_HIGH => self.queue_desc >> 32,
            REG_QUEUE_DRIVER_LOW => self.queue_driver & 0xffff_ffff,
            REG_QUEUE_DRIVER_HIGH => self.queue_driver >> 32,
            REG_QUEUE_DEVICE_LOW => self.queue_device & 0xffff_ffff,
            REG_QUEUE_DEVICE_HIGH => self.queue_device >> 32,
            REG_CONFIG_GENERATION => 0,
            o if o >= REG_CONFIG => self.config_read(o - REG_CONFIG, size),
            _ => 0,
        };
        mask_to_size(value, size)
    }

    pub(crate) fn write(
        &mut self,
        offset: u64,
        _size: u8,
        value: u64,
        mem: &mut dyn GuestMemoryMut,
    ) {
        match offset {
            REG_DEVICE_FEATURES_SEL => self.device_features_sel = value as u32,
            REG_DRIVER_FEATURES_SEL => self.driver_features_sel = value as u32,
            REG_DRIVER_FEATURES => {
                if self.driver_features_sel < 2 {
                    self.driver_features[self.driver_features_sel as usize] = value as u32;
                }
            }
            REG_GUEST_PAGE_SIZE if self.transport == VirtioMmioTransport::Legacy => {
                if value != 0 {
                    self.guest_page_size = value as u32;
                }
            }
            REG_QUEUE_SEL => self.queue_sel = value as u32,
            REG_QUEUE_NUM => {
                if self.queue_sel == 0 {
                    self.queue_num = (value as u16).min(QUEUE_MAX);
                }
            }
            REG_QUEUE_ALIGN if self.transport == VirtioMmioTransport::Legacy => {
                if value != 0 {
                    self.queue_align = value as u32;
                    if self.queue_ready {
                        self.derive_legacy_queue_addresses();
                    }
                }
            }
            REG_QUEUE_PFN if self.transport == VirtioMmioTransport::Legacy => {
                if self.queue_sel == 0 && value != 0 {
                    self.queue_desc = value.saturating_mul(u64::from(self.guest_page_size));
                    self.queue_ready = true;
                    self.derive_legacy_queue_addresses();
                } else if self.queue_sel == 0 {
                    self.queue_ready = false;
                    self.last_avail_idx = 0;
                }
            }
            REG_QUEUE_READY => {
                if self.queue_sel == 0 && self.transport == VirtioMmioTransport::Modern {
                    self.queue_ready = value != 0;
                    if !self.queue_ready {
                        self.last_avail_idx = 0;
                    }
                }
            }
            REG_QUEUE_NOTIFY => {
                if value == 0 {
                    self.stats.notify_count = self.stats.notify_count.saturating_add(1);
                    self.process_queue(mem);
                }
            }
            REG_INTERRUPT_ACK => self.interrupt_status &= !(value as u32),
            REG_STATUS => {
                self.status = value as u32;
                if value == 0 {
                    self.reset_queue();
                }
            }
            REG_QUEUE_DESC_LOW if self.transport == VirtioMmioTransport::Modern => {
                self.queue_desc = set_low(self.queue_desc, value)
            }
            REG_QUEUE_DESC_HIGH if self.transport == VirtioMmioTransport::Modern => {
                self.queue_desc = set_high(self.queue_desc, value)
            }
            REG_QUEUE_DRIVER_LOW if self.transport == VirtioMmioTransport::Modern => {
                self.queue_driver = set_low(self.queue_driver, value)
            }
            REG_QUEUE_DRIVER_HIGH if self.transport == VirtioMmioTransport::Modern => {
                self.queue_driver = set_high(self.queue_driver, value)
            }
            REG_QUEUE_DEVICE_LOW if self.transport == VirtioMmioTransport::Modern => {
                self.queue_device = set_low(self.queue_device, value)
            }
            REG_QUEUE_DEVICE_HIGH if self.transport == VirtioMmioTransport::Modern => {
                self.queue_device = set_high(self.queue_device, value)
            }
            _ => {}
        }
    }

    pub(crate) fn reset_queue(&mut self) {
        self.queue_num = 0;
        self.queue_ready = false;
        self.queue_desc = 0;
        self.queue_driver = 0;
        self.queue_device = 0;
        self.queue_align = 4096;
        self.interrupt_status = 0;
        self.last_avail_idx = 0;
    }

    pub(crate) fn config_read(&self, offset: u64, size: u8) -> u64 {
        let capacity = self.backend.capacity_sectors();
        let mut config = [0u8; 0x40];
        config[0..8].copy_from_slice(&capacity.to_le_bytes());
        config[0x14..0x18].copy_from_slice(&(SECTOR_SIZE as u32).to_le_bytes());
        read_le_from_bytes(&config, offset, size).unwrap_or(0)
    }
}
