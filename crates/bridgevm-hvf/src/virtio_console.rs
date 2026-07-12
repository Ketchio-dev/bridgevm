//! Modern virtio-console / virtio-serial PCI device model.
//!
//! This intentionally mirrors the existing modern virtio-pci transport shape
//! used by virtio-net/gpu while keeping the BridgeVM agent port logic isolated.

use std::collections::VecDeque;

use crate::{
    fwcfg::GuestMemoryMut,
    msix::{MsixMessage, MsixTable},
    pcie::{
        VIRTIO_CONSOLE_MSIX_PBA_OFFSET, VIRTIO_CONSOLE_MSIX_TABLE_OFFSET,
        VIRTIO_CONSOLE_MSIX_VECTOR_COUNT,
    },
};

/// Env-gated (`BRIDGEVM_VIRTIO_CONSOLE_TRACE=1`) one-line trace of control-plane
/// events. Collapses to a cached bool check when disabled. Defined before first
/// use so the whole module can call it.
macro_rules! console_trace {
    ($($arg:tt)*) => {
        if console_trace_enabled() {
            eprintln!("[vcon] {}", format_args!($($arg)*));
        }
    };
}

const MAGIC_VALUE: u32 = 0x7472_6976;
const VERSION_MODERN: u32 = 2;
const DEVICE_ID_CONSOLE: u32 = 3;
const VENDOR_ID_QEMU: u32 = 0x554d_4551;

const REG_MAGIC: u64 = 0x000;
const REG_VERSION: u64 = 0x004;
const REG_DEVICE_ID: u64 = 0x008;
const REG_VENDOR_ID: u64 = 0x00c;
const REG_DEVICE_FEATURES: u64 = 0x010;
const REG_DEVICE_FEATURES_SEL: u64 = 0x014;
const REG_DRIVER_FEATURES: u64 = 0x020;
const REG_DRIVER_FEATURES_SEL: u64 = 0x024;
const REG_QUEUE_SEL: u64 = 0x030;
const REG_QUEUE_NUM_MAX: u64 = 0x034;
const REG_QUEUE_NUM: u64 = 0x038;
const REG_QUEUE_READY: u64 = 0x044;
const REG_QUEUE_NOTIFY: u64 = 0x050;
const REG_INTERRUPT_STATUS: u64 = 0x060;
const REG_INTERRUPT_ACK: u64 = 0x064;
const REG_STATUS: u64 = 0x070;
const REG_QUEUE_DESC_LOW: u64 = 0x080;
const REG_QUEUE_DESC_HIGH: u64 = 0x084;
const REG_QUEUE_DRIVER_LOW: u64 = 0x090;
const REG_QUEUE_DRIVER_HIGH: u64 = 0x094;
const REG_QUEUE_DEVICE_LOW: u64 = 0x0a0;
const REG_QUEUE_DEVICE_HIGH: u64 = 0x0a4;
const REG_CONFIG_GENERATION: u64 = 0x0fc;

const PCI_COMMON_CFG_OFFSET: u64 = 0x0000;
const PCI_ISR_CFG_OFFSET: u64 = 0x1000;
const PCI_DEVICE_CFG_OFFSET: u64 = 0x2000;
const PCI_NOTIFY_CFG_OFFSET: u64 = 0x3000;
const PCI_CFG_REGION_SIZE: u64 = 0x1000;

const COMMON_DEVICE_FEATURE_SELECT: u64 = 0x00;
const COMMON_DEVICE_FEATURE: u64 = 0x04;
const COMMON_DRIVER_FEATURE_SELECT: u64 = 0x08;
const COMMON_DRIVER_FEATURE: u64 = 0x0c;
const COMMON_CONFIG_MSIX_VECTOR: u64 = 0x10;
const COMMON_NUM_QUEUES: u64 = 0x12;
const COMMON_DEVICE_STATUS: u64 = 0x14;
const COMMON_CONFIG_GENERATION: u64 = 0x15;
const COMMON_QUEUE_SELECT: u64 = 0x16;
const COMMON_QUEUE_SIZE: u64 = 0x18;
const COMMON_QUEUE_MSIX_VECTOR: u64 = 0x1a;
const COMMON_QUEUE_ENABLE: u64 = 0x1c;
const COMMON_QUEUE_NOTIFY_OFF: u64 = 0x1e;
const COMMON_QUEUE_DESC: u64 = 0x20;
const COMMON_QUEUE_DRIVER: u64 = 0x28;
const COMMON_QUEUE_DEVICE: u64 = 0x30;

const VIRTIO_CONSOLE_F_MULTIPORT: u32 = 1 << 1;
const VIRTIO_F_VERSION_1: u32 = 1 << 0;
const VIRTIO_MSI_NO_VECTOR: u16 = 0xffff;

pub const QUEUE_PORT0_RX: usize = 0;
pub const QUEUE_PORT0_TX: usize = 1;
pub const QUEUE_CONTROL_RX: usize = 2;
pub const QUEUE_CONTROL_TX: usize = 3;
pub const QUEUE_AGENT_RX: usize = 4;
pub const QUEUE_AGENT_TX: usize = 5;
const QUEUE_COUNT: usize = 6;
const QUEUE_MAX: u16 = 64;
const DESC_SIZE: u64 = 16;
const DESC_F_NEXT: u16 = 1;
const DESC_F_WRITE: u16 = 2;

const PORT_COUNT: usize = 2;
const AGENT_PORT_ID: u32 = 1;
pub const AGENT_PORT_NAME: &[u8] = b"org.bridgevm.agent.0";

const VIRTIO_CONSOLE_DEVICE_READY: u16 = 0;
const VIRTIO_CONSOLE_DEVICE_ADD: u16 = 1;
const VIRTIO_CONSOLE_DEVICE_REMOVE: u16 = 2;
const VIRTIO_CONSOLE_PORT_READY: u16 = 3;
const VIRTIO_CONSOLE_CONSOLE_PORT: u16 = 4;
const VIRTIO_CONSOLE_RESIZE: u16 = 5;
const VIRTIO_CONSOLE_PORT_OPEN: u16 = 6;
const VIRTIO_CONSOLE_PORT_NAME: u16 = 7;
const CONTROL_LEN: usize = 8;
const MAX_CONTROL_MESSAGE_LEN: usize = CONTROL_LEN + AGENT_PORT_NAME.len();
// Agent replies are line-oriented. The largest current wire line is one
// base64-encoded 24 KiB file chunk, so 64 KiB leaves protocol headroom while
// preventing a guest-controlled descriptor length from growing host memory
// without bound.
const MAX_AGENT_MESSAGE_LEN: usize = 64 * 1024;

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
    device_features_sel: u32,
    driver_features_sel: u32,
    driver_features: [u32; 2],
    config_msix_vector: u16,
    queue_sel: u32,
    queues: [VirtioConsoleQueue; QUEUE_COUNT],
    pending_msix_queue_bits: u8,
    status: u32,
    interrupt_status: u32,
    emerg_wr: u32,
    ports: [PortState; PORT_COUNT],
    pending_control: VecDeque<PendingControlMessage>,
    host_to_guest: VecDeque<u8>,
    host_inbound: Vec<u8>,
    descriptor_scratch: Vec<Descriptor>,
    read_scratch: Vec<u8>,
    // Set once the guest driver has actually pushed bytes on the agent TX
    // queue. Guest TX only flows after vioser latches HostConnected=TRUE (its
    // WillWriteBlock gate), so the first inbound byte is our only host-side
    // proof that our PORT_OPEN(host) took effect. Until then we keep
    // re-asserting PORT_OPEN; afterwards we stop (see maybe_reassert_host_open).
    agent_connected_confirmed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PortState {
    ready: bool,
    guest_open: bool,
    host_open: bool,
}

