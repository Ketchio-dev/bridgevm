//! The virtio-blk request path: logical framing, bounds checks, chunked read, status.

use super::*;
use crate::fwcfg::GuestMemoryMut;
#[path = "request_layout.rs"]
mod request_layout;
use request_layout::RequestLayout;

const VIRTIO_BLK_T_OUT: u32 = 1;

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
            return RequestCompletion { written_len: 0 };
        }
        let Some(layout) = RequestLayout::parse(descs) else {
            return RequestCompletion { written_len: 0 };
        };
        let Some(header) = layout.header(mem, descs) else {
            return RequestCompletion::write_status(mem, &layout, VIRTIO_BLK_S_IOERR, 0);
        };
        let req_type = u32::from_le_bytes(header[0..4].try_into().unwrap());
        let sector = u64::from_le_bytes(header[8..16].try_into().unwrap());
        let data_len = u32::try_from(layout.payload_len(req_type)).unwrap_or(u32::MAX);
        self.stats.request_count = self.stats.request_count.saturating_add(1);
        self.stats.last_sector = Some(sector);
        if req_type != VIRTIO_BLK_T_IN {
            let status = if req_type == VIRTIO_BLK_T_OUT {
                self.stats.io_error_count = self.stats.io_error_count.saturating_add(1);
                VIRTIO_BLK_S_IOERR
            } else {
                self.stats.unsupported_count = self.stats.unsupported_count.saturating_add(1);
                VIRTIO_BLK_S_UNSUPP
            };
            self.stats.last_status = Some(status);
            self.record_request_trace(req_type, sector, data_len, status);
            return RequestCompletion::write_status(mem, &layout, status, 0);
        }
        self.stats.read_count = self.stats.read_count.saturating_add(1);
        let mut written_len = 0u32;
        let media_len = self.backend.capacity_sectors().saturating_mul(SECTOR_SIZE);
        let valid_offset = sector.checked_mul(SECTOR_SIZE).filter(|offset| {
            layout.readable_len == 16
                && layout.data_len % SECTOR_SIZE == 0
                && layout.data_len < u64::from(u32::MAX)
                && offset
                    .checked_add(layout.data_len)
                    .is_some_and(|end| end <= media_len)
        });
        let status = if let Some(mut byte_offset) = valid_offset {
            let mut status = VIRTIO_BLK_S_OK;
            'data: for (guest_addr, len) in layout.data(descs) {
                let mut copied = 0;
                while copied < len {
                    let chunk_len = (len - copied).min(READ_CHUNK_BYTES);
                    read_buf.resize(chunk_len, 0);
                    // Layout and media bounds already cover every accessed byte.
                    if self.backend.read_at_into(byte_offset, read_buf).is_err()
                        || !mem.write_bytes(guest_addr + copied as u64, read_buf.as_slice())
                    {
                        status = VIRTIO_BLK_S_IOERR;
                        break 'data;
                    }
                    byte_offset += chunk_len as u64;
                    copied += chunk_len;
                    written_len += chunk_len as u32;
                    self.stats.bytes_read = self.stats.bytes_read.saturating_add(chunk_len as u64);
                }
            }
            status
        } else {
            VIRTIO_BLK_S_IOERR
        };
        if status == VIRTIO_BLK_S_IOERR {
            self.stats.io_error_count = self.stats.io_error_count.saturating_add(1);
        }
        self.stats.last_len = written_len;
        self.stats.last_status = Some(status);
        self.record_request_trace(req_type, sector, data_len, status);
        RequestCompletion::write_status(mem, &layout, status, written_len)
    }
}

impl RequestCompletion {
    fn write_status(
        mem: &mut dyn GuestMemoryMut,
        layout: &RequestLayout,
        status: u8,
        written_len: u32,
    ) -> Self {
        let status_written = mem.write_bytes(layout.status_addr, &[status]);
        // Used length describes a written prefix, never an isolated status
        // after an unwritten data gap (VirtIO 1.2, section 2.7.8.2).
        Self {
            written_len: written_len
                + u32::from(status_written && u64::from(written_len) == layout.data_len),
        }
    }
}

#[cfg(test)]
#[path = "request_layout_tests.rs"]
mod request_layout_tests;
