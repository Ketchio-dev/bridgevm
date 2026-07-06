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
pub const AGENT_PORT_NAME: &[u8] = b"org.bridgevm.agent.0\0";

const VIRTIO_CONSOLE_DEVICE_READY: u16 = 0;
const VIRTIO_CONSOLE_DEVICE_ADD: u16 = 1;
const VIRTIO_CONSOLE_DEVICE_REMOVE: u16 = 2;
const VIRTIO_CONSOLE_PORT_READY: u16 = 3;
const VIRTIO_CONSOLE_CONSOLE_PORT: u16 = 4;
const VIRTIO_CONSOLE_RESIZE: u16 = 5;
const VIRTIO_CONSOLE_PORT_OPEN: u16 = 6;
const VIRTIO_CONSOLE_PORT_NAME: u16 = 7;
const CONTROL_LEN: usize = 8;

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
    status: u32,
    interrupt_status: u32,
    emerg_wr: u32,
    ports: [PortState; PORT_COUNT],
    pending_control: VecDeque<Vec<u8>>,
    host_to_guest: VecDeque<u8>,
    host_inbound: Vec<u8>,
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtioConsoleStats {
    pub status: u32,
    pub interrupt_status: u32,
    pub driver_features: u64,
    pub port1_ready: bool,
    pub port1_guest_open: bool,
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
            status: 0,
            interrupt_status: 0,
            emerg_wr: 0,
            ports: [PortState::new(), PortState::new()],
            pending_control: VecDeque::new(),
            host_to_guest: VecDeque::new(),
            host_inbound: Vec::new(),
        }
    }

    pub fn stats(&self) -> VirtioConsoleStats {
        let mut stats = VirtioConsoleStats::default();
        stats.status = self.status;
        stats.interrupt_status = self.interrupt_status;
        stats.driver_features =
            u64::from(self.driver_features[0]) | (u64::from(self.driver_features[1]) << 32);
        stats.port1_ready = self.ports[1].ready;
        stats.port1_guest_open = self.ports[1].guest_open;
        stats.pending_control = self.pending_control.len();
        stats.host_to_guest_len = self.host_to_guest.len();
        stats.host_inbound_len = self.host_inbound.len();
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
        self.status = 0;
        self.interrupt_status = 0;
        self.emerg_wr = 0;
        self.ports = [PortState::new(), PortState::new()];
        self.pending_control.clear();
        self.host_to_guest.clear();
        self.host_inbound.clear();
    }

    pub fn agent_send(&mut self, data: &[u8]) {
        self.host_to_guest.extend(data.iter().copied());
    }

    pub fn take_inbound(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.host_inbound)
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
        match usize::from(queue_index) {
            QUEUE_CONTROL_RX => {
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
        let mut progressed = false;
        while self.queues[queue_index].last_avail_idx != avail_idx {
            let last_avail_idx = self.queues[queue_index].last_avail_idx;
            let ring_off = 4 + u64::from(last_avail_idx % queue.size) * 2;
            let Some(head) = read_u16(mem, queue.driver + ring_off) else {
                return progressed;
            };
            if let Some(bytes) = self.read_chain(mem, &queue, head) {
                self.handle_control_tx(&bytes);
            }
            Self::write_used(mem, &queue, head, 0);
            self.queues[queue_index].last_avail_idx = last_avail_idx.wrapping_add(1);
            self.mark_queue_interrupt(queue_index);
            progressed = true;
        }
        progressed
    }

    fn handle_control_tx(&mut self, bytes: &[u8]) {
        let Some(control) = Control::parse(bytes) else {
            return;
        };
        match control.event {
            VIRTIO_CONSOLE_DEVICE_READY if control.value == 1 => {
                self.enqueue_control(Control::new(0, VIRTIO_CONSOLE_DEVICE_ADD, 0).bytes());
                self.enqueue_control(Control::new(1, VIRTIO_CONSOLE_DEVICE_ADD, 0).bytes());
            }
            VIRTIO_CONSOLE_PORT_READY if control.value == 1 => {
                if let Some(port) = self.ports.get_mut(control.id as usize) {
                    port.ready = true;
                }
                if control.id == AGENT_PORT_ID {
                    let mut name = Control::new(AGENT_PORT_ID, VIRTIO_CONSOLE_PORT_NAME, 0)
                        .bytes()
                        .to_vec();
                    name.extend_from_slice(AGENT_PORT_NAME);
                    self.enqueue_control(name);
                    self.ports[1].host_open = true;
                    self.enqueue_control(
                        Control::new(AGENT_PORT_ID, VIRTIO_CONSOLE_PORT_OPEN, 1).bytes(),
                    );
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

    fn enqueue_control(&mut self, message: impl Into<Vec<u8>>) {
        self.pending_control.push_back(message.into());
    }

    fn flush_pending_control(&mut self, mem: &mut dyn GuestMemoryMut) -> bool {
        let mut progressed = false;
        while let Some(message) = self.pending_control.pop_front() {
            if self.deliver_to_rx_queue(QUEUE_CONTROL_RX, &message, mem) {
                progressed = true;
            } else {
                self.pending_control.push_front(message);
                break;
            }
        }
        progressed
    }

    fn deliver_agent_rx(&mut self, mem: &mut dyn GuestMemoryMut) -> bool {
        if !self.ports[1].guest_open || self.host_to_guest.is_empty() {
            return false;
        }
        let bytes: Vec<u8> = self.host_to_guest.iter().copied().collect();
        let Some(written) = self.deliver_partial_to_rx_queue(QUEUE_AGENT_RX, &bytes, mem) else {
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
        let mut progressed = false;
        while self.queues[queue_index].last_avail_idx != avail_idx {
            let last_avail_idx = self.queues[queue_index].last_avail_idx;
            let ring_off = 4 + u64::from(last_avail_idx % queue.size) * 2;
            let Some(head) = read_u16(mem, queue.driver + ring_off) else {
                return progressed;
            };
            if let Some(mut bytes) = self.read_chain(mem, &queue, head) {
                self.host_inbound.append(&mut bytes);
            }
            Self::write_used(mem, &queue, head, 0);
            self.queues[queue_index].last_avail_idx = last_avail_idx.wrapping_add(1);
            self.mark_queue_interrupt(queue_index);
            progressed = true;
        }
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
        let queue = self.queues[queue_index];
        if !queue.ready || queue.size == 0 || queue.desc == 0 || bytes.is_empty() {
            return None;
        }
        let avail_idx = read_u16(mem, queue.driver + 2)?;
        if self.queues[queue_index].last_avail_idx == avail_idx {
            return None;
        }
        let last_avail_idx = self.queues[queue_index].last_avail_idx;
        let ring_off = 4 + u64::from(last_avail_idx % queue.size) * 2;
        let head = read_u16(mem, queue.driver + ring_off)?;
        let descs = Self::descriptor_chain(mem, &queue, head)?;
        let written = Self::scatter_write_partial(mem, &descs, bytes)?;
        Self::write_used(
            mem,
            &queue,
            head,
            u32::try_from(written).unwrap_or(u32::MAX),
        );
        self.queues[queue_index].last_avail_idx = last_avail_idx.wrapping_add(1);
        self.mark_queue_interrupt(queue_index);
        Some(written)
    }

    fn read_chain(
        &self,
        mem: &dyn GuestMemoryMut,
        queue: &VirtioConsoleQueue,
        head: u16,
    ) -> Option<Vec<u8>> {
        let descs = Self::descriptor_chain(mem, queue, head)?;
        let mut out = Vec::new();
        for desc in descs {
            if desc.flags & DESC_F_WRITE != 0 {
                return None;
            }
            let mut bytes = mem.read_bytes(desc.addr, desc.len as usize)?;
            out.append(&mut bytes);
        }
        Some(out)
    }

    fn scatter_write_partial(
        mem: &mut dyn GuestMemoryMut,
        descs: &[Descriptor],
        bytes: &[u8],
    ) -> Option<usize> {
        let mut offset = 0usize;
        for desc in descs {
            if desc.flags & DESC_F_WRITE == 0 {
                return None;
            }
            let writable = (desc.len as usize).min(bytes.len().saturating_sub(offset));
            if writable == 0 {
                continue;
            }
            if !mem.write_bytes(desc.addr, &bytes[offset..offset + writable]) {
                return None;
            }
            offset += writable;
            if offset == bytes.len() {
                break;
            }
        }
        (offset > 0).then_some(offset)
    }

    fn mark_queue_interrupt(&mut self, queue_index: usize) {
        if let Some(queue) = self.queues.get_mut(queue_index) {
            queue.pending_msix = true;
        }
        self.interrupt_status |= 1;
    }

    fn descriptor_chain(
        mem: &dyn GuestMemoryMut,
        queue: &VirtioConsoleQueue,
        head: u16,
    ) -> Option<Vec<Descriptor>> {
        if head >= queue.size {
            return None;
        }
        let mut out = Vec::new();
        let mut index = head;
        for _ in 0..queue.size {
            let desc = Descriptor::read(mem, queue.desc + u64::from(index) * DESC_SIZE)?;
            let has_next = desc.flags & DESC_F_NEXT != 0;
            out.push(desc);
            if !has_next {
                return Some(out);
            }
            index = desc.next;
            if index >= queue.size {
                return None;
            }
        }
        None
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
        let mut messages = self.msix.drain_pending(function_enabled, function_masked);
        for message in &messages {
            self.clear_pending_queue_for_vector(message.vector);
        }
        messages.extend(self.raise_pending_msix(function_enabled, function_masked));
        messages
    }

    fn raise_pending_msix(
        &mut self,
        function_enabled: bool,
        function_masked: bool,
    ) -> Vec<MsixMessage> {
        let mut messages = Vec::new();
        for queue_index in 0..self.console.queues.len() {
            if !self.console.queues[queue_index].pending_msix {
                continue;
            }
            let vector = self.console.queues[queue_index].msix_vector;
            if vector == VIRTIO_MSI_NO_VECTOR {
                continue;
            }
            if let Some(message) = self.msix.raise(vector, function_enabled, function_masked) {
                self.console.queues[queue_index].pending_msix = false;
                messages.push(message);
            }
        }
        messages
    }

    fn clear_pending_queue_for_vector(&mut self, vector: u16) {
        for queue in &mut self.console.queues {
            if queue.msix_vector == vector {
                queue.pending_msix = false;
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

#[derive(Debug, Clone, Copy)]
struct Descriptor {
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
}

impl Descriptor {
    fn read(mem: &dyn GuestMemoryMut, gpa: u64) -> Option<Self> {
        let bytes = mem.read_bytes(gpa, DESC_SIZE as usize)?;
        Some(Self {
            addr: u64::from_le_bytes(bytes[0..8].try_into().ok()?),
            len: u32::from_le_bytes(bytes[8..12].try_into().ok()?),
            flags: u16::from_le_bytes(bytes[12..14].try_into().ok()?),
            next: u16::from_le_bytes(bytes[14..16].try_into().ok()?),
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
    let bytes = mem.read_bytes(gpa, 2)?;
    Some(u16::from_le_bytes(bytes.try_into().ok()?))
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
            mem.read(out3, 8),
            control_bytes(1, VIRTIO_CONSOLE_PORT_OPEN, 1)
        );
        assert!(dev.stats().queues[2].pending_msix);
        assert!(dev.stats().queues[3].pending_msix);
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
        dev.console.pending_control.push_back(vec![1, 2, 3]);
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
}
