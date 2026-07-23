//! The VirtioConsole device model, per-queue and per-port state, stats, construction and reset.

use super::*;
use std::collections::VecDeque;

#[derive(Debug)]
pub struct VirtioConsole {
    pub(crate) device_features_sel: u32,
    pub(crate) driver_features_sel: u32,
    pub(crate) driver_features: [u32; 2],
    pub(crate) config_msix_vector: u16,
    pub(crate) queue_sel: u32,
    pub(crate) queues: [VirtioConsoleQueue; QUEUE_COUNT],
    pub(crate) pending_msix_queue_bits: u8,
    pub(crate) status: u32,
    pub(crate) interrupt_status: u32,
    pub(crate) emerg_wr: u32,
    pub(crate) ports: [PortState; PORT_COUNT],
    pub(crate) pending_control: VecDeque<PendingControlMessage>,
    pub(crate) host_to_guest: VecDeque<u8>,
    pub(crate) host_inbound: Vec<u8>,
    pub(crate) descriptor_scratch: Vec<Descriptor>,
    pub(crate) read_scratch: Vec<u8>,
    // Set once the guest driver has actually pushed bytes on the agent TX
    // queue. Guest TX only flows after vioser latches HostConnected=TRUE (its
    // WillWriteBlock gate), so the first inbound byte is our only host-side
    // proof that our PORT_OPEN(host) took effect. Until then we keep
    // re-asserting PORT_OPEN; afterwards we stop (see maybe_reassert_host_open).
    pub(crate) agent_connected_confirmed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PortState {
    pub(crate) ready: bool,
    pub(crate) guest_open: bool,
    pub(crate) host_open: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VirtioConsoleQueue {
    pub(crate) size: u16,
    pub(crate) ready: bool,
    pub(crate) desc: u64,
    pub(crate) driver: u64,
    pub(crate) device: u64,
    pub(crate) msix_vector: u16,
    pub(crate) notify_off: u16,
    pub(crate) last_avail_idx: u16,
    pub(crate) pending_msix: bool,
    // Diagnostics (never reset by the queue's own reset; cleared only on a full
    // device reset). notify_count = guest doorbells; last_avail_seen = the most
    // recent avail->idx we read; used_produced = used entries we published;
    // rx_no_buffers = delivery attempts that found no fresh avail buffer. If the
    // guest keeps kicking (notify_count climbs) and posting (last_avail_seen
    // climbs) but delivery stalls, the bug is our consume path; if last_avail
    // stops climbing, the guest stopped replenishing.
    pub(crate) notify_count: u64,
    pub(crate) last_avail_seen: u16,
    pub(crate) used_produced: u64,
    pub(crate) rx_no_buffers: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VirtioConsoleQueueStats {
    pub size: u16,
    pub ready: bool,
    pub desc: u64,
    pub driver: u64,
    pub device: u64,
    pub msix_vector: u16,
    pub notify_off: u16,
    pub last_avail_idx: u16,
    pub pending_msix: bool,
    pub notify_count: u64,
    pub last_avail_seen: u16,
    pub used_produced: u64,
    pub rx_no_buffers: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtioConsoleStats {
    pub status: u32,
    pub interrupt_status: u32,
    pub driver_features: u64,
    pub port1_ready: bool,
    pub port1_guest_open: bool,
    pub port1_host_open: bool,
    pub agent_connected_confirmed: bool,
    pub pending_control: usize,
    pub host_to_guest_len: usize,
    pub host_inbound_len: usize,
    pub queues: [VirtioConsoleQueueStats; QUEUE_COUNT],
}

impl PortState {
    pub(crate) const fn new() -> Self {
        Self {
            ready: false,
            guest_open: false,
            host_open: false,
        }
    }
}

impl VirtioConsoleQueue {
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
            notify_count: 0,
            last_avail_seen: 0,
            used_produced: 0,
            rx_no_buffers: 0,
        }
    }

    pub(crate) fn reset(&mut self) {
        let notify_off = self.notify_off;
        *self = Self::new(notify_off);
    }
}

impl Default for VirtioConsoleStats {
    fn default() -> Self {
        Self {
            status: 0,
            interrupt_status: 0,
            driver_features: 0,
            port1_ready: false,
            port1_guest_open: false,
            port1_host_open: false,
            agent_connected_confirmed: false,
            pending_control: 0,
            host_to_guest_len: 0,
            host_inbound_len: 0,
            queues: [VirtioConsoleQueueStats::default(); QUEUE_COUNT],
        }
    }
}

impl VirtioConsole {
    pub fn new() -> Self {
        Self {
            device_features_sel: 0,
            driver_features_sel: 0,
            driver_features: [0; 2],
            config_msix_vector: VIRTIO_MSI_NO_VECTOR,
            queue_sel: 0,
            queues: [
                VirtioConsoleQueue::new(0),
                VirtioConsoleQueue::new(1),
                VirtioConsoleQueue::new(2),
                VirtioConsoleQueue::new(3),
                VirtioConsoleQueue::new(4),
                VirtioConsoleQueue::new(5),
            ],
            pending_msix_queue_bits: 0,
            status: 0,
            interrupt_status: 0,
            emerg_wr: 0,
            ports: [PortState::new(), PortState::new()],
            pending_control: VecDeque::new(),
            host_to_guest: VecDeque::new(),
            host_inbound: Vec::new(),
            descriptor_scratch: Vec::new(),
            read_scratch: Vec::new(),
            agent_connected_confirmed: false,
        }
    }

    pub fn stats(&self) -> VirtioConsoleStats {
        let mut stats = VirtioConsoleStats {
            status: self.status,
            interrupt_status: self.interrupt_status,
            driver_features: u64::from(self.driver_features[0])
                | (u64::from(self.driver_features[1]) << 32),
            port1_ready: self.ports[1].ready,
            port1_guest_open: self.ports[1].guest_open,
            port1_host_open: self.ports[1].host_open,
            agent_connected_confirmed: self.agent_connected_confirmed,
            pending_control: self.pending_control.len(),
            host_to_guest_len: self.host_to_guest.len(),
            host_inbound_len: self.host_inbound.len(),
            ..VirtioConsoleStats::default()
        };
        for (out, queue) in stats.queues.iter_mut().zip(self.queues) {
            *out = VirtioConsoleQueueStats {
                size: queue.size,
                ready: queue.ready,
                desc: queue.desc,
                driver: queue.driver,
                device: queue.device,
                msix_vector: queue.msix_vector,
                notify_off: queue.notify_off,
                last_avail_idx: queue.last_avail_idx,
                pending_msix: queue.pending_msix,
                notify_count: queue.notify_count,
                last_avail_seen: queue.last_avail_seen,
                used_produced: queue.used_produced,
                rx_no_buffers: queue.rx_no_buffers,
            };
        }
        stats
    }

    pub fn interrupt_line_level(&self) -> bool {
        self.interrupt_status != 0
    }

    pub fn reset_runtime_state(&mut self) {
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
        self.emerg_wr = 0;
        self.ports = [PortState::new(), PortState::new()];
        self.pending_control.clear();
        self.host_to_guest.clear();
        self.host_inbound.clear();
        self.descriptor_scratch.clear();
        self.read_scratch.clear();
        self.agent_connected_confirmed = false;
    }
}

impl Default for VirtioConsole {
    fn default() -> Self {
        Self::new()
    }
}
