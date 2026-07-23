//! Split out of virtio_net.rs to keep files under 850 lines.

use super::*;

use std::collections::VecDeque;

use crate::fwcfg::GuestMemoryMut;

pub(crate) const MAGIC_VALUE: u32 = 0x7472_6976;
pub(crate) const VERSION_MODERN: u32 = 2;
pub(crate) const DEVICE_ID_NET: u32 = 1;
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

pub(crate) const VIRTIO_NET_F_MAC: u32 = 1 << 5;
pub(crate) const VIRTIO_NET_F_STATUS: u32 = 1 << 16;
pub(crate) const VIRTIO_F_VERSION_1: u32 = 1 << 0;
pub(crate) const VIRTIO_NET_S_LINK_UP: u16 = 1;
pub(crate) const VIRTIO_MSI_NO_VECTOR: u16 = 0xffff;

pub(crate) const QUEUE_RX: usize = 0;
pub(crate) const QUEUE_TX: usize = 1;
pub(crate) const QUEUE_COUNT: usize = 2;
pub(crate) const QUEUE_MAX: u16 = 256;
pub(crate) const DESC_SIZE: u64 = 16;
pub(crate) const DESC_F_NEXT: u16 = 1;
pub(crate) const DESC_F_WRITE: u16 = 2;
pub(crate) const VIRTIO_NET_HDR_LEN: usize = 12;
pub(crate) const MAX_TX_PACKET_LEN: usize = VIRTIO_NET_HDR_LEN + 65_535;
pub(crate) const DEFAULT_MAC: [u8; 6] = [0x52, 0x54, 0x00, 0x42, 0x56, 0x01];

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
    pub(crate) transmitted: Vec<Vec<u8>>,
    pub(crate) receive: VecDeque<Vec<u8>>,
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

    pub(crate) fn access_common(
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
        self.write_mmio_alias(offset, value, mem);
    }

    pub(crate) fn write_mmio_alias(
        &mut self,
        offset: u64,
        value: u64,
        mem: &mut dyn GuestMemoryMut,
    ) {
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

    pub(crate) fn selected_queue(&self) -> Option<VirtioNetQueue> {
        self.queues.get(self.queue_sel as usize).copied()
    }

    pub(crate) fn write_selected_queue(&mut self, write: impl FnOnce(&mut VirtioNetQueue)) {
        if let Some(queue) = self.queues.get_mut(self.queue_sel as usize) {
            write(queue);
        }
    }

    pub(crate) fn device_features(&self) -> u32 {
        offered_features_word(self.device_features_sel)
    }

    pub(crate) fn config_read(&self, offset: u64, size: u8) -> u64 {
        let mut config = [0u8; 0x40];
        config[0..6].copy_from_slice(&self.mac);
        config[6..8].copy_from_slice(&VIRTIO_NET_S_LINK_UP.to_le_bytes());
        read_le_from_bytes(&config, offset, size).unwrap_or(0)
    }

    pub(crate) fn notify_queue(&mut self, queue_index: u16, mem: &mut dyn GuestMemoryMut) {
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

    pub(crate) fn process_tx_queue(&mut self, mem: &mut dyn GuestMemoryMut) {
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

    pub(crate) fn tx_frame_from_chain_into(
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

    pub(crate) fn deliver_rx_frame(&mut self, frame: &[u8], mem: &mut dyn GuestMemoryMut) -> bool {
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

    pub(crate) fn scatter_write_slices(
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

    pub(crate) fn mark_queue_interrupt(&mut self, queue_index: usize) {
        if let Some(queue) = self.queues.get_mut(queue_index) {
            queue.pending_msix = true;
            if let Some(bit) = queue_bit(queue_index) {
                self.pending_msix_queue_bits |= bit;
            }
        }
        self.interrupt_status |= 1;
    }

    pub(crate) fn descriptor_chain_into(
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

    pub(crate) fn write_used(
        mem: &mut dyn GuestMemoryMut,
        queue: &VirtioNetQueue,
        id: u16,
        len: u32,
    ) {
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
