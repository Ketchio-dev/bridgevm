//! Legacy virtio-pci I/O BAR aliasing and PFN-derived ring address layout.

use super::VIRTIO_BLK_F_RO;
use super::*;
use crate::fwcfg::GuestMemoryMut;

pub(crate) fn align_up(value: u64, align: u64) -> u64 {
    let align = align.max(1);
    value.div_ceil(align).saturating_mul(align)
}

impl VirtioMmioBlock {
    pub(crate) fn legacy_pci_io_read(&self, offset: u64, size: u8) -> u64 {
        let value = match offset {
            0x00 => u64::from(VIRTIO_BLK_F_RO | VIRTIO_BLK_F_BLK_SIZE),
            0x04 => u64::from(self.driver_features[0]),
            0x08 => {
                if self.queue_ready && self.guest_page_size != 0 {
                    self.queue_desc / u64::from(self.guest_page_size)
                } else {
                    0
                }
            }
            0x0c => {
                if self.queue_sel == 0 {
                    u64::from(QUEUE_MAX)
                } else {
                    0
                }
            }
            0x0e => u64::from(self.queue_sel),
            0x12 => u64::from(self.status),
            0x13 => u64::from(self.interrupt_status),
            o if o >= 0x14 => self.config_read(o - 0x14, size),
            _ => 0,
        };
        mask_to_size(value, size)
    }

    pub(crate) fn legacy_pci_io_write(
        &mut self,
        offset: u64,
        _size: u8,
        value: u64,
        mem: &mut dyn GuestMemoryMut,
    ) {
        match offset {
            0x04 => self.driver_features[0] = value as u32,
            0x08 => {
                if self.queue_sel == 0 && value != 0 {
                    self.guest_page_size = 4096;
                    self.queue_num = QUEUE_MAX;
                    self.queue_desc = value.saturating_mul(u64::from(self.guest_page_size));
                    self.queue_ready = true;
                    self.derive_legacy_queue_addresses();
                } else if self.queue_sel == 0 {
                    self.queue_ready = false;
                    self.last_avail_idx = 0;
                }
            }
            0x0e => self.queue_sel = value as u32,
            0x10 => {
                if value == 0 {
                    self.stats.notify_count = self.stats.notify_count.saturating_add(1);
                    self.process_queue(mem);
                }
            }
            0x12 => {
                self.status = value as u32;
                if value == 0 {
                    self.reset_queue();
                }
            }
            0x13 => self.interrupt_status &= !(value as u32),
            _ => {}
        }
    }

    pub(crate) fn derive_legacy_queue_addresses(&mut self) {
        if self.queue_num == 0 || self.queue_desc == 0 {
            return;
        }
        let avail = self
            .queue_desc
            .saturating_add(u64::from(self.queue_num) * DESC_SIZE);
        let used_unaligned = avail
            .saturating_add(4)
            .saturating_add(u64::from(self.queue_num) * 2);
        self.queue_driver = avail;
        self.queue_device = align_up(used_unaligned, u64::from(self.queue_align.max(1)));
    }
}
