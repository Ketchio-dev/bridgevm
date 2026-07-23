//! Continuation of the `sq_entry_size` impl block, split for the 1000-line rule.

use super::*;

use crate::fwcfg::GuestMemoryMut;
use crate::msix::MsixMessage;
use crate::pcie::NVME_MSIX_PBA_OFFSET;
use crate::pcie::NVME_MSIX_TABLE_OFFSET;

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

    /// Post a 16-byte completion-queue entry for `cmd` into completion queue
    /// `cqid`, advancing its tail and toggling the phase tag on wrap.
    pub(crate) fn post_completion(
        &mut self,
        cqid: u16,
        sqid: u16,
        cmd: &SubmissionEntry,
        status: u16,
        mem: &mut dyn GuestMemoryMut,
    ) -> (bool, Option<NvmeCompletionEvent>) {
        let dw0 = std::mem::take(&mut self.last_feature_result);
        let (base, tail, size, phase, sq_head, interrupt_vector, interrupts_enabled) =
            match self.cqs.get(cqid as usize) {
                Some(Some(cq)) => {
                    let sq_head = match self.sqs.get(sqid as usize) {
                        Some(Some(sq)) => sq.head,
                        _ => 0,
                    };
                    (
                        cq.base,
                        cq.tail,
                        cq.size,
                        cq.phase,
                        sq_head,
                        cq.interrupt_vector,
                        cq.interrupts_enabled,
                    )
                }
                _ => return (false, None),
            };

        // Status field (CDW3 bits 31:17 = status code, bit 16 = phase tag).
        let phase_bit = u16::from(phase);
        let status_field = (status << 1) | phase_bit;

        let mut entry = [0u8; 16];
        entry[0..4].copy_from_slice(&dw0.to_le_bytes()); // command-specific DW0
                                                         // DW1 reserved (4..8).
        entry[8..10].copy_from_slice(&sq_head.to_le_bytes()); // SQ head pointer
        entry[10..12].copy_from_slice(&sqid.to_le_bytes()); // SQ identifier
        entry[12..14].copy_from_slice(&cmd.command_id.to_le_bytes());
        entry[14..16].copy_from_slice(&status_field.to_le_bytes());

        let entry_gpa = base + u64::from(tail) * CQ_ENTRY_SIZE;
        if !mem.write_bytes(entry_gpa, &entry) {
            return (false, None);
        }

        // Advance the CQ tail, toggling the phase tag when it wraps.
        let new_tail = (tail + 1) % size;
        if let Some(Some(cq)) = self.cqs.get_mut(cqid as usize) {
            cq.tail = new_tail;
            if new_tail == 0 {
                cq.phase = !cq.phase;
            }
        }
        (
            true,
            interrupts_enabled.then_some(NvmeCompletionEvent {
                cqid,
                vector: interrupt_vector,
            }),
        )
    }

    pub fn raise_msix(
        &mut self,
        vector: u16,
        function_enabled: bool,
        function_masked: bool,
    ) -> Option<MsixMessage> {
        self.msix.raise(vector, function_enabled, function_masked)
    }

    pub fn drain_pending_msix(
        &mut self,
        function_enabled: bool,
        function_masked: bool,
    ) -> Vec<MsixMessage> {
        self.msix.drain_pending(function_enabled, function_masked)
    }

    pub fn drain_pending_msix_into(
        &mut self,
        function_enabled: bool,
        function_masked: bool,
        out: &mut Vec<MsixMessage>,
    ) {
        self.msix
            .drain_pending_into(function_enabled, function_masked, out);
    }

    pub(crate) fn msix_table_offset(&self, offset: u64) -> Option<u64> {
        let base = u64::from(NVME_MSIX_TABLE_OFFSET);
        let rel = offset.checked_sub(base)?;
        (rel < self.msix.table_byte_len()).then_some(rel)
    }

    pub(crate) fn msix_pba_offset(&self, offset: u64) -> Option<u64> {
        let base = u64::from(NVME_MSIX_PBA_OFFSET);
        let rel = offset.checked_sub(base)?;
        (rel < self.msix.pba_byte_len()).then_some(rel)
    }

    pub fn snapshot_state(&self) -> Vec<u8> {
        let mut out = crate::checkpoint::StateWriter::new();
        out.write_u32(1);
        out.write_u32(self.cc);
        out.write_u32(self.csts);
        out.write_u32(self.aqa);
        out.write_u64(self.asq);
        out.write_u64(self.acq);
        out.write_u32(self.intms);
        out.write_u16(self.max_io_queues);
        out.write_u16(0);
        out.write_u32(self.last_feature_result);
        out.write_bool(self.volatile_write_cache_enabled);
        out.write_bool(self.direct_dma_enabled);
        out.write_u8(self.pending_async_event_requests);
        out.write_u8(0);

        out.write_u32(self.sqs.len() as u32);
        for queue in &self.sqs {
            out.write_bool(queue.is_some());
            if let Some(queue) = queue {
                out.write_u64(queue.base);
                out.write_u16(queue.size);
                out.write_u16(queue.head);
                out.write_u16(queue.tail_doorbell);
                out.write_u16(queue.cqid);
            }
        }

        out.write_u32(self.cqs.len() as u32);
        for queue in &self.cqs {
            out.write_bool(queue.is_some());
            if let Some(queue) = queue {
                out.write_u64(queue.base);
                out.write_u16(queue.size);
                out.write_u16(queue.tail);
                out.write_bool(queue.phase);
                out.write_bool(queue.interrupts_enabled);
                out.write_u16(queue.head);
                out.write_u16(queue.interrupt_vector);
            }
        }

        out.write_u32(self.pending_sq_bits.len() as u32);
        for word in &self.pending_sq_bits {
            out.write_u64(*word);
        }

        out.write_blob(&self.msix.snapshot_state());
        out.into_inner()
    }

    pub fn restore_state(&mut self, data: &[u8]) {
        let mut input = crate::checkpoint::StateReader::new(data);
        assert_eq!(input.read_u32(), 1, "unsupported NVMe snapshot version");

        self.cc = input.read_u32();
        self.csts = input.read_u32();
        self.aqa = input.read_u32();
        self.asq = input.read_u64();
        self.acq = input.read_u64();
        self.intms = input.read_u32();
        self.max_io_queues = input.read_u16();
        assert_eq!(input.read_u16(), 0, "invalid NVMe snapshot");
        self.last_feature_result = input.read_u32();
        self.volatile_write_cache_enabled = input.read_bool();
        self.direct_dma_enabled = input.read_bool();
        self.pending_async_event_requests = input.read_u8();
        assert_eq!(input.read_u8(), 0, "invalid NVMe snapshot");

        let sq_count = input.read_u32() as usize;
        assert!(
            sq_count <= MAX_IO_QUEUE_PAIRS as usize + 1,
            "NVMe SQ count exceeds controller capacity"
        );
        self.sqs.clear();
        self.sqs.reserve(sq_count);
        for _ in 0..sq_count {
            self.sqs.push(if input.read_bool() {
                Some(SubmissionQueue {
                    base: input.read_u64(),
                    size: input.read_u16(),
                    head: input.read_u16(),
                    tail_doorbell: input.read_u16(),
                    cqid: input.read_u16(),
                })
            } else {
                None
            });
        }

        let cq_count = input.read_u32() as usize;
        assert!(
            cq_count <= MAX_IO_QUEUE_PAIRS as usize + 1,
            "NVMe CQ count exceeds controller capacity"
        );
        self.cqs.clear();
        self.cqs.reserve(cq_count);
        for _ in 0..cq_count {
            self.cqs.push(if input.read_bool() {
                Some(CompletionQueue {
                    base: input.read_u64(),
                    size: input.read_u16(),
                    tail: input.read_u16(),
                    phase: input.read_bool(),
                    interrupts_enabled: input.read_bool(),
                    head: input.read_u16(),
                    interrupt_vector: input.read_u16(),
                })
            } else {
                None
            });
        }

        let pending_words = input.read_u32() as usize;
        self.pending_sq_bits.clear();
        self.pending_sq_bits.reserve(pending_words);
        for _ in 0..pending_words {
            self.pending_sq_bits.push(input.read_u64());
        }

        self.msix.restore_state(&input.read_blob());

        self.command_trace.clear();
        self.io_scratch.clear();
        self.prp_spans_scratch.clear();
        self.io_segments_scratch.clear();
        input.finish();
    }
}
