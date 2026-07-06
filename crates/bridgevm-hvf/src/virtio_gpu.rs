//! Minimal modern virtio-gpu PCI device model.
//!
//! This is a 2D-only display device for Windows' Display-Only virtio GPU
//! driver. It deliberately mirrors the proven modern virtio-pci transport in
//! `virtio_net.rs` instead of sharing transport code, so existing net/block
//! paths keep their validated behavior.

use std::collections::BTreeMap;

use crate::{
    fwcfg::GuestMemoryMut,
    msix::{MsixMessage, MsixTable},
    pcie::{
        VIRTIO_GPU_MSIX_PBA_OFFSET, VIRTIO_GPU_MSIX_TABLE_OFFSET, VIRTIO_GPU_MSIX_VECTOR_COUNT,
    },
    ramfb::DRM_FORMAT_XRGB8888,
    virtio_gpu_3d::{
        self, CompletedFence, CtrlHdr3d, GpuShmMapPort, VirtioGpu3d, VirtioGpu3dBackend,
        VirtioGpu3dStats, VIRTIO_GPU_CMD_CTX_ATTACH_RESOURCE, VIRTIO_GPU_CMD_CTX_CREATE,
        VIRTIO_GPU_CMD_CTX_DESTROY, VIRTIO_GPU_CMD_CTX_DETACH_RESOURCE, VIRTIO_GPU_CMD_GET_CAPSET,
        VIRTIO_GPU_CMD_GET_CAPSET_INFO, VIRTIO_GPU_CMD_RESOURCE_CREATE_BLOB,
        VIRTIO_GPU_CMD_RESOURCE_MAP_BLOB, VIRTIO_GPU_CMD_RESOURCE_UNMAP_BLOB,
        VIRTIO_GPU_CMD_SUBMIT_3D, VIRTIO_GPU_FLAG_FENCE, VIRTIO_GPU_F_CONTEXT_INIT,
        VIRTIO_GPU_F_RESOURCE_BLOB, VIRTIO_GPU_F_VIRGL,
    },
};

const MAGIC_VALUE: u32 = 0x7472_6976;
const VERSION_MODERN: u32 = 2;
const DEVICE_ID_GPU: u32 = 16;
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

const VIRTIO_GPU_F_EDID: u32 = 1 << 1;
const VIRTIO_F_VERSION_1: u32 = 1 << 0;
const VIRTIO_MSI_NO_VECTOR: u16 = 0xffff;

const QUEUE_CONTROL: usize = 0;
const QUEUE_CURSOR: usize = 1;
const QUEUE_COUNT: usize = 2;
const QUEUE_MAX: u16 = 64;
const DESC_SIZE: u64 = 16;
const DESC_F_NEXT: u16 = 1;
const DESC_F_WRITE: u16 = 2;

const VIRTIO_GPU_CMD_GET_DISPLAY_INFO: u32 = 0x0100;
const VIRTIO_GPU_CMD_RESOURCE_CREATE_2D: u32 = 0x0101;
const VIRTIO_GPU_CMD_RESOURCE_UNREF: u32 = 0x0102;
const VIRTIO_GPU_CMD_SET_SCANOUT: u32 = 0x0103;
const VIRTIO_GPU_CMD_RESOURCE_FLUSH: u32 = 0x0104;
const VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D: u32 = 0x0105;
const VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING: u32 = 0x0106;
const VIRTIO_GPU_CMD_RESOURCE_DETACH_BACKING: u32 = 0x0107;
const VIRTIO_GPU_CMD_GET_EDID: u32 = 0x010a;
const VIRTIO_GPU_CMD_UPDATE_CURSOR: u32 = 0x0300;
const VIRTIO_GPU_CMD_MOVE_CURSOR: u32 = 0x0301;
const VIRTIO_GPU_RESP_OK_NODATA: u32 = 0x1100;
const VIRTIO_GPU_RESP_OK_DISPLAY_INFO: u32 = 0x1101;
const VIRTIO_GPU_RESP_OK_EDID: u32 = 0x1104;
const VIRTIO_GPU_RESP_ERR_UNSPEC: u32 = 0x1200;

