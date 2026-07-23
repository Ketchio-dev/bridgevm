//! Split out of virtio_console.rs to keep files under 850 lines.

use super::*;
use crate::fwcfg::GuestMemoryMut;
use std::collections::VecDeque;

macro_rules! console_trace {
    ($($arg:tt)*) => {
        if console_trace_enabled() {
            eprintln!("[vcon] {}", format_args!($($arg)*));
        }
    };
}

pub(crate) const MAGIC_VALUE: u32 = 0x7472_6976;
pub(crate) const VERSION_MODERN: u32 = 2;
pub(crate) const DEVICE_ID_CONSOLE: u32 = 3;
pub(crate) const VENDOR_ID_QEMU: u32 = 0x554d_4551;

pub(crate) const REG_MAGIC: u64 = 0x000;
pub(crate) const REG_VERSION: u64 = 0x004;
pub(crate) const REG_DEVICE_ID: u64 = 0x008;
pub(crate) const REG_VENDOR_ID: u64 = 0x00c;
pub(crate) const REG_DEVICE_FEATURES: u64 = 0x010;
pub(crate) const REG_DEVICE_FEATURES_SEL: u64 = 0x014;
pub(crate) const REG_DRIVER_FEATURES: u64 = 0x020;
pub(crate) const REG_DRIVER_FEATURES_SEL: u64 = 0x024;
pub(crate) const REG_QUEUE_SEL: u64 = 0x030;
pub(crate) const REG_QUEUE_NUM_MAX: u64 = 0x034;
pub(crate) const REG_QUEUE_NUM: u64 = 0x038;
pub(crate) const REG_QUEUE_READY: u64 = 0x044;
pub(crate) const REG_QUEUE_NOTIFY: u64 = 0x050;
pub(crate) const REG_INTERRUPT_STATUS: u64 = 0x060;
pub(crate) const REG_INTERRUPT_ACK: u64 = 0x064;
pub(crate) const REG_STATUS: u64 = 0x070;
pub(crate) const REG_QUEUE_DESC_LOW: u64 = 0x080;
pub(crate) const REG_QUEUE_DESC_HIGH: u64 = 0x084;
pub(crate) const REG_QUEUE_DRIVER_LOW: u64 = 0x090;
pub(crate) const REG_QUEUE_DRIVER_HIGH: u64 = 0x094;
pub(crate) const REG_QUEUE_DEVICE_LOW: u64 = 0x0a0;
pub(crate) const REG_QUEUE_DEVICE_HIGH: u64 = 0x0a4;
pub(crate) const REG_CONFIG_GENERATION: u64 = 0x0fc;

pub(crate) const PCI_COMMON_CFG_OFFSET: u64 = 0x0000;
pub(crate) const PCI_ISR_CFG_OFFSET: u64 = 0x1000;
pub(crate) const PCI_DEVICE_CFG_OFFSET: u64 = 0x2000;
pub(crate) const PCI_NOTIFY_CFG_OFFSET: u64 = 0x3000;
pub(crate) const PCI_CFG_REGION_SIZE: u64 = 0x1000;

pub(crate) const COMMON_DEVICE_FEATURE_SELECT: u64 = 0x00;
pub(crate) const COMMON_DEVICE_FEATURE: u64 = 0x04;
pub(crate) const COMMON_DRIVER_FEATURE_SELECT: u64 = 0x08;
pub(crate) const COMMON_DRIVER_FEATURE: u64 = 0x0c;
pub(crate) const COMMON_CONFIG_MSIX_VECTOR: u64 = 0x10;
pub(crate) const COMMON_NUM_QUEUES: u64 = 0x12;
pub(crate) const COMMON_DEVICE_STATUS: u64 = 0x14;
pub(crate) const COMMON_CONFIG_GENERATION: u64 = 0x15;
pub(crate) const COMMON_QUEUE_SELECT: u64 = 0x16;
pub(crate) const COMMON_QUEUE_SIZE: u64 = 0x18;
pub(crate) const COMMON_QUEUE_MSIX_VECTOR: u64 = 0x1a;
pub(crate) const COMMON_QUEUE_ENABLE: u64 = 0x1c;
pub(crate) const COMMON_QUEUE_NOTIFY_OFF: u64 = 0x1e;
pub(crate) const COMMON_QUEUE_DESC: u64 = 0x20;
pub(crate) const COMMON_QUEUE_DRIVER: u64 = 0x28;
pub(crate) const COMMON_QUEUE_DEVICE: u64 = 0x30;