impl PortState {
    const fn new() -> Self {
        Self {
            ready: false,
            guest_open: false,
            host_open: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct VirtioConsoleQueue {
    size: u16,
    ready: bool,
    desc: u64,
    driver: u64,
    device: u64,
    msix_vector: u16,
    notify_off: u16,
    last_avail_idx: u16,
    pending_msix: bool,
    // Diagnostics (never reset by the queue's own reset; cleared only on a full
    // device reset). notify_count = guest doorbells; last_avail_seen = the most
    // recent avail->idx we read; used_produced = used entries we published;
    // rx_no_buffers = delivery attempts that found no fresh avail buffer. If the
    // guest keeps kicking (notify_count climbs) and posting (last_avail_seen
    // climbs) but delivery stalls, the bug is our consume path; if last_avail
    // stops climbing, the guest stopped replenishing.
    notify_count: u64,
    last_avail_seen: u16,
    used_produced: u64,
    rx_no_buffers: u64,
}

impl VirtioConsoleQueue {
    const fn new(notify_off: u16) -> Self {
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

    fn reset(&mut self) {
        let notify_off = self.notify_off;
        *self = Self::new(notify_off);
    }
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

    fn access_common(
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

    fn read_common(&self, offset: u64, size: u8) -> u64 {
        if let Some(value) = self.read_common_field(offset, size) {
            return value;
        }
        self.read_mmio_alias(offset, size)
    }

    fn read_mmio_alias(&self, offset: u64, size: u8) -> u64 {
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

    fn write_common(&mut self, offset: u64, size: u8, value: u64, mem: &mut dyn GuestMemoryMut) {
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

    fn write_driver_features(&mut self, value: u64) {
        if self.driver_features_sel < 2 {
            let index = self.driver_features_sel as usize;
            self.driver_features[index] = (value as u32) & offered_features_word(index as u32);
        }
    }

    fn read_common_field(&self, offset: u64, size: u8) -> Option<u64> {
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

    fn write_common_field(&mut self, offset: u64, size: u8, value: u64) -> bool {
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

    fn write_status(&mut self, value: u64) {
        self.status = value as u32;
        if value == 0 {
            self.reset_runtime_state();
        }
    }

    fn selected_queue(&self) -> Option<VirtioConsoleQueue> {
        self.queues.get(self.queue_sel as usize).copied()
    }

    fn write_selected_queue(&mut self, write: impl FnOnce(&mut VirtioConsoleQueue)) {
        if let Some(queue) = self.queues.get_mut(self.queue_sel as usize) {
            write(queue);
        }
    }

    fn device_features(&self) -> u32 {
        offered_features_word(self.device_features_sel)
    }

    fn config_read(&self, offset: u64, size: u8) -> u64 {
        let mut config = [0u8; 12];
        config[4..8].copy_from_slice(&(PORT_COUNT as u32).to_le_bytes());
        config[8..12].copy_from_slice(&self.emerg_wr.to_le_bytes());
        read_le_from_bytes(&config, offset, size).unwrap_or(0)
    }

    fn config_write(&mut self, offset: u64, size: u8, value: u64) {
        if (8..12).contains(&offset) && size <= 4 {
            let current = self.emerg_wr;
            let merged = insert_u32(current, offset - 8, size, value);
            self.emerg_wr = merged;
        }
    }

    fn notify_queue(&mut self, queue_index: u16, mem: &mut dyn GuestMemoryMut) {
        if let Some(queue) = self.queues.get_mut(usize::from(queue_index)) {
            queue.notify_count = queue.notify_count.saturating_add(1);
        }
        if console_trace_enabled() {
            let avail = self
                .queues
                .get(usize::from(queue_index))
                .and_then(|queue| read_u16(mem, queue.driver + 2));
            let last = self
                .queues
                .get(usize::from(queue_index))
                .map(|queue| queue.last_avail_idx);
            eprintln!("[vcon] notify q{queue_index} avail_idx={avail:?} last_consumed={last:?}");
        }
        match usize::from(queue_index) {
            QUEUE_CONTROL_RX => {
                // Only drain what is genuinely queued. We must NOT synthesize a
                // PORT_OPEN re-assert here: delivering one makes vioser consume
                // and refill the control-RX ring, which kicks this very queue,
                // which would re-assert again -> a self-sustaining MSI-X storm
                // that livelocks the guest. Re-assertion is driven only by the
                // bounded triggers (agent_send heartbeat, PORT_READY epoch).
                self.flush_pending_control(mem);
            }
            QUEUE_CONTROL_TX => {
                self.process_control_tx_queue(mem);
                self.flush_pending_control(mem);
            }
            QUEUE_AGENT_RX => {
                self.deliver_agent_rx(mem);
            }
            QUEUE_AGENT_TX => {
                self.process_agent_tx_queue(mem);
            }
            _ => {}
        }
    }

    fn process_control_tx_queue(&mut self, mem: &mut dyn GuestMemoryMut) -> bool {
        let queue_index = QUEUE_CONTROL_TX;
        let queue = self.queues[queue_index];
        if !queue.ready || queue.size == 0 || queue.desc == 0 {
            return false;
        }
        let Some(avail_idx) = read_u16(mem, queue.driver + 2) else {
            return false;
        };
        self.queues[queue_index].last_avail_seen = avail_idx;
        let mut progressed = false;
        let mut descs = std::mem::take(&mut self.descriptor_scratch);
        let mut bytes = std::mem::take(&mut self.read_scratch);
        while self.queues[queue_index].last_avail_idx != avail_idx {
            let last_avail_idx = self.queues[queue_index].last_avail_idx;
            let ring_off = 4 + u64::from(last_avail_idx % queue.size) * 2;
            let Some(head) = read_u16(mem, queue.driver + ring_off) else {
                break;
            };
            if Self::read_chain_into(
                mem,
                &queue,
                head,
                &mut descs,
                &mut bytes,
                MAX_CONTROL_MESSAGE_LEN,
            ) {
                self.handle_control_tx(&bytes);
            }
            Self::write_used(mem, &queue, head, 0);
            self.queues[queue_index].last_avail_idx = last_avail_idx.wrapping_add(1);
            self.queues[queue_index].used_produced =
                self.queues[queue_index].used_produced.saturating_add(1);
            self.mark_queue_interrupt(queue_index);
            progressed = true;
        }
        descs.clear();
        bytes.clear();
        self.descriptor_scratch = descs;
        self.read_scratch = bytes;
        progressed
    }

    fn handle_control_tx(&mut self, bytes: &[u8]) {
        let Some(control) = Control::parse(bytes) else {
            return;
        };
        console_trace!(
            "ctrl<-guest id={} event={} value={}",
            control.id,
            control.event,
            control.value
        );
        match control.event {
            VIRTIO_CONSOLE_DEVICE_READY if control.value == 1 => {
                self.enqueue_control(Control::new(0, VIRTIO_CONSOLE_DEVICE_ADD, 0));
                self.enqueue_control(Control::new(1, VIRTIO_CONSOLE_DEVICE_ADD, 0));
            }
            VIRTIO_CONSOLE_PORT_READY if control.value == 1 => {
                if let Some(port) = self.ports.get_mut(control.id as usize) {
                    port.ready = true;
                }
                if control.id == AGENT_PORT_ID {
                    self.enqueue_control(Self::agent_port_name_message());
                    self.ports[1].host_open = true;
                    // A fresh PORT_READY means the port (re)entered D0; vioser
                    // clears HostConnected on every D0 exit, so treat this as a
                    // new connection epoch and resume re-asserting PORT_NAME +
                    // PORT_OPEN.
                    self.agent_connected_confirmed = false;
                    self.enqueue_control(Control::new(AGENT_PORT_ID, VIRTIO_CONSOLE_PORT_OPEN, 1));
                }
            }
            VIRTIO_CONSOLE_PORT_OPEN => {
                if let Some(port) = self.ports.get_mut(control.id as usize) {
                    port.guest_open = control.value != 0;
                }
            }
            VIRTIO_CONSOLE_DEVICE_READY
            | VIRTIO_CONSOLE_DEVICE_ADD
            | VIRTIO_CONSOLE_DEVICE_REMOVE
            | VIRTIO_CONSOLE_CONSOLE_PORT
            | VIRTIO_CONSOLE_RESIZE
            | VIRTIO_CONSOLE_PORT_NAME => {}
            _ => {}
        }
    }

    fn enqueue_control(&mut self, message: impl Into<PendingControlMessage>) {
        let message = message.into();
        if console_trace_enabled() {
            if let Some(control) = Control::parse(message.as_slice()) {
                eprintln!(
                    "[vcon] ctrl->guest id={} event={} value={} bytes={}",
                    control.id,
                    control.event,
                    control.value,
                    message.len()
                );
            }
        }
        self.pending_control.push_back(message);
    }

    /// Re-assert PORT_OPEN(host) toward the agent port while the connection is
    /// still unconfirmed. This replaces the old fixed-count resend budget: a
    /// one-shot burst permanently gives up if the guest's port only stabilizes
    /// (e.g. after a PnP resource rebalance / D0 bounce) *after* the burst is
    /// spent, which strands HostConnected=FALSE forever. Instead we keep
    /// re-asserting on every control-RX rearm and every host->guest send, but
    /// only ever leave one re-assert in flight (no flooding) and stop entirely
    /// once the guest proves the link by sending a TX byte. vioser's PORT_OPEN
    /// handler is idempotent (`if HostConnected != Connected`), so a redundant
    /// PORT_OPEN after the latch is a harmless no-op.
    fn maybe_reassert_host_open(&mut self) {
        let port = self.ports[AGENT_PORT_ID as usize];
        if !port.ready || !port.host_open || self.agent_connected_confirmed {
            return;
        }
        if self.host_open_reassert_pending() {
            return;
        }
        // Re-send PORT_NAME as well as PORT_OPEN. vioser's VIOSerialFindPortById
        // drops control messages that arrive before the port PDO fully resolves;
        // the same race that made the first PORT_OPEN need re-sending also drops
        // the first PORT_NAME. If PORT_NAME is lost, vioser never sets the port's
        // NameString, so it never creates the `\DosDevices\<name>` symbolic link
        // and the guest agent's CreateFile on `\\.\<name>` fails (the port has
        // only its default `vportNpM` desc, no friendly name). vioser's
        // VIOSerialPortCreateName is idempotent (`if (!NameString.Buffer)`), so a
        // redundant PORT_NAME after the name is set is a harmless no-op. Bounded
        // to the PING heartbeat with one pair in flight, so no control-queue flood.
        self.enqueue_control(Self::agent_port_name_message());
        self.enqueue_control(Control::new(AGENT_PORT_ID, VIRTIO_CONSOLE_PORT_OPEN, 1));
    }

    /// The PORT_NAME control message for the agent port: an 8-byte control
    /// header followed by the port name bytes (no trailing NUL — vioser's
    /// VIOSerialPortCreateName derives the length from the used-ring length and
    /// appends its own NUL, per virtio 1.2 5.3).
    fn agent_port_name_message() -> PendingControlMessage {
        PendingControlMessage::agent_port_name()
    }

    fn host_open_reassert_pending(&self) -> bool {
        let target =
            PendingControlMessage::from(Control::new(AGENT_PORT_ID, VIRTIO_CONSOLE_PORT_OPEN, 1));
        self.pending_control
            .iter()
            .any(|message| message.as_slice() == target.as_slice())
    }

    fn flush_pending_control(&mut self, mem: &mut dyn GuestMemoryMut) -> bool {
        let mut progressed = false;
        while let Some(message) = self.pending_control.pop_front() {
            if self.deliver_to_rx_queue(QUEUE_CONTROL_RX, message.as_slice(), mem) {
                progressed = true;
            } else {
                self.pending_control.push_front(message);
                break;
            }
        }
        progressed
    }

    fn deliver_agent_rx(&mut self, mem: &mut dyn GuestMemoryMut) -> bool {
        // Deliver host->guest bytes whenever the guest has posted receive
        // buffers on the agent RX queue. We intentionally do NOT gate on
        // ports[1].guest_open: the real vioser driver does not reliably emit a
        // guest PORT_OPEN we can observe (its VIOSerialPortCreate can
        // short-circuit), yet it still posts RX buffers once the app opens the
        // port. Gating on an unobservable guest_open deadlocked the channel.
        if self.host_to_guest.is_empty() {
            return false;
        }
        let (front, back) = self.host_to_guest.as_slices();
        let Some(written) = Self::deliver_partial_slices_to_rx_queue(
            &mut self.queues,
            &mut self.pending_msix_queue_bits,
            &mut self.interrupt_status,
            &mut self.descriptor_scratch,
            QUEUE_AGENT_RX,
            front,
            back,
            mem,
        ) else {
            return false;
        };
        self.host_to_guest.drain(..written);
        true
    }

    fn process_agent_tx_queue(&mut self, mem: &mut dyn GuestMemoryMut) -> bool {
        let queue_index = QUEUE_AGENT_TX;
        let queue = self.queues[queue_index];
        if !queue.ready || queue.size == 0 || queue.desc == 0 {
            return false;
        }
        let Some(avail_idx) = read_u16(mem, queue.driver + 2) else {
            return false;
        };
        self.queues[queue_index].last_avail_seen = avail_idx;
        let mut progressed = false;
        let mut descs = std::mem::take(&mut self.descriptor_scratch);
        let mut bytes = std::mem::take(&mut self.read_scratch);
        while self.queues[queue_index].last_avail_idx != avail_idx {
            let last_avail_idx = self.queues[queue_index].last_avail_idx;
            let ring_off = 4 + u64::from(last_avail_idx % queue.size) * 2;
            let Some(head) = read_u16(mem, queue.driver + ring_off) else {
                break;
            };
            if Self::read_chain_into(
                mem,
                &queue,
                head,
                &mut descs,
                &mut bytes,
                MAX_AGENT_MESSAGE_LEN,
            ) {
                // Any guest TX proves vioser latched HostConnected (its
                // WillWriteBlock gate blocks writes until then), so we can stop
                // re-asserting PORT_OPEN from here on.
                if !bytes.is_empty() {
                    self.agent_connected_confirmed = true;
                }
                console_trace!("agent-tx<-guest len={}", bytes.len());
                self.host_inbound.extend_from_slice(&bytes);
            }
            Self::write_used(mem, &queue, head, 0);
            self.queues[queue_index].last_avail_idx = last_avail_idx.wrapping_add(1);
            self.queues[queue_index].used_produced =
                self.queues[queue_index].used_produced.saturating_add(1);
            self.mark_queue_interrupt(queue_index);
            progressed = true;
        }
        descs.clear();
        bytes.clear();
        self.descriptor_scratch = descs;
        self.read_scratch = bytes;
        progressed
    }

    fn deliver_to_rx_queue(
        &mut self,
        queue_index: usize,
        bytes: &[u8],
        mem: &mut dyn GuestMemoryMut,
    ) -> bool {
        self.deliver_partial_to_rx_queue(queue_index, bytes, mem)
            .is_some_and(|written| written == bytes.len())
    }

    fn deliver_partial_to_rx_queue(
        &mut self,
        queue_index: usize,
        bytes: &[u8],
        mem: &mut dyn GuestMemoryMut,
    ) -> Option<usize> {
        Self::deliver_partial_slices_to_rx_queue(
            &mut self.queues,
            &mut self.pending_msix_queue_bits,
            &mut self.interrupt_status,
            &mut self.descriptor_scratch,
            queue_index,
            bytes,
            &[],
            mem,
        )
    }

    fn deliver_partial_slices_to_rx_queue(
        queues: &mut [VirtioConsoleQueue; QUEUE_COUNT],
        pending_msix_queue_bits: &mut u8,
        interrupt_status: &mut u32,
        descriptor_scratch: &mut Vec<Descriptor>,
        queue_index: usize,
        first: &[u8],
        second: &[u8],
        mem: &mut dyn GuestMemoryMut,
    ) -> Option<usize> {
        let bytes_len = first.len().checked_add(second.len())?;
        let queue = queues[queue_index];
        if !queue.ready || queue.size == 0 || queue.desc == 0 || bytes_len == 0 {
            return None;
        }
        let avail_idx = read_u16(mem, queue.driver + 2)?;
        queues[queue_index].last_avail_seen = avail_idx;
        let last_avail_idx = queues[queue_index].last_avail_idx;
        if last_avail_idx == avail_idx {
            // The guest has not published a fresh avail buffer since we last
            // consumed. If this keeps firing while notify_count / last_avail_seen
            // stay flat, the guest stopped replenishing (not our consume path).
            queues[queue_index].rx_no_buffers = queues[queue_index].rx_no_buffers.saturating_add(1);
            console_trace!(
                "rx q{queue_index} NO-BUFFERS last_consumed={last_avail_idx} avail_idx={avail_idx} bytes={}",
                bytes_len
            );
            return None;
        }
        let ring_off = 4 + u64::from(last_avail_idx % queue.size) * 2;
        let head = read_u16(mem, queue.driver + ring_off)?;
        let mut descs = std::mem::take(descriptor_scratch);
        if !Self::descriptor_chain_into(mem, &queue, head, &mut descs) {
            descs.clear();
            *descriptor_scratch = descs;
            // avail advanced but we could not walk the chain (head >= size, or a
            // bad next link). This is the "replenished buffers are invisible to
            // us" signature -> our consume path or a size mismatch.
            console_trace!(
                "rx q{queue_index} CHAIN-FAIL head={head} size={} last_consumed={last_avail_idx} avail_idx={avail_idx}",
                queue.size
            );
            return None;
        }
        let Some(written) = Self::scatter_write_partial_slices(mem, &descs, first, second) else {
            console_trace!(
                "rx q{queue_index} SCATTER-FAIL head={head} descs={}",
                descs.len()
            );
            descs.clear();
            *descriptor_scratch = descs;
            return None;
        };
        descs.clear();
        *descriptor_scratch = descs;
        Self::write_used(
            mem,
            &queue,
            head,
            u32::try_from(written).unwrap_or(u32::MAX),
        );
        queues[queue_index].last_avail_idx = last_avail_idx.wrapping_add(1);
        queues[queue_index].used_produced = queues[queue_index].used_produced.saturating_add(1);
        queues[queue_index].pending_msix = true;
        if let Some(bit) = queue_bit(queue_index) {
            *pending_msix_queue_bits |= bit;
        }
        *interrupt_status |= 1;
        console_trace!(
            "rx q{queue_index} DELIVER head={head} len={written} last_consumed->{} avail_idx={avail_idx}",
            last_avail_idx.wrapping_add(1)
        );
        Some(written)
    }

    fn read_chain_into(
        mem: &dyn GuestMemoryMut,
        queue: &VirtioConsoleQueue,
        head: u16,
        descs: &mut Vec<Descriptor>,
        out: &mut Vec<u8>,
        max_len: usize,
    ) -> bool {
        out.clear();
        if !Self::descriptor_chain_into(mem, queue, head, descs) {
            return false;
        }
        for desc in descs.iter() {
            if desc.flags & DESC_F_WRITE != 0 {
                return false;
            }
            let start = out.len();
            let Some(end) = start.checked_add(desc.len as usize) else {
                return false;
            };
            if end > max_len {
                return false;
            }
            // `read_bytes` validates the guest range before allocating in the
            // live RAM implementation. Only append after that validation so an
            // unbacked, oversized descriptor cannot resize reusable scratch.
            let Some(bytes) = mem.read_bytes(desc.addr, desc.len as usize) else {
                return false;
            };
            out.extend_from_slice(&bytes);
        }
        true
    }

    fn scatter_write_partial_slices(
        mem: &mut dyn GuestMemoryMut,
        descs: &[Descriptor],
        first: &[u8],
        second: &[u8],
    ) -> Option<usize> {
        let bytes_len = first.len().checked_add(second.len())?;
        let mut offset = 0usize;
        for desc in descs {
            if desc.flags & DESC_F_WRITE == 0 {
                return None;
            }
            let mut desc_addr = desc.addr;
            let mut desc_remaining = desc.len as usize;
            while desc_remaining > 0 && offset < bytes_len {
                let chunk = Self::slice_pair_chunk(first, second, offset)?;
                let writable = desc_remaining.min(chunk.len());
                if writable == 0 {
                    break;
                }
                if !mem.write_bytes(desc_addr, &chunk[..writable]) {
                    return None;
                }
                offset += writable;
                desc_addr = desc_addr.checked_add(writable as u64)?;
                desc_remaining -= writable;
            }
            if offset == bytes_len {
                break;
            }
        }
        (offset > 0).then_some(offset)
    }

    fn slice_pair_chunk<'a>(first: &'a [u8], second: &'a [u8], offset: usize) -> Option<&'a [u8]> {
        if offset < first.len() {
            return Some(&first[offset..]);
        }
        let second_offset = offset.checked_sub(first.len())?;
        (second_offset < second.len()).then_some(&second[second_offset..])
    }

    fn mark_queue_interrupt(&mut self, queue_index: usize) {
        if let Some(queue) = self.queues.get_mut(queue_index) {
            queue.pending_msix = true;
            if let Some(bit) = queue_bit(queue_index) {
                self.pending_msix_queue_bits |= bit;
            }
        }
        self.interrupt_status |= 1;
    }

    fn descriptor_chain_into(
        mem: &dyn GuestMemoryMut,
        queue: &VirtioConsoleQueue,
        head: u16,
        out: &mut Vec<Descriptor>,
    ) -> bool {
        out.clear();
        if head >= queue.size {
            return false;
        }
        let mut index = head;
        for _ in 0..queue.size {
            let Some(desc) = Descriptor::read(mem, queue.desc + u64::from(index) * DESC_SIZE)
            else {
                out.clear();
                return false;
            };
            let has_next = desc.flags & DESC_F_NEXT != 0;
            out.push(desc);
            if !has_next {
                return true;
            }
            index = desc.next;
            if index >= queue.size {
                out.clear();
                return false;
            }
        }
        out.clear();
        false
    }

    fn write_used(mem: &mut dyn GuestMemoryMut, queue: &VirtioConsoleQueue, id: u16, len: u32) {
        if queue.size == 0 || queue.device == 0 {
            return;
        }
        let Some(used_idx) = read_u16(mem, queue.device + 2) else {
            return;
        };
        let elem = queue.device + 4 + u64::from(used_idx % queue.size) * 8;
        let _ = mem.write_bytes(elem, &u32::from(id).to_le_bytes());
        let _ = mem.write_bytes(elem + 4, &len.to_le_bytes());
        let _ = mem.write_bytes(queue.device + 2, &used_idx.wrapping_add(1).to_le_bytes());
    }
}

impl Default for VirtioConsole {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub struct VirtioPciConsole {
    console: VirtioConsole,
    msix: MsixTable,
}

impl VirtioPciConsole {
    pub fn new() -> Self {
        Self {
            console: VirtioConsole::new(),
            msix: MsixTable::new(VIRTIO_CONSOLE_MSIX_VECTOR_COUNT),
        }
    }

    pub fn stats(&self) -> VirtioConsoleStats {
        self.console.stats()
    }

    pub fn interrupt_line_level(&self) -> bool {
        self.console.interrupt_line_level()
    }

    pub fn reset_runtime_state(&mut self) {
        self.console.reset_runtime_state();
        self.msix = MsixTable::new(VIRTIO_CONSOLE_MSIX_VECTOR_COUNT);
    }

    pub fn agent_send(&mut self, data: &[u8]) {
        self.console.agent_send(data);
    }

    pub fn take_inbound(&mut self) -> Vec<u8> {
        self.console.take_inbound()
    }

    pub fn drain_inbound_into(&mut self, out: &mut Vec<u8>) {
        self.console.drain_inbound_into(out);
    }

    pub fn poll(&mut self, mem: &mut dyn GuestMemoryMut) -> bool {
        self.console.poll(mem)
    }

    pub fn access(
        &mut self,
        offset: u64,
        op: VirtioPciConsoleOp,
        mem: &mut dyn GuestMemoryMut,
    ) -> VirtioConsoleResult {
        if let Some(common_offset) = common_cfg_offset(offset) {
            return match op {
                VirtioPciConsoleOp::Read { size } => {
                    self.console
                        .access_common(common_offset, false, size, 0, mem)
                }
                VirtioPciConsoleOp::Write { size, value } => {
                    self.console
                        .access_common(common_offset, true, size, value, mem)
                }
            };
        }
        if let Some(device_offset) = device_cfg_offset(offset) {
            return match op {
                VirtioPciConsoleOp::Read { size } => {
                    VirtioConsoleResult::ReadValue(self.console.config_read(device_offset, size))
                }
                VirtioPciConsoleOp::Write { size, value } => {
                    self.console.config_write(device_offset, size, value);
                    VirtioConsoleResult::WriteAck
                }
            };
        }
        if let Some(queue_index) = notify_queue_index(offset) {
            return match op {
                VirtioPciConsoleOp::Read { .. } => VirtioConsoleResult::ReadValue(0),
                VirtioPciConsoleOp::Write { value, .. } => {
                    let queue = if offset == PCI_NOTIFY_CFG_OFFSET {
                        value as u16
                    } else {
                        queue_index
                    };
                    self.console.notify_queue(queue, mem);
                    VirtioConsoleResult::WriteAck
                }
            };
        }
        if offset == PCI_ISR_CFG_OFFSET {
            return match op {
                VirtioPciConsoleOp::Read { size } => VirtioConsoleResult::ReadValue(mask_to_size(
                    u64::from(self.console.interrupt_status),
                    size,
                )),
                VirtioPciConsoleOp::Write { value, .. } => {
                    self.console.interrupt_status &= !(value as u32);
                    VirtioConsoleResult::WriteAck
                }
            };
        }
        match op {
            VirtioPciConsoleOp::Read { .. } => VirtioConsoleResult::ReadValue(0),
            VirtioPciConsoleOp::Write { .. } => VirtioConsoleResult::WriteAck,
        }
    }

    pub fn msix_bar_access(&mut self, offset: u64, op: VirtioPciConsoleOp) -> VirtioConsoleResult {
        if let Some(table_offset) = self.msix_table_offset(offset) {
            return match op {
                VirtioPciConsoleOp::Read { size } => {
                    VirtioConsoleResult::ReadValue(self.msix.table_read(table_offset, size))
                }
                VirtioPciConsoleOp::Write { size, value } => {
                    self.msix.table_write(table_offset, size, value);
                    VirtioConsoleResult::WriteAck
                }
            };
        }
        if let Some(pba_offset) = self.msix_pba_offset(offset) {
            return match op {
                VirtioPciConsoleOp::Read { size } => {
                    VirtioConsoleResult::ReadValue(self.msix.pba_read(pba_offset, size))
                }
                VirtioPciConsoleOp::Write { size, value } => {
                    self.msix.pba_write(pba_offset, size, value);
                    VirtioConsoleResult::WriteAck
                }
            };
        }
        match op {
            VirtioPciConsoleOp::Read { .. } => VirtioConsoleResult::ReadValue(0),
            VirtioPciConsoleOp::Write { .. } => VirtioConsoleResult::WriteAck,
        }
    }

    pub fn drain_pending_msix(
        &mut self,
        function_enabled: bool,
        function_masked: bool,
    ) -> Vec<MsixMessage> {
        let mut messages = Vec::new();
        self.drain_pending_msix_into(function_enabled, function_masked, &mut messages);
        messages
    }

    pub fn drain_pending_msix_into(
        &mut self,
        function_enabled: bool,
        function_masked: bool,
        out: &mut Vec<MsixMessage>,
    ) {
        let start = out.len();
        self.msix
            .drain_pending_into(function_enabled, function_masked, out);
        for message in &out[start..] {
            self.clear_pending_queue_for_vector(message.vector);
        }
        self.raise_pending_msix_into(function_enabled, function_masked, out);
    }

    fn raise_pending_msix_into(
        &mut self,
        function_enabled: bool,
        function_masked: bool,
        out: &mut Vec<MsixMessage>,
    ) {
        let mut pending = self.console.pending_msix_queue_bits;
        while pending != 0 {
            let queue_index = pending.trailing_zeros() as usize;
            let vector = self.console.queues[queue_index].msix_vector;
            if vector == VIRTIO_MSI_NO_VECTOR {
                pending &= !(1u8 << queue_index);
                continue;
            }
            if let Some(message) = self.msix.raise(vector, function_enabled, function_masked) {
                self.console.queues[queue_index].pending_msix = false;
                self.console.pending_msix_queue_bits &= !(1u8 << queue_index);
                out.push(message);
            }
            pending &= !(1u8 << queue_index);
        }
    }

    fn clear_pending_queue_for_vector(&mut self, vector: u16) {
        for (queue_index, queue) in self.console.queues.iter_mut().enumerate() {
            if queue.msix_vector == vector {
                queue.pending_msix = false;
                if let Some(bit) = queue_bit(queue_index) {
                    self.console.pending_msix_queue_bits &= !bit;
                }
            }
        }
    }

    fn msix_table_offset(&self, offset: u64) -> Option<u64> {
        let rel = offset.checked_sub(u64::from(VIRTIO_CONSOLE_MSIX_TABLE_OFFSET))?;
        (rel < self.msix.table_byte_len()).then_some(rel)
    }

    fn msix_pba_offset(&self, offset: u64) -> Option<u64> {
        let rel = offset.checked_sub(u64::from(VIRTIO_CONSOLE_MSIX_PBA_OFFSET))?;
        (rel < self.msix.pba_byte_len()).then_some(rel)
    }
}

impl Default for VirtioPciConsole {
    fn default() -> Self {
        Self::new()
    }
}

fn common_cfg_offset(offset: u64) -> Option<u64> {
    (PCI_COMMON_CFG_OFFSET..PCI_COMMON_CFG_OFFSET + PCI_CFG_REGION_SIZE)
        .contains(&offset)
        .then_some(offset - PCI_COMMON_CFG_OFFSET)
}

fn device_cfg_offset(offset: u64) -> Option<u64> {
    (PCI_DEVICE_CFG_OFFSET..PCI_DEVICE_CFG_OFFSET + PCI_CFG_REGION_SIZE)
        .contains(&offset)
        .then_some(offset - PCI_DEVICE_CFG_OFFSET)
}

fn notify_queue_index(offset: u64) -> Option<u16> {
    let rel = offset.checked_sub(PCI_NOTIFY_CFG_OFFSET)?;
    (rel < PCI_CFG_REGION_SIZE).then_some((rel / 4) as u16)
}

fn queue_bit(index: usize) -> Option<u8> {
    (index < u8::BITS as usize).then(|| 1u8 << index)
}

#[derive(Debug, Clone, Copy)]
struct Descriptor {
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
}

impl Descriptor {
    fn read(mem: &dyn GuestMemoryMut, gpa: u64) -> Option<Self> {
        let mut bytes = [0u8; 16];
        if !mem.read_into(gpa, &mut bytes) {
            return None;
        }
        Some(Self {
            addr: u64::from_le_bytes(bytes[0..8].try_into().unwrap()),
            len: u32::from_le_bytes(bytes[8..12].try_into().unwrap()),
            flags: u16::from_le_bytes(bytes[12..14].try_into().unwrap()),
            next: u16::from_le_bytes(bytes[14..16].try_into().unwrap()),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Control {
    id: u32,
    event: u16,
    value: u16,
}

impl Control {
    const fn new(id: u32, event: u16, value: u16) -> Self {
        Self { id, event, value }
    }

    fn parse(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < CONTROL_LEN {
            return None;
        }
        Some(Self {
            id: u32::from_le_bytes(bytes[0..4].try_into().ok()?),
            event: u16::from_le_bytes(bytes[4..6].try_into().ok()?),
            value: u16::from_le_bytes(bytes[6..8].try_into().ok()?),
        })
    }

    fn bytes(self) -> [u8; CONTROL_LEN] {
        let mut out = [0u8; CONTROL_LEN];
        out[0..4].copy_from_slice(&self.id.to_le_bytes());
        out[4..6].copy_from_slice(&self.event.to_le_bytes());
        out[6..8].copy_from_slice(&self.value.to_le_bytes());
        out
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PendingControlMessage {
    len: usize,
    bytes: [u8; MAX_CONTROL_MESSAGE_LEN],
}

impl PendingControlMessage {
    fn from_slice(bytes: &[u8]) -> Self {
        assert!(bytes.len() <= MAX_CONTROL_MESSAGE_LEN);
        let mut out = [0u8; MAX_CONTROL_MESSAGE_LEN];
        out[..bytes.len()].copy_from_slice(bytes);
        Self {
            len: bytes.len(),
            bytes: out,
        }
    }

    fn agent_port_name() -> Self {
        let mut out = [0u8; MAX_CONTROL_MESSAGE_LEN];
        out[..CONTROL_LEN]
            .copy_from_slice(&Control::new(AGENT_PORT_ID, VIRTIO_CONSOLE_PORT_NAME, 0).bytes());
        out[CONTROL_LEN..MAX_CONTROL_MESSAGE_LEN].copy_from_slice(AGENT_PORT_NAME);
        Self {
            len: MAX_CONTROL_MESSAGE_LEN,
            bytes: out,
        }
    }

    fn as_slice(&self) -> &[u8] {
        &self.bytes[..self.len]
    }

    fn len(&self) -> usize {
        self.len
    }
}

impl From<Control> for PendingControlMessage {
    fn from(control: Control) -> Self {
        Self::from_slice(&control.bytes())
    }
}

fn set_low(current: u64, value: u64) -> u64 {
    (current & !0xffff_ffff) | (value & 0xffff_ffff)
}

fn set_high(current: u64, value: u64) -> u64 {
    (current & 0xffff_ffff) | ((value & 0xffff_ffff) << 32)
}

fn offered_features_word(select: u32) -> u32 {
    match select {
        0 => VIRTIO_CONSOLE_F_MULTIPORT,
        1 => VIRTIO_F_VERSION_1,
        _ => 0,
    }
}

fn is_supported_common_access_size(size: u8) -> bool {
    matches!(size, 1 | 2 | 4 | 8)
}

fn common_access_touches(base: u64, width: u8, offset: u64, size: u8) -> bool {
    let access_end = offset.saturating_add(u64::from(size));
    let field_end = base + u64::from(width);
    offset < field_end && base < access_end
}

fn common_access_touches_queue_field(offset: u64, size: u8) -> bool {
    [
        (COMMON_QUEUE_SIZE, 2),
        (COMMON_QUEUE_MSIX_VECTOR, 2),
        (COMMON_QUEUE_ENABLE, 2),
        (COMMON_QUEUE_DESC, 8),
        (COMMON_QUEUE_DRIVER, 8),
        (COMMON_QUEUE_DEVICE, 8),
    ]
    .iter()
    .any(|(base, width)| common_access_touches(*base, *width, offset, size))
}

fn read_common_register(base: u64, width: u8, value: u64, offset: u64, size: u8) -> Option<u64> {
    if !common_access_touches(base, width, offset, size) {
        return None;
    }
    let mut out = 0u64;
    for access_byte in 0..size {
        let byte_offset = offset + u64::from(access_byte);
        if byte_offset < base || byte_offset >= base + u64::from(width) {
            continue;
        }
        let field_byte = byte_offset - base;
        let byte = (value >> (field_byte * 8)) & 0xff;
        out |= byte << (u64::from(access_byte) * 8);
    }
    Some(mask_to_size(out, size))
}

fn write_common_register(
    current: u64,
    base: u64,
    width: u8,
    offset: u64,
    size: u8,
    value: u64,
) -> u64 {
    let mut out = current;
    for access_byte in 0..size {
        let byte_offset = offset + u64::from(access_byte);
        if byte_offset < base || byte_offset >= base + u64::from(width) {
            continue;
        }
        let field_byte = byte_offset - base;
        let shift = field_byte * 8;
        let byte = (value >> (u64::from(access_byte) * 8)) & 0xff;
        out = (out & !(0xff << shift)) | (byte << shift);
    }
    let bits = u64::from(width) * 8;
    if bits == 64 {
        out
    } else {
        out & ((1u64 << bits) - 1)
    }
}

fn mask_to_size(value: u64, size: u8) -> u64 {
    match size {
        1 => value & 0xff,
        2 => value & 0xffff,
        4 => value & 0xffff_ffff,
        _ => value,
    }
}

fn read_le_from_bytes(bytes: &[u8], offset: u64, size: u8) -> Option<u64> {
    let offset = usize::try_from(offset).ok()?;
    let size = usize::from(size);
    if offset.checked_add(size)? > bytes.len() || size > 8 {
        return None;
    }
    let mut buf = [0u8; 8];
    buf[..size].copy_from_slice(&bytes[offset..offset + size]);
    Some(u64::from_le_bytes(buf))
}

fn insert_u32(current: u32, offset: u64, size: u8, value: u64) -> u32 {
    let shift = u32::try_from(offset).unwrap_or(0) * 8;
    let width_mask: u32 = match size {
        1 => 0xff,
        2 => 0xffff,
        4 => 0xffff_ffff,
        _ => 0xffff_ffff,
    };
    let field_mask = width_mask.checked_shl(shift).unwrap_or(0);
    let placed = ((value as u32) & width_mask)
        .checked_shl(shift)
        .unwrap_or(0);
    (current & !field_mask) | placed
}

fn read_u16(mem: &dyn GuestMemoryMut, gpa: u64) -> Option<u16> {
    let mut bytes = [0u8; 2];
    if !mem.read_into(gpa, &mut bytes) {
        return None;
    }
    Some(u16::from_le_bytes(bytes))
}

/// Whether the env-gated control-plane trace is on. Read once; when off the
/// per-event trace sites collapse to a single cached bool check.
fn console_trace_enabled() -> bool {
    use std::sync::OnceLock;
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        matches!(
            std::env::var("BRIDGEVM_VIRTIO_CONSOLE_TRACE")
                .as_deref()
                .map(str::trim),
            Ok("1" | "true" | "TRUE" | "yes" | "YES" | "on" | "ON")
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct TestMem {
        base: u64,
        bytes: Vec<u8>,
    }

    impl TestMem {
        fn new(base: u64, len: usize) -> Self {
            Self {
                base,
                bytes: vec![0; len],
            }
        }

        fn write(&mut self, gpa: u64, data: &[u8]) {
            assert!(self.write_bytes(gpa, data));
        }

        fn read(&self, gpa: u64, len: usize) -> Vec<u8> {
            self.read_bytes(gpa, len).unwrap()
        }
    }

    impl GuestMemoryMut for TestMem {
        fn write_bytes(&mut self, gpa: u64, data: &[u8]) -> bool {
            let Some(off) = gpa.checked_sub(self.base).map(|v| v as usize) else {
                return false;
            };
            let Some(end) = off.checked_add(data.len()) else {
                return false;
            };
            if end > self.bytes.len() {
                return false;
            }
            self.bytes[off..end].copy_from_slice(data);
            true
        }

        fn read_bytes(&self, gpa: u64, len: usize) -> Option<Vec<u8>> {
            let off = gpa.checked_sub(self.base)? as usize;
            let end = off.checked_add(len)?;
            (end <= self.bytes.len()).then(|| self.bytes[off..end].to_vec())
        }

        fn read_into(&self, gpa: u64, dst: &mut [u8]) -> bool {
            let Some(off) = gpa.checked_sub(self.base).map(|v| v as usize) else {
                return false;
            };
            let Some(end) = off.checked_add(dst.len()) else {
                return false;
            };
            if end > self.bytes.len() {
                return false;
            }
            dst.copy_from_slice(&self.bytes[off..end]);
            true
        }
    }

    fn pci_write(dev: &mut VirtioPciConsole, offset: u64, size: u8, value: u64, mem: &mut TestMem) {
        assert_eq!(
            dev.access(offset, VirtioPciConsoleOp::Write { size, value }, mem),
            VirtioConsoleResult::WriteAck
        );
    }

    fn pci_read(dev: &mut VirtioPciConsole, offset: u64, size: u8, mem: &mut TestMem) -> u64 {
        match dev.access(offset, VirtioPciConsoleOp::Read { size }, mem) {
            VirtioConsoleResult::ReadValue(value) => value,
            VirtioConsoleResult::WriteAck => panic!("read returned write ack"),
        }
    }

    fn write_desc(
        mem: &mut TestMem,
        table: u64,
        index: u16,
        addr: u64,
        len: u32,
        flags: u16,
        next: u16,
    ) {
        let gpa = table + u64::from(index) * DESC_SIZE;
        mem.write(gpa, &addr.to_le_bytes());
        mem.write(gpa + 8, &len.to_le_bytes());
        mem.write(gpa + 12, &flags.to_le_bytes());
        mem.write(gpa + 14, &next.to_le_bytes());
    }

    fn setup_queue(
        dev: &mut VirtioPciConsole,
        mem: &mut TestMem,
        queue: u16,
        desc: u64,
        avail: u64,
        used: u64,
        vector: u16,
    ) {
        pci_write(dev, COMMON_QUEUE_SELECT, 2, u64::from(queue), mem);
        pci_write(dev, COMMON_QUEUE_SIZE, 2, 8, mem);
        pci_write(dev, COMMON_QUEUE_DESC, 8, desc, mem);
        pci_write(dev, COMMON_QUEUE_DRIVER, 8, avail, mem);
        pci_write(dev, COMMON_QUEUE_DEVICE, 8, used, mem);
        pci_write(dev, COMMON_QUEUE_MSIX_VECTOR, 2, u64::from(vector), mem);
        pci_write(dev, COMMON_QUEUE_ENABLE, 2, 1, mem);
    }

    fn program_msix_vector(dev: &mut VirtioPciConsole, vector: u16, address: u64, data: u32) {
        let off = u64::from(VIRTIO_CONSOLE_MSIX_TABLE_OFFSET) + u64::from(vector) * 16;
        assert_eq!(
            dev.msix_bar_access(
                off,
                VirtioPciConsoleOp::Write {
                    size: 8,
                    value: address,
                },
            ),
            VirtioConsoleResult::WriteAck
        );
        assert_eq!(
            dev.msix_bar_access(
                off + 8,
                VirtioPciConsoleOp::Write {
                    size: 4,
                    value: u64::from(data),
                },
            ),
            VirtioConsoleResult::WriteAck
        );
        assert_eq!(
            dev.msix_bar_access(off + 12, VirtioPciConsoleOp::Write { size: 4, value: 0 },),
            VirtioConsoleResult::WriteAck
        );
    }

    fn post_rx(mem: &mut TestMem, desc: u64, avail: u64, data: u64, len: u32, idx: u16) {
        write_desc(mem, desc, idx, data, len, DESC_F_WRITE, 0);
        mem.write(avail + 2, &idx.wrapping_add(1).to_le_bytes());
        mem.write(avail + 4 + u64::from(idx) * 2, &idx.to_le_bytes());
    }

    fn send_tx(
        dev: &mut VirtioPciConsole,
        mem: &mut TestMem,
        queue: u16,
        desc: u64,
        avail: u64,
        data_addr: u64,
        data: &[u8],
        idx: u16,
    ) {
        mem.write(data_addr, data);
        write_desc(mem, desc, idx, data_addr, data.len() as u32, 0, 0);
        mem.write(avail + 2, &idx.wrapping_add(1).to_le_bytes());
        mem.write(avail + 4 + u64::from(idx) * 2, &idx.to_le_bytes());
        pci_write(dev, PCI_NOTIFY_CFG_OFFSET + u64::from(queue) * 4, 4, 0, mem);
    }

    fn control_bytes(id: u32, event: u16, value: u16) -> [u8; 8] {
        Control::new(id, event, value).bytes()
    }

    #[test]
    fn feature_negotiation_advertises_version_1_and_multiport_and_masks_driver_bits() {
        let mut dev = VirtioPciConsole::new();
        let mut mem = TestMem::new(0x4000_0000, 0x1000);

        pci_write(&mut dev, COMMON_DEVICE_FEATURE_SELECT, 4, 0, &mut mem);
        assert_eq!(
            pci_read(&mut dev, COMMON_DEVICE_FEATURE, 4, &mut mem),
            u64::from(VIRTIO_CONSOLE_F_MULTIPORT)
        );
        pci_write(&mut dev, COMMON_DEVICE_FEATURE_SELECT, 4, 1, &mut mem);
        assert_eq!(
            pci_read(&mut dev, COMMON_DEVICE_FEATURE, 4, &mut mem),
            u64::from(VIRTIO_F_VERSION_1)
        );
        pci_write(&mut dev, COMMON_DRIVER_FEATURE_SELECT, 4, 0, &mut mem);
        pci_write(&mut dev, COMMON_DRIVER_FEATURE, 4, 0xffff_ffff, &mut mem);
        pci_write(&mut dev, COMMON_DRIVER_FEATURE_SELECT, 4, 1, &mut mem);
        pci_write(&mut dev, COMMON_DRIVER_FEATURE, 4, 0xffff_ffff, &mut mem);

        assert_eq!(
            dev.stats().driver_features,
            u64::from(VIRTIO_CONSOLE_F_MULTIPORT) | (u64::from(VIRTIO_F_VERSION_1) << 32)
        );
        assert_eq!(pci_read(&mut dev, COMMON_NUM_QUEUES, 2, &mut mem), 6);
        assert_eq!(
            pci_read(&mut dev, PCI_DEVICE_CFG_OFFSET + 4, 4, &mut mem),
            2
        );
    }

    #[test]
    fn full_control_handshake_emits_add_name_and_host_open() {
        let mut dev = VirtioPciConsole::new();
        let mut mem = TestMem::new(0x4000_0000, 0x40000);
        let crx_desc = 0x4000_1000;
        let crx_avail = 0x4000_2000;
        let crx_used = 0x4000_3000;
        let ctx_desc = 0x4000_4000;
        let ctx_avail = 0x4000_5000;
        let ctx_used = 0x4000_6000;
        let out0 = 0x4000_7000;
        let out1 = 0x4000_7100;
        let out2 = 0x4000_7200;
        let out3 = 0x4000_7300;

        setup_queue(&mut dev, &mut mem, 2, crx_desc, crx_avail, crx_used, 2);
        setup_queue(&mut dev, &mut mem, 3, ctx_desc, ctx_avail, ctx_used, 3);
        for (idx, out) in [out0, out1, out2, out3].into_iter().enumerate() {
            post_rx(&mut mem, crx_desc, crx_avail, out, 64, idx as u16);
        }
        send_tx(
            &mut dev,
            &mut mem,
            3,
            ctx_desc,
            ctx_avail,
            0x4000_8000,
            &control_bytes(0, VIRTIO_CONSOLE_DEVICE_READY, 1),
            0,
        );

        assert_eq!(
            mem.read(out0, 8),
            control_bytes(0, VIRTIO_CONSOLE_DEVICE_ADD, 0)
        );
        assert_eq!(
            mem.read(out1, 8),
            control_bytes(1, VIRTIO_CONSOLE_DEVICE_ADD, 0)
        );
        send_tx(
            &mut dev,
            &mut mem,
            3,
            ctx_desc,
            ctx_avail,
            0x4000_8100,
            &control_bytes(1, VIRTIO_CONSOLE_PORT_READY, 1),
            1,
        );
        let mut expected_name = control_bytes(1, VIRTIO_CONSOLE_PORT_NAME, 0).to_vec();
        expected_name.extend_from_slice(AGENT_PORT_NAME);
        assert_eq!(mem.read(out2, expected_name.len()), expected_name);
        assert_eq!(
            u32::from_le_bytes(mem.read(crx_used + 4 + 2 * 8 + 4, 4).try_into().unwrap()),
            expected_name.len() as u32
        );
        assert_eq!(
            mem.read(out3, 8),
            control_bytes(1, VIRTIO_CONSOLE_PORT_OPEN, 1)
        );
        assert!(dev.stats().queues[2].pending_msix);
        assert!(dev.stats().queues[3].pending_msix);
    }

    #[test]
    fn vioser_sequence_resends_host_open_before_agent_tx_without_guest_open_control() {
        let mut dev = VirtioPciConsole::new();
        let mut mem = TestMem::new(0x4000_0000, 0x70000);
        let crx_desc = 0x4000_1000;
        let crx_avail = 0x4000_2000;
        let crx_used = 0x4000_3000;
        let ctx_desc = 0x4000_4000;
        let ctx_avail = 0x4000_5000;
        let ctx_used = 0x4000_6000;
        let atx_desc = 0x4000_7000;
        let atx_avail = 0x4000_8000;
        let atx_used = 0x4000_9000;

        setup_queue(&mut dev, &mut mem, 2, crx_desc, crx_avail, crx_used, 2);
        setup_queue(&mut dev, &mut mem, 3, ctx_desc, ctx_avail, ctx_used, 3);
        setup_queue(&mut dev, &mut mem, 5, atx_desc, atx_avail, atx_used, 5);
        for (idx, out) in [
            0x4000_a000,
            0x4000_a100,
            0x4000_a200,
            0x4000_a300,
            0x4000_a400,
            0x4000_a500,
        ]
        .into_iter()
        .enumerate()
        {
            post_rx(&mut mem, crx_desc, crx_avail, out, 64, idx as u16);
        }

        send_tx(
            &mut dev,
            &mut mem,
            3,
            ctx_desc,
            ctx_avail,
            0x4000_b000,
            &control_bytes(0xffff_ffff, VIRTIO_CONSOLE_DEVICE_READY, 1),
            0,
        );
        send_tx(
            &mut dev,
            &mut mem,
            3,
            ctx_desc,
            ctx_avail,
            0x4000_b100,
            &control_bytes(0, VIRTIO_CONSOLE_PORT_READY, 1),
            1,
        );
        send_tx(
            &mut dev,
            &mut mem,
            3,
            ctx_desc,
            ctx_avail,
            0x4000_b200,
            &control_bytes(1, VIRTIO_CONSOLE_PORT_READY, 1),
            2,
        );

        assert!(!dev.stats().port1_guest_open);
        let mut expected_name = control_bytes(1, VIRTIO_CONSOLE_PORT_NAME, 0).to_vec();
        expected_name.extend_from_slice(AGENT_PORT_NAME);
        assert_eq!(mem.read(0x4000_a200, expected_name.len()), expected_name);
        assert_eq!(
            u32::from_le_bytes(mem.read(crx_used + 4 + 2 * 8 + 4, 4).try_into().unwrap()),
            expected_name.len() as u32
        );
        assert_eq!(
            mem.read(0x4000_a300, 8),
            control_bytes(1, VIRTIO_CONSOLE_PORT_OPEN, 1)
        );

        // The host retry heartbeat (any host->guest send) re-asserts PORT_OPEN
        // while the link is still unconfirmed. A bare control-RX rearm must NOT
        // (that path storms; see control_rx_notifies_alone_never_reassert...).
        pci_write(
            &mut dev,
            PCI_NOTIFY_CFG_OFFSET + u64::from(QUEUE_CONTROL_RX as u16) * 4,
            4,
            0,
            &mut mem,
        );
        assert_eq!(
            mem.read(0x4000_a400, 8),
            [0u8; 8],
            "a control-RX notify alone must not synthesize a PORT_OPEN"
        );
        dev.agent_send(b"PING");
        dev.poll(&mut mem);
        // The retry heartbeat re-sends PORT_NAME (in case vioser dropped the
        // first) followed by PORT_OPEN, so the next two RX buffers carry the pair.
        let mut expected_name = control_bytes(1, VIRTIO_CONSOLE_PORT_NAME, 0).to_vec();
        expected_name.extend_from_slice(AGENT_PORT_NAME);
        assert_eq!(mem.read(0x4000_a400, expected_name.len()), expected_name);
        assert_eq!(
            mem.read(0x4000_a500, 8),
            control_bytes(1, VIRTIO_CONSOLE_PORT_OPEN, 1)
        );

        send_tx(
            &mut dev,
            &mut mem,
            5,
            atx_desc,
            atx_avail,
            0x4000_c000,
            b"READY",
            0,
        );
        assert_eq!(dev.take_inbound(), b"READY");
    }

    #[test]
    fn data_loopback_after_port_open() {
        let mut dev = VirtioPciConsole::new();
        let mut mem = TestMem::new(0x4000_0000, 0x50000);
        setup_queue(
            &mut dev,
            &mut mem,
            4,
            0x4000_1000,
            0x4000_2000,
            0x4000_3000,
            4,
        );
        setup_queue(
            &mut dev,
            &mut mem,
            5,
            0x4000_4000,
            0x4000_5000,
            0x4000_6000,
            5,
        );
        dev.console.ports[1].guest_open = true;

        post_rx(&mut mem, 0x4000_1000, 0x4000_2000, 0x4000_7000, 16, 0);
        dev.agent_send(b"ping");
        assert!(dev.poll(&mut mem));
        assert_eq!(mem.read(0x4000_7000, 4), b"ping");

        send_tx(
            &mut dev,
            &mut mem,
            5,
            0x4000_4000,
            0x4000_5000,
            0x4000_8000,
            b"pong",
            0,
        );
        assert_eq!(dev.take_inbound(), b"pong");
    }

    #[test]
    fn drain_inbound_into_preserves_buffers() {
        let mut dev = VirtioPciConsole::new();
        dev.console.host_inbound.reserve(64);
        dev.console.host_inbound.extend_from_slice(b"READY\nPONG\n");
        let internal_capacity = dev.console.host_inbound.capacity();

        let mut out = Vec::with_capacity(32);
        let out_capacity = out.capacity();
        out.extend_from_slice(b"prefix:");
        dev.drain_inbound_into(&mut out);

        assert_eq!(out, b"prefix:READY\nPONG\n");
        assert_eq!(dev.console.host_inbound.len(), 0);
        assert_eq!(dev.console.host_inbound.capacity(), internal_capacity);
        assert_eq!(out.capacity(), out_capacity);

        dev.drain_inbound_into(&mut out);
        assert_eq!(out, b"prefix:READY\nPONG\n");
        assert_eq!(dev.console.host_inbound.capacity(), internal_capacity);
    }

    #[test]
    fn tx_queues_reuse_descriptor_and_read_scratch_across_messages() {
        let mut dev = VirtioPciConsole::new();
        let mut mem = TestMem::new(0x4000_0000, 0x70000);
        let ctx_desc = 0x4000_1000;
        let ctx_avail = 0x4000_2000;
        let ctx_used = 0x4000_3000;
        let atx_desc = 0x4000_4000;
        let atx_avail = 0x4000_5000;
        let atx_used = 0x4000_6000;

        setup_queue(&mut dev, &mut mem, 3, ctx_desc, ctx_avail, ctx_used, 3);
        setup_queue(&mut dev, &mut mem, 5, atx_desc, atx_avail, atx_used, 5);

        send_tx(
            &mut dev,
            &mut mem,
            3,
            ctx_desc,
            ctx_avail,
            0x4000_7000,
            &control_bytes(1, VIRTIO_CONSOLE_PORT_OPEN, 1),
            0,
        );

        let desc_cap = dev.console.descriptor_scratch.capacity();
        let desc_ptr = dev.console.descriptor_scratch.as_ptr();
        let read_cap = dev.console.read_scratch.capacity();
        let read_ptr = dev.console.read_scratch.as_ptr();
        assert!(desc_cap >= 1);
        assert!(read_cap >= CONTROL_LEN);

        send_tx(
            &mut dev,
            &mut mem,
            5,
            atx_desc,
            atx_avail,
            0x4000_7100,
            b"READY",
            0,
        );

        assert_eq!(dev.take_inbound(), b"READY");
        assert_eq!(dev.console.descriptor_scratch.capacity(), desc_cap);
        assert_eq!(dev.console.descriptor_scratch.as_ptr(), desc_ptr);
        assert_eq!(dev.console.read_scratch.capacity(), read_cap);
        assert_eq!(dev.console.read_scratch.as_ptr(), read_ptr);
    }

    #[test]
    fn tx_chain_rejects_oversized_guest_length_before_growing_scratch() {
        let mut mem = TestMem::new(0x4000_0000, 0x1000);
        let desc_table = 0x4000_0100;
        write_desc(&mut mem, desc_table, 0, 0x4000_0800, u32::MAX, 0, 0);
        let mut queue = VirtioConsoleQueue::new(0);
        queue.size = 1;
        queue.desc = desc_table;
        let mut descs = Vec::new();
        let mut bytes = Vec::with_capacity(32);
        let capacity = bytes.capacity();

        assert!(!VirtioConsole::read_chain_into(
            &mem,
            &queue,
            0,
            &mut descs,
            &mut bytes,
            MAX_AGENT_MESSAGE_LEN,
        ));
        assert!(bytes.is_empty());
        assert_eq!(bytes.capacity(), capacity);
    }

    #[test]
    fn agent_rx_delivers_wrapped_host_queue_and_reuses_descriptor_scratch() {
        let mut dev = VirtioPciConsole::new();
        let mut mem = TestMem::new(0x4000_0000, 0x50000);
        let arx_desc = 0x4000_1000;
        let arx_avail = 0x4000_2000;
        let arx_used = 0x4000_3000;
        let out0 = 0x4000_4000;
        let out1 = 0x4000_4100;

        setup_queue(&mut dev, &mut mem, 4, arx_desc, arx_avail, arx_used, 4);
        post_rx(&mut mem, arx_desc, arx_avail, out0, 16, 0);
        let mut wrapped = VecDeque::with_capacity(8);
        wrapped.extend(b"ABCDEFGH".iter().copied());
        for _ in 0..6 {
            assert!(wrapped.pop_front().is_some());
        }
        wrapped.extend(b"IJKL".iter().copied());
        assert_eq!(wrapped.iter().copied().collect::<Vec<_>>(), b"GHIJKL");
        assert!(
            !wrapped.as_slices().0.is_empty() && !wrapped.as_slices().1.is_empty(),
            "test setup must exercise VecDeque's split-slice layout"
        );
        dev.console.host_to_guest = wrapped;

        assert!(dev.poll(&mut mem));
        assert_eq!(mem.read(out0, 6), b"GHIJKL");
        assert_eq!(dev.stats().host_to_guest_len, 0);

        let desc_cap = dev.console.descriptor_scratch.capacity();
        let desc_ptr = dev.console.descriptor_scratch.as_ptr();
        assert!(desc_cap >= 1);

        post_rx(&mut mem, arx_desc, arx_avail, out1, 16, 1);
        dev.agent_send(b"pong");

        assert!(dev.poll(&mut mem));
        assert_eq!(mem.read(out1, 4), b"pong");
        assert_eq!(dev.console.descriptor_scratch.capacity(), desc_cap);
        assert_eq!(dev.console.descriptor_scratch.as_ptr(), desc_ptr);
    }

    #[test]
    fn agent_rx_pending_msix_survives_until_table_entry_is_programmed() {
        let mut dev = VirtioPciConsole::new();
        let mut mem = TestMem::new(0x4000_0000, 0x50000);
        let arx_desc = 0x4000_1000;
        let arx_avail = 0x4000_2000;
        let arx_used = 0x4000_3000;
        let out = 0x4000_4000;

        setup_queue(&mut dev, &mut mem, 4, arx_desc, arx_avail, arx_used, 4);
        post_rx(&mut mem, arx_desc, arx_avail, out, 16, 0);
        dev.agent_send(b"PING");

        assert!(dev.poll(&mut mem));
        assert_eq!(mem.read(out, 4), b"PING");
        assert!(dev.stats().queues[4].pending_msix);
        assert_eq!(dev.drain_pending_msix(true, false), Vec::new());
        assert!(dev.stats().queues[4].pending_msix);

        program_msix_vector(&mut dev, 4, 0xfee0_0000, 0x54);

        assert_eq!(
            dev.drain_pending_msix(true, false),
            vec![MsixMessage {
                vector: 4,
                address: 0xfee0_0000,
                data: 0x54,
            }]
        );
        assert!(!dev.stats().queues[4].pending_msix);
    }

    #[test]
    fn control_backpressure_queues_until_rx_buffer_is_posted() {
        let mut dev = VirtioPciConsole::new();
        let mut mem = TestMem::new(0x4000_0000, 0x30000);
        setup_queue(
            &mut dev,
            &mut mem,
            2,
            0x4000_1000,
            0x4000_2000,
            0x4000_3000,
            2,
        );
        setup_queue(
            &mut dev,
            &mut mem,
            3,
            0x4000_4000,
            0x4000_5000,
            0x4000_6000,
            3,
        );

        send_tx(
            &mut dev,
            &mut mem,
            3,
            0x4000_4000,
            0x4000_5000,
            0x4000_8000,
            &control_bytes(0, VIRTIO_CONSOLE_DEVICE_READY, 1),
            0,
        );
        assert_eq!(dev.stats().pending_control, 2);

        post_rx(&mut mem, 0x4000_1000, 0x4000_2000, 0x4000_7000, 16, 0);
        assert!(dev.poll(&mut mem));
        assert_eq!(
            mem.read(0x4000_7000, 8),
            control_bytes(0, VIRTIO_CONSOLE_DEVICE_ADD, 0)
        );
        assert_eq!(dev.stats().pending_control, 1);
    }

    #[test]
    fn reset_clears_port_state_and_pending_buffers() {
        let mut dev = VirtioPciConsole::new();
        dev.console.ports[1].ready = true;
        dev.console.ports[1].guest_open = true;
        dev.console
            .pending_control
            .push_back(PendingControlMessage::from_slice(&[1, 2, 3]));
        dev.agent_send(b"ping");
        dev.console.host_inbound.extend_from_slice(b"pong");

        dev.reset_runtime_state();

        let stats = dev.stats();
        assert!(!stats.port1_ready);
        assert!(!stats.port1_guest_open);
        assert_eq!(stats.pending_control, 0);
        assert_eq!(stats.host_to_guest_len, 0);
        assert_eq!(stats.host_inbound_len, 0);
    }

    // ---- Faithful guest (vioser) model ---------------------------------
    //
    // These tests replay the control messages our device delivers on the
    // control-RX queue through a small state machine that mirrors the parts of
    // vioser that gate the data plane: VIOSerialFindPortById (a port is only
    // resolvable after PORT_ADD created it), the PORT_NAME name handler, and
    // the PORT_OPEN -> HostConnected latch. Modeling the real driver contract
    // (rather than poking device internals) is what catches the netkvm-class
    // bug where bytes move but the driver-visible state never advances.

    #[derive(Default)]
    struct GuestModel {
        present: std::collections::BTreeSet<u32>,
        named: std::collections::BTreeSet<u32>,
        host_conn: std::collections::BTreeMap<u32, bool>,
    }

    impl GuestModel {
        fn apply(&mut self, raw: &[u8]) {
            let control = Control::parse(raw).expect("control message");
            match control.event {
                VIRTIO_CONSOLE_DEVICE_ADD => {
                    // vioser's PORT_ADD handler: the port becomes findable.
                    self.present.insert(control.id);
                    self.host_conn.entry(control.id).or_insert(false);
                }
                VIRTIO_CONSOLE_PORT_NAME => {
                    // VIOSerialPortCreateName only runs if the port resolves.
                    if self.present.contains(&control.id) {
                        self.named.insert(control.id);
                    }
                }
                VIRTIO_CONSOLE_PORT_OPEN => {
                    // VIOSerialHandleCtrlMsg PORT_OPEN: only latches when the
                    // port resolves; value drives HostConnected.
                    if self.present.contains(&control.id) {
                        self.host_conn.insert(control.id, control.value != 0);
                    }
                }
                _ => {}
            }
        }

        fn host_connected(&self, id: u32) -> bool {
            self.host_conn.get(&id).copied().unwrap_or(false)
        }
    }

    fn setup_queue_sized(
        dev: &mut VirtioPciConsole,
        mem: &mut TestMem,
        queue: u16,
        desc: u64,
        avail: u64,
        used: u64,
        vector: u16,
        size: u16,
    ) {
        pci_write(dev, COMMON_QUEUE_SELECT, 2, u64::from(queue), mem);
        pci_write(dev, COMMON_QUEUE_SIZE, 2, u64::from(size), mem);
        pci_write(dev, COMMON_QUEUE_DESC, 8, desc, mem);
        pci_write(dev, COMMON_QUEUE_DRIVER, 8, avail, mem);
        pci_write(dev, COMMON_QUEUE_DEVICE, 8, used, mem);
        pci_write(dev, COMMON_QUEUE_MSIX_VECTOR, 2, u64::from(vector), mem);
        pci_write(dev, COMMON_QUEUE_ENABLE, 2, 1, mem);
    }

    fn post_control_rx(mem: &mut TestMem, desc: u64, avail: u64, data: u64, size: u16, n: u16) {
        let slot = n % size;
        write_desc(mem, desc, slot, data, 64, DESC_F_WRITE, 0);
        mem.write(avail + 4 + u64::from(slot) * 2, &slot.to_le_bytes());
        mem.write(avail + 2, &n.wrapping_add(1).to_le_bytes());
    }

    /// Read every control-RX buffer the device has newly published, returning
    /// the raw message bytes in order and advancing `seen`.
    fn drain_control_rx(
        mem: &TestMem,
        desc: u64,
        used: u64,
        size: u16,
        seen: &mut u16,
    ) -> Vec<Vec<u8>> {
        let used_idx = u16::from_le_bytes(mem.read(used + 2, 2).try_into().unwrap());
        let mut messages = Vec::new();
        while *seen != used_idx {
            let slot = u64::from(*seen % size);
            let entry = used + 4 + slot * 8;
            let head = u32::from_le_bytes(mem.read(entry, 4).try_into().unwrap());
            let len = u32::from_le_bytes(mem.read(entry + 4, 4).try_into().unwrap()) as usize;
            let addr = u64::from_le_bytes(
                mem.read(desc + u64::from(head) * DESC_SIZE, 8)
                    .try_into()
                    .unwrap(),
            );
            messages.push(mem.read(addr, len));
            *seen = seen.wrapping_add(1);
        }
        messages
    }

    fn guest_control(
        dev: &mut VirtioPciConsole,
        mem: &mut TestMem,
        data_addr: u64,
        message: &[u8],
        idx: u16,
    ) {
        send_tx(
            dev,
            mem,
            3,
            0x4000_4000,
            0x4000_5000,
            data_addr,
            message,
            idx,
        );
    }

    #[test]
    fn guest_model_latches_host_connected_and_relatches_after_d0_bounce() {
        let mut dev = VirtioPciConsole::new();
        let mut mem = TestMem::new(0x4000_0000, 0x100000);
        let size: u16 = 32;
        setup_queue_sized(
            &mut dev,
            &mut mem,
            2,
            0x4000_1000,
            0x4000_2000,
            0x4000_3000,
            2,
            size,
        );
        setup_queue_sized(
            &mut dev,
            &mut mem,
            3,
            0x4000_4000,
            0x4000_5000,
            0x4000_6000,
            3,
            size,
        );
        for n in 0..24u16 {
            post_control_rx(
                &mut mem,
                0x4000_1000,
                0x4000_2000,
                0x4004_0000 + u64::from(n) * 0x100,
                size,
                n,
            );
        }
        let mut seen = 0u16;
        let mut guest = GuestModel::default();

        // Boot handshake exactly as vioser emits it: DEVICE_READY(BAD_ID),
        // then a PORT_READY per port as each PDO enters D0.
        guest_control(
            &mut dev,
            &mut mem,
            0x4005_0000,
            &control_bytes(0xffff_ffff, VIRTIO_CONSOLE_DEVICE_READY, 1),
            0,
        );
        guest_control(
            &mut dev,
            &mut mem,
            0x4005_0100,
            &control_bytes(0, VIRTIO_CONSOLE_PORT_READY, 1),
            1,
        );
        guest_control(
            &mut dev,
            &mut mem,
            0x4005_0200,
            &control_bytes(1, VIRTIO_CONSOLE_PORT_READY, 1),
            2,
        );
        for message in drain_control_rx(&mem, 0x4000_1000, 0x4000_3000, size, &mut seen) {
            guest.apply(&message);
        }

        assert!(guest.present.contains(&1), "PORT_ADD must create port 1");
        assert!(guest.named.contains(&1), "PORT_NAME must resolve port 1");
        assert!(
            guest.host_connected(1),
            "PORT_OPEN must latch HostConnected in the same drain PORT_NAME resolved"
        );
        assert!(!guest.host_connected(0), "host never opens port 0");

        // A PnP resource rebalance / D0 bounce: vioser clears HostConnected in
        // VIOSerialPortEvtDeviceD0Exit, then re-enters D0 and re-emits
        // PORT_READY(1). The device must re-assert PORT_OPEN so the link heals.
        guest.host_conn.insert(1, false);
        guest_control(
            &mut dev,
            &mut mem,
            0x4005_0300,
            &control_bytes(1, VIRTIO_CONSOLE_PORT_READY, 1),
            3,
        );
        for message in drain_control_rx(&mem, 0x4000_1000, 0x4000_3000, size, &mut seen) {
            guest.apply(&message);
        }
        assert!(
            guest.host_connected(1),
            "a fresh PORT_READY after a D0 bounce must re-latch HostConnected"
        );
    }

    #[test]
    fn host_open_reassert_is_sustained_until_agent_tx_then_stops() {
        let mut dev = VirtioPciConsole::new();
        let mut mem = TestMem::new(0x4000_0000, 0x100000);
        let size: u16 = 32;
        setup_queue_sized(
            &mut dev,
            &mut mem,
            2,
            0x4000_1000,
            0x4000_2000,
            0x4000_3000,
            2,
            size,
        );
        setup_queue_sized(
            &mut dev,
            &mut mem,
            3,
            0x4000_4000,
            0x4000_5000,
            0x4000_6000,
            3,
            size,
        );
        setup_queue_sized(
            &mut dev,
            &mut mem,
            5,
            0x4000_7000,
            0x4000_8000,
            0x4000_9000,
            5,
            size,
        );
        for n in 0..28u16 {
            post_control_rx(
                &mut mem,
                0x4000_1000,
                0x4000_2000,
                0x4004_0000 + u64::from(n) * 0x100,
                size,
                n,
            );
        }
        let mut seen = 0u16;

        guest_control(
            &mut dev,
            &mut mem,
            0x4005_0000,
            &control_bytes(0xffff_ffff, VIRTIO_CONSOLE_DEVICE_READY, 1),
            0,
        );
        guest_control(
            &mut dev,
            &mut mem,
            0x4005_0100,
            &control_bytes(1, VIRTIO_CONSOLE_PORT_READY, 1),
            1,
        );
        let _ = drain_control_rx(&mem, 0x4000_1000, 0x4000_3000, size, &mut seen);
        assert!(dev.stats().port1_host_open);
        assert!(!dev.stats().agent_connected_confirmed);

        // The host retry heartbeat (harness PINGs, or any host->guest send)
        // re-asserts PORT_OPEN while the link is still unconfirmed, so a port
        // that only stabilizes after the boot burst still latches. Exactly one
        // re-assert is in flight at a time (no control-queue flooding).
        let open = control_bytes(1, VIRTIO_CONSOLE_PORT_OPEN, 1).to_vec();
        for _ in 0..4 {
            dev.agent_send(b"PING");
            dev.poll(&mut mem);
            let delivered = drain_control_rx(&mem, 0x4000_1000, 0x4000_3000, size, &mut seen);
            assert_eq!(
                delivered
                    .iter()
                    .filter(|message| message.as_slice() == open.as_slice())
                    .count(),
                1,
                "each heartbeat re-asserts exactly one PORT_OPEN while unconfirmed"
            );
        }

        // First guest TX byte proves vioser latched HostConnected (its
        // WillWriteBlock gate blocks writes until then).
        send_tx(
            &mut dev,
            &mut mem,
            5,
            0x4000_7000,
            0x4000_8000,
            0x4006_0000,
            b"READY",
            0,
        );
        assert!(dev.stats().agent_connected_confirmed);
        assert_eq!(dev.take_inbound(), b"READY");

        // Re-assertion stops once the link is proven.
        dev.agent_send(b"PING");
        dev.poll(&mut mem);
        let delivered = drain_control_rx(&mem, 0x4000_1000, 0x4000_3000, size, &mut seen);
        assert_eq!(
            delivered
                .iter()
                .filter(|message| message.as_slice() == open.as_slice())
                .count(),
            0,
            "re-assertion stops after the agent proves the link is live"
        );
    }

    #[test]
    fn control_rx_notifies_alone_never_reassert_port_open_no_storm() {
        // Regression for the MSI-X storm: delivering a PORT_OPEN makes vioser
        // consume + refill the control-RX ring and kick control-RX. If that
        // kick re-asserts another PORT_OPEN, the cycle runs at full interrupt
        // speed and livelocks the guest. A bare control-RX notify must produce
        // no work at all so the loop cannot sustain itself.
        let mut dev = VirtioPciConsole::new();
        let mut mem = TestMem::new(0x4000_0000, 0x100000);
        let size: u16 = 32;
        setup_queue_sized(
            &mut dev,
            &mut mem,
            2,
            0x4000_1000,
            0x4000_2000,
            0x4000_3000,
            2,
            size,
        );
        setup_queue_sized(
            &mut dev,
            &mut mem,
            3,
            0x4000_4000,
            0x4000_5000,
            0x4000_6000,
            3,
            size,
        );
        for n in 0..30u16 {
            post_control_rx(
                &mut mem,
                0x4000_1000,
                0x4000_2000,
                0x4004_0000 + u64::from(n) * 0x100,
                size,
                n,
            );
        }
        let mut seen = 0u16;

        guest_control(
            &mut dev,
            &mut mem,
            0x4005_0000,
            &control_bytes(0xffff_ffff, VIRTIO_CONSOLE_DEVICE_READY, 1),
            0,
        );
        guest_control(
            &mut dev,
            &mut mem,
            0x4005_0100,
            &control_bytes(1, VIRTIO_CONSOLE_PORT_READY, 1),
            1,
        );
        let _ = drain_control_rx(&mem, 0x4000_1000, 0x4000_3000, size, &mut seen);
        assert!(dev.stats().port1_host_open);
        assert!(!dev.stats().agent_connected_confirmed);
        assert_eq!(dev.stats().pending_control, 0);

        // Replay the vioser "consumed + refilled -> kick control-RX" many times
        // with no agent TX. Each kick must manufacture nothing: no control
        // message delivered, nothing queued, so nothing to interrupt on.
        let open = control_bytes(1, VIRTIO_CONSOLE_PORT_OPEN, 1).to_vec();
        for _ in 0..64 {
            pci_write(
                &mut dev,
                PCI_NOTIFY_CFG_OFFSET + u64::from(QUEUE_CONTROL_RX as u16) * 4,
                4,
                0,
                &mut mem,
            );
            let delivered = drain_control_rx(&mem, 0x4000_1000, 0x4000_3000, size, &mut seen);
            assert!(
                delivered.is_empty(),
                "a bare control-RX notify must not deliver any control message"
            );
            assert_eq!(
                dev.stats().pending_control,
                0,
                "a bare control-RX notify must not enqueue a PORT_OPEN"
            );
        }

        // The bounded heartbeat trigger still re-asserts exactly once.
        dev.agent_send(b"PING");
        dev.poll(&mut mem);
        let delivered = drain_control_rx(&mem, 0x4000_1000, 0x4000_3000, size, &mut seen);
        assert_eq!(
            delivered
                .iter()
                .filter(|message| message.as_slice() == open.as_slice())
                .count(),
            1,
            "agent_send remains the bounded re-assert trigger"
        );
    }

    fn post_rx_buffer(
        mem: &mut TestMem,
        desc: u64,
        avail: u64,
        size: u16,
        avail_idx: u16,
        slot: u16,
        buf: u64,
        buf_len: u32,
    ) {
        write_desc(mem, desc, slot, buf, buf_len, DESC_F_WRITE, 0);
        mem.write(
            avail + 4 + u64::from(avail_idx % size) * 2,
            &slot.to_le_bytes(),
        );
        mem.write(avail + 2, &avail_idx.wrapping_add(1).to_le_bytes());
    }

    #[test]
    fn agent_rx_delivery_resumes_after_ring_consumed_and_replenished() {
        // Replenishment regression for the "stops after the first lap" wall:
        // consume a full RX ring, then have the guest re-post buffers past the
        // initial size (wrapping the ring slots) with NO doorbell, and assert
        // the periodic poll picks them up. Exercises absolute-avail-idx tracking
        // across a lap and delivery via poll rather than only via notify.
        let mut dev = VirtioPciConsole::new();
        let mut mem = TestMem::new(0x4000_0000, 0x100000);
        let size: u16 = 4;
        setup_queue_sized(
            &mut dev,
            &mut mem,
            4,
            0x4000_1000,
            0x4000_2000,
            0x4000_3000,
            4,
            size,
        );

        // Lap 1: guest fills the whole ring (avail 0..4, descriptor slots 0..4).
        for k in 0..4u16 {
            post_rx_buffer(
                &mut mem,
                0x4000_1000,
                0x4000_2000,
                size,
                k,
                k,
                0x4004_0000 + u64::from(k) * 0x100,
                64,
            );
        }

        // Consume the entire ring (one host->guest message per buffer).
        for k in 0..4u16 {
            dev.agent_send(format!("m{k}").as_bytes());
            dev.poll(&mut mem);
        }
        assert_eq!(dev.stats().queues[4].last_avail_idx, 4);
        assert_eq!(dev.stats().queues[4].used_produced, 4);
        assert_eq!(mem.read(0x4004_0000, 2), b"m0");
        assert_eq!(mem.read(0x4004_0300, 2), b"m3");

        // Ring drained: a further send finds no buffer and is retained.
        let no_buf_before = dev.stats().queues[4].rx_no_buffers;
        dev.agent_send(b"stall");
        dev.poll(&mut mem);
        assert!(dev.stats().queues[4].rx_no_buffers > no_buf_before);
        assert_eq!(
            dev.stats().host_to_guest_len,
            5,
            "undeliverable bytes are held"
        );

        // Lap 2: guest replenishes past the initial size (avail 4..8) reusing
        // ring slots 0..4, and does NOT ring the doorbell.
        for (i, k) in (4..8u16).enumerate() {
            post_rx_buffer(
                &mut mem,
                0x4000_1000,
                0x4000_2000,
                size,
                k,
                i as u16,
                0x4005_0000 + u64::from(i as u16) * 0x100,
                64,
            );
        }

        // The periodic poll (no notify) must resume delivery into lap-2 buffers.
        assert!(dev.poll(&mut mem), "poll must consume replenished buffers");
        assert_eq!(dev.stats().queues[4].last_avail_idx, 5);
        assert_eq!(mem.read(0x4005_0000, 5), b"stall");
        assert_eq!(dev.stats().host_to_guest_len, 0);
    }
}
