//! Minimal modern virtio-net PCI device model.
//!
//! Stage 1 deliberately stops at the device model: TX frames are handed to an
//! in-memory backend and RX frames are pulled from that backend by an explicit
//! pump. No host networking is opened here.

use std::collections::VecDeque;

use crate::{
    fwcfg::GuestMemoryMut,
    msix::{MsixMessage, MsixTable},
    pcie::{
        VIRTIO_NET_MSIX_PBA_OFFSET, VIRTIO_NET_MSIX_TABLE_OFFSET, VIRTIO_NET_MSIX_VECTOR_COUNT,
    },
};

const MAGIC_VALUE: u32 = 0x7472_6976;
const VERSION_MODERN: u32 = 2;
const DEVICE_ID_NET: u32 = 1;
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

const VIRTIO_NET_F_MAC: u32 = 1 << 5;
const VIRTIO_NET_F_STATUS: u32 = 1 << 16;
const VIRTIO_F_VERSION_1: u32 = 1 << 0;
const VIRTIO_NET_S_LINK_UP: u16 = 1;
const VIRTIO_MSI_NO_VECTOR: u16 = 0xffff;

const QUEUE_RX: usize = 0;
const QUEUE_TX: usize = 1;
const QUEUE_COUNT: usize = 2;
const QUEUE_MAX: u16 = 256;
const DESC_SIZE: u64 = 16;
const DESC_F_NEXT: u16 = 1;
const DESC_F_WRITE: u16 = 2;
const VIRTIO_NET_HDR_LEN: usize = 12;
const MAX_TX_PACKET_LEN: usize = VIRTIO_NET_HDR_LEN + 65_535;
const DEFAULT_MAC: [u8; 6] = [0x52, 0x54, 0x00, 0x42, 0x56, 0x01];

pub trait NetBackend: Send {
    fn transmit(&mut self, frame: &[u8]);
    fn poll_receive(&mut self) -> Option<Vec<u8>>;
    fn poll_receive_into(&mut self, out: &mut Vec<u8>) -> bool {
        let Some(mut frame) = self.poll_receive() else {
            return false;
        };
        out.clear();
        out.append(&mut frame);
        true
    }
    fn poll_host_sockets(&mut self) {}
    #[cfg(test)]
    fn test_transmitted_frames(&self) -> Option<&[Vec<u8>]> {
        None
    }
}

impl NetBackend for Box<dyn NetBackend> {
    fn transmit(&mut self, frame: &[u8]) {
        self.as_mut().transmit(frame);
    }

    fn poll_receive(&mut self) -> Option<Vec<u8>> {
        self.as_mut().poll_receive()
    }

    fn poll_receive_into(&mut self, out: &mut Vec<u8>) -> bool {
        self.as_mut().poll_receive_into(out)
    }

    fn poll_host_sockets(&mut self) {
        self.as_mut().poll_host_sockets();
    }

    #[cfg(test)]
    fn test_transmitted_frames(&self) -> Option<&[Vec<u8>]> {
        self.as_ref().test_transmitted_frames()
    }
}

impl std::fmt::Debug for dyn NetBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NetBackend").finish_non_exhaustive()
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct LoopbackTestBackend {
    transmitted: Vec<Vec<u8>>,
    receive: VecDeque<Vec<u8>>,
}

impl LoopbackTestBackend {
    pub fn push_receive(&mut self, frame: impl Into<Vec<u8>>) {
        self.receive.push_back(frame.into());
    }

    pub fn transmitted_frames(&self) -> &[Vec<u8>] {
        &self.transmitted
    }
}

impl NetBackend for LoopbackTestBackend {
    fn transmit(&mut self, frame: &[u8]) {
        self.transmitted.push(frame.to_vec());
    }

    fn poll_receive(&mut self) -> Option<Vec<u8>> {
        self.receive.pop_front()
    }

    fn poll_receive_into(&mut self, out: &mut Vec<u8>) -> bool {
        let Some(mut frame) = self.receive.pop_front() else {
            return false;
        };
        out.clear();
        out.append(&mut frame);
        true
    }

