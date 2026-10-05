//! WRITE DMA paths and the FUA/disabled-cache completion sync boundary.

use super::*;
use crate::fwcfg::GuestMemoryMut;

const WRITE_FUA_BIT: u32 = 1 << 30;

impl NvmeController {
    /// NVM WRITE (0x01). Same addressing as READ; copies guest data into disk.
    ///
    /// A successful write with FUA or disabled volatile cache synchronizes its
    /// namespace through the existing backend flush before posting completion.
    pub(crate) fn io_write(&mut self, cmd: &SubmissionEntry, mem: &mut dyn GuestMemoryMut) -> u16 {
        let Some(byte_len) = self.backend_for_nsid(cmd.nsid).map(DiskBackend::byte_len) else {
            return SC_INVALID_FIELD;
        };
        let Some((start, len)) = transfer_range(cmd, byte_len) else {
            return SC_INVALID_FIELD;
        };
        let mut spans = std::mem::take(&mut self.prp_spans_scratch);
        spans.clear();
        if !prp_spans_into(cmd, len, mem, &mut spans, &mut self.io_scratch) {
            self.prp_spans_scratch = spans;
            return SC_INVALID_FIELD;
        }
        let mut segments = std::mem::take(&mut self.io_segments_scratch);
        segments.clear();
        coalesce_spans_into(&spans, &mut segments);
        let direct_status = if self.direct_dma_enabled {
            self.io_write_direct(cmd.nsid, start, &segments, mem)
        } else {
            None
        };
        let status = match direct_status {
            Some(status) => status,
            None => self.io_write_buffered(cmd.nsid, start, len, &segments, mem),
        };
        spans.clear();
        segments.clear();
        self.prp_spans_scratch = spans;
        self.io_segments_scratch = segments;
        if status == SC_SUCCESS
            && (!self.volatile_write_cache_enabled || cmd.cdw12 & WRITE_FUA_BIT != 0)
        {
            return self.io_flush(cmd);
        }
        status
    }

    /// Zero-copy write fast path: `pwrite` guest RAM straight to the backing store
    /// through [`GuestMemoryMut::host_ptr`]. Returns `None` (fall back to buffered)
    /// when no host pointer is available, without having written anything.
    pub(crate) fn io_write_direct(
        &mut self,
        nsid: u32,
        start: u64,
        segments: &[(u64, usize)],
        mem: &mut dyn GuestMemoryMut,
    ) -> Option<u16> {
        let first = segments.first()?;
        mem.host_ptr(first.0, first.1)?;
        let mut disk_off = start;
        for &(gpa, seg_len) in segments {
            let Some(ptr) = mem.host_ptr(gpa, seg_len) else {
                return Some(SC_INVALID_FIELD);
            };
            // SAFETY: host_ptr validated [gpa, gpa+seg_len) lies inside the guest
            // RAM mapping; this is a read-only view for the disk write, and
            // process() holds the platform lock so the span is not mutated
            // concurrently.
            let src = unsafe { std::slice::from_raw_parts(ptr, seg_len) };
            let Some(backend) = self.backend_for_nsid_mut(nsid) else {
                return Some(SC_INVALID_FIELD);
            };
            if backend.write_at(disk_off, src).is_err() {
                return Some(SC_INVALID_FIELD);
            }
            disk_off += seg_len as u64;
        }
        Some(SC_SUCCESS)
    }

    /// Buffered write path: gather the contiguous guest segments into the reusable
    /// scratch buffer with `read_into`, then one `write_at` for the whole
    /// contiguous disk range. Used when direct DMA is unavailable.
    pub(crate) fn io_write_buffered(
        &mut self,
        nsid: u32,
        start: u64,
        len: usize,
        segments: &[(u64, usize)],
        mem: &mut dyn GuestMemoryMut,
    ) -> u16 {
        if len == 0 {
            return SC_SUCCESS;
        }
        let mut scratch = std::mem::take(&mut self.io_scratch);
        if scratch.len() < len {
            scratch.resize(len, 0);
        }
        let mut off = 0usize;
        let mut gathered = true;
        for &(gpa, seg_len) in segments {
            if !mem.read_into(gpa, &mut scratch[off..off + seg_len]) {
                gathered = false;
                break;
            }
            off += seg_len;
        }
        let status = if !gathered {
            SC_INVALID_FIELD
        } else if let Some(backend) = self.backend_for_nsid_mut(nsid) {
            if backend.write_at(start, &scratch[..len]).is_ok() {
                SC_SUCCESS
            } else {
                SC_INVALID_FIELD
            }
        } else {
            SC_INVALID_FIELD
        };
        self.io_scratch = scratch;
        status
    }
}
