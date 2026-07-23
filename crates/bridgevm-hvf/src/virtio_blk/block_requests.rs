//! The virtio-blk request path: header parse, bounds checks, chunked read, completion status.

use super::VIRTIO_BLK_S_IOERR;
use super::VIRTIO_BLK_S_OK;
use super::VIRTIO_BLK_T_IN;
use super::*;
use crate::fwcfg::GuestMemoryMut;

#[derive(Debug, Clone, Copy)]
pub(crate) struct RequestCompletion {
    pub(crate) written_len: u32,
}

impl VirtioMmioBlock {
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
}

impl RequestCompletion {
    pub(crate) fn status_only(mem: &mut dyn GuestMemoryMut, status_addr: u64, status: u8) -> Self {
        if status_addr != 0 {
            let _ = mem.write_bytes(status_addr, &[status]);
        }
        Self { written_len: 1 }
    }

    pub(crate) fn write_status(
        mem: &mut dyn GuestMemoryMut,
        status_desc: Option<&Descriptor>,
        status: u8,
        written_len: u32,
    ) -> Self {
        if let Some(desc) = status_desc {
            if desc.flags & DESC_F_WRITE != 0 && desc.len >= 1 {
                let _ = mem.write_bytes(desc.addr, &[status]);
            }
        }
        Self { written_len }
    }
}