const FORMAT_B8G8R8A8_UNORM: u32 = 1;
const FORMAT_B8G8R8X8_UNORM: u32 = 2;
const FORMAT_X8R8G8B8_UNORM: u32 = 3;
const FORMAT_R8G8B8X8_UNORM: u32 = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VirtioGpuResult {
    ReadValue(u64),
    WriteAck,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VirtioPciGpuOp {
    Read { size: u8 },
    Write { size: u8, value: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VirtioGpuScanout<'a> {
    pub bytes: &'a [u8],
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub fourcc: u32,
}

#[derive(Debug)]
pub struct VirtioGpu {
    width: u32,
    height: u32,
    device_features_sel: u32,
    driver_features_sel: u32,
    driver_features: [u32; 2],
    config_msix_vector: u16,
    queue_sel: u32,
    queues: [VirtioGpuQueue; QUEUE_COUNT],
    status: u32,
    interrupt_status: u32,
    events_clear: u32,
    resources: BTreeMap<u32, GpuResource>,
    scanout_resource: Option<u32>,
    scanout: Vec<u8>,
    three_d: VirtioGpu3d,
    pending_fenced: Vec<PendingFencedResponse>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct VirtioGpuQueue {
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

impl VirtioGpuQueue {
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
pub struct VirtioGpuQueueStats {
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
pub struct VirtioGpuStats {
    pub status: u32,
    pub interrupt_status: u32,
    pub driver_features: u64,
    pub resources: usize,
    pub scanout_active: bool,
    pub three_d: VirtioGpu3dStats,
    pub queues: [VirtioGpuQueueStats; QUEUE_COUNT],
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GpuResource {
    format: u32,
    width: u32,
    height: u32,
    host_pixels: Vec<u8>,
    backing: Vec<BackingEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BackingEntry {
    addr: u64,
    len: u32,
}

#[derive(Debug, Clone)]
struct PendingFencedResponse {
    queue_index: usize,
    queue: VirtioGpuQueue,
    head: u16,
    descs: Vec<Descriptor>,
    response: Vec<u8>,
    fence: CompletedFence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ChainCompletion {
    Immediate(u32),
    Parked,
}

#[derive(Debug, Clone, Copy)]
struct CtrlHdr {
    typ: u32,
    flags: u32,
    fence_id: u64,
    ctx_id: u32,
    padding: u32,
}

#[derive(Debug, Clone, Copy)]
struct Rect {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

impl VirtioGpu {
    pub fn new(width: u32, height: u32) -> Self {
        assert!(
            width > 0 && height > 0,
            "virtio-gpu resolution must be non-zero"
        );
        let len = scanout_len(width, height);
        Self {
            width,
            height,
            device_features_sel: 0,
            driver_features_sel: 0,
            driver_features: [0; 2],
            config_msix_vector: VIRTIO_MSI_NO_VECTOR,
            queue_sel: 0,
            queues: [VirtioGpuQueue::new(0), VirtioGpuQueue::new(1)],
            status: 0,
            interrupt_status: 0,
            events_clear: 0,
            resources: BTreeMap::new(),
            scanout_resource: None,
            scanout: vec![0; len],
            three_d: VirtioGpu3d::new(),
            pending_fenced: Vec::new(),
        }
    }

    pub fn with_3d_backend(width: u32, height: u32, backend: Box<dyn VirtioGpu3dBackend>) -> Self {
        let mut gpu = Self::new(width, height);
        gpu.three_d = VirtioGpu3d::with_backend(backend);
        gpu
    }

    pub fn set_shm_map_port(&mut self, port: Box<dyn GpuShmMapPort>, window_size: u64) {
        self.three_d.set_shm_map_port(port, window_size);
    }

    pub fn new_from_env() -> Self {
        let (width, height) = parse_resolution_env();
        Self::new(width, height)
    }

    pub fn stats(&self) -> VirtioGpuStats {
        let mut stats = VirtioGpuStats {
            status: self.status,
            interrupt_status: self.interrupt_status,
            driver_features: u64::from(self.driver_features[0])
                | (u64::from(self.driver_features[1]) << 32),
            resources: self.resources.len(),
            scanout_active: self.scanout_resource.is_some(),
            three_d: self.three_d.stats(self.pending_fenced.len()),
            queues: [VirtioGpuQueueStats::default(); QUEUE_COUNT],
        };
        for (out, queue) in stats.queues.iter_mut().zip(self.queues) {
            *out = VirtioGpuQueueStats {
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
        let width = self.width;
        let height = self.height;
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
        self.events_clear = 0;
        self.resources.clear();
        self.scanout_resource = None;
        self.scanout = vec![0; scanout_len(width, height)];
        self.three_d.reset();
        self.pending_fenced.clear();
    }

    pub fn scanout(&self) -> Option<VirtioGpuScanout<'_>> {
        self.scanout_resource.map(|_| VirtioGpuScanout {
            bytes: &self.scanout,
            width: self.width,
            height: self.height,
            stride: self.width * 4,
            fourcc: DRM_FORMAT_XRGB8888,
        })
    }

    fn access_common(
        &mut self,
        offset: u64,
        is_write: bool,
        size: u8,
        value: u64,
        mem: &mut dyn GuestMemoryMut,
    ) -> VirtioGpuResult {
        if !is_write {
            return VirtioGpuResult::ReadValue(self.read_common(offset, size));
        }
        self.write_common(offset, size, value, mem);
        VirtioGpuResult::WriteAck
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
            REG_DEVICE_ID => u64::from(DEVICE_ID_GPU),
            REG_VENDOR_ID => u64::from(VENDOR_ID_QEMU),
            REG_DEVICE_FEATURES => u64::from(self.offered_features_word(self.device_features_sel)),
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
            self.driver_features[index] = (value as u32) & self.offered_features_word(index as u32);
        }
    }

    fn offered_features_word(&self, select: u32) -> u32 {
        match select {
            0 => {
                let mut features = VIRTIO_GPU_F_EDID;
                if self.three_d.has_backend() {
                    features |=
                        VIRTIO_GPU_F_VIRGL | VIRTIO_GPU_F_RESOURCE_BLOB | VIRTIO_GPU_F_CONTEXT_INIT;
                }
                features
            }
            1 => VIRTIO_F_VERSION_1,
            _ => 0,
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
                u64::from(self.offered_features_word(self.device_features_sel)),
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

    fn selected_queue(&self) -> Option<VirtioGpuQueue> {
        self.queues.get(self.queue_sel as usize).copied()
    }

    fn write_selected_queue(&mut self, write: impl FnOnce(&mut VirtioGpuQueue)) {
        if let Some(queue) = self.queues.get_mut(self.queue_sel as usize) {
            write(queue);
        }
    }

    fn config_read(&self, offset: u64, size: u8) -> u64 {
        // struct virtio_gpu_config: le32 events_read @0, le32 events_clear @4,
        // le32 num_scanouts @8, le32 num_capsets @12. num_capsets was being
        // written into the num_scanouts slot, so Linux saw "number of cap
        // sets: 0" and never queried the venus capset (and a 2D-only device
        // reported zero scanouts).
        let mut config = [0u8; 16];
        config[4..8].copy_from_slice(&self.events_clear.to_le_bytes());
        config[8..12].copy_from_slice(&1u32.to_le_bytes());
        let num_capsets = if self.three_d.has_backend() {
            1u32
        } else {
            0u32
        };
        config[12..16].copy_from_slice(&num_capsets.to_le_bytes());
        read_le_from_bytes(&config, offset, size).unwrap_or(0)
    }

    fn config_write(&mut self, offset: u64, size: u8, value: u64) {
        if common_access_touches(4, 4, offset, size) {
            self.events_clear =
                write_common_register(self.events_clear.into(), 4, 4, offset, size, value) as u32;
        }
    }

    fn notify_queue(&mut self, queue_index: u16, mem: &mut dyn GuestMemoryMut) {
        match usize::from(queue_index) {
            QUEUE_CONTROL => self.process_control_queue(mem),
            QUEUE_CURSOR => self.process_cursor_queue(mem),
            _ => {}
        }
    }

    fn process_control_queue(&mut self, mem: &mut dyn GuestMemoryMut) {
        self.process_queue(QUEUE_CONTROL, mem, true);
    }

    fn process_cursor_queue(&mut self, mem: &mut dyn GuestMemoryMut) {
        self.process_queue(QUEUE_CURSOR, mem, false);
    }

    fn process_queue(&mut self, queue_index: usize, mem: &mut dyn GuestMemoryMut, control: bool) {
        let queue = self.queues[queue_index];
        if !queue.ready || queue.size == 0 || queue.desc == 0 {
            return;
        }
        let Some(avail_idx) = read_u16(mem, queue.driver + 2) else {
            return;
        };
        while self.queues[queue_index].last_avail_idx != avail_idx {
            let last_avail_idx = self.queues[queue_index].last_avail_idx;
            let ring_off = 4 + u64::from(last_avail_idx % queue.size) * 2;
            let Some(head) = read_u16(mem, queue.driver + ring_off) else {
                return;
            };
            let completion = self.process_chain(mem, &queue, queue_index, head, control);
            self.queues[queue_index].last_avail_idx = last_avail_idx.wrapping_add(1);
            if let ChainCompletion::Immediate(used_len) = completion {
                Self::write_used(mem, &queue, head, used_len);
                self.mark_queue_interrupt(queue_index);
            }
        }
        self.drain_completed_fences(mem);
    }

    fn process_chain(
        &mut self,
        mem: &mut dyn GuestMemoryMut,
        queue: &VirtioGpuQueue,
        queue_index: usize,
        head: u16,
        control: bool,
    ) -> ChainCompletion {
        let Some(descs) = Self::descriptor_chain(mem, queue, head) else {
            return ChainCompletion::Immediate(0);
        };
        let request = Self::gather_readable(mem, &descs);
        let response = if control {
            self.handle_control_request(mem, &request)
        } else {
            self.handle_cursor_request(&request)
        };
        let Some(hdr) = CtrlHdr::parse(&request) else {
            return ChainCompletion::Immediate(Self::scatter_write(mem, &descs, &response));
        };
        if control && hdr.flags & VIRTIO_GPU_FLAG_FENCE != 0 && self.three_d.has_backend() {
            let fence = CompletedFence {
                ctx_id: hdr.ctx_id,
                ring_idx: hdr.ring_idx(),
                fence_id: hdr.fence_id,
            };
            if self.three_d.create_fence(fence) {
                self.pending_fenced.push(PendingFencedResponse {
                    queue_index,
                    queue: *queue,
                    head,
                    descs,
                    response,
                    fence,
                });
                return ChainCompletion::Parked;
            }
        }
        ChainCompletion::Immediate(Self::scatter_write(mem, &descs, &response))
    }

    fn handle_cursor_request(&mut self, request: &[u8]) -> Vec<u8> {
        let hdr = CtrlHdr::parse(request);
        match hdr.map(|h| h.typ) {
            Some(VIRTIO_GPU_CMD_UPDATE_CURSOR | VIRTIO_GPU_CMD_MOVE_CURSOR) => {
                response_hdr(VIRTIO_GPU_RESP_OK_NODATA, hdr)
            }
            _ => response_hdr(VIRTIO_GPU_RESP_ERR_UNSPEC, hdr),
        }
    }

    fn handle_control_request(&mut self, mem: &dyn GuestMemoryMut, request: &[u8]) -> Vec<u8> {
        let Some(hdr) = CtrlHdr::parse(request) else {
            return response_hdr(VIRTIO_GPU_RESP_ERR_UNSPEC, None);
        };
        match hdr.typ {
            VIRTIO_GPU_CMD_GET_DISPLAY_INFO => self.response_display_info(Some(hdr)),
            VIRTIO_GPU_CMD_GET_EDID => self.response_edid(Some(hdr)),
            VIRTIO_GPU_CMD_RESOURCE_CREATE_2D => self.resource_create_2d(request, Some(hdr)),
            VIRTIO_GPU_CMD_RESOURCE_UNREF => self.resource_unref(request, Some(hdr)),
            VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING => self.attach_backing(request, Some(hdr)),
            VIRTIO_GPU_CMD_RESOURCE_DETACH_BACKING => self.detach_backing(request, Some(hdr)),
            VIRTIO_GPU_CMD_SET_SCANOUT => self.set_scanout(request, Some(hdr)),
            VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D => self.transfer_to_host_2d(mem, request, Some(hdr)),
            VIRTIO_GPU_CMD_RESOURCE_FLUSH => self.resource_flush(request, Some(hdr)),
            VIRTIO_GPU_CMD_GET_CAPSET_INFO
            | VIRTIO_GPU_CMD_GET_CAPSET
            | VIRTIO_GPU_CMD_RESOURCE_CREATE_BLOB
            | VIRTIO_GPU_CMD_CTX_CREATE
            | VIRTIO_GPU_CMD_CTX_DESTROY
            | VIRTIO_GPU_CMD_CTX_ATTACH_RESOURCE
            | VIRTIO_GPU_CMD_CTX_DETACH_RESOURCE
            | VIRTIO_GPU_CMD_SUBMIT_3D
            | VIRTIO_GPU_CMD_RESOURCE_MAP_BLOB
            | VIRTIO_GPU_CMD_RESOURCE_UNMAP_BLOB => {
                let hdr3d = CtrlHdr3d::parse(request).unwrap();
                self.three_d
                    .handle_with_mem(Some(mem), request, hdr3d)
                    .unwrap_or_else(|| {
                        virtio_gpu_3d::response_hdr(
                            virtio_gpu_3d::VIRTIO_GPU_RESP_ERR_UNSPEC,
                            Some(hdr3d),
                        )
                    })
            }
            _ => response_hdr(VIRTIO_GPU_RESP_ERR_UNSPEC, Some(hdr)),
        }
    }

    pub fn drain_completed_fences(&mut self, mem: &mut dyn GuestMemoryMut) {
        let completed = self.three_d.drain_completed_fences();
        if completed.is_empty() || self.pending_fenced.is_empty() {
            return;
        }
        let mut remaining = Vec::with_capacity(self.pending_fenced.len());
        let pending = std::mem::take(&mut self.pending_fenced);
        for pending_response in pending {
            let ready = completed.iter().any(|completed| {
                completed.ctx_id == pending_response.fence.ctx_id
                    && completed.ring_idx == pending_response.fence.ring_idx
                    && completed.fence_id >= pending_response.fence.fence_id
            });
            if ready {
                let used_len =
                    Self::scatter_write(mem, &pending_response.descs, &pending_response.response);
                Self::write_used(
                    mem,
                    &pending_response.queue,
                    pending_response.head,
                    used_len,
                );
                self.mark_queue_interrupt(pending_response.queue_index);
            } else {
                remaining.push(pending_response);
            }
        }
        self.pending_fenced = remaining;
    }

    fn response_display_info(&self, hdr: Option<CtrlHdr>) -> Vec<u8> {
        let mut out = response_hdr(VIRTIO_GPU_RESP_OK_DISPLAY_INFO, hdr);
        for scanout in 0..16 {
            if scanout == 0 {
                push_rect(
                    &mut out,
                    Rect {
                        x: 0,
                        y: 0,
                        width: self.width,
                        height: self.height,
                    },
                );
                out.extend_from_slice(&1u32.to_le_bytes());
                out.extend_from_slice(&0u32.to_le_bytes());
            } else {
                out.extend_from_slice(&[0u8; 24]);
            }
        }
        out
    }

    fn response_edid(&self, hdr: Option<CtrlHdr>) -> Vec<u8> {
        let mut out = response_hdr(VIRTIO_GPU_RESP_OK_EDID, hdr);
        out.extend_from_slice(&128u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        let edid = build_edid(self.width, self.height);
        out.extend_from_slice(&edid);
        out.resize(out.len() + (1024 - 128), 0);
        out
    }

    fn resource_create_2d(&mut self, request: &[u8], hdr: Option<CtrlHdr>) -> Vec<u8> {
        let Some(resource_id) = read_le_u32(request, 24) else {
            return response_hdr(VIRTIO_GPU_RESP_ERR_UNSPEC, hdr);
        };
        let format = read_le_u32(request, 28).unwrap_or(0);
        let width = read_le_u32(request, 32).unwrap_or(0);
        let height = read_le_u32(request, 36).unwrap_or(0);
        if resource_id == 0 || width == 0 || height == 0 || !format_supported(format) {
            return response_hdr(VIRTIO_GPU_RESP_ERR_UNSPEC, hdr);
        }
        let Some(len) = u64::from(width)
            .checked_mul(u64::from(height))
            .and_then(|pixels| pixels.checked_mul(4))
            .and_then(|bytes| usize::try_from(bytes).ok())
        else {
            return response_hdr(VIRTIO_GPU_RESP_ERR_UNSPEC, hdr);
        };
        self.resources.insert(
            resource_id,
            GpuResource {
                format,
                width,
                height,
                host_pixels: vec![0; len],
                backing: Vec::new(),
            },
        );
        self.three_d.register_2d_resource(resource_id);
        response_hdr(VIRTIO_GPU_RESP_OK_NODATA, hdr)
    }

    fn resource_unref(&mut self, request: &[u8], hdr: Option<CtrlHdr>) -> Vec<u8> {
        if let Some(resource_id) = read_le_u32(request, 24) {
            self.resources.remove(&resource_id);
            self.three_d.unref_resource(resource_id);
            if self.scanout_resource == Some(resource_id) {
                self.scanout_resource = None;
            }
        }
        response_hdr(VIRTIO_GPU_RESP_OK_NODATA, hdr)
    }

    fn attach_backing(&mut self, request: &[u8], hdr: Option<CtrlHdr>) -> Vec<u8> {
        let Some(resource_id) = read_le_u32(request, 24) else {
            return response_hdr(VIRTIO_GPU_RESP_ERR_UNSPEC, hdr);
        };
        let nr_entries = read_le_u32(request, 28).unwrap_or(0);
        let Some(resource) = self.resources.get_mut(&resource_id) else {
            return response_hdr(VIRTIO_GPU_RESP_ERR_UNSPEC, hdr);
        };
        let mut entries = Vec::new();
        let mut offset = 32usize;
        for _ in 0..nr_entries {
            let Some(addr) = read_le_u64(request, offset) else {
                return response_hdr(VIRTIO_GPU_RESP_ERR_UNSPEC, hdr);
            };
            let Some(len) = read_le_u32(request, offset + 8) else {
                return response_hdr(VIRTIO_GPU_RESP_ERR_UNSPEC, hdr);
            };
            entries.push(BackingEntry { addr, len });
            offset += 16;
        }
        resource.backing = entries;
        response_hdr(VIRTIO_GPU_RESP_OK_NODATA, hdr)
    }

    fn detach_backing(&mut self, request: &[u8], hdr: Option<CtrlHdr>) -> Vec<u8> {
        if let Some(resource_id) = read_le_u32(request, 24) {
            if let Some(resource) = self.resources.get_mut(&resource_id) {
                resource.backing.clear();
            }
        }
        response_hdr(VIRTIO_GPU_RESP_OK_NODATA, hdr)
    }

    fn set_scanout(&mut self, request: &[u8], hdr: Option<CtrlHdr>) -> Vec<u8> {
        let scanout_id = read_le_u32(request, 40).unwrap_or(u32::MAX);
        let resource_id = read_le_u32(request, 44).unwrap_or(0);
        if scanout_id != 0 {
            return response_hdr(VIRTIO_GPU_RESP_OK_NODATA, hdr);
        }
        if resource_id == 0 {
            self.scanout_resource = None;
        } else if self.resources.contains_key(&resource_id) {
            self.scanout_resource = Some(resource_id);
        } else {
            return response_hdr(VIRTIO_GPU_RESP_ERR_UNSPEC, hdr);
        }
        response_hdr(VIRTIO_GPU_RESP_OK_NODATA, hdr)
    }

    fn transfer_to_host_2d(
        &mut self,
        mem: &dyn GuestMemoryMut,
        request: &[u8],
        hdr: Option<CtrlHdr>,
    ) -> Vec<u8> {
        let rect = read_rect(request, 24).unwrap_or(Rect {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        });
        let offset = read_le_u64(request, 40).unwrap_or(0);
        let resource_id = read_le_u32(request, 48).unwrap_or(0);
        let Some(resource) = self.resources.get_mut(&resource_id) else {
            return response_hdr(VIRTIO_GPU_RESP_ERR_UNSPEC, hdr);
        };
        copy_backing_to_resource(mem, resource, rect, offset);
        response_hdr(VIRTIO_GPU_RESP_OK_NODATA, hdr)
    }

    fn resource_flush(&mut self, request: &[u8], hdr: Option<CtrlHdr>) -> Vec<u8> {
        let rect = read_rect(request, 24).unwrap_or(Rect {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        });
        let resource_id = read_le_u32(request, 40).unwrap_or(0);
        if self.scanout_resource == Some(resource_id) {
            if let Some(resource) = self.resources.get(&resource_id) {
                composite_resource_to_scanout(
                    &mut self.scanout,
                    self.width,
                    self.height,
                    resource,
                    rect,
                );
            }
        }
        response_hdr(VIRTIO_GPU_RESP_OK_NODATA, hdr)
    }

    fn mark_queue_interrupt(&mut self, queue_index: usize) {
        if let Some(queue) = self.queues.get_mut(queue_index) {
            queue.pending_msix = true;
        }
        self.interrupt_status |= 1;
    }

    fn descriptor_chain(
        mem: &dyn GuestMemoryMut,
        queue: &VirtioGpuQueue,
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

    fn gather_readable(mem: &dyn GuestMemoryMut, descs: &[Descriptor]) -> Vec<u8> {
        let mut out = Vec::new();
        for desc in descs {
            if desc.flags & DESC_F_WRITE != 0 {
                continue;
            }
            if let Some(mut bytes) = mem.read_bytes(desc.addr, desc.len as usize) {
                out.append(&mut bytes);
            }
        }
        out
    }

    fn scatter_write(mem: &mut dyn GuestMemoryMut, descs: &[Descriptor], bytes: &[u8]) -> u32 {
        let mut offset = 0usize;
        for desc in descs {
            if desc.flags & DESC_F_WRITE == 0 {
                continue;
            }
            let writable = (desc.len as usize).min(bytes.len().saturating_sub(offset));
            if writable == 0 {
                continue;
            }
            if !mem.write_bytes(desc.addr, &bytes[offset..offset + writable]) {
                break;
            }
            offset += writable;
            if offset == bytes.len() {
                break;
            }
        }
        u32::try_from(offset).unwrap_or(u32::MAX)
    }

    fn write_used(mem: &mut dyn GuestMemoryMut, queue: &VirtioGpuQueue, id: u16, len: u32) {
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
pub struct VirtioPciGpu {
    gpu: VirtioGpu,
    msix: MsixTable,
}

impl VirtioPciGpu {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            gpu: VirtioGpu::new(width, height),
            msix: MsixTable::new(VIRTIO_GPU_MSIX_VECTOR_COUNT),
        }
    }

    pub fn with_3d_backend(width: u32, height: u32, backend: Box<dyn VirtioGpu3dBackend>) -> Self {
        Self {
            gpu: VirtioGpu::with_3d_backend(width, height, backend),
            msix: MsixTable::new(VIRTIO_GPU_MSIX_VECTOR_COUNT),
        }
    }

    pub fn with_3d_backend_and_shm_map_port(
        width: u32,
        height: u32,
        backend: Box<dyn VirtioGpu3dBackend>,
        map_port: Box<dyn GpuShmMapPort>,
        shm_window_size: u64,
    ) -> Self {
        let mut gpu = VirtioGpu::with_3d_backend(width, height, backend);
        gpu.set_shm_map_port(map_port, shm_window_size);
        Self {
            gpu,
            msix: MsixTable::new(VIRTIO_GPU_MSIX_VECTOR_COUNT),
        }
    }

    pub fn set_shm_map_port(&mut self, port: Box<dyn GpuShmMapPort>, window_size: u64) {
        self.gpu.set_shm_map_port(port, window_size);
    }

    pub fn new_from_env() -> Self {
        let (width, height) = parse_resolution_env();
        Self::new(width, height)
    }

    pub fn stats(&self) -> VirtioGpuStats {
        self.gpu.stats()
    }

    pub fn interrupt_line_level(&self) -> bool {
        self.gpu.interrupt_line_level()
    }

    pub fn reset_runtime_state(&mut self) {
        self.gpu.reset_runtime_state();
        self.msix = MsixTable::new(VIRTIO_GPU_MSIX_VECTOR_COUNT);
    }

    pub fn drain_completed_fences(&mut self, mem: &mut dyn GuestMemoryMut) {
        self.gpu.drain_completed_fences(mem);
    }

    pub fn scanout(&self) -> Option<VirtioGpuScanout<'_>> {
        self.gpu.scanout()
    }

    pub fn access(
        &mut self,
        offset: u64,
        op: VirtioPciGpuOp,
        mem: &mut dyn GuestMemoryMut,
    ) -> VirtioGpuResult {
        let is_write = matches!(op, VirtioPciGpuOp::Write { .. });
        if let Some(common_offset) = common_cfg_offset(offset) {
            return match op {
                VirtioPciGpuOp::Read { size } => {
                    self.gpu.access_common(common_offset, false, size, 0, mem)
                }
                VirtioPciGpuOp::Write { size, value } => {
                    let result = self
                        .gpu
                        .access_common(common_offset, true, size, value, mem);
                    self.gpu.drain_completed_fences(mem);
                    result
                }
            };
        }
        if let Some(device_offset) = device_cfg_offset(offset) {
            return match op {
                VirtioPciGpuOp::Read { size } => {
                    VirtioGpuResult::ReadValue(self.gpu.config_read(device_offset, size))
                }
                VirtioPciGpuOp::Write { size, value } => {
                    self.gpu.config_write(device_offset, size, value);
                    VirtioGpuResult::WriteAck
                }
            };
        }
        if let Some(queue_index) = notify_queue_index(offset) {
            return match op {
                VirtioPciGpuOp::Read { .. } => VirtioGpuResult::ReadValue(0),
                VirtioPciGpuOp::Write { value, .. } => {
                    let queue = if offset == PCI_NOTIFY_CFG_OFFSET {
                        value as u16
                    } else {
                        queue_index
                    };
                    self.gpu.notify_queue(queue, mem);
                    VirtioGpuResult::WriteAck
                }
            };
        }
        if offset == PCI_ISR_CFG_OFFSET {
            return match op {
                VirtioPciGpuOp::Read { size } => VirtioGpuResult::ReadValue(mask_to_size(
                    u64::from(self.gpu.interrupt_status),
                    size,
                )),
                VirtioPciGpuOp::Write { value, .. } => {
                    self.gpu.interrupt_status &= !(value as u32);
                    VirtioGpuResult::WriteAck
                }
            };
        }
        match (op, is_write) {
            (VirtioPciGpuOp::Read { .. }, _) => VirtioGpuResult::ReadValue(0),
            (VirtioPciGpuOp::Write { .. }, _) => VirtioGpuResult::WriteAck,
        }
    }

    pub fn msix_bar_access(&mut self, offset: u64, op: VirtioPciGpuOp) -> VirtioGpuResult {
        if let Some(table_offset) = self.msix_table_offset(offset) {
            return match op {
                VirtioPciGpuOp::Read { size } => {
                    VirtioGpuResult::ReadValue(self.msix.table_read(table_offset, size))
                }
                VirtioPciGpuOp::Write { size, value } => {
                    self.msix.table_write(table_offset, size, value);
                    VirtioGpuResult::WriteAck
                }
            };
        }
        if let Some(pba_offset) = self.msix_pba_offset(offset) {
            return match op {
                VirtioPciGpuOp::Read { size } => {
                    VirtioGpuResult::ReadValue(self.msix.pba_read(pba_offset, size))
                }
                VirtioPciGpuOp::Write { size, value } => {
                    self.msix.pba_write(pba_offset, size, value);
                    VirtioGpuResult::WriteAck
                }
            };
        }
        match op {
            VirtioPciGpuOp::Read { .. } => VirtioGpuResult::ReadValue(0),
            VirtioPciGpuOp::Write { .. } => VirtioGpuResult::WriteAck,
        }
    }

    pub fn raise_pending_msix(
        &mut self,
        function_enabled: bool,
        function_masked: bool,
    ) -> Vec<MsixMessage> {
        let mut messages = Vec::new();
        for queue_index in 0..self.gpu.queues.len() {
            if !self.gpu.queues[queue_index].pending_msix {
                continue;
            }
            let vector = self.gpu.queues[queue_index].msix_vector;
            if vector == VIRTIO_MSI_NO_VECTOR {
                continue;
            }
            if let Some(message) = self.msix.raise(vector, function_enabled, function_masked) {
                self.gpu.queues[queue_index].pending_msix = false;
                messages.push(message);
            }
        }
        messages
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

    fn clear_pending_queue_for_vector(&mut self, vector: u16) {
        for queue in &mut self.gpu.queues {
            if queue.msix_vector == vector {
                queue.pending_msix = false;
            }
        }
    }

    fn msix_table_offset(&self, offset: u64) -> Option<u64> {
        let rel = offset.checked_sub(u64::from(VIRTIO_GPU_MSIX_TABLE_OFFSET))?;
        (rel < self.msix.table_byte_len()).then_some(rel)
    }

    fn msix_pba_offset(&self, offset: u64) -> Option<u64> {
        let rel = offset.checked_sub(u64::from(VIRTIO_GPU_MSIX_PBA_OFFSET))?;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

impl CtrlHdr {
    fn parse(bytes: &[u8]) -> Option<Self> {
        Some(Self {
            typ: read_le_u32(bytes, 0)?,
            flags: read_le_u32(bytes, 4)?,
            fence_id: read_le_u64(bytes, 8)?,
            ctx_id: read_le_u32(bytes, 16)?,
            padding: read_le_u32(bytes, 20)?,
        })
    }

    fn response(self, typ: u32) -> Self {
        Self {
            typ,
            flags: self.flags & VIRTIO_GPU_FLAG_FENCE,
            fence_id: if self.flags & VIRTIO_GPU_FLAG_FENCE != 0 {
                self.fence_id
            } else {
                0
            },
            ctx_id: self.ctx_id,
            padding: self.padding,
        }
    }

    fn ring_idx(self) -> u8 {
        if self.flags & virtio_gpu_3d::VIRTIO_GPU_FLAG_INFO_RING_IDX != 0 {
            (self.padding & 0xff) as u8
        } else {
            0
        }
    }

    fn append_to(self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.typ.to_le_bytes());
        out.extend_from_slice(&self.flags.to_le_bytes());
        out.extend_from_slice(&self.fence_id.to_le_bytes());
        out.extend_from_slice(&self.ctx_id.to_le_bytes());
        out.extend_from_slice(&self.padding.to_le_bytes());
    }
}

fn response_hdr(typ: u32, request: Option<CtrlHdr>) -> Vec<u8> {
    let hdr = request.map_or(
        CtrlHdr {
            typ,
            flags: 0,
            fence_id: 0,
            ctx_id: 0,
            padding: 0,
        },
        |hdr| hdr.response(typ),
    );
    let mut out = Vec::with_capacity(24);
    hdr.append_to(&mut out);
    out
}

fn copy_backing_to_resource(
    mem: &dyn GuestMemoryMut,
    resource: &mut GpuResource,
    rect: Rect,
    offset: u64,
) {
    let x_end = rect.x.saturating_add(rect.width).min(resource.width);
    let y_end = rect.y.saturating_add(rect.height).min(resource.height);
    let stride = u64::from(resource.width) * 4;
    // Per the virtio-gpu spec (and QEMU), `offset` locates the box's top-left
    // (rect.x, rect.y) in the backing; source rows advance by `stride` from
    // there. So the backing offset for absolute pixel (x, y) is
    // offset + (y - rect.y) * stride + (x - rect.x) * 4 — NOT offset + y*stride
    // + x*4, which double-counts rect.{x,y} and sends every non-origin partial
    // update (taskbar, clock, cursor) out of bounds so it silently vanishes.
    for y in rect.y..y_end {
        for x in rect.x..x_end {
            let guest_off = offset + u64::from(y - rect.y) * stride + u64::from(x - rect.x) * 4;
            let Some(pixel) = read_from_backing(mem, &resource.backing, guest_off, 4) else {
                continue;
            };
            let dst = ((y as usize) * (resource.width as usize) + (x as usize)) * 4;
            resource.host_pixels[dst..dst + 4].copy_from_slice(&pixel);
        }
    }
}

fn composite_resource_to_scanout(
    scanout: &mut [u8],
    scanout_width: u32,
    scanout_height: u32,
    resource: &GpuResource,
    rect: Rect,
) {
    let x_end = rect
        .x
        .saturating_add(rect.width)
        .min(scanout_width)
        .min(resource.width);
    let y_end = rect
        .y
        .saturating_add(rect.height)
        .min(scanout_height)
        .min(resource.height);
    for y in rect.y..y_end {
        for x in rect.x..x_end {
            let src = ((y as usize) * (resource.width as usize) + (x as usize)) * 4;
            let dst = ((y as usize) * (scanout_width as usize) + (x as usize)) * 4;
            let pixel = &resource.host_pixels[src..src + 4];
            scanout[dst..dst + 4].copy_from_slice(&to_xrgb8888(pixel, resource.format));
        }
    }
}

fn to_xrgb8888(pixel: &[u8], format: u32) -> [u8; 4] {
    match format {
        FORMAT_B8G8R8A8_UNORM | FORMAT_B8G8R8X8_UNORM => [pixel[0], pixel[1], pixel[2], 0],
        FORMAT_X8R8G8B8_UNORM => [pixel[3], pixel[2], pixel[1], 0],
        FORMAT_R8G8B8X8_UNORM => [pixel[2], pixel[1], pixel[0], 0],
        _ => [0, 0, 0, 0],
    }
}

fn read_from_backing(
    mem: &dyn GuestMemoryMut,
    backing: &[BackingEntry],
    offset: u64,
    len: usize,
) -> Option<Vec<u8>> {
    let mut base = 0u64;
    for entry in backing {
        let end = base.checked_add(u64::from(entry.len))?;
        let len_u64 = u64::try_from(len).ok()?;
        if offset >= base && offset.checked_add(len_u64)? <= end {
            let rel = offset - base;
            return mem.read_bytes(entry.addr + rel, len);
        }
        base = end;
    }
    None
}

fn build_edid(width: u32, height: u32) -> [u8; 128] {
    let mut edid = [0u8; 128];
    edid[0..8].copy_from_slice(&[0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x00]);
    edid[8..10].copy_from_slice(&encode_manufacturer("BVM"));
    edid[10..12].copy_from_slice(&0x0001u16.to_le_bytes());
    edid[12..16].copy_from_slice(&1u32.to_le_bytes());
    edid[16] = 1;
    edid[17] = 34;
    edid[18] = 1;
    edid[19] = 4;
    edid[20] = 0xa5;
    edid[21] = ((width / 100).clamp(1, 255)) as u8;
    edid[22] = ((height / 100).clamp(1, 255)) as u8;
    edid[23] = 0x78;
    edid[24] = 0x0a;
    edid[25] = 0xcf;
    edid[26] = 0x74;
    edid[27] = 0xa3;
    edid[28] = 0x57;
    edid[29] = 0x4c;
    edid[30] = 0xb0;
    edid[31] = 0x23;
    edid[32] = 0x09;
    edid[35] = 0x81;
    edid[36] = 0x80;

    let dtd = detailed_timing_descriptor(width, height);
    edid[54..72].copy_from_slice(&dtd);
    edid[72..90].copy_from_slice(&monitor_descriptor(
        0xfd,
        &[50, 75, 30, 90, 16, 0, 0, 0, 0, 0, 0, 0, 0],
    ));
    edid[90..108].copy_from_slice(&monitor_descriptor_text(0xfc, b"BridgeVM GPU"));
    edid[108..126].copy_from_slice(&monitor_descriptor_text(0xfe, b"virtio-gpu"));
    edid[126] = 0;
    let sum = edid[..127]
        .iter()
        .fold(0u8, |acc, byte| acc.wrapping_add(*byte));
    edid[127] = 0u8.wrapping_sub(sum);
    edid
}

fn detailed_timing_descriptor(width: u32, height: u32) -> [u8; 18] {
    let h_blank = 160u32.max(width / 8);
    let v_blank = 45u32.max(height / 20);
    let h_sync_offset = 48u32.min(h_blank / 3);
    let h_sync_width = 32u32.min(h_blank.saturating_sub(h_sync_offset).max(1));
    let v_sync_offset = 3u32;
    let v_sync_width = 5u32;
    let pixel_clock_10khz = (((width + h_blank) * (height + v_blank) * 60) / 10_000).max(1);

    let mut dtd = [0u8; 18];
    dtd[0..2].copy_from_slice(&(pixel_clock_10khz as u16).to_le_bytes());
    dtd[2] = width as u8;
    dtd[3] = h_blank as u8;
    dtd[4] = (((width >> 8) as u8) << 4) | ((h_blank >> 8) as u8 & 0x0f);
    dtd[5] = height as u8;
    dtd[6] = v_blank as u8;
    dtd[7] = (((height >> 8) as u8) << 4) | ((v_blank >> 8) as u8 & 0x0f);
    dtd[8] = h_sync_offset as u8;
    dtd[9] = h_sync_width as u8;
    dtd[10] = ((v_sync_offset as u8) << 4) | (v_sync_width as u8 & 0x0f);
    dtd[11] = (((h_sync_offset >> 8) as u8 & 0x03) << 6)
        | (((h_sync_width >> 8) as u8 & 0x03) << 4)
        | (((v_sync_offset >> 4) as u8 & 0x03) << 2)
        | ((v_sync_width >> 4) as u8 & 0x03);
    dtd[12] = ((width * 254 / 96) / 10).min(4095) as u8;
    dtd[13] = ((height * 254 / 96) / 10).min(4095) as u8;
    dtd[14] = ((((width * 254 / 96) / 10) >> 8) as u8 & 0x0f) << 4
        | ((((height * 254 / 96) / 10) >> 8) as u8 & 0x0f);
    dtd[17] = 0x1a;
    dtd
}

fn monitor_descriptor(tag: u8, payload: &[u8]) -> [u8; 18] {
    let mut desc = [0u8; 18];
    desc[3] = tag;
    let n = payload.len().min(13);
    desc[5..5 + n].copy_from_slice(&payload[..n]);
    desc
}

fn monitor_descriptor_text(tag: u8, text: &[u8]) -> [u8; 18] {
    let mut payload = [b' '; 13];
    let n = text.len().min(12);
    payload[..n].copy_from_slice(&text[..n]);
    payload[n] = b'\n';
    monitor_descriptor(tag, &payload)
}

fn encode_manufacturer(value: &str) -> [u8; 2] {
    let mut code = 0u16;
    for byte in value.bytes().take(3) {
        let letter = u16::from(byte.to_ascii_uppercase().saturating_sub(b'@') & 0x1f);
        code = (code << 5) | letter;
    }
    code.to_be_bytes()
}

fn push_rect(out: &mut Vec<u8>, rect: Rect) {
    out.extend_from_slice(&rect.x.to_le_bytes());
    out.extend_from_slice(&rect.y.to_le_bytes());
    out.extend_from_slice(&rect.width.to_le_bytes());
    out.extend_from_slice(&rect.height.to_le_bytes());
}

fn read_rect(bytes: &[u8], offset: usize) -> Option<Rect> {
    Some(Rect {
        x: read_le_u32(bytes, offset)?,
        y: read_le_u32(bytes, offset + 4)?,
        width: read_le_u32(bytes, offset + 8)?,
        height: read_le_u32(bytes, offset + 12)?,
    })
}

fn format_supported(format: u32) -> bool {
    matches!(
        format,
        FORMAT_B8G8R8A8_UNORM
            | FORMAT_B8G8R8X8_UNORM
            | FORMAT_X8R8G8B8_UNORM
            | FORMAT_R8G8B8X8_UNORM
    )
}

fn parse_resolution_env() -> (u32, u32) {
    let value = std::env::var("BRIDGEVM_VIRTIO_GPU_RES").unwrap_or_else(|_| "1280x800".into());
    parse_resolution(&value).unwrap_or_else(|| {
        panic!("BRIDGEVM_VIRTIO_GPU_RES must be WIDTHxHEIGHT, for example 1600x900")
    })
}

fn parse_resolution(value: &str) -> Option<(u32, u32)> {
    let (width, height) = value.trim().split_once('x')?;
    let width = width.parse::<u32>().ok()?;
    let height = height.parse::<u32>().ok()?;
    (width > 0 && height > 0).then_some((width, height))
}

fn scanout_len(width: u32, height: u32) -> usize {
    u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|pixels| pixels.checked_mul(4))
        .and_then(|bytes| usize::try_from(bytes).ok())
        .expect("virtio-gpu scanout size overflow")
}

fn set_low(current: u64, value: u64) -> u64 {
    (current & !0xffff_ffff) | (value & 0xffff_ffff)
}

fn set_high(current: u64, value: u64) -> u64 {
    (current & 0xffff_ffff) | ((value & 0xffff_ffff) << 32)
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
    let bytes = mem.read_bytes(gpa, 2)?;
    Some(u16::from_le_bytes(bytes.try_into().ok()?))
}

fn read_le_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn read_le_u64(bytes: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_le_bytes(
        bytes.get(offset..offset + 8)?.try_into().ok()?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::virtio_gpu_3d::MockBackend;
    use std::sync::{Arc, Mutex};

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

    fn pci_write(dev: &mut VirtioPciGpu, offset: u64, size: u8, value: u64, mem: &mut TestMem) {
        assert_eq!(
            dev.access(offset, VirtioPciGpuOp::Write { size, value }, mem),
            VirtioGpuResult::WriteAck
        );
    }

    fn pci_read(dev: &mut VirtioPciGpu, offset: u64, size: u8, mem: &mut TestMem) -> u64 {
        match dev.access(offset, VirtioPciGpuOp::Read { size }, mem) {
            VirtioGpuResult::ReadValue(value) => value,
            VirtioGpuResult::WriteAck => panic!("read returned write ack"),
        }
    }

    fn setup_queue(
        dev: &mut VirtioPciGpu,
        mem: &mut TestMem,
        queue: u16,
        desc: u64,
        avail: u64,
        used: u64,
        vector: u16,
    ) {
        pci_write(dev, COMMON_QUEUE_SELECT, 2, u64::from(queue), mem);
        pci_write(dev, COMMON_QUEUE_SIZE, 2, 16, mem);
        pci_write(dev, COMMON_QUEUE_DESC, 8, desc, mem);
        pci_write(dev, COMMON_QUEUE_DRIVER, 8, avail, mem);
        pci_write(dev, COMMON_QUEUE_DEVICE, 8, used, mem);
        pci_write(dev, COMMON_QUEUE_MSIX_VECTOR, 2, u64::from(vector), mem);
        pci_write(dev, COMMON_QUEUE_ENABLE, 2, 1, mem);
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

    fn ctrl_req(typ: u32) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&typ.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&0u64.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out
    }

    fn ctrl_req_ctx(typ: u32, ctx_id: u32) -> Vec<u8> {
        let mut out = ctrl_req(typ);
        out[16..20].copy_from_slice(&ctx_id.to_le_bytes());
        out
    }

    fn ctrl_req_fenced(typ: u32, ctx_id: u32, ring_idx: u8, fence_id: u64) -> Vec<u8> {
        let mut out = ctrl_req_ctx(typ, ctx_id);
        out[4..8].copy_from_slice(
            &(VIRTIO_GPU_FLAG_FENCE | virtio_gpu_3d::VIRTIO_GPU_FLAG_INFO_RING_IDX).to_le_bytes(),
        );
        out[8..16].copy_from_slice(&fence_id.to_le_bytes());
        out[20] = ring_idx;
        out
    }

    fn dev_with_mock() -> (VirtioPciGpu, Arc<Mutex<MockBackend>>) {
        let backend = Arc::new(Mutex::new(MockBackend::new_venus()));
        (
            VirtioPciGpu::with_3d_backend(1280, 800, Box::new(backend.clone())),
            backend,
        )
    }

    fn submit_control(
        dev: &mut VirtioPciGpu,
        mem: &mut TestMem,
        request: &[u8],
        response_len: u32,
    ) -> Vec<u8> {
        let desc = 0x4000_1000;
        let avail = 0x4000_2000;
        let used = 0x4000_3000;
        let req = 0x4000_4000;
        let resp = 0x4000_5000;
        setup_queue(dev, mem, 0, desc, avail, used, 0);
        let next_avail = dev.stats().queues[0].last_avail_idx.wrapping_add(1);
        let ring_slot = dev.stats().queues[0].last_avail_idx % 16;
        mem.write(req, request);
        write_desc(mem, desc, 0, req, request.len() as u32, DESC_F_NEXT, 1);
        write_desc(mem, desc, 1, resp, response_len, DESC_F_WRITE, 0);
        mem.write(avail + 2, &next_avail.to_le_bytes());
        mem.write(avail + 4 + u64::from(ring_slot) * 2, &0u16.to_le_bytes());
        pci_write(dev, PCI_NOTIFY_CFG_OFFSET, 4, 0, mem);
        assert_eq!(
            u16::from_le_bytes(mem.read(used + 2, 2).try_into().unwrap()),
            next_avail
        );
        mem.read(resp, response_len as usize)
    }

    fn submit_control_readable_descs(
        dev: &mut VirtioPciGpu,
        mem: &mut TestMem,
        readable: &[&[u8]],
        response_len: u32,
    ) -> (Vec<u8>, u16) {
        let desc = 0x4000_1000;
        let avail = 0x4000_2000;
        let used = 0x4000_3000;
        let req = 0x4000_4000;
        let resp = 0x4000_9000;
        setup_queue(dev, mem, 0, desc, avail, used, 0);
        let next_avail = dev.stats().queues[0].last_avail_idx.wrapping_add(1);
        let ring_slot = dev.stats().queues[0].last_avail_idx % 16;
        let mut addr = req;
        for (i, bytes) in readable.iter().enumerate() {
            mem.write(addr, bytes);
            let next = (i + 1) as u16;
            write_desc(
                mem,
                desc,
                i as u16,
                addr,
                bytes.len() as u32,
                DESC_F_NEXT,
                next,
            );
            addr += 0x100;
        }
        let response_index = readable.len() as u16;
        write_desc(
            mem,
            desc,
            response_index,
            resp,
            response_len,
            DESC_F_WRITE,
            0,
        );
        mem.write(avail + 2, &next_avail.to_le_bytes());
        mem.write(avail + 4 + u64::from(ring_slot) * 2, &0u16.to_le_bytes());
        pci_write(dev, PCI_NOTIFY_CFG_OFFSET, 4, 0, mem);
        let used_idx = u16::from_le_bytes(mem.read(used + 2, 2).try_into().unwrap());
        (mem.read(resp, response_len as usize), used_idx)
    }

    fn ctx_create_req(ctx_id: u32, context_init: u32, name: &[u8]) -> Vec<u8> {
        let mut req = ctrl_req_ctx(VIRTIO_GPU_CMD_CTX_CREATE, ctx_id);
        req.extend_from_slice(&(name.len() as u32).to_le_bytes());
        req.extend_from_slice(&context_init.to_le_bytes());
        let mut debug_name = [0u8; 64];
        debug_name[..name.len().min(64)].copy_from_slice(&name[..name.len().min(64)]);
        req.extend_from_slice(&debug_name);
        req
    }

    fn submit_3d_req(ctx_id: u32, cmdbuf: &[u8]) -> Vec<u8> {
        let mut req = ctrl_req_ctx(VIRTIO_GPU_CMD_SUBMIT_3D, ctx_id);
        req.extend_from_slice(&(cmdbuf.len() as u32).to_le_bytes());
        req.extend_from_slice(&0u32.to_le_bytes());
        req.extend_from_slice(cmdbuf);
        req
    }

    #[test]
    fn modern_driver_common_config_sequence_advertises_and_enables_both_queues() {
        let mut dev = VirtioPciGpu::new(1280, 800);
        let mut mem = TestMem::new(0x4000_0000, 0x10000);
        pci_write(&mut dev, COMMON_DEVICE_FEATURE_SELECT, 4, 0, &mut mem);
        assert_eq!(
            pci_read(&mut dev, COMMON_DEVICE_FEATURE, 4, &mut mem),
            u64::from(VIRTIO_GPU_F_EDID)
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
            u64::from(VIRTIO_GPU_F_EDID) | (u64::from(VIRTIO_F_VERSION_1) << 32)
        );
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
        assert_eq!(stats.queues[0].size, 16);
        assert!(stats.queues[0].ready);
        assert_eq!(stats.queues[1].size, 16);
        assert!(stats.queues[1].ready);
    }

    #[test]
    fn get_display_info_reports_configured_scanout() {
        let mut dev = VirtioPciGpu::new(1600, 900);
        let mut mem = TestMem::new(0x4000_0000, 0x20000);
        let resp = submit_control(
            &mut dev,
            &mut mem,
            &ctrl_req(VIRTIO_GPU_CMD_GET_DISPLAY_INFO),
            408,
        );
        assert_eq!(read_le_u32(&resp, 0), Some(VIRTIO_GPU_RESP_OK_DISPLAY_INFO));
        assert_eq!(read_le_u32(&resp, 24 + 8), Some(1600));
        assert_eq!(read_le_u32(&resp, 24 + 12), Some(900));
        assert_eq!(read_le_u32(&resp, 24 + 16), Some(1));
    }

    #[test]
    fn get_edid_returns_checksum_valid_base_block() {
        let mut dev = VirtioPciGpu::new(1280, 800);
        let mut mem = TestMem::new(0x4000_0000, 0x20000);
        let resp = submit_control(&mut dev, &mut mem, &ctrl_req(VIRTIO_GPU_CMD_GET_EDID), 1056);
        assert_eq!(read_le_u32(&resp, 0), Some(VIRTIO_GPU_RESP_OK_EDID));
        assert_eq!(read_le_u32(&resp, 24), Some(128));
        let edid = &resp[32..160];
        assert_eq!(&edid[0..8], &[0, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0]);
        assert_eq!(
            edid.iter().fold(0u8, |acc, byte| acc.wrapping_add(*byte)),
            0
        );
    }

    #[test]
    fn resource_transfer_flush_presents_pixels_to_scanout() {
        let mut dev = VirtioPciGpu::new(4, 3);
        let mut mem = TestMem::new(0x4000_0000, 0x30000);
        let desc = 0x4000_1000;
        let avail = 0x4000_2000;
        let used = 0x4000_3000;
        let req = 0x4000_4000;
        let resp = 0x4000_5000;
        let backing = 0x4000_8000;
        setup_queue(&mut dev, &mut mem, 0, desc, avail, used, 0);

        let mut backing_bytes = vec![0u8; 4 * 3 * 4];
        backing_bytes[4 * 4..4 * 4 + 4].copy_from_slice(&[0x33, 0x22, 0x11, 0xaa]);
        backing_bytes[5 * 4..5 * 4 + 4].copy_from_slice(&[0x66, 0x55, 0x44, 0xbb]);
        mem.write(backing, &backing_bytes);

        let mut chains = Vec::new();
        let mut create = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_CREATE_2D);
        create.extend_from_slice(&1u32.to_le_bytes());
        create.extend_from_slice(&FORMAT_B8G8R8A8_UNORM.to_le_bytes());
        create.extend_from_slice(&4u32.to_le_bytes());
        create.extend_from_slice(&3u32.to_le_bytes());
        chains.push(create);

        let mut attach = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING);
        attach.extend_from_slice(&1u32.to_le_bytes());
        attach.extend_from_slice(&1u32.to_le_bytes());
        attach.extend_from_slice(&backing.to_le_bytes());
        attach.extend_from_slice(&(backing_bytes.len() as u32).to_le_bytes());
        attach.extend_from_slice(&0u32.to_le_bytes());
        chains.push(attach);

        let mut set_scanout = ctrl_req(VIRTIO_GPU_CMD_SET_SCANOUT);
        push_rect(
            &mut set_scanout,
            Rect {
                x: 0,
                y: 0,
                width: 4,
                height: 3,
            },
        );
        set_scanout.extend_from_slice(&0u32.to_le_bytes());
        set_scanout.extend_from_slice(&1u32.to_le_bytes());
        chains.push(set_scanout);

        let mut transfer = ctrl_req(VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D);
        push_rect(
            &mut transfer,
            Rect {
                x: 0,
                y: 1,
                width: 2,
                height: 1,
            },
        );
        // offset locates the box top-left (0, 1) in the backing: y*stride =
        // 1 * (width 4 * 4bpp) = 16, matching a full-surface backing where the
        // guest points offset at the dirty region's origin (Convention B).
        transfer.extend_from_slice(&16u64.to_le_bytes());
        transfer.extend_from_slice(&1u32.to_le_bytes());
        transfer.extend_from_slice(&0u32.to_le_bytes());
        chains.push(transfer);

        let mut flush = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_FLUSH);
        push_rect(
            &mut flush,
            Rect {
                x: 0,
                y: 1,
                width: 2,
                height: 1,
            },
        );
        flush.extend_from_slice(&1u32.to_le_bytes());
        flush.extend_from_slice(&0u32.to_le_bytes());
        chains.push(flush);

        for (i, request) in chains.iter().enumerate() {
            let req_addr = req + (i as u64) * 0x100;
            let resp_addr = resp + (i as u64) * 0x100;
            mem.write(req_addr, request);
            write_desc(
                &mut mem,
                desc,
                (i * 2) as u16,
                req_addr,
                request.len() as u32,
                DESC_F_NEXT,
                (i * 2 + 1) as u16,
            );
            write_desc(
                &mut mem,
                desc,
                (i * 2 + 1) as u16,
                resp_addr,
                24,
                DESC_F_WRITE,
                0,
            );
            mem.write(avail + 4 + (i as u64) * 2, &((i * 2) as u16).to_le_bytes());
        }
        mem.write(avail + 2, &(chains.len() as u16).to_le_bytes());
        pci_write(&mut dev, PCI_NOTIFY_CFG_OFFSET, 4, 0, &mut mem);
        assert_eq!(
            u16::from_le_bytes(mem.read(used + 2, 2).try_into().unwrap()),
            chains.len() as u16
        );
        let scanout = dev.scanout().unwrap();
        let row1 = (scanout.stride as usize)..(scanout.stride as usize + 8);
        assert_eq!(
            &scanout.bytes[row1],
            &[0x33, 0x22, 0x11, 0, 0x66, 0x55, 0x44, 0]
        );
    }

    #[test]
    fn unknown_command_returns_err_unspec_without_wedging() {
        let mut dev = VirtioPciGpu::new(1280, 800);
        let mut mem = TestMem::new(0x4000_0000, 0x20000);
        let resp = submit_control(&mut dev, &mut mem, &ctrl_req(0xdead_beef), 24);
        assert_eq!(read_le_u32(&resp, 0), Some(VIRTIO_GPU_RESP_ERR_UNSPEC));
        assert_eq!(dev.stats().queues[0].last_avail_idx, 1);
    }

    #[test]
    fn three_d_backend_advertises_features_and_capsets() {
        let (mut dev, _) = dev_with_mock();
        let mut mem = TestMem::new(0x4000_0000, 0x20000);
        pci_write(&mut dev, COMMON_DEVICE_FEATURE_SELECT, 4, 0, &mut mem);
        assert_eq!(
            pci_read(&mut dev, COMMON_DEVICE_FEATURE, 4, &mut mem),
            u64::from(
                VIRTIO_GPU_F_EDID
                    | VIRTIO_GPU_F_VIRGL
                    | VIRTIO_GPU_F_RESOURCE_BLOB
                    | VIRTIO_GPU_F_CONTEXT_INIT
            )
        );
        // num_scanouts @8 stays 1; num_capsets @12 is 1 with a backend.
        assert_eq!(
            pci_read(&mut dev, PCI_DEVICE_CFG_OFFSET + 8, 4, &mut mem),
            1
        );
        assert_eq!(
            pci_read(&mut dev, PCI_DEVICE_CFG_OFFSET + 12, 4, &mut mem),
            1
        );
        let mut info = ctrl_req(VIRTIO_GPU_CMD_GET_CAPSET_INFO);
        info.extend_from_slice(&0u32.to_le_bytes());
        info.extend_from_slice(&0u32.to_le_bytes());
        let resp = submit_control(&mut dev, &mut mem, &info, 40);
        assert_eq!(
            read_le_u32(&resp, 0),
            Some(virtio_gpu_3d::VIRTIO_GPU_RESP_OK_CAPSET_INFO)
        );
        assert_eq!(read_le_u32(&resp, 24), Some(4));
        assert_eq!(read_le_u32(&resp, 28), Some(1));
        assert_eq!(read_le_u32(&resp, 32), Some(160));

        let mut get = ctrl_req(VIRTIO_GPU_CMD_GET_CAPSET);
        get.extend_from_slice(&4u32.to_le_bytes());
        get.extend_from_slice(&1u32.to_le_bytes());
        let resp = submit_control(&mut dev, &mut mem, &get, 24 + 160);
        assert_eq!(
            read_le_u32(&resp, 0),
            Some(virtio_gpu_3d::VIRTIO_GPU_RESP_OK_CAPSET)
        );
        assert_eq!(read_le_u32(&resp, 24), Some(1));
    }

    #[test]
    fn two_d_only_rejects_three_d_and_reports_zero_capsets() {
        let mut dev = VirtioPciGpu::new(1280, 800);
        let mut mem = TestMem::new(0x4000_0000, 0x20000);
        // virtio_gpu_config: num_scanouts @8 (always 1), num_capsets @12.
        assert_eq!(
            pci_read(&mut dev, PCI_DEVICE_CFG_OFFSET + 8, 4, &mut mem),
            1
        );
        assert_eq!(
            pci_read(&mut dev, PCI_DEVICE_CFG_OFFSET + 12, 4, &mut mem),
            0
        );
        let mut info = ctrl_req(VIRTIO_GPU_CMD_GET_CAPSET_INFO);
        info.extend_from_slice(&0u32.to_le_bytes());
        info.extend_from_slice(&0u32.to_le_bytes());
        let resp = submit_control(&mut dev, &mut mem, &info, 24);
        assert_eq!(
            read_le_u32(&resp, 0),
            Some(virtio_gpu_3d::VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER)
        );
    }

    #[test]
    fn ctx_lifecycle_and_unknown_ctx_errors() {
        let (mut dev, backend) = dev_with_mock();
        let mut mem = TestMem::new(0x4000_0000, 0x20000);
        let resp = submit_control(&mut dev, &mut mem, &ctx_create_req(7, 4, b"ctx"), 24);
        assert_eq!(read_le_u32(&resp, 0), Some(VIRTIO_GPU_RESP_OK_NODATA));
        assert_eq!(dev.stats().three_d.ctx_active, 1);
        assert_eq!(backend.lock().unwrap().created[0], (7, 4, b"ctx".to_vec()));

        let resp = submit_control(&mut dev, &mut mem, &submit_3d_req(9, &[]), 24);
        assert_eq!(
            read_le_u32(&resp, 0),
            Some(virtio_gpu_3d::VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER)
        );

        let resp = submit_control(
            &mut dev,
            &mut mem,
            &ctrl_req_ctx(VIRTIO_GPU_CMD_CTX_DESTROY, 7),
            24,
        );
        assert_eq!(read_le_u32(&resp, 0), Some(VIRTIO_GPU_RESP_OK_NODATA));
        assert_eq!(dev.stats().three_d.ctx_active, 0);
        assert_eq!(backend.lock().unwrap().destroyed, vec![7]);
    }

    #[test]
    fn submit_3d_gathers_split_readable_descriptors() {
        let (mut dev, backend) = dev_with_mock();
        let mut mem = TestMem::new(0x4000_0000, 0x30000);
        let _ = submit_control(&mut dev, &mut mem, &ctx_create_req(1, 4, b"ctx"), 24);
        let mut prefix = ctrl_req_ctx(VIRTIO_GPU_CMD_SUBMIT_3D, 1);
        prefix.extend_from_slice(&6u32.to_le_bytes());
        prefix.extend_from_slice(&0u32.to_le_bytes());
        let suffix = [1u8, 2, 3, 4, 5, 6];
        let (resp, used_idx) =
            submit_control_readable_descs(&mut dev, &mut mem, &[&prefix, &suffix], 24);
        assert_eq!(used_idx, 2);
        assert_eq!(read_le_u32(&resp, 0), Some(VIRTIO_GPU_RESP_OK_NODATA));
        assert_eq!(backend.lock().unwrap().submits, vec![(1, suffix.to_vec())]);
    }

    #[test]
    fn fence_defers_used_ring_until_mock_signals_with_ring_idx() {
        let (mut dev, backend) = dev_with_mock();
        let mut mem = TestMem::new(0x4000_0000, 0x30000);
        let _ = submit_control(&mut dev, &mut mem, &ctx_create_req(1, 4, b"ctx"), 24);
        let mut req = ctrl_req_fenced(VIRTIO_GPU_CMD_SUBMIT_3D, 1, 3, 42);
        req.extend_from_slice(&0u32.to_le_bytes());
        req.extend_from_slice(&0u32.to_le_bytes());
        let (_resp, used_idx) = submit_control_readable_descs(&mut dev, &mut mem, &[&req], 24);
        assert_eq!(used_idx, 1);
        assert_eq!(dev.stats().three_d.fences_pending, 1);
        assert_eq!(
            backend.lock().unwrap().fences,
            vec![CompletedFence {
                ctx_id: 1,
                ring_idx: 3,
                fence_id: 42
            }]
        );

        backend.lock().unwrap().completed.push(CompletedFence {
            ctx_id: 1,
            ring_idx: 2,
            fence_id: 42,
        });
        dev.drain_completed_fences(&mut mem);
        assert_eq!(dev.stats().three_d.fences_pending, 1);

        backend.lock().unwrap().completed.push(CompletedFence {
            ctx_id: 1,
            ring_idx: 3,
            fence_id: 42,
        });
        dev.drain_completed_fences(&mut mem);
        assert_eq!(dev.stats().three_d.fences_pending, 0);
        assert_eq!(
            u16::from_le_bytes(mem.read(0x4000_3000 + 2, 2).try_into().unwrap()),
            2
        );
    }

    #[test]
    fn reset_drops_parked_fences_without_stale_used_write() {
        let (mut dev, _backend) = dev_with_mock();
        let mut mem = TestMem::new(0x4000_0000, 0x30000);
        let _ = submit_control(&mut dev, &mut mem, &ctx_create_req(1, 4, b"ctx"), 24);
        let mut req = ctrl_req_fenced(VIRTIO_GPU_CMD_SUBMIT_3D, 1, 0, 9);
        req.extend_from_slice(&0u32.to_le_bytes());
        req.extend_from_slice(&0u32.to_le_bytes());
        let (_resp, used_idx) = submit_control_readable_descs(&mut dev, &mut mem, &[&req], 24);
        assert_eq!(used_idx, 1);
        assert_eq!(dev.stats().three_d.fences_pending, 1);
        pci_write(&mut dev, COMMON_DEVICE_STATUS, 1, 0, &mut mem);
        assert_eq!(dev.stats().three_d.fences_pending, 0);
        assert_eq!(
            u16::from_le_bytes(mem.read(0x4000_3000 + 2, 2).try_into().unwrap()),
            1
        );
    }
}
