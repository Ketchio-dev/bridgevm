//! The VirtioMmioBlock device model, stats, constructors, reset, and request-trace recording.

use super::VIRTIO_BLK_F_RO;
use super::*;
use std::io;
use std::path::Path;
use trace::RecentVirtioBlockRequests;

#[derive(Debug)]
pub struct VirtioMmioBlock {
    pub(crate) backend: RawFileBackend,
    pub(crate) stats: VirtioMmioBlockStats,
    pub(crate) transport: VirtioMmioTransport,
    pub(crate) device_features_sel: u32,
    pub(crate) driver_features_sel: u32,
    pub(crate) driver_features: [u32; 2],
    pub(crate) guest_page_size: u32,
    pub(crate) queue_sel: u32,
    pub(crate) queue_num: u16,
    pub(crate) queue_align: u32,
    pub(crate) queue_ready: bool,
    pub(crate) queue_desc: u64,
    pub(crate) queue_driver: u64,
    pub(crate) queue_device: u64,
    pub(crate) status: u32,
    pub(crate) interrupt_status: u32,
    pub(crate) last_avail_idx: u16,
    pub(crate) request_sequence: u64,
    pub(crate) request_trace: RecentVirtioBlockRequests,
    pub(crate) descriptor_scratch: Vec<Descriptor>,
    pub(crate) read_scratch: Vec<u8>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VirtioMmioBlockStats {
    pub transport_version: u32,
    pub notify_count: u64,
    pub request_count: u64,
    pub read_count: u64,
    pub unsupported_count: u64,
    pub io_error_count: u64,
    pub bytes_read: u64,
    pub last_sector: Option<u64>,
    pub last_len: u32,
    pub last_status: Option<u8>,
    pub queue_num: u16,
    pub queue_ready: bool,
    pub queue_desc: u64,
    pub queue_driver: u64,
    pub queue_device: u64,
    pub status: u32,
    pub driver_features: u64,
}

impl VirtioMmioBlock {
    pub fn open_read_only(path: impl AsRef<Path>) -> io::Result<Self> {
        Self::open_read_only_with_transport(path, VirtioMmioTransport::Legacy)
    }

    pub(crate) fn open_read_only_modern(path: impl AsRef<Path>) -> io::Result<Self> {
        Self::open_read_only_with_transport(path, VirtioMmioTransport::Modern)
    }

    pub(crate) fn open_read_only_with_transport(
        path: impl AsRef<Path>,
        transport: VirtioMmioTransport,
    ) -> io::Result<Self> {
        Ok(Self {
            backend: RawFileBackend::open(path)?,
            stats: VirtioMmioBlockStats::default(),
            transport,
            device_features_sel: 0,
            driver_features_sel: 0,
            driver_features: [0; 2],
            guest_page_size: 4096,
            queue_sel: 0,
            queue_num: 0,
            queue_align: 4096,
            queue_ready: false,
            queue_desc: 0,
            queue_driver: 0,
            queue_device: 0,
            status: 0,
            interrupt_status: 0,
            last_avail_idx: 0,
            request_sequence: 0,
            request_trace: RecentVirtioBlockRequests::default(),
            descriptor_scratch: Vec::new(),
            read_scratch: Vec::new(),
        })
    }

    pub fn len(&self) -> u64 {
        self.backend.len
    }

    pub fn is_empty(&self) -> bool {
        self.backend.len == 0
    }

    pub fn stats(&self) -> VirtioMmioBlockStats {
        let mut stats = self.stats;
        stats.transport_version = self.transport.version();
        stats.queue_num = self.queue_num;
        stats.queue_ready = self.queue_ready;
        stats.queue_desc = self.queue_desc;
        stats.queue_driver = self.queue_driver;
        stats.queue_device = self.queue_device;
        stats.status = self.status;
        stats.driver_features =
            u64::from(self.driver_features[0]) | (u64::from(self.driver_features[1]) << 32);
        stats
    }

    pub fn interrupt_line_level(&self) -> bool {
        self.interrupt_status != 0
    }

    pub fn recent_request_trace(&self) -> Vec<VirtioBlockRequestTrace> {
        self.request_trace.snapshot()
    }

    pub fn reset_runtime_state(&mut self) {
        self.stats = VirtioMmioBlockStats::default();
        self.device_features_sel = 0;
        self.driver_features_sel = 0;
        self.driver_features = [0; 2];
        self.guest_page_size = 4096;
        self.queue_sel = 0;
        self.queue_num = 0;
        self.queue_align = 4096;
        self.queue_ready = false;
        self.queue_desc = 0;
        self.queue_driver = 0;
        self.queue_device = 0;
        self.status = 0;
        self.interrupt_status = 0;
        self.last_avail_idx = 0;
        self.request_sequence = 0;
        self.request_trace = RecentVirtioBlockRequests::default();
        self.descriptor_scratch.clear();
        self.read_scratch.clear();
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
}
