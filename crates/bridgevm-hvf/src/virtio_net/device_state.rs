//! The VirtioNet device model, per-queue state, stats, construction and reset.

use super::*;

#[derive(Debug)]
pub struct VirtioNet<B: NetBackend> {
    pub(crate) backend: B,
    pub(crate) stats: VirtioNetStats,
    pub(crate) mac: [u8; 6],
    pub(crate) device_features_sel: u32,
    pub(crate) driver_features_sel: u32,
    pub(crate) driver_features: [u32; 2],
    pub(crate) config_msix_vector: u16,
    pub(crate) queue_sel: u32,
    pub(crate) queues: [VirtioNetQueue; QUEUE_COUNT],
    pub(crate) pending_msix_queue_bits: u8,
    pub(crate) status: u32,
    pub(crate) interrupt_status: u32,
    pub(crate) pending_rx_frame: Option<Vec<u8>>,
    pub(crate) descriptor_scratch: Vec<Descriptor>,
    pub(crate) tx_packet_scratch: Vec<u8>,
    pub(crate) rx_frame_scratch: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VirtioNetQueue {
    pub(crate) size: u16,
    pub(crate) ready: bool,
    pub(crate) desc: u64,
    pub(crate) driver: u64,
    pub(crate) device: u64,
    pub(crate) msix_vector: u16,
    pub(crate) notify_off: u16,
    pub(crate) last_avail_idx: u16,
    pub(crate) pending_msix: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VirtioNetQueueStats {
    pub size: u16,
    pub ready: bool,
    pub desc: u64,
    pub driver: u64,
    pub device: u64,
    pub msix_vector: u16,
    pub notify_off: u16,
    pub last_avail_idx: u16,
    pub pending_msix: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VirtioNetStats {
    pub notify_count: u64,
    pub tx_count: u64,
    pub rx_count: u64,
    pub tx_bytes: u64,
    pub rx_bytes: u64,
    pub status: u32,
    pub interrupt_status: u32,
    pub driver_features: u64,
    pub pending_rx_frame: bool,
    pub queues: [VirtioNetQueueStats; QUEUE_COUNT],
}

impl VirtioNetQueue {
    pub(crate) const fn new(notify_off: u16) -> Self {
        Self {
            size: 0,
            ready: false,
            desc: 0,
            driver: 0,
            device: 0,
            msix_vector: VIRTIO_MSI_NO_VECTOR,
            notify_off,
            last_avail_idx: 0,
            pending_msix: false,
        }
    }

    pub(crate) fn reset(&mut self) {
        let notify_off = self.notify_off;
        *self = Self::new(notify_off);
    }
}

impl<B: NetBackend> VirtioNet<B> {
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            stats: VirtioNetStats::default(),
            mac: DEFAULT_MAC,
            device_features_sel: 0,
            driver_features_sel: 0,
            driver_features: [0; 2],
            config_msix_vector: VIRTIO_MSI_NO_VECTOR,
            queue_sel: 0,
            queues: [VirtioNetQueue::new(0), VirtioNetQueue::new(1)],
            pending_msix_queue_bits: 0,
            status: 0,
            interrupt_status: 0,
            pending_rx_frame: None,
            descriptor_scratch: Vec::new(),
            tx_packet_scratch: Vec::new(),
            rx_frame_scratch: Vec::new(),
        }
    }

    pub fn backend(&self) -> &B {
        &self.backend
    }

    pub fn backend_mut(&mut self) -> &mut B {
        &mut self.backend
    }

    pub fn stats(&self) -> VirtioNetStats {
        let mut stats = self.stats;
        stats.status = self.status;
        stats.interrupt_status = self.interrupt_status;
        stats.driver_features =
            u64::from(self.driver_features[0]) | (u64::from(self.driver_features[1]) << 32);
        stats.pending_rx_frame = self.pending_rx_frame.is_some();
        for (out, queue) in stats.queues.iter_mut().zip(self.queues) {
            *out = VirtioNetQueueStats {
                size: queue.size,
                ready: queue.ready,
                desc: queue.desc,
                driver: queue.driver,
                device: queue.device,
                msix_vector: queue.msix_vector,
                notify_off: queue.notify_off,
                last_avail_idx: queue.last_avail_idx,
                pending_msix: queue.pending_msix,
            };
        }
        stats
    }

    pub fn interrupt_line_level(&self) -> bool {
        self.interrupt_status != 0
    }

    pub fn reset_runtime_state(&mut self) {
        self.stats = VirtioNetStats::default();
        self.device_features_sel = 0;
        self.driver_features_sel = 0;
        self.driver_features = [0; 2];
        self.config_msix_vector = VIRTIO_MSI_NO_VECTOR;
        self.queue_sel = 0;
        for queue in &mut self.queues {
            queue.reset();
        }
        self.pending_msix_queue_bits = 0;
        self.status = 0;
        self.interrupt_status = 0;
        self.pending_rx_frame = None;
        self.descriptor_scratch.clear();
        self.tx_packet_scratch.clear();
        self.rx_frame_scratch.clear();
    }
}
