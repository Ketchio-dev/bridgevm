//! The NvmeController struct, construction, disk (re)load, reset, and data-path configuration.

use super::*;
use crate::msix::MsixTable;
use crate::pcie::NVME_MSIX_VECTOR_COUNT;
use std::collections::VecDeque;
use std::io;
use std::path::Path;

/// A modelled minimal NVMe controller.
#[derive(Debug)]
pub struct NvmeController {
    // --- BAR0 register backing store ---
    pub(crate) cc: u32,
    pub(crate) csts: u32,
    pub(crate) aqa: u32,
    pub(crate) asq: u64,
    pub(crate) acq: u64,
    pub(crate) intms: u32,

    // --- Queues. Index 0 is the admin queue; 1.. are I/O queues. ---
    pub(crate) sqs: Vec<Option<SubmissionQueue>>,
    pub(crate) cqs: Vec<Option<CompletionQueue>>,
    pub(crate) pending_sq_bits: Vec<u64>,

    // --- Backend ---
    /// Raw disk backing store for NSID 1, `LBA_SIZE`-byte logical blocks.
    pub(crate) disk: DiskBackend,
    /// Optional NSID-2 backing store (blank Windows install target).
    pub(crate) disk2: Option<DiskBackend>,
    /// Negotiated maximum number of I/O queue pairs (SET FEATURES 0x07).
    pub(crate) max_io_queues: u16,
    /// Command-specific result for the *next* completion's DW0 (e.g. the queue
    /// count granted by SET FEATURES). Consumed when the completion is posted.
    pub(crate) last_feature_result: u32,
    /// Current volatile write cache state. QEMU's default NVMe endpoint
    /// advertises a present cache and boots with it enabled.
    pub(crate) volatile_write_cache_enabled: bool,
    /// BAR-backed MSI-X table and PBA for this endpoint.
    pub(crate) msix: MsixTable,
    /// Recent command/completion history for live Windows bring-up.
    pub(crate) command_trace: VecDeque<NvmeCommandTrace>,
    /// Reusable staging buffer for the data path's buffered fallback (used when
    /// the guest-memory view exposes no stable host pointer for direct DMA). Kept
    /// at its high-water mark across commands so a steady stream of IO reuses one
    /// allocation instead of allocating per PRP page. Holds no state between
    /// commands; each command fully overwrites the prefix it reads.
    pub(crate) io_scratch: Vec<u8>,
    /// Whether the zero-copy host-pointer data path is allowed. The default is
    /// enabled; `BRIDGEVM_NVME_BUFFERED_IO=1` forces the byte-identical buffered
    /// path for one-process storage-integrity A/B diagnostics.
    pub(crate) direct_dma_enabled: bool,
    /// Reusable PRP decode output for NVM READ/WRITE. The hot I/O path fills this
    /// per command instead of allocating a span vector for every transfer.
    pub(crate) prp_spans_scratch: Vec<(u64, usize)>,
    /// Reusable physically-contiguous guest segments derived from PRP spans.
    pub(crate) io_segments_scratch: Vec<(u64, usize)>,
    /// Outstanding AER commands that should complete only when an async event is
    /// raised. This minimal controller does not raise events yet, so it only
    /// tracks the advertised limit and leaves accepted requests pending.
    pub(crate) pending_async_event_requests: u8,
}

impl NvmeController {
    /// Create a controller with a `disk_bytes`-sized backing store. The size is
    /// rounded up to a whole number of `LBA_SIZE` blocks.
    pub fn new(disk_bytes: usize) -> Self {
        Self::with_disk_backend(DiskBackend::memory(vec![0u8; rounded_disk_len(disk_bytes)]))
    }

    /// Create a controller from an existing raw disk image. The image is padded
    /// with zeros up to a whole number of `LBA_SIZE` blocks so namespace capacity
    /// and transfer bounds stay block-aligned.
    pub fn with_disk_image(disk: Vec<u8>) -> Self {
        Self::with_disk_backend(DiskBackend::memory(disk))
    }

    /// Create a controller backed by a host raw disk file. When `write_back` is
    /// false, guest writes are kept in a sparse in-memory overlay; when true,
    /// writes are applied directly to the host file.
    pub fn with_raw_file(path: impl AsRef<Path>, write_back: bool) -> io::Result<Self> {
        Ok(Self::with_disk_backend(DiskBackend::raw_file(
            path, write_back,
        )?))
    }

    pub(crate) fn with_disk_backend(disk: DiskBackend) -> Self {
        Self {
            cc: 0,
            csts: 0,
            aqa: 0,
            asq: 0,
            acq: 0,
            intms: 0,
            // Slot 0 reserved for the admin queue; grow lazily for I/O queues.
            sqs: vec![None],
            cqs: vec![None],
            pending_sq_bits: vec![0],
            disk,
            disk2: None,
            // Capacity for SET FEATURES (NUMBER OF QUEUES) to negotiate against.
            // The model only ever drives one I/O queue, but advertises a small
            // pool so a guest requesting several is granted a sane non-zero count.
            max_io_queues: MAX_IO_QUEUE_PAIRS,
            last_feature_result: 0,
            volatile_write_cache_enabled: true,
            msix: MsixTable::new(NVME_MSIX_VECTOR_COUNT),
            command_trace: VecDeque::with_capacity(COMMAND_TRACE_CAPACITY),
            io_scratch: Vec::new(),
            direct_dma_enabled: true,
            prp_spans_scratch: Vec::new(),
            io_segments_scratch: Vec::new(),
            pending_async_event_requests: 0,
        }
    }

    /// Replace the backing disk image, padding to a full LBA. This resets queue
    /// and controller register state, mirroring a cold-plugged different device.
    pub fn load_disk_image(&mut self, disk: Vec<u8>) {
        let direct_dma_enabled = self.direct_dma_enabled;
        *self = Self::with_disk_image(disk);
        self.direct_dma_enabled = direct_dma_enabled;
    }

    /// Replace the backing disk with a host raw file and reset controller state.
    pub fn load_raw_file(&mut self, path: impl AsRef<Path>, write_back: bool) -> io::Result<()> {
        let direct_dma_enabled = self.direct_dma_enabled;
        *self = Self::with_raw_file(path, write_back)?;
        self.direct_dma_enabled = direct_dma_enabled;
        Ok(())
    }

    pub fn reset_registers_keep_disks(&mut self) {
        let direct_dma_enabled = self.direct_dma_enabled;
        let disk = std::mem::replace(&mut self.disk, DiskBackend::memory(Vec::new()));
        let disk2 = self.disk2.take();
        *self = Self::with_disk_backend(disk);
        self.disk2 = disk2;
        self.direct_dma_enabled = direct_dma_enabled;
    }

    /// Select the NVMe data path without changing queue or media state. This is
    /// primarily useful for deterministic diagnostics and tests.
    pub fn set_direct_dma_enabled(&mut self, enabled: bool) {
        self.direct_dma_enabled = enabled;
    }

    pub fn direct_dma_enabled(&self) -> bool {
        self.direct_dma_enabled
    }
}
