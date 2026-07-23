//! Continuation of the `installer_iso_slot` impl block, split for the 1000-line rule.

use super::*;

use super::VIRTIO_BLK_F_RO;
use super::VIRTIO_BLK_S_IOERR;
use super::VIRTIO_BLK_S_OK;
use super::VIRTIO_BLK_T_IN;
use crate::fwcfg::GuestMemoryMut;
use trace::RecentVirtioBlockRequests;

impl VirtioMmioBlock {
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

    pub(crate) fn record_request_trace(
        &mut self,
        req_type: u32,
        sector: u64,
        data_len: u32,
        status: u8,
    ) {
        self.request_sequence = self.request_sequence.saturating_add(1);
        self.request_trace.record(VirtioBlockRequestTrace {
            sequence: self.request_sequence,
            request_type: req_type,
            sector,
            data_len,
            status,
        });
    }

    pub(crate) fn device_features(&self) -> u32 {
        if self.transport == VirtioMmioTransport::Legacy {
            return VIRTIO_BLK_F_RO | VIRTIO_BLK_F_BLK_SIZE;
        }
        match self.device_features_sel {
            0 => VIRTIO_BLK_F_RO | VIRTIO_BLK_F_BLK_SIZE,
            1 => VIRTIO_F_VERSION_1,
            _ => 0,
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

    pub(crate) fn config_read(&self, offset: u64, size: u8) -> u64 {
        let capacity = self.backend.capacity_sectors();
        let mut config = [0u8; 0x40];
        config[0..8].copy_from_slice(&capacity.to_le_bytes());
        config[0x14..0x18].copy_from_slice(&(SECTOR_SIZE as u32).to_le_bytes());
        read_le_from_bytes(&config, offset, size).unwrap_or(0)
    }

    pub(crate) fn process_queue(&mut self, mem: &mut dyn GuestMemoryMut) {
        if !self.queue_ready || self.queue_num == 0 || self.queue_desc == 0 {
            return;
        }
        let Some(avail_idx) = read_u16(mem, self.queue_driver + 2) else {
            return;
        };
        let mut descs = std::mem::take(&mut self.descriptor_scratch);
        let mut read_buf = std::mem::take(&mut self.read_scratch);
        while self.last_avail_idx != avail_idx {
            let ring_off = 4 + u64::from(self.last_avail_idx % self.queue_num) * 2;
            let Some(head) = read_u16(mem, self.queue_driver + ring_off) else {
                break;
            };
            let completion = self.process_descriptor_chain(mem, head, &mut descs, &mut read_buf);
            self.write_used(mem, head, completion.written_len);
            self.last_avail_idx = self.last_avail_idx.wrapping_add(1);
            self.interrupt_status |= 1;
        }
        descs.clear();
        read_buf.clear();
        self.descriptor_scratch = descs;
        self.read_scratch = read_buf;
    }

    pub(crate) fn process_descriptor_chain(
        &mut self,
        mem: &mut dyn GuestMemoryMut,
        head: u16,
        descs: &mut Vec<Descriptor>,
        read_buf: &mut Vec<u8>,
    ) -> RequestCompletion {
        if !Self::descriptor_chain_into(mem, self.queue_num, self.queue_desc, head, descs) {
            return RequestCompletion::status_only(mem, 0, VIRTIO_BLK_S_IOERR);
        }
        if descs.len() < 3 {
            return RequestCompletion::write_status(mem, descs.last(), VIRTIO_BLK_S_IOERR, 1);
        }
        let mut header = [0u8; 16];
        if !mem.read_into(descs[0].addr, &mut header) {
            return RequestCompletion::write_status(mem, descs.last(), VIRTIO_BLK_S_IOERR, 1);
        }
        let req_type = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
        let sector = u64::from_le_bytes([
            header[8], header[9], header[10], header[11], header[12], header[13], header[14],
            header[15],
        ]);
        let status = descs[descs.len() - 1];
        let data_descs = &descs[1..descs.len() - 1];
        let data_len_u64 = match data_descs
            .iter()
            .try_fold(0u64, |sum, desc| sum.checked_add(u64::from(desc.len)))
        {
            Some(len) => len,
            None => {
                return RequestCompletion::write_status(mem, Some(&status), VIRTIO_BLK_S_IOERR, 1);
            }
        };
        let data_len = u32::try_from(data_len_u64).unwrap_or(u32::MAX);
        self.stats.request_count = self.stats.request_count.saturating_add(1);
        self.stats.last_sector = Some(sector);

        if req_type != VIRTIO_BLK_T_IN {
            self.stats.unsupported_count = self.stats.unsupported_count.saturating_add(1);
            self.stats.last_status = Some(VIRTIO_BLK_S_UNSUPP);
            self.record_request_trace(req_type, sector, data_len, VIRTIO_BLK_S_UNSUPP);
            return RequestCompletion::write_status(mem, Some(&status), VIRTIO_BLK_S_UNSUPP, 1);
        }
        self.stats.read_count = self.stats.read_count.saturating_add(1);

        let mut byte_offset = match sector.checked_mul(SECTOR_SIZE) {
            Some(offset) => offset,
            None => {
                self.stats.io_error_count = self.stats.io_error_count.saturating_add(1);
                self.stats.last_status = Some(VIRTIO_BLK_S_IOERR);
                self.record_request_trace(req_type, sector, data_len, VIRTIO_BLK_S_IOERR);
                return RequestCompletion::write_status(mem, Some(&status), VIRTIO_BLK_S_IOERR, 1);
            }
        };
        let media_len = self.backend.capacity_sectors().saturating_mul(SECTOR_SIZE);
        if byte_offset
            .checked_add(data_len_u64)
            .map_or(true, |end| end > media_len)
        {
            self.stats.io_error_count = self.stats.io_error_count.saturating_add(1);
            self.stats.last_status = Some(VIRTIO_BLK_S_IOERR);
            self.record_request_trace(req_type, sector, data_len, VIRTIO_BLK_S_IOERR);
            return RequestCompletion::write_status(mem, Some(&status), VIRTIO_BLK_S_IOERR, 1);
        }
        let mut written_len = 0u32;
        for desc in data_descs {
            if desc.flags & DESC_F_WRITE == 0 {
                self.stats.io_error_count = self.stats.io_error_count.saturating_add(1);
                self.stats.last_status = Some(VIRTIO_BLK_S_IOERR);
                self.record_request_trace(req_type, sector, data_len, VIRTIO_BLK_S_IOERR);
                return RequestCompletion::write_status(mem, Some(&status), VIRTIO_BLK_S_IOERR, 1);
            }
            let mut remaining = desc.len as usize;
            let mut guest_addr = desc.addr;
            while remaining > 0 {
                let chunk_len = remaining.min(READ_CHUNK_BYTES);
                read_buf.resize(chunk_len, 0);
                if self.backend.read_at_into(byte_offset, read_buf).is_err()
                    || !mem.write_bytes(guest_addr, read_buf.as_slice())
                {
                    self.stats.io_error_count = self.stats.io_error_count.saturating_add(1);
                    self.stats.last_status = Some(VIRTIO_BLK_S_IOERR);
                    self.record_request_trace(req_type, sector, data_len, VIRTIO_BLK_S_IOERR);
                    return RequestCompletion::write_status(
                        mem,
                        Some(&status),
                        VIRTIO_BLK_S_IOERR,
                        1,
                    );
                }
                byte_offset = match byte_offset.checked_add(chunk_len as u64) {
                    Some(next) => next,
                    None => {
                        self.stats.io_error_count = self.stats.io_error_count.saturating_add(1);
                        self.stats.last_status = Some(VIRTIO_BLK_S_IOERR);
                        self.record_request_trace(req_type, sector, data_len, VIRTIO_BLK_S_IOERR);
                        return RequestCompletion::write_status(
                            mem,
                            Some(&status),
                            VIRTIO_BLK_S_IOERR,
                            1,
                        );
                    }
                };
                guest_addr = match guest_addr.checked_add(chunk_len as u64) {
                    Some(next) => next,
                    None => {
                        self.stats.io_error_count = self.stats.io_error_count.saturating_add(1);
                        self.stats.last_status = Some(VIRTIO_BLK_S_IOERR);
                        self.record_request_trace(req_type, sector, data_len, VIRTIO_BLK_S_IOERR);
                        return RequestCompletion::write_status(
                            mem,
                            Some(&status),
                            VIRTIO_BLK_S_IOERR,
                            1,
                        );
                    }
                };
                remaining -= chunk_len;
            }
            written_len = written_len.saturating_add(desc.len);
            self.stats.bytes_read = self.stats.bytes_read.saturating_add(u64::from(desc.len));
        }
        self.stats.last_len = written_len;
        self.stats.last_status = Some(VIRTIO_BLK_S_OK);
        self.record_request_trace(req_type, sector, data_len, VIRTIO_BLK_S_OK);
        RequestCompletion::write_status(
            mem,
            Some(&status),
            VIRTIO_BLK_S_OK,
            written_len.saturating_add(1),
        )
    }

    pub(crate) fn descriptor_chain_into(
        mem: &dyn GuestMemoryMut,
        queue_num: u16,
        queue_desc: u64,
        head: u16,
        out: &mut Vec<Descriptor>,
    ) -> bool {
        out.clear();
        if head >= queue_num {
            return false;
        }
        let mut index = head;
        for _ in 0..queue_num {
            let Some(desc) = Descriptor::read(mem, queue_desc + u64::from(index) * DESC_SIZE)
            else {
                out.clear();
                return false;
            };
            let has_next = desc.flags & DESC_F_NEXT != 0;
            out.push(desc);
            if !has_next {
                return true;
            }
            index = desc.next;
            if index >= queue_num {
                out.clear();
                return false;
            }
        }
        out.clear();
        false
    }

    pub(crate) fn write_used(&self, mem: &mut dyn GuestMemoryMut, id: u16, len: u32) {
        let Some(used_idx) = read_u16(mem, self.queue_device + 2) else {
            return;
        };
        let elem = self.queue_device + 4 + u64::from(used_idx % self.queue_num) * 8;
        let _ = mem.write_bytes(elem, &u32::from(id).to_le_bytes());
        let _ = mem.write_bytes(elem + 4, &len.to_le_bytes());
        let _ = mem.write_bytes(
            self.queue_device + 2,
            &used_idx.wrapping_add(1).to_le_bytes(),
        );
    }

    pub fn snapshot_state(&self) -> Vec<u8> {
        let mut out = crate::checkpoint::StateWriter::new();
        out.write_u32(1);
        out.write_u32(self.transport.version());
        out.write_u32(self.device_features_sel);
        out.write_u32(self.driver_features_sel);
        out.write_u32(self.driver_features[0]);
        out.write_u32(self.driver_features[1]);
        out.write_u32(self.guest_page_size);
        out.write_u32(self.queue_sel);
        out.write_u16(self.queue_num);
        out.write_u16(0);
        out.write_u32(self.queue_align);
        out.write_bool(self.queue_ready);
        out.write_u8(0);
        out.write_u16(0);
        out.write_u64(self.queue_desc);
        out.write_u64(self.queue_driver);
        out.write_u64(self.queue_device);
        out.write_u32(self.status);
        out.write_u32(self.interrupt_status);
        out.write_u16(self.last_avail_idx);
        out.write_u16(0);
        out.write_u64(self.request_sequence);
        out.into_inner()
    }

    pub fn restore_state(&mut self, data: &[u8]) {
        let mut input = crate::checkpoint::StateReader::new(data);
        assert_eq!(
            input.read_u32(),
            1,
            "unsupported virtio-block snapshot version"
        );
        assert_eq!(
            input.read_u32(),
            self.transport.version(),
            "virtio-block transport mismatch on restore"
        );

        self.device_features_sel = input.read_u32();
        self.driver_features_sel = input.read_u32();
        self.driver_features = [input.read_u32(), input.read_u32()];
        self.guest_page_size = input.read_u32();
        self.queue_sel = input.read_u32();
        self.queue_num = input.read_u16();
        assert_eq!(input.read_u16(), 0, "invalid virtio-block snapshot");
        self.queue_align = input.read_u32();
        self.queue_ready = input.read_bool();
        assert_eq!(input.read_u8(), 0, "invalid virtio-block snapshot");
        assert_eq!(input.read_u16(), 0, "invalid virtio-block snapshot");
        self.queue_desc = input.read_u64();
        self.queue_driver = input.read_u64();
        self.queue_device = input.read_u64();
        self.status = input.read_u32();
        self.interrupt_status = input.read_u32();
        self.last_avail_idx = input.read_u16();
        assert_eq!(input.read_u16(), 0, "invalid virtio-block snapshot");
        self.request_sequence = input.read_u64();

        self.request_trace = RecentVirtioBlockRequests::default();
        self.descriptor_scratch.clear();
        self.read_scratch.clear();
        input.finish();
    }
}