pub(crate) const VIRTIO_CONSOLE_F_MULTIPORT: u32 = 1 << 1;
pub(crate) const VIRTIO_F_VERSION_1: u32 = 1 << 0;
pub(crate) const VIRTIO_MSI_NO_VECTOR: u16 = 0xffff;

pub const QUEUE_PORT0_RX: usize = 0;
pub const QUEUE_PORT0_TX: usize = 1;
pub const QUEUE_CONTROL_RX: usize = 2;
pub const QUEUE_CONTROL_TX: usize = 3;
pub const QUEUE_AGENT_RX: usize = 4;
pub const QUEUE_AGENT_TX: usize = 5;
pub(crate) const QUEUE_COUNT: usize = 6;
pub(crate) const QUEUE_MAX: u16 = 64;
pub(crate) const DESC_SIZE: u64 = 16;
pub(crate) const DESC_F_NEXT: u16 = 1;
pub(crate) const DESC_F_WRITE: u16 = 2;

pub(crate) const PORT_COUNT: usize = 2;
pub(crate) const AGENT_PORT_ID: u32 = 1;
pub const AGENT_PORT_NAME: &[u8] = b"org.bridgevm.agent.0";

pub(crate) const VIRTIO_CONSOLE_DEVICE_READY: u16 = 0;
pub(crate) const VIRTIO_CONSOLE_DEVICE_ADD: u16 = 1;
pub(crate) const VIRTIO_CONSOLE_DEVICE_REMOVE: u16 = 2;
pub(crate) const VIRTIO_CONSOLE_PORT_READY: u16 = 3;
pub(crate) const VIRTIO_CONSOLE_CONSOLE_PORT: u16 = 4;
pub(crate) const VIRTIO_CONSOLE_RESIZE: u16 = 5;
pub(crate) const VIRTIO_CONSOLE_PORT_OPEN: u16 = 6;
pub(crate) const VIRTIO_CONSOLE_PORT_NAME: u16 = 7;
pub(crate) const CONTROL_LEN: usize = 8;
pub(crate) const MAX_CONTROL_MESSAGE_LEN: usize = CONTROL_LEN + AGENT_PORT_NAME.len();
// Agent replies are line-oriented. The largest current wire line is one
// base64-encoded 24 KiB file chunk, so 64 KiB leaves protocol headroom while
// preventing a guest-controlled descriptor length from growing host memory
// without bound.
pub(crate) const MAX_AGENT_MESSAGE_LEN: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VirtioConsoleResult {
    ReadValue(u64),
    WriteAck,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VirtioPciConsoleOp {
    Read { size: u8 },
    Write { size: u8, value: u64 },
}

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

impl PortState {
    pub(crate) const fn new() -> Self {
        Self {
            ready: false,
            guest_open: false,
            host_open: false,
        }
    }
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

pub(crate) struct RxQueueDeliveryState<'a> {
    pub(crate) queues: &'a mut [VirtioConsoleQueue; QUEUE_COUNT],
    pub(crate) pending_msix_queue_bits: &'a mut u8,
    pub(crate) interrupt_status: &'a mut u32,
    pub(crate) descriptor_scratch: &'a mut Vec<Descriptor>,
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

    pub fn agent_send(&mut self, data: &[u8]) {
        self.host_to_guest.extend(data.iter().copied());
        // Every host->guest send is also a retry heartbeat: if the channel has
        // not been confirmed yet, re-assert PORT_OPEN(host) so a driver whose
        // port stabilized after our initial burst still latches HostConnected.
        // Delivered on the next poll()/control-RX notify; self-terminates once
        // the guest sends any TX byte (see maybe_reassert_host_open).
        self.maybe_reassert_host_open();
    }

    pub fn take_inbound(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.host_inbound)
    }

    pub fn drain_inbound_into(&mut self, out: &mut Vec<u8>) {
        out.append(&mut self.host_inbound);
    }

    pub fn poll(&mut self, mem: &mut dyn GuestMemoryMut) -> bool {
        let mut progressed = false;
        progressed |= self.process_control_tx_queue(mem);
        progressed |= self.flush_pending_control(mem);
        progressed |= self.deliver_agent_rx(mem);
        progressed |= self.process_agent_tx_queue(mem);
        progressed
    }

    pub(crate) fn access_common(
        &mut self,
        offset: u64,
        is_write: bool,
        size: u8,
        value: u64,
        mem: &mut dyn GuestMemoryMut,
    ) -> VirtioConsoleResult {
        if !is_write {
            return VirtioConsoleResult::ReadValue(self.read_common(offset, size));
        }
        self.write_common(offset, size, value, mem);
        VirtioConsoleResult::WriteAck
    }

    pub(crate) fn read_common(&self, offset: u64, size: u8) -> u64 {
        if let Some(value) = self.read_common_field(offset, size) {
            return value;
        }
        self.read_mmio_alias(offset, size)
    }

    pub(crate) fn read_mmio_alias(&self, offset: u64, size: u8) -> u64 {
        let value = match offset {
            REG_MAGIC => u64::from(MAGIC_VALUE),
            REG_VERSION => u64::from(VERSION_MODERN),
            REG_DEVICE_ID => u64::from(DEVICE_ID_CONSOLE),
            REG_VENDOR_ID => u64::from(VENDOR_ID_QEMU),
            REG_DEVICE_FEATURES => u64::from(self.device_features()),
            REG_DRIVER_FEATURES => {
                u64::from(self.driver_features[self.driver_features_sel.min(1) as usize])
            }
            REG_QUEUE_NUM_MAX => {
                if self.selected_queue().is_some() {
                    u64::from(QUEUE_MAX)
                } else {
                    0
                }
            }
            REG_QUEUE_NUM => self.selected_queue().map_or(0, |q| u64::from(q.size)),
            REG_QUEUE_READY => self
                .selected_queue()
                .map_or(0, |q| u64::from(q.ready as u8)),
            REG_INTERRUPT_STATUS => u64::from(self.interrupt_status),
            REG_STATUS => u64::from(self.status),
            REG_QUEUE_DESC_LOW => self.selected_queue().map_or(0, |q| q.desc & 0xffff_ffff),
            REG_QUEUE_DESC_HIGH => self.selected_queue().map_or(0, |q| q.desc >> 32),
            REG_QUEUE_DRIVER_LOW => self.selected_queue().map_or(0, |q| q.driver & 0xffff_ffff),
            REG_QUEUE_DRIVER_HIGH => self.selected_queue().map_or(0, |q| q.driver >> 32),
            REG_QUEUE_DEVICE_LOW => self.selected_queue().map_or(0, |q| q.device & 0xffff_ffff),
            REG_QUEUE_DEVICE_HIGH => self.selected_queue().map_or(0, |q| q.device >> 32),
            REG_CONFIG_GENERATION => 0,
            _ => 0,
        };
        mask_to_size(value, size)
    }

    pub(crate) fn write_common(
        &mut self,
        offset: u64,
        size: u8,
        value: u64,
        mem: &mut dyn GuestMemoryMut,
    ) {
        if self.write_common_field(offset, size, value) {
            return;
        }
        match offset {
            REG_DEVICE_FEATURES_SEL => self.device_features_sel = value as u32,
            REG_DRIVER_FEATURES_SEL => self.driver_features_sel = value as u32,
            REG_DRIVER_FEATURES => self.write_driver_features(value),
            REG_QUEUE_SEL => self.queue_sel = value as u32,
            REG_QUEUE_NUM => self.write_selected_queue(|q| q.size = (value as u16).min(QUEUE_MAX)),
            REG_QUEUE_READY => self.write_selected_queue(|q| {
                q.ready = value != 0;
                if !q.ready {
                    q.last_avail_idx = 0;
                }
            }),
            REG_QUEUE_NOTIFY => self.notify_queue(value as u16, mem),
            REG_INTERRUPT_ACK => self.interrupt_status &= !(value as u32),
            REG_STATUS => self.write_status(value),
            REG_QUEUE_DESC_LOW => self.write_selected_queue(|q| q.desc = set_low(q.desc, value)),
            REG_QUEUE_DESC_HIGH => self.write_selected_queue(|q| q.desc = set_high(q.desc, value)),
            REG_QUEUE_DRIVER_LOW => {
                self.write_selected_queue(|q| q.driver = set_low(q.driver, value))
            }
            REG_QUEUE_DRIVER_HIGH => {
                self.write_selected_queue(|q| q.driver = set_high(q.driver, value))
            }
            REG_QUEUE_DEVICE_LOW => {
                self.write_selected_queue(|q| q.device = set_low(q.device, value))
            }
            REG_QUEUE_DEVICE_HIGH => {
                self.write_selected_queue(|q| q.device = set_high(q.device, value))
            }
            _ => {}
        }
    }

    pub(crate) fn write_driver_features(&mut self, value: u64) {
        if self.driver_features_sel < 2 {
            let index = self.driver_features_sel as usize;
            self.driver_features[index] = (value as u32) & offered_features_word(index as u32);
        }
    }

    pub(crate) fn read_common_field(&self, offset: u64, size: u8) -> Option<u64> {
        if !is_supported_common_access_size(size) {
            return None;
        }
        let selected_queue = self.selected_queue();
        let fields = [
            (
                COMMON_DEVICE_FEATURE_SELECT,
                4,
                u64::from(self.device_features_sel),
            ),
            (
                COMMON_DEVICE_FEATURE,
                4,
                u64::from(offered_features_word(self.device_features_sel)),
            ),
            (
                COMMON_DRIVER_FEATURE_SELECT,
                4,
                u64::from(self.driver_features_sel),
            ),
            (
                COMMON_DRIVER_FEATURE,
                4,
                u64::from(self.driver_features[self.driver_features_sel.min(1) as usize]),
            ),
            (
                COMMON_CONFIG_MSIX_VECTOR,
                2,
                u64::from(self.config_msix_vector),
            ),
            (COMMON_NUM_QUEUES, 2, QUEUE_COUNT as u64),
            (COMMON_DEVICE_STATUS, 1, u64::from(self.status & 0xff)),
            (COMMON_CONFIG_GENERATION, 1, 0),
            (COMMON_QUEUE_SELECT, 2, u64::from(self.queue_sel as u16)),
            (
                COMMON_QUEUE_SIZE,
                2,
                selected_queue.map_or(0, |q| {
                    u64::from(if q.size == 0 { QUEUE_MAX } else { q.size })
                }),
            ),
            (
                COMMON_QUEUE_MSIX_VECTOR,
                2,
                selected_queue.map_or(u64::from(VIRTIO_MSI_NO_VECTOR), |q| {
                    u64::from(q.msix_vector)
                }),
            ),
            (
                COMMON_QUEUE_ENABLE,
                2,
                selected_queue.map_or(0, |q| u64::from(q.ready as u8)),
            ),
            (
                COMMON_QUEUE_NOTIFY_OFF,
                2,
                selected_queue.map_or(0, |q| u64::from(q.notify_off)),
            ),
            (COMMON_QUEUE_DESC, 8, selected_queue.map_or(0, |q| q.desc)),
            (
                COMMON_QUEUE_DRIVER,
                8,
                selected_queue.map_or(0, |q| q.driver),
            ),
            (
                COMMON_QUEUE_DEVICE,
                8,
                selected_queue.map_or(0, |q| q.device),
            ),
        ];
        fields.iter().find_map(|(base, width, value)| {
            read_common_register(*base, *width, *value, offset, size)
        })
    }

    pub(crate) fn write_common_field(&mut self, offset: u64, size: u8, value: u64) -> bool {
        if !is_supported_common_access_size(size) {
            return false;
        }
        if common_access_touches(COMMON_DEVICE_FEATURE_SELECT, 4, offset, size) {
            self.device_features_sel = write_common_register(
                self.device_features_sel.into(),
                COMMON_DEVICE_FEATURE_SELECT,
                4,
                offset,
                size,
                value,
            ) as u32;
            return true;
        }
        if common_access_touches(COMMON_DRIVER_FEATURE_SELECT, 4, offset, size) {
            self.driver_features_sel = write_common_register(
                self.driver_features_sel.into(),
                COMMON_DRIVER_FEATURE_SELECT,
                4,
                offset,
                size,
                value,
            ) as u32;
            return true;
        }
        if common_access_touches(COMMON_DRIVER_FEATURE, 4, offset, size) {
            let current = self.driver_features[self.driver_features_sel.min(1) as usize];
            let merged = write_common_register(
                current.into(),
                COMMON_DRIVER_FEATURE,
                4,
                offset,
                size,
                value,
            );
            self.write_driver_features(merged);
            return true;
        }
        if common_access_touches(COMMON_CONFIG_MSIX_VECTOR, 2, offset, size) {
            self.config_msix_vector = write_common_register(
                self.config_msix_vector.into(),
                COMMON_CONFIG_MSIX_VECTOR,
                2,
                offset,
                size,
                value,
            ) as u16;
            return true;
        }
        if common_access_touches(COMMON_DEVICE_STATUS, 1, offset, size) {
            let status = write_common_register(
                u64::from(self.status & 0xff),
                COMMON_DEVICE_STATUS,
                1,
                offset,
                size,
                value,
            );
            self.write_status(status);
            return true;
        }
        if common_access_touches(COMMON_QUEUE_SELECT, 2, offset, size) {
            self.queue_sel = write_common_register(
                u64::from(self.queue_sel as u16),
                COMMON_QUEUE_SELECT,
                2,
                offset,
                size,
                value,
            ) as u32;
            return true;
        }
        let Some(queue) = self.queues.get_mut(self.queue_sel as usize) else {
            return common_access_touches_queue_field(offset, size);
        };
        if common_access_touches(COMMON_QUEUE_SIZE, 2, offset, size) {
            queue.size = (write_common_register(
                u64::from(queue.size),
                COMMON_QUEUE_SIZE,
                2,
                offset,
                size,
                value,
            ) as u16)
                .min(QUEUE_MAX);
            return true;
        }
        if common_access_touches(COMMON_QUEUE_MSIX_VECTOR, 2, offset, size) {
            queue.msix_vector = write_common_register(
                u64::from(queue.msix_vector),
                COMMON_QUEUE_MSIX_VECTOR,
                2,
                offset,
                size,
                value,
            ) as u16;
            return true;
        }
        if common_access_touches(COMMON_QUEUE_ENABLE, 2, offset, size) {
            let enable = write_common_register(
                u64::from(queue.ready as u8),
                COMMON_QUEUE_ENABLE,
                2,
                offset,
                size,
                value,
            );
            queue.ready = enable == 1;
            if !queue.ready {
                queue.last_avail_idx = 0;
            }
            return true;
        }
        if common_access_touches(COMMON_QUEUE_DESC, 8, offset, size) {
            queue.desc =
                write_common_register(queue.desc, COMMON_QUEUE_DESC, 8, offset, size, value);
            return true;
        }
        if common_access_touches(COMMON_QUEUE_DRIVER, 8, offset, size) {
            queue.driver =
                write_common_register(queue.driver, COMMON_QUEUE_DRIVER, 8, offset, size, value);
            return true;
        }
        if common_access_touches(COMMON_QUEUE_DEVICE, 8, offset, size) {
            queue.device =
                write_common_register(queue.device, COMMON_QUEUE_DEVICE, 8, offset, size, value);
            return true;
        }
        false
    }

    pub(crate) fn write_status(&mut self, value: u64) {
        self.status = value as u32;
        if value == 0 {
            self.reset_runtime_state();
        }
    }

    pub(crate) fn selected_queue(&self) -> Option<VirtioConsoleQueue> {
        self.queues.get(self.queue_sel as usize).copied()
    }

    pub(crate) fn write_selected_queue(&mut self, write: impl FnOnce(&mut VirtioConsoleQueue)) {
        if let Some(queue) = self.queues.get_mut(self.queue_sel as usize) {
            write(queue);
        }
    }

    pub(crate) fn device_features(&self) -> u32 {
        offered_features_word(self.device_features_sel)
    }

    pub(crate) fn config_read(&self, offset: u64, size: u8) -> u64 {
        let mut config = [0u8; 12];
        config[4..8].copy_from_slice(&(PORT_COUNT as u32).to_le_bytes());
        config[8..12].copy_from_slice(&self.emerg_wr.to_le_bytes());
        read_le_from_bytes(&config, offset, size).unwrap_or(0)
    }
}
