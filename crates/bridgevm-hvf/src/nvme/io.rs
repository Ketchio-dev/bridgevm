//! NVM I/O command execution (FLUSH/READ/WRITE) over the direct-DMA and buffered data paths.

use super::*;
use crate::fwcfg::GuestMemoryMut;

impl NvmeController {
    /// Execute an NVM I/O command against the disk backend.
    pub(crate) fn execute_io(
        &mut self,
        cmd: &SubmissionEntry,
        mem: &mut dyn GuestMemoryMut,
    ) -> CommandResult {
        let status = match cmd.opcode {
            NVM_OP_FLUSH => self.io_flush(cmd),
            NVM_OP_READ => self.io_read(cmd, mem),
            NVM_OP_WRITE => self.io_write(cmd, mem),
            _ => SC_INVALID_OPCODE,
        };
        CommandResult::complete(status)
    }

    /// NVM FLUSH (0x00). QEMU accepts both NSID 1 and broadcast NSID for a
    /// single-NVM-namespace controller. Memory-backed media is already coherent;
    /// write-through host-file media issues a durable data sync.
    pub(crate) fn io_flush(&mut self, cmd: &SubmissionEntry) -> u16 {
        // Broadcast NSID flushes every active namespace.
        if cmd.nsid == u32::MAX {
            let mut ok = self.disk.flush().is_ok();
            if let Some(disk2) = self.disk2.as_mut() {
                ok &= disk2.flush().is_ok();
            }
            return if ok {
                SC_SUCCESS
            } else {
                SC_INTERNAL_DEVICE_ERROR
            };
        }
        let Some(backend) = self.backend_for_nsid_mut(cmd.nsid) else {
            return SC_INVALID_FIELD;
        };
        match backend.flush() {
            Ok(()) => SC_SUCCESS,
            Err(_) => SC_INTERNAL_DEVICE_ERROR,
        }
    }

    /// NVM READ (0x02). SLBA in CDW10/11 (64-bit), NLB in CDW12 bits 15:0
    /// (0-based). Data is scattered through PRP1/PRP2 or a PRP list.
    ///
    /// The transfer is decoded once into physically-contiguous guest segments,
    /// then either DMA'd straight from the backing store into guest RAM (when the
    /// memory view exposes stable host pointers) or staged through the reusable
    /// scratch buffer. Both paths issue at most one disk read per contiguous guest
    /// segment — commonly one for the whole command — where the old path did one
    /// allocation + `pread` per 4 KiB PRP page.
    pub(crate) fn io_read(&mut self, cmd: &SubmissionEntry, mem: &mut dyn GuestMemoryMut) -> u16 {
        let Some(byte_len) = self.backend_for_nsid(cmd.nsid).map(DiskBackend::byte_len) else {
            return SC_INVALID_FIELD;
        };
        let Some((start, len)) = transfer_range(cmd, byte_len) else {
            return SC_INVALID_FIELD;
        };
        let mut spans = std::mem::take(&mut self.prp_spans_scratch);
        spans.clear();
        if !prp_spans_into(cmd, len, mem, &mut spans) {
            self.prp_spans_scratch = spans;
            return SC_INVALID_FIELD;
        }
        let mut segments = std::mem::take(&mut self.io_segments_scratch);
        segments.clear();
        coalesce_spans_into(&spans, &mut segments);
        let direct_status = if self.direct_dma_enabled {
            self.io_read_direct(cmd.nsid, start, &segments, mem)
        } else {
            None
        };
        let status = match direct_status {
            Some(status) => status,
            None => self.io_read_buffered(cmd.nsid, start, len, &segments, mem),
        };
        spans.clear();
        segments.clear();
        self.prp_spans_scratch = spans;
        self.io_segments_scratch = segments;
        status
    }

    /// Zero-copy read fast path: `pread` the backing store straight into guest
    /// RAM through [`GuestMemoryMut::host_ptr`]. Returns `None` (so the caller
    /// falls back to the buffered path) when the memory view exposes no host
    /// pointer, without having touched guest RAM.
    pub(crate) fn io_read_direct(
        &mut self,
        nsid: u32,
        start: u64,
        segments: &[(u64, usize)],
        mem: &mut dyn GuestMemoryMut,
    ) -> Option<u16> {
        let first = segments.first()?;
        // Probe before writing anything: an all-or-nothing host_ptr view either
        // resolves the first span or the whole memory lacks direct pointers.
        mem.host_ptr(first.0, first.1)?;
        let mut disk_off = start;
        for &(gpa, seg_len) in segments {
            let Some(ptr) = mem.host_ptr(gpa, seg_len) else {
                return Some(SC_INVALID_FIELD);
            };
            // SAFETY: host_ptr validated [gpa, gpa+seg_len) lies inside the guest
            // RAM mapping. process() runs under the platform lock, so no vCPU
            // accesses this span concurrently, and segments are processed one at a
            // time so at most one mutable view is live.
            let dst = unsafe { std::slice::from_raw_parts_mut(ptr, seg_len) };
            let Some(backend) = self.backend_for_nsid_mut(nsid) else {
                return Some(SC_INVALID_FIELD);
            };
            if backend.read_at_into(disk_off, dst).is_err() {
                return Some(SC_INVALID_FIELD);
            }
            disk_off += seg_len as u64;
        }
        Some(SC_SUCCESS)
    }

    /// Buffered read path: one coalesced `read_at_into` per contiguous guest
    /// segment through the reusable scratch buffer, then scattered into guest RAM
    /// with `write_bytes`. Used when direct DMA is unavailable (unit tests, any
    /// memory view without `host_ptr`).
    pub(crate) fn io_read_buffered(
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
        let status = if let Some(backend) = self.backend_for_nsid_mut(nsid) {
            if backend.read_at_into(start, &mut scratch[..len]).is_err() {
                SC_INVALID_FIELD
            } else {
                let mut off = 0usize;
                let mut ok = true;
                for &(gpa, seg_len) in segments {
                    if !mem.write_bytes(gpa, &scratch[off..off + seg_len]) {
                        ok = false;
                        break;
                    }
                    off += seg_len;
                }
                if ok {
                    SC_SUCCESS
                } else {
                    SC_INVALID_FIELD
                }
            }
        } else {
            SC_INVALID_FIELD
        };
        self.io_scratch = scratch;
        status
    }

    /// NVM WRITE (0x01). Same addressing as READ; copies guest data into disk.
    ///
    /// Synchronous write-back is preserved exactly: in write-through mode each
    /// segment's `write_at` lands in the host file before the command completes.
    pub(crate) fn io_write(&mut self, cmd: &SubmissionEntry, mem: &mut dyn GuestMemoryMut) -> u16 {
        let Some(byte_len) = self.backend_for_nsid(cmd.nsid).map(DiskBackend::byte_len) else {
            return SC_INVALID_FIELD;
        };
        let Some((start, len)) = transfer_range(cmd, byte_len) else {
            return SC_INVALID_FIELD;
        };
        let mut spans = std::mem::take(&mut self.prp_spans_scratch);
        spans.clear();
        if !prp_spans_into(cmd, len, mem, &mut spans) {
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