    #[cfg(test)]
    fn test_transmitted_frames(&self) -> Option<&[Vec<u8>]> {
        Some(self.transmitted_frames())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VirtioNetResult {
    ReadValue(u64),
    WriteAck,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VirtioPciNetOp {
    Read { size: u8 },
    Write { size: u8, value: u64 },
}

#[derive(Debug)]
pub struct VirtioNet<B: NetBackend> {
    backend: B,
    stats: VirtioNetStats,
    mac: [u8; 6],
    device_features_sel: u32,
    driver_features_sel: u32,
    driver_features: [u32; 2],
    config_msix_vector: u16,
    queue_sel: u32,
    queues: [VirtioNetQueue; QUEUE_COUNT],
    pending_msix_queue_bits: u8,
    status: u32,
    interrupt_status: u32,
    pending_rx_frame: Option<Vec<u8>>,
    descriptor_scratch: Vec<Descriptor>,
    tx_packet_scratch: Vec<u8>,
    rx_frame_scratch: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct VirtioNetQueue {
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

impl VirtioNetQueue {
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

    fn access_common(
        &mut self,
        offset: u64,
        is_write: bool,
        size: u8,
        value: u64,
        mem: &mut dyn GuestMemoryMut,
    ) -> VirtioNetResult {
        if !is_write {
            return VirtioNetResult::ReadValue(self.read_common(offset, size));
        }
        self.write_common(offset, size, value, mem);
        VirtioNetResult::WriteAck
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
            REG_DEVICE_ID => u64::from(DEVICE_ID_NET),
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
        self.write_mmio_alias(offset, value, mem);
    }

    fn write_mmio_alias(&mut self, offset: u64, value: u64, mem: &mut dyn GuestMemoryMut) {
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

    fn selected_queue(&self) -> Option<VirtioNetQueue> {
        self.queues.get(self.queue_sel as usize).copied()
    }

    fn write_selected_queue(&mut self, write: impl FnOnce(&mut VirtioNetQueue)) {
        if let Some(queue) = self.queues.get_mut(self.queue_sel as usize) {
            write(queue);
        }
    }

    fn device_features(&self) -> u32 {
        offered_features_word(self.device_features_sel)
    }

    fn config_read(&self, offset: u64, size: u8) -> u64 {
        let mut config = [0u8; 0x40];
        config[0..6].copy_from_slice(&self.mac);
        config[6..8].copy_from_slice(&VIRTIO_NET_S_LINK_UP.to_le_bytes());
        read_le_from_bytes(&config, offset, size).unwrap_or(0)
    }

    fn notify_queue(&mut self, queue_index: u16, mem: &mut dyn GuestMemoryMut) {
        self.stats.notify_count = self.stats.notify_count.saturating_add(1);
        if usize::from(queue_index) == QUEUE_TX {
            self.process_tx_queue(mem);
        }
    }

    pub fn pump_receive(&mut self, mem: &mut dyn GuestMemoryMut) -> bool {
        let mut frame = if let Some(frame) = self.pending_rx_frame.take() {
            frame
        } else {
            let mut frame = std::mem::take(&mut self.rx_frame_scratch);
            if !self.backend.poll_receive_into(&mut frame) {
                self.rx_frame_scratch = frame;
                return false;
            }
            frame
        };
        if self.deliver_rx_frame(&frame, mem) {
            frame.clear();
            self.rx_frame_scratch = frame;
            return true;
        }
        self.pending_rx_frame = Some(frame);
        false
    }

    fn process_tx_queue(&mut self, mem: &mut dyn GuestMemoryMut) {
        let queue_index = QUEUE_TX;
        let queue = self.queues[queue_index];
        if !queue.ready || queue.size == 0 || queue.desc == 0 {
            return;
        }
        let Some(avail_idx) = read_u16(mem, queue.driver + 2) else {
            return;
        };
        let mut descs = std::mem::take(&mut self.descriptor_scratch);
        let mut packet = std::mem::take(&mut self.tx_packet_scratch);
        while self.queues[queue_index].last_avail_idx != avail_idx {
            let last_avail_idx = self.queues[queue_index].last_avail_idx;
            let ring_off = 4 + u64::from(last_avail_idx % queue.size) * 2;
            let Some(head) = read_u16(mem, queue.driver + ring_off) else {
                break;
            };
            if Self::tx_frame_from_chain_into(mem, &queue, head, &mut descs, &mut packet) {
                let frame = &packet[VIRTIO_NET_HDR_LEN..];
                self.stats.tx_count = self.stats.tx_count.saturating_add(1);
                self.stats.tx_bytes = self.stats.tx_bytes.saturating_add(frame.len() as u64);
                self.backend.transmit(frame);
            }
            Self::write_used(mem, &queue, head, 0);
            self.queues[queue_index].last_avail_idx = last_avail_idx.wrapping_add(1);
            self.mark_queue_interrupt(queue_index);
        }
        descs.clear();
        packet.clear();
        self.descriptor_scratch = descs;
        self.tx_packet_scratch = packet;
    }

    fn tx_frame_from_chain_into(
        mem: &dyn GuestMemoryMut,
        queue: &VirtioNetQueue,
        head: u16,
        descs: &mut Vec<Descriptor>,
        packet: &mut Vec<u8>,
    ) -> bool {
        packet.clear();
        if !Self::descriptor_chain_into(mem, queue, head, descs) {
            return false;
        }
        for desc in descs.iter() {
            if desc.flags & DESC_F_WRITE != 0 {
                return false;
            }
            let start = packet.len();
            let Some(end) = start.checked_add(desc.len as usize) else {
                return false;
            };
            if end > MAX_TX_PACKET_LEN {
                return false;
            }
            let Some(bytes) = mem.read_bytes(desc.addr, desc.len as usize) else {
                return false;
            };
            packet.extend_from_slice(&bytes);
        }
        packet.len() >= VIRTIO_NET_HDR_LEN
    }

    fn deliver_rx_frame(&mut self, frame: &[u8], mem: &mut dyn GuestMemoryMut) -> bool {
        let queue_index = QUEUE_RX;
        let queue = self.queues[queue_index];
        if !queue.ready || queue.size == 0 || queue.desc == 0 {
            return false;
        }
        let Some(avail_idx) = read_u16(mem, queue.driver + 2) else {
            return false;
        };
        if self.queues[queue_index].last_avail_idx == avail_idx {
            return false;
        }
        let last_avail_idx = self.queues[queue_index].last_avail_idx;
        let ring_off = 4 + u64::from(last_avail_idx % queue.size) * 2;
        let Some(head) = read_u16(mem, queue.driver + ring_off) else {
            return false;
        };
        let mut descs = std::mem::take(&mut self.descriptor_scratch);
        let delivered = Self::descriptor_chain_into(mem, &queue, head, &mut descs);
        if !delivered {
            self.descriptor_scratch = descs;
            return false;
        }
        let mut hdr = [0u8; VIRTIO_NET_HDR_LEN];
        hdr[10..12].copy_from_slice(&1u16.to_le_bytes());
        if !Self::scatter_write_slices(mem, &descs, &[&hdr, frame]) {
            self.descriptor_scratch = descs;
            return false;
        }
        let used_len =
            u32::try_from(VIRTIO_NET_HDR_LEN.saturating_add(frame.len())).unwrap_or(u32::MAX);
        descs.clear();
        self.descriptor_scratch = descs;
        Self::write_used(mem, &queue, head, used_len);
        self.queues[queue_index].last_avail_idx = last_avail_idx.wrapping_add(1);
        self.stats.rx_count = self.stats.rx_count.saturating_add(1);
        self.stats.rx_bytes = self.stats.rx_bytes.saturating_add(frame.len() as u64);
        self.mark_queue_interrupt(queue_index);
        true
    }

    fn scatter_write_slices(
        mem: &mut dyn GuestMemoryMut,
        descs: &[Descriptor],
        slices: &[&[u8]],
    ) -> bool {
        let total_len = slices
            .iter()
            .try_fold(0usize, |sum, slice| sum.checked_add(slice.len()));
        let Some(total_len) = total_len else {
            return false;
        };
        if total_len == 0 {
            return true;
        }

        let mut slice_index = 0usize;
        let mut slice_offset = 0usize;
        let mut written = 0usize;

        for desc in descs {
            if desc.flags & DESC_F_WRITE == 0 {
                return false;
            }
            let mut desc_offset = 0usize;
            let desc_len = desc.len as usize;
            while desc_offset < desc_len && written < total_len {
                while slice_index < slices.len() && slice_offset == slices[slice_index].len() {
                    slice_index += 1;
                    slice_offset = 0;
                }
                if slice_index == slices.len() {
                    return written == total_len;
                }

                let slice = slices[slice_index];
                let copy_len = (desc_len - desc_offset).min(slice.len() - slice_offset);
                let Some(gpa) = desc.addr.checked_add(desc_offset as u64) else {
                    return false;
                };
                if !mem.write_bytes(gpa, &slice[slice_offset..slice_offset + copy_len]) {
                    return false;
                }
                desc_offset += copy_len;
                slice_offset += copy_len;
                written += copy_len;
            }
        }
        written == total_len
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
        queue: &VirtioNetQueue,
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

    fn write_used(mem: &mut dyn GuestMemoryMut, queue: &VirtioNetQueue, id: u16, len: u32) {
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

#[derive(Debug)]
pub struct VirtioPciNet<B: NetBackend = LoopbackTestBackend> {
    net: VirtioNet<B>,
    msix: MsixTable,
}

impl VirtioPciNet<LoopbackTestBackend> {
    pub fn new_loopback() -> Self {
        Self::new(LoopbackTestBackend::default())
    }
}

impl<B: NetBackend> VirtioPciNet<B> {
    pub fn new(backend: B) -> Self {
        Self {
            net: VirtioNet::new(backend),
            msix: MsixTable::new(VIRTIO_NET_MSIX_VECTOR_COUNT),
        }
    }

    pub fn backend(&self) -> &B {
        self.net.backend()
    }

    pub fn backend_mut(&mut self) -> &mut B {
        self.net.backend_mut()
    }

    pub fn stats(&self) -> VirtioNetStats {
        self.net.stats()
    }

    pub fn interrupt_line_level(&self) -> bool {
        self.net.interrupt_line_level()
    }

    pub fn reset_runtime_state(&mut self) {
        self.net.reset_runtime_state();
        self.msix = MsixTable::new(VIRTIO_NET_MSIX_VECTOR_COUNT);
    }

    pub fn pump_receive(&mut self, mem: &mut dyn GuestMemoryMut) -> bool {
        self.net.pump_receive(mem)
    }

    pub fn poll_host_sockets(&mut self) {
        self.net.backend_mut().poll_host_sockets();
    }

    pub fn access(
        &mut self,
        offset: u64,
        op: VirtioPciNetOp,
        mem: &mut dyn GuestMemoryMut,
    ) -> VirtioNetResult {
        let is_write = matches!(op, VirtioPciNetOp::Write { .. });
        if let Some(common_offset) = common_cfg_offset(offset) {
            return match op {
                VirtioPciNetOp::Read { size } => {
                    self.net.access_common(common_offset, false, size, 0, mem)
                }
                VirtioPciNetOp::Write { size, value } => {
                    self.net
                        .access_common(common_offset, true, size, value, mem)
                }
            };
        }
        if let Some(device_offset) = device_cfg_offset(offset) {
            return match op {
                VirtioPciNetOp::Read { size } => {
                    VirtioNetResult::ReadValue(self.net.config_read(device_offset, size))
                }
                VirtioPciNetOp::Write { .. } => VirtioNetResult::WriteAck,
            };
        }
        if let Some(queue_index) = notify_queue_index(offset) {
            return match op {
                VirtioPciNetOp::Read { .. } => VirtioNetResult::ReadValue(0),
                VirtioPciNetOp::Write { value, .. } => {
                    let queue = if offset == PCI_NOTIFY_CFG_OFFSET {
                        value as u16
                    } else {
                        queue_index
                    };
                    self.net.notify_queue(queue, mem);
                    VirtioNetResult::WriteAck
                }
            };
        }
        if offset == PCI_ISR_CFG_OFFSET {
            return match op {
                VirtioPciNetOp::Read { size } => VirtioNetResult::ReadValue(mask_to_size(
                    u64::from(self.net.interrupt_status),
                    size,
                )),
                VirtioPciNetOp::Write { value, .. } => {
                    self.net.interrupt_status &= !(value as u32);
                    VirtioNetResult::WriteAck
                }
            };
        }
        match (op, is_write) {
            (VirtioPciNetOp::Read { .. }, _) => VirtioNetResult::ReadValue(0),
            (VirtioPciNetOp::Write { .. }, _) => VirtioNetResult::WriteAck,
        }
    }

    pub fn msix_bar_access(&mut self, offset: u64, op: VirtioPciNetOp) -> VirtioNetResult {
        if let Some(table_offset) = self.msix_table_offset(offset) {
            return match op {
                VirtioPciNetOp::Read { size } => {
                    VirtioNetResult::ReadValue(self.msix.table_read(table_offset, size))
                }
                VirtioPciNetOp::Write { size, value } => {
                    self.msix.table_write(table_offset, size, value);
                    VirtioNetResult::WriteAck
                }
            };
        }
        if let Some(pba_offset) = self.msix_pba_offset(offset) {
            return match op {
                VirtioPciNetOp::Read { size } => {
                    VirtioNetResult::ReadValue(self.msix.pba_read(pba_offset, size))
                }
                VirtioPciNetOp::Write { size, value } => {
                    self.msix.pba_write(pba_offset, size, value);
                    VirtioNetResult::WriteAck
                }
            };
        }
        match op {
            VirtioPciNetOp::Read { .. } => VirtioNetResult::ReadValue(0),
            VirtioPciNetOp::Write { .. } => VirtioNetResult::WriteAck,
        }
    }

    pub fn raise_pending_msix(
        &mut self,
        function_enabled: bool,
        function_masked: bool,
    ) -> Vec<MsixMessage> {
        let mut messages = Vec::new();
        self.raise_pending_msix_into(function_enabled, function_masked, &mut messages);
        messages
    }

    pub fn raise_pending_msix_into(
        &mut self,
        function_enabled: bool,
        function_masked: bool,
        out: &mut Vec<MsixMessage>,
    ) {
        let mut pending = self.net.pending_msix_queue_bits;
        while pending != 0 {
            let queue_index = pending.trailing_zeros() as usize;
            let vector = self.net.queues[queue_index].msix_vector;
            if vector == VIRTIO_MSI_NO_VECTOR {
                pending &= !(1u8 << queue_index);
                continue;
            }
            if let Some(message) = self.msix.raise(vector, function_enabled, function_masked) {
                self.net.queues[queue_index].pending_msix = false;
                self.net.pending_msix_queue_bits &= !(1u8 << queue_index);
                out.push(message);
            }
            pending &= !(1u8 << queue_index);
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

    fn clear_pending_queue_for_vector(&mut self, vector: u16) {
        for (queue_index, queue) in self.net.queues.iter_mut().enumerate() {
            if queue.msix_vector == vector {
                queue.pending_msix = false;
                if let Some(bit) = queue_bit(queue_index) {
                    self.net.pending_msix_queue_bits &= !bit;
                }
            }
        }
    }

    fn msix_table_offset(&self, offset: u64) -> Option<u64> {
        let rel = offset.checked_sub(u64::from(VIRTIO_NET_MSIX_TABLE_OFFSET))?;
        (rel < self.msix.table_byte_len()).then_some(rel)
    }

    fn msix_pba_offset(&self, offset: u64) -> Option<u64> {
        let rel = offset.checked_sub(u64::from(VIRTIO_NET_MSIX_PBA_OFFSET))?;
        (rel < self.msix.pba_byte_len()).then_some(rel)
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

fn set_low(current: u64, value: u64) -> u64 {
    (current & !0xffff_ffff) | (value & 0xffff_ffff)
}

fn set_high(current: u64, value: u64) -> u64 {
    (current & 0xffff_ffff) | ((value & 0xffff_ffff) << 32)
}

fn offered_features_word(select: u32) -> u32 {
    match select {
        0 => VIRTIO_NET_F_MAC | VIRTIO_NET_F_STATUS,
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

fn read_u16(mem: &dyn GuestMemoryMut, gpa: u64) -> Option<u16> {
    let mut bytes = [0u8; 2];
    if !mem.read_into(gpa, &mut bytes) {
        return None;
    }
    Some(u16::from_le_bytes(bytes))
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

    fn pci_write(dev: &mut VirtioPciNet, offset: u64, size: u8, value: u64, mem: &mut TestMem) {
        assert_eq!(
            dev.access(offset, VirtioPciNetOp::Write { size, value }, mem),
            VirtioNetResult::WriteAck
        );
    }

    fn pci_read(dev: &mut VirtioPciNet, offset: u64, size: u8, mem: &mut TestMem) -> u64 {
        match dev.access(offset, VirtioPciNetOp::Read { size }, mem) {
            VirtioNetResult::ReadValue(value) => value,
            VirtioNetResult::WriteAck => panic!("read returned write ack"),
        }
    }

    fn pci_write_split_u64(dev: &mut VirtioPciNet, offset: u64, value: u64, mem: &mut TestMem) {
        pci_write(dev, offset, 4, value & 0xffff_ffff, mem);
        pci_write(dev, offset + 4, 4, value >> 32, mem);
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
        dev: &mut VirtioPciNet,
        mem: &mut TestMem,
        queue: u16,
        desc: u64,
        avail: u64,
        used: u64,
        vector: u16,
    ) {
        pci_write(dev, COMMON_QUEUE_SELECT, 2, u64::from(queue), mem);
        pci_write(dev, COMMON_QUEUE_SIZE, 2, 8, mem);
        pci_write_split_u64(dev, COMMON_QUEUE_DESC, desc, mem);
        pci_write_split_u64(dev, COMMON_QUEUE_DRIVER, avail, mem);
        pci_write_split_u64(dev, COMMON_QUEUE_DEVICE, used, mem);
        pci_write(dev, COMMON_QUEUE_MSIX_VECTOR, 2, u64::from(vector), mem);
        pci_write(dev, COMMON_QUEUE_ENABLE, 2, 1, mem);
    }

    fn program_msix_vector(dev: &mut VirtioPciNet, vector: u16, address: u64, data: u32) {
        let off = u64::from(VIRTIO_NET_MSIX_TABLE_OFFSET) + u64::from(vector) * 16;
        assert_eq!(
            dev.msix_bar_access(
                off,
                VirtioPciNetOp::Write {
                    size: 8,
                    value: address,
                },
            ),
            VirtioNetResult::WriteAck
        );
        assert_eq!(
            dev.msix_bar_access(
                off + 8,
                VirtioPciNetOp::Write {
                    size: 4,
                    value: u64::from(data),
                },
            ),
            VirtioNetResult::WriteAck
        );
        assert_eq!(
            dev.msix_bar_access(off + 12, VirtioPciNetOp::Write { size: 4, value: 0 },),
            VirtioNetResult::WriteAck
        );
    }

    #[test]
    fn feature_negotiation_reads_both_windows_and_status_round_trips() {
        let mut dev = VirtioPciNet::new_loopback();
        let mut mem = TestMem::new(0x4000_0000, 0x1000);

        pci_write(&mut dev, COMMON_DEVICE_FEATURE_SELECT, 4, 0, &mut mem);
        assert_eq!(
            pci_read(&mut dev, COMMON_DEVICE_FEATURE, 4, &mut mem),
            u64::from(VIRTIO_NET_F_MAC | VIRTIO_NET_F_STATUS)
        );
        pci_write(&mut dev, COMMON_DEVICE_FEATURE_SELECT, 4, 1, &mut mem);
        assert_eq!(
            pci_read(&mut dev, COMMON_DEVICE_FEATURE, 4, &mut mem),
            u64::from(VIRTIO_F_VERSION_1)
        );

        pci_write(&mut dev, COMMON_DRIVER_FEATURE_SELECT, 4, 0, &mut mem);
        pci_write(
            &mut dev,
            COMMON_DRIVER_FEATURE,
            4,
            u64::from(VIRTIO_NET_F_MAC | VIRTIO_NET_F_STATUS),
            &mut mem,
        );
        pci_write(&mut dev, COMMON_DRIVER_FEATURE_SELECT, 4, 1, &mut mem);
        pci_write(
            &mut dev,
            COMMON_DRIVER_FEATURE,
            4,
            u64::from(VIRTIO_F_VERSION_1),
            &mut mem,
        );
        pci_write(&mut dev, COMMON_DEVICE_STATUS, 1, 0x0f, &mut mem);

        assert_eq!(pci_read(&mut dev, COMMON_DEVICE_STATUS, 1, &mut mem), 0x0f);
        assert_eq!(
            dev.stats().driver_features,
            u64::from(VIRTIO_NET_F_MAC | VIRTIO_NET_F_STATUS)
                | (u64::from(VIRTIO_F_VERSION_1) << 32)
        );
    }

    #[test]
    fn modern_common_config_masks_features_and_accepts_split_queue_address_writes() {
        let mut dev = VirtioPciNet::new_loopback();
        let mut mem = TestMem::new(0x4000_0000, 0x1000);
        let rx_desc = 0x0000_0001_4000_1000;
        let rx_avail = 0x0000_0001_4000_2000;
        let rx_used = 0x0000_0001_4000_3000;
        let tx_desc = 0x0000_0001_4000_5000;
        let tx_avail = 0x0000_0001_4000_6000;
        let tx_used = 0x0000_0001_4000_7000;
        let offered = u64::from(VIRTIO_NET_F_MAC | VIRTIO_NET_F_STATUS)
            | (u64::from(VIRTIO_F_VERSION_1) << 32);

        pci_write(&mut dev, COMMON_DEVICE_FEATURE_SELECT, 4, 0, &mut mem);
        assert_eq!(
            pci_read(&mut dev, COMMON_DEVICE_FEATURE, 4, &mut mem),
            0x0001_0020
        );
        pci_write(&mut dev, COMMON_DEVICE_FEATURE_SELECT, 4, 1, &mut mem);
        assert_eq!(
            pci_read(&mut dev, COMMON_DEVICE_FEATURE, 4, &mut mem),
            0x0000_0001
        );

        pci_write(&mut dev, COMMON_DRIVER_FEATURE_SELECT, 4, 0, &mut mem);
        pci_write(&mut dev, COMMON_DRIVER_FEATURE, 4, 0xffff_ffff, &mut mem);
        pci_write(&mut dev, COMMON_DRIVER_FEATURE_SELECT, 4, 1, &mut mem);
        pci_write(&mut dev, COMMON_DRIVER_FEATURE, 4, 0x112f_8001, &mut mem);
        assert_eq!(dev.stats().driver_features, offered);
        assert_eq!(dev.stats().driver_features & !offered, 0);

        pci_write(&mut dev, COMMON_DEVICE_STATUS, 1, 0x01, &mut mem);
        pci_write(&mut dev, COMMON_DEVICE_STATUS, 1, 0x03, &mut mem);
        pci_write(&mut dev, COMMON_DEVICE_STATUS, 1, 0x07, &mut mem);

        for (queue, desc, avail, used, vector) in [
            (0, rx_desc, rx_avail, rx_used, 1u16),
            (1, tx_desc, tx_avail, tx_used, 0u16),
        ] {
            pci_write(&mut dev, COMMON_QUEUE_SELECT, 2, queue, &mut mem);
            assert_eq!(
                pci_read(&mut dev, COMMON_QUEUE_SIZE, 2, &mut mem),
                u64::from(QUEUE_MAX)
            );
            pci_write(
                &mut dev,
                COMMON_QUEUE_SIZE,
                2,
                u64::from(QUEUE_MAX),
                &mut mem,
            );
            pci_write_split_u64(&mut dev, COMMON_QUEUE_DESC, desc, &mut mem);
            pci_write_split_u64(&mut dev, COMMON_QUEUE_DRIVER, avail, &mut mem);
            pci_write_split_u64(&mut dev, COMMON_QUEUE_DEVICE, used, &mut mem);
            pci_write(
                &mut dev,
                COMMON_QUEUE_MSIX_VECTOR,
                2,
                u64::from(vector),
                &mut mem,
            );
            pci_write(&mut dev, COMMON_QUEUE_ENABLE, 2, 1, &mut mem);
        }

        pci_write(&mut dev, COMMON_DEVICE_STATUS, 1, 0x0f, &mut mem);

        let stats = dev.stats();
        assert_eq!(stats.status, 0x0f);
        assert_eq!(stats.queues[0].size, QUEUE_MAX);
        assert!(stats.queues[0].ready);
        assert_eq!(stats.queues[0].desc, rx_desc);
        assert_eq!(stats.queues[0].driver, rx_avail);
        assert_eq!(stats.queues[0].device, rx_used);
        assert_eq!(stats.queues[0].msix_vector, 1);
        assert_eq!(stats.queues[1].size, QUEUE_MAX);
        assert!(stats.queues[1].ready);
        assert_eq!(stats.queues[1].desc, tx_desc);
        assert_eq!(stats.queues[1].driver, tx_avail);
        assert_eq!(stats.queues[1].device, tx_used);
        assert_eq!(stats.queues[1].msix_vector, 0);
    }

    #[test]
    fn modern_driver_common_config_sequence_advertises_and_enables_both_queues() {
        let mut dev = VirtioPciNet::new_loopback();
        let mut mem = TestMem::new(0x4000_0000, 0x30000);
        let rx_desc = 0x4000_1000;
        let rx_avail = 0x4000_2000;
        let rx_used = 0x4000_3000;
        let tx_desc = 0x4000_5000;
        let tx_avail = 0x4000_6000;
        let tx_used = 0x4000_7000;
        let tx_hdr = 0x4000_8000;
        let tx_payload = 0x4000_9000;
        let frame = b"\x02\x00\x00\x00\x00\x01\x52\x54\x00\x42\x56\x01\x08\x00modern";

        pci_write(&mut dev, COMMON_DEVICE_STATUS, 1, 0x01, &mut mem);
        pci_write(&mut dev, COMMON_DEVICE_STATUS, 1, 0x03, &mut mem);

        pci_write(&mut dev, COMMON_DEVICE_FEATURE_SELECT, 4, 0, &mut mem);
        assert_eq!(
            pci_read(&mut dev, COMMON_DEVICE_FEATURE, 4, &mut mem),
            u64::from(VIRTIO_NET_F_MAC | VIRTIO_NET_F_STATUS)
        );
        pci_write(&mut dev, COMMON_DEVICE_FEATURE_SELECT, 4, 1, &mut mem);
        assert_eq!(
            pci_read(&mut dev, COMMON_DEVICE_FEATURE, 4, &mut mem),
            u64::from(VIRTIO_F_VERSION_1)
        );
        pci_write(&mut dev, COMMON_DRIVER_FEATURE_SELECT, 4, 0, &mut mem);
        pci_write(
            &mut dev,
            COMMON_DRIVER_FEATURE,
            4,
            u64::from(VIRTIO_NET_F_MAC | VIRTIO_NET_F_STATUS),
            &mut mem,
        );
        pci_write(&mut dev, COMMON_DRIVER_FEATURE_SELECT, 4, 1, &mut mem);
        pci_write(
            &mut dev,
            COMMON_DRIVER_FEATURE,
            4,
            u64::from(VIRTIO_F_VERSION_1),
            &mut mem,
        );
        pci_write(&mut dev, COMMON_DEVICE_STATUS, 1, 0x07, &mut mem);
        assert_eq!(pci_read(&mut dev, COMMON_DEVICE_STATUS, 1, &mut mem), 0x07);
        assert_eq!(pci_read(&mut dev, COMMON_NUM_QUEUES, 2, &mut mem), 2);

        for (queue, desc, avail, used, vector) in [
            (0, rx_desc, rx_avail, rx_used, 0u16),
            (1, tx_desc, tx_avail, tx_used, 1u16),
        ] {
            pci_write(&mut dev, COMMON_QUEUE_SELECT, 2, queue, &mut mem);
            assert_eq!(
                pci_read(&mut dev, COMMON_QUEUE_SIZE, 2, &mut mem),
                u64::from(QUEUE_MAX)
            );
            pci_write(&mut dev, COMMON_QUEUE_SIZE, 2, 8, &mut mem);
            pci_write(&mut dev, COMMON_QUEUE_DESC, 8, desc, &mut mem);
            pci_write(&mut dev, COMMON_QUEUE_DRIVER, 8, avail, &mut mem);
            pci_write(&mut dev, COMMON_QUEUE_DEVICE, 8, used, &mut mem);
            pci_write(
                &mut dev,
                COMMON_QUEUE_MSIX_VECTOR,
                2,
                u64::from(vector),
                &mut mem,
            );
            pci_write(&mut dev, COMMON_QUEUE_ENABLE, 2, 1, &mut mem);
        }

        pci_write(&mut dev, COMMON_DEVICE_STATUS, 1, 0x0f, &mut mem);

        let stats = dev.stats();
        assert_eq!(stats.status, 0x0f);
        assert_eq!(stats.queues[0].size, 8);
        assert!(stats.queues[0].ready);
        assert_eq!(stats.queues[0].desc, rx_desc);
        assert_eq!(stats.queues[0].driver, rx_avail);
        assert_eq!(stats.queues[0].device, rx_used);
        assert_eq!(stats.queues[0].msix_vector, 0);
        assert_eq!(stats.queues[1].size, 8);
        assert!(stats.queues[1].ready);
        assert_eq!(stats.queues[1].desc, tx_desc);
        assert_eq!(stats.queues[1].driver, tx_avail);
        assert_eq!(stats.queues[1].device, tx_used);
        assert_eq!(stats.queues[1].msix_vector, 1);

        mem.write(tx_hdr, &[0; VIRTIO_NET_HDR_LEN]);
        mem.write(tx_payload, frame);
        write_desc(
            &mut mem,
            tx_desc,
            0,
            tx_hdr,
            VIRTIO_NET_HDR_LEN as u32,
            DESC_F_NEXT,
            1,
        );
        write_desc(&mut mem, tx_desc, 1, tx_payload, frame.len() as u32, 0, 0);
        mem.write(tx_avail + 2, &1u16.to_le_bytes());
        mem.write(tx_avail + 4, &0u16.to_le_bytes());

        pci_write(&mut dev, PCI_NOTIFY_CFG_OFFSET + 4, 4, 0, &mut mem);

        assert_eq!(dev.backend().transmitted_frames(), &[frame.to_vec()]);
        assert_eq!(dev.stats().notify_count, 1);
    }

    #[test]
    fn queue_setup_preserves_rx_and_tx_state_across_queue_selection() {
        let mut dev = VirtioPciNet::new_loopback();
        let mut mem = TestMem::new(0x4000_0000, 0x10000);

        setup_queue(
            &mut dev,
            &mut mem,
            0,
            0x4000_1000,
            0x4000_2000,
            0x4000_3000,
            0,
        );
        setup_queue(
            &mut dev,
            &mut mem,
            1,
            0x4000_4000,
            0x4000_5000,
            0x4000_6000,
            1,
        );

        let stats = dev.stats();
        assert_eq!(stats.queues[0].size, 8);
        assert!(stats.queues[0].ready);
        assert_eq!(stats.queues[0].desc, 0x4000_1000);
        assert_eq!(stats.queues[0].driver, 0x4000_2000);
        assert_eq!(stats.queues[0].device, 0x4000_3000);
        assert_eq!(stats.queues[0].msix_vector, 0);
        assert_eq!(stats.queues[0].notify_off, 0);
        assert_eq!(stats.queues[1].size, 8);
        assert!(stats.queues[1].ready);
        assert_eq!(stats.queues[1].desc, 0x4000_4000);
        assert_eq!(stats.queues[1].driver, 0x4000_5000);
        assert_eq!(stats.queues[1].device, 0x4000_6000);
        assert_eq!(stats.queues[1].msix_vector, 1);
        assert_eq!(stats.queues[1].notify_off, 1);

        pci_write(&mut dev, COMMON_QUEUE_SELECT, 2, 0, &mut mem);
        assert_eq!(pci_read(&mut dev, COMMON_QUEUE_SIZE, 2, &mut mem), 8);
        assert_eq!(pci_read(&mut dev, COMMON_QUEUE_NOTIFY_OFF, 2, &mut mem), 0);
        pci_write(&mut dev, COMMON_QUEUE_SELECT, 2, 1, &mut mem);
        assert_eq!(
            pci_read(&mut dev, COMMON_QUEUE_DESC, 4, &mut mem),
            0x4000_4000
        );
        assert_eq!(pci_read(&mut dev, COMMON_QUEUE_NOTIFY_OFF, 2, &mut mem), 1);
    }

    #[test]
    fn tx_notify_strips_virtio_net_header_posts_used_and_raises_msix() {
        let mut dev = VirtioPciNet::new_loopback();
        let mut mem = TestMem::new(0x4000_0000, 0x20000);
        let desc = 0x4000_1000;
        let avail = 0x4000_2000;
        let used = 0x4000_3000;
        let hdr = 0x4000_4000;
        let payload = 0x4000_5000;
        let frame = b"\x02\x00\x00\x00\x00\x01\x52\x54\x00\x42\x56\x01\x08\x00payload";

        setup_queue(&mut dev, &mut mem, 1, desc, avail, used, 1);
        program_msix_vector(&mut dev, 1, 0xfee0_0000, 0x51);
        mem.write(hdr, &[0; VIRTIO_NET_HDR_LEN]);
        mem.write(payload, frame);
        write_desc(
            &mut mem,
            desc,
            0,
            hdr,
            VIRTIO_NET_HDR_LEN as u32,
            DESC_F_NEXT,
            1,
        );
        write_desc(&mut mem, desc, 1, payload, frame.len() as u32, 0, 0);
        mem.write(avail + 2, &1u16.to_le_bytes());
        mem.write(avail + 4, &0u16.to_le_bytes());

        pci_write(&mut dev, PCI_NOTIFY_CFG_OFFSET + 4, 4, 0, &mut mem);

        assert_eq!(dev.backend().transmitted_frames(), &[frame.to_vec()]);
        assert_eq!(
            u16::from_le_bytes(mem.read(used + 2, 2).try_into().unwrap()),
            1
        );
        assert_eq!(
            u32::from_le_bytes(mem.read(used + 4, 4).try_into().unwrap()),
            0
        );
        assert_eq!(
            u32::from_le_bytes(mem.read(used + 8, 4).try_into().unwrap()),
            0
        );
        assert_eq!(
            dev.drain_pending_msix(true, false),
            vec![MsixMessage {
                vector: 1,
                address: 0xfee0_0000,
                data: 0x51,
            }]
        );
    }

    #[test]
    fn tx_notify_reuses_descriptor_and_packet_scratch_across_frames() {
        let mut dev = VirtioPciNet::new_loopback();
        let mut mem = TestMem::new(0x4000_0000, 0x20000);
        let desc = 0x4000_1000;
        let avail = 0x4000_2000;
        let used = 0x4000_3000;
        let hdr = 0x4000_4000;
        let payload = 0x4000_5000;
        let frame1 = b"\x02\x00\x00\x00\x00\x01\x52\x54\x00\x42\x56\x01\x08\x00first";
        let frame2 = b"\x02\x00\x00\x00\x00\x01\x52\x54\x00\x42\x56\x01\x08\x00again";

        setup_queue(&mut dev, &mut mem, 1, desc, avail, used, 1);
        mem.write(hdr, &[0; VIRTIO_NET_HDR_LEN]);
        mem.write(payload, frame1);
        write_desc(
            &mut mem,
            desc,
            0,
            hdr,
            VIRTIO_NET_HDR_LEN as u32,
            DESC_F_NEXT,
            1,
        );
        write_desc(&mut mem, desc, 1, payload, frame1.len() as u32, 0, 0);
        mem.write(avail + 2, &1u16.to_le_bytes());
        mem.write(avail + 4, &0u16.to_le_bytes());

        pci_write(&mut dev, PCI_NOTIFY_CFG_OFFSET + 4, 4, 0, &mut mem);

        let desc_cap = dev.net.descriptor_scratch.capacity();
        let desc_ptr = dev.net.descriptor_scratch.as_ptr();
        let packet_cap = dev.net.tx_packet_scratch.capacity();
        let packet_ptr = dev.net.tx_packet_scratch.as_ptr();
        assert!(desc_cap >= 2);
        assert!(packet_cap >= VIRTIO_NET_HDR_LEN + frame1.len());

        mem.write(hdr, &[0; VIRTIO_NET_HDR_LEN]);
        mem.write(payload, frame2);
        write_desc(
            &mut mem,
            desc,
            2,
            hdr,
            VIRTIO_NET_HDR_LEN as u32,
            DESC_F_NEXT,
            3,
        );
        write_desc(&mut mem, desc, 3, payload, frame2.len() as u32, 0, 0);
        mem.write(avail + 2, &2u16.to_le_bytes());
        mem.write(avail + 6, &2u16.to_le_bytes());

        pci_write(&mut dev, PCI_NOTIFY_CFG_OFFSET + 4, 4, 0, &mut mem);

        assert_eq!(
            dev.backend().transmitted_frames(),
            &[frame1.to_vec(), frame2.to_vec()]
        );
        assert_eq!(dev.net.descriptor_scratch.capacity(), desc_cap);
        assert_eq!(dev.net.descriptor_scratch.as_ptr(), desc_ptr);
        assert_eq!(dev.net.tx_packet_scratch.capacity(), packet_cap);
        assert_eq!(dev.net.tx_packet_scratch.as_ptr(), packet_ptr);
    }

    #[test]
    fn tx_chain_rejects_oversized_guest_length_before_growing_scratch() {
        let mut mem = TestMem::new(0x4000_0000, 0x1000);
        let desc_table = 0x4000_0100;
        write_desc(&mut mem, desc_table, 0, 0x4000_0800, u32::MAX, 0, 0);
        let mut queue = VirtioNetQueue::new(0);
        queue.size = 1;
        queue.desc = desc_table;
        let mut descs = Vec::new();
        let mut packet = Vec::with_capacity(32);
        let capacity = packet.capacity();

        assert!(!VirtioNet::<LoopbackTestBackend>::tx_frame_from_chain_into(
            &mem,
            &queue,
            0,
            &mut descs,
            &mut packet,
        ));
        assert!(packet.is_empty());
        assert_eq!(packet.capacity(), capacity);
    }

    #[test]
    fn rx_pump_prepends_header_posts_used_and_raises_msix() {
        let mut dev = VirtioPciNet::new_loopback();
        let mut mem = TestMem::new(0x4000_0000, 0x20000);
        let desc = 0x4000_1000;
        let avail = 0x4000_2000;
        let used = 0x4000_3000;
        let buf = 0x4000_4000;
        let frame = b"\x52\x54\x00\x42\x56\x01\x02\x00\x00\x00\x00\x01\x08\x00hello";

        setup_queue(&mut dev, &mut mem, 0, desc, avail, used, 0);
        program_msix_vector(&mut dev, 0, 0xfee0_0000, 0x50);
        write_desc(&mut mem, desc, 0, buf, 128, DESC_F_WRITE, 0);
        mem.write(avail + 2, &1u16.to_le_bytes());
        mem.write(avail + 4, &0u16.to_le_bytes());
        dev.backend_mut().push_receive(frame.to_vec());

        assert!(dev.pump_receive(&mut mem));

        let packet = mem.read(buf, VIRTIO_NET_HDR_LEN + frame.len());
        assert_eq!(&packet[0..10], &[0; 10]);
        assert_eq!(&packet[10..12], &1u16.to_le_bytes());
        assert_eq!(&packet[VIRTIO_NET_HDR_LEN..], frame);
        assert_eq!(
            u16::from_le_bytes(mem.read(used + 2, 2).try_into().unwrap()),
            1
        );
        assert_eq!(
            u32::from_le_bytes(mem.read(used + 4, 4).try_into().unwrap()),
            0
        );
        assert_eq!(
            u32::from_le_bytes(mem.read(used + 8, 4).try_into().unwrap()),
            (VIRTIO_NET_HDR_LEN + frame.len()) as u32
        );
        assert_eq!(
            dev.drain_pending_msix(true, false),
            vec![MsixMessage {
                vector: 0,
                address: 0xfee0_0000,
                data: 0x50,
            }]
        );
    }

    #[test]
    fn rx_pending_msix_survives_until_vector_is_programmed() {
        let mut dev = VirtioPciNet::new_loopback();
        let mut mem = TestMem::new(0x4000_0000, 0x20000);
        let desc = 0x4000_1000;
        let avail = 0x4000_2000;
        let used = 0x4000_3000;
        let buf = 0x4000_4000;
        let frame = b"\x52\x54\x00\x42\x56\x01\x02\x00\x00\x00\x00\x01\x08\x00late-vector";

        setup_queue(&mut dev, &mut mem, 0, desc, avail, used, 0);
        write_desc(&mut mem, desc, 0, buf, 128, DESC_F_WRITE, 0);
        mem.write(avail + 2, &1u16.to_le_bytes());
        mem.write(avail + 4, &0u16.to_le_bytes());
        dev.backend_mut().push_receive(frame.to_vec());

        assert!(dev.pump_receive(&mut mem));
        assert!(dev.stats().queues[0].pending_msix);
        assert_eq!(dev.drain_pending_msix(true, false), Vec::new());
        assert!(dev.stats().queues[0].pending_msix);

        program_msix_vector(&mut dev, 0, 0xfee0_0000, 0x50);

        assert_eq!(
            dev.drain_pending_msix(true, false),
            vec![MsixMessage {
                vector: 0,
                address: 0xfee0_0000,
                data: 0x50,
            }]
        );
        assert!(!dev.stats().queues[0].pending_msix);
    }

    #[test]
    fn rx_pump_scatters_header_and_frame_across_split_descriptors() {
        let mut dev = VirtioPciNet::new_loopback();
        let mut mem = TestMem::new(0x4000_0000, 0x20000);
        let desc = 0x4000_1000;
        let avail = 0x4000_2000;
        let used = 0x4000_3000;
        let header_prefix = 0x4000_4000;
        let header_suffix = 0x4000_4100;
        let payload_buf = 0x4000_4200;
        let frame = b"\x52\x54\x00\x42\x56\x01\x02\x00\x00\x00\x00\x01\x08\x00split-rx";

        setup_queue(&mut dev, &mut mem, 0, desc, avail, used, 0);
        write_desc(
            &mut mem,
            desc,
            0,
            header_prefix,
            10,
            DESC_F_WRITE | DESC_F_NEXT,
            1,
        );
        write_desc(
            &mut mem,
            desc,
            1,
            header_suffix,
            4,
            DESC_F_WRITE | DESC_F_NEXT,
            2,
        );
        write_desc(&mut mem, desc, 2, payload_buf, 128, DESC_F_WRITE, 0);
        mem.write(avail + 2, &1u16.to_le_bytes());
        mem.write(avail + 4, &0u16.to_le_bytes());
        dev.backend_mut().push_receive(frame.to_vec());

        assert!(dev.pump_receive(&mut mem));

        assert_eq!(mem.read(header_prefix, 10), [0; 10]);
        assert_eq!(&mem.read(header_suffix, 4)[..2], &1u16.to_le_bytes());
        assert_eq!(&mem.read(header_suffix + 2, 2), &frame[..2]);
        assert_eq!(&mem.read(payload_buf, frame.len() - 2), &frame[2..]);
        assert_eq!(
            u32::from_le_bytes(mem.read(used + 8, 4).try_into().unwrap()),
            (VIRTIO_NET_HDR_LEN + frame.len()) as u32
        );
    }

    #[test]
    fn rx_pump_reuses_descriptor_scratch_across_frames_without_packet_copy() {
        let mut dev = VirtioPciNet::new_loopback();
        let mut mem = TestMem::new(0x4000_0000, 0x20000);
        let desc = 0x4000_1000;
        let avail = 0x4000_2000;
        let used = 0x4000_3000;
        let buf1 = 0x4000_4000;
        let buf2 = 0x4000_5000;
        let frame1 = b"\x52\x54\x00\x42\x56\x01\x02\x00\x00\x00\x00\x01\x08\x00one";
        let frame2 = b"\x52\x54\x00\x42\x56\x01\x02\x00\x00\x00\x00\x01\x08\x00two";

        setup_queue(&mut dev, &mut mem, 0, desc, avail, used, 0);
        write_desc(&mut mem, desc, 0, buf1, 128, DESC_F_WRITE, 0);
        mem.write(avail + 2, &1u16.to_le_bytes());
        mem.write(avail + 4, &0u16.to_le_bytes());
        dev.backend_mut().push_receive(frame1.to_vec());

        assert!(dev.pump_receive(&mut mem));

        let desc_cap = dev.net.descriptor_scratch.capacity();
        let desc_ptr = dev.net.descriptor_scratch.as_ptr();
        assert!(desc_cap >= 1);

        write_desc(&mut mem, desc, 1, buf2, 128, DESC_F_WRITE, 0);
        mem.write(avail + 2, &2u16.to_le_bytes());
        mem.write(avail + 6, &1u16.to_le_bytes());
        dev.backend_mut().push_receive(frame2.to_vec());

        assert!(dev.pump_receive(&mut mem));

        assert_eq!(
            &mem.read(buf1 + VIRTIO_NET_HDR_LEN as u64, frame1.len()),
            frame1
        );
        assert_eq!(
            &mem.read(buf2 + VIRTIO_NET_HDR_LEN as u64, frame2.len()),
            frame2
        );
        assert_eq!(dev.net.descriptor_scratch.capacity(), desc_cap);
        assert_eq!(dev.net.descriptor_scratch.as_ptr(), desc_ptr);
    }

    #[test]
    fn rx_pump_reuses_backend_receive_scratch_across_frames() {
        let mut dev = VirtioPciNet::new_loopback();
        let mut mem = TestMem::new(0x4000_0000, 0x20000);
        let desc = 0x4000_1000;
        let avail = 0x4000_2000;
        let used = 0x4000_3000;
        let buf1 = 0x4000_4000;
        let buf2 = 0x4000_5000;
        let frame1 = [0x33u8; 96];
        let frame2 = [0x44u8; 64];

        setup_queue(&mut dev, &mut mem, 0, desc, avail, used, 0);
        write_desc(&mut mem, desc, 0, buf1, 160, DESC_F_WRITE, 0);
        mem.write(avail + 2, &1u16.to_le_bytes());
        mem.write(avail + 4, &0u16.to_le_bytes());
        dev.backend_mut().push_receive(frame1);

        assert!(dev.pump_receive(&mut mem));
        let rx_cap = dev.net.rx_frame_scratch.capacity();
        let rx_ptr = dev.net.rx_frame_scratch.as_ptr();
        assert!(rx_cap >= frame1.len());
        assert!(dev.net.rx_frame_scratch.is_empty());

        write_desc(&mut mem, desc, 1, buf2, 160, DESC_F_WRITE, 0);
        mem.write(avail + 2, &2u16.to_le_bytes());
        mem.write(avail + 6, &1u16.to_le_bytes());
        dev.backend_mut().push_receive(frame2);

        assert!(dev.pump_receive(&mut mem));
        assert_eq!(dev.net.rx_frame_scratch.capacity(), rx_cap);
        assert_eq!(dev.net.rx_frame_scratch.as_ptr(), rx_ptr);
        assert!(dev.net.rx_frame_scratch.is_empty());
        assert_eq!(
            &mem.read(buf1 + VIRTIO_NET_HDR_LEN as u64, frame1.len()),
            &frame1
        );
        assert_eq!(
            &mem.read(buf2 + VIRTIO_NET_HDR_LEN as u64, frame2.len()),
            &frame2
        );
    }

    #[test]
    fn rx_without_buffers_holds_one_frame_until_buffer_is_posted() {
        let mut dev = VirtioPciNet::new_loopback();
        let mut mem = TestMem::new(0x4000_0000, 0x20000);
        let desc = 0x4000_1000;
        let avail = 0x4000_2000;
        let used = 0x4000_3000;
        let buf = 0x4000_4000;
        let frame = b"\xaa\xbb\xcc\xdd";

        dev.backend_mut().push_receive(frame.to_vec());
        assert!(!dev.pump_receive(&mut mem));
        assert!(dev.stats().pending_rx_frame);
        assert!(dev.net.rx_frame_scratch.is_empty());

        setup_queue(&mut dev, &mut mem, 0, desc, avail, used, 0);
        write_desc(&mut mem, desc, 0, buf, 64, DESC_F_WRITE, 0);
        mem.write(avail + 2, &1u16.to_le_bytes());
        mem.write(avail + 4, &0u16.to_le_bytes());

        assert!(dev.pump_receive(&mut mem));
        assert!(!dev.stats().pending_rx_frame);
        assert!(dev.net.rx_frame_scratch.capacity() >= frame.len());
        assert!(dev.net.rx_frame_scratch.is_empty());
        assert_eq!(
            &mem.read(buf + VIRTIO_NET_HDR_LEN as u64, frame.len()),
            frame
        );
    }
}
