//! Split out of magic_value.rs to keep files under 600 lines.

use super::*;

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::{
    fwcfg::GuestMemoryMut,
    pcie::VIRTIO_GPU_MSIX_VECTOR_COUNT,
    ramfb::DRM_FORMAT_XRGB8888,
    virtio_gpu_3d::{
        self, BlobMemEntry, CompletedFence, CtrlHdr3d, GpuShmMapPort, VirtioGpu3d,
        VirtioGpu3dBackend, VirtioGpu3dStats, VIRTIO_GPU_BLOB_MEM_GUEST,
        VIRTIO_GPU_BLOB_MEM_HOST3D, VIRTIO_GPU_CMD_CTX_ATTACH_RESOURCE, VIRTIO_GPU_CMD_CTX_CREATE,
        VIRTIO_GPU_CMD_CTX_DESTROY, VIRTIO_GPU_CMD_CTX_DETACH_RESOURCE, VIRTIO_GPU_CMD_GET_CAPSET,
        VIRTIO_GPU_CMD_GET_CAPSET_INFO, VIRTIO_GPU_CMD_RESOURCE_CREATE_3D,
        VIRTIO_GPU_CMD_RESOURCE_CREATE_BLOB, VIRTIO_GPU_CMD_RESOURCE_MAP_BLOB,
        VIRTIO_GPU_CMD_RESOURCE_UNMAP_BLOB, VIRTIO_GPU_CMD_SUBMIT_3D,
        VIRTIO_GPU_CMD_TRANSFER_FROM_HOST_3D, VIRTIO_GPU_CMD_TRANSFER_TO_HOST_3D,
        VIRTIO_GPU_FLAG_FENCE, VIRTIO_GPU_F_CONTEXT_INIT, VIRTIO_GPU_F_RESOURCE_BLOB,
        VIRTIO_GPU_F_VIRGL,
    },
    virtio_gpu_trace::{venus_start_trace_enabled, write_json_string, VirtioGpuTraceRecorder},
};

pub(crate) const MAGIC_VALUE: u32 = 0x7472_6976;
pub(crate) const VERSION_MODERN: u32 = 2;
pub(crate) const DEVICE_ID_GPU: u32 = 16;
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

pub(crate) const VIRTIO_GPU_F_EDID: u32 = 1 << 1;
pub(crate) const VIRTIO_F_VERSION_1: u32 = 1 << 0;
pub(crate) const VIRTIO_MSI_NO_VECTOR: u16 = 0xffff;

/// virtio-gpu config `events_read` bit: the host changed the scanout layout
/// (resolution), so the guest should re-query GET_DISPLAY_INFO/GET_EDID.
pub(crate) const VIRTIO_GPU_EVENT_DISPLAY: u32 = 1 << 0;
/// Largest scanout the resize path accepts, matching the EDID/mode range the
/// viogpu3d driver advertises. Guards the scanout allocation.
pub(crate) const MAX_SCANOUT_DIMENSION: u32 = 7680;

pub(crate) const QUEUE_CONTROL: usize = 0;
pub(crate) const QUEUE_CURSOR: usize = 1;
pub(crate) const QUEUE_COUNT: usize = 2;
pub(crate) const PARKED_RESPONSE_BUFFER_POOL_LIMIT: usize = 4;
pub(crate) const QUEUE_MAX: u16 = 64;
pub(crate) const DESC_SIZE: u64 = 16;
pub(crate) const DESC_F_NEXT: u16 = 1;
pub(crate) const DESC_F_WRITE: u16 = 2;
pub(crate) const MAX_GPU_REQUEST_LEN: usize = 64 * 1024 * 1024;

pub(crate) const VIRTIO_GPU_CMD_GET_DISPLAY_INFO: u32 = 0x0100;
pub(crate) const VIRTIO_GPU_CMD_RESOURCE_CREATE_2D: u32 = 0x0101;
pub(crate) const VIRTIO_GPU_CMD_RESOURCE_UNREF: u32 = 0x0102;
pub(crate) const VIRTIO_GPU_CMD_SET_SCANOUT: u32 = 0x0103;
pub(crate) const VIRTIO_GPU_CMD_RESOURCE_FLUSH: u32 = 0x0104;
pub(crate) const VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D: u32 = 0x0105;
pub(crate) const VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING: u32 = 0x0106;
pub(crate) const VIRTIO_GPU_CMD_RESOURCE_DETACH_BACKING: u32 = 0x0107;
pub(crate) const VIRTIO_GPU_CMD_GET_EDID: u32 = 0x010a;
pub(crate) const VIRTIO_GPU_CMD_SET_SCANOUT_BLOB: u32 = 0x010d;
pub(crate) const VIRTIO_GPU_CMD_UPDATE_CURSOR: u32 = 0x0300;
pub(crate) const VIRTIO_GPU_CMD_MOVE_CURSOR: u32 = 0x0301;
pub(crate) const VIRTIO_GPU_RESP_OK_NODATA: u32 = 0x1100;
pub(crate) const VIRTIO_GPU_RESP_OK_DISPLAY_INFO: u32 = 0x1101;
pub(crate) const VIRTIO_GPU_RESP_OK_EDID: u32 = 0x1104;
pub(crate) const VIRTIO_GPU_RESP_ERR_UNSPEC: u32 = 0x1200;

pub(crate) const FORMAT_B8G8R8A8_UNORM: u32 = 1;
pub(crate) const FORMAT_B8G8R8X8_UNORM: u32 = 2;
pub(crate) const FORMAT_X8R8G8B8_UNORM: u32 = 3;
pub(crate) const FORMAT_R8G8B8X8_UNORM: u32 = 4;
pub(crate) const SET_SCANOUT_BLOB_LEN: usize = 24 + 16 + 4 + 4 + 4 + 4 + 4 + 4 + 16 + 16;

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

/// Lock-free wake signal for host vblank pacing. The device (behind the
/// platform mutex, on a vCPU thread) publishes "a parked vsync NOP exists and
/// becomes due at `deadline_ns`"; a host waker thread reads it WITHOUT any
/// lock and forces a vCPU exit only when the deadline has passed, so the
/// per-exit drain retires the NOP even while the guest idles in WFI. This is
/// the piece the earlier host-pacing attempts lacked: a host thread must never
/// contend for the platform mutex (vCPU threads hold it almost continuously
/// under 3D load), and it must never force exits unconditionally (exit storm).
#[derive(Debug)]
pub struct VblankWakeState {
    pub(crate) base: Instant,
    pub(crate) parked: AtomicBool,
    pub(crate) deadline_ns: AtomicU64,
}

impl VblankWakeState {
    pub fn new() -> Self {
        Self {
            base: Instant::now(),
            parked: AtomicBool::new(false),
            deadline_ns: AtomicU64::new(0),
        }
    }

    pub(crate) fn publish(&self, parked: bool, deadline: Option<Instant>) {
        let deadline_ns = deadline
            .map(|d| {
                u64::try_from(d.saturating_duration_since(self.base).as_nanos()).unwrap_or(u64::MAX)
            })
            .unwrap_or(0);
        self.deadline_ns.store(deadline_ns, Ordering::SeqCst);
        self.parked.store(parked, Ordering::SeqCst);
    }

    pub fn parked(&self) -> bool {
        self.parked.load(Ordering::SeqCst)
    }

    /// Time remaining until the parked NOP is due, `Duration::ZERO` when due
    /// now, or `None` when nothing is parked.
    pub fn time_to_deadline(&self, now: Instant) -> Option<Duration> {
        if !self.parked() {
            return None;
        }
        let deadline_ns = self.deadline_ns.load(Ordering::SeqCst);
        let now_ns =
            u64::try_from(now.saturating_duration_since(self.base).as_nanos()).unwrap_or(u64::MAX);
        Some(Duration::from_nanos(deadline_ns.saturating_sub(now_ns)))
    }
}

impl Default for VblankWakeState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub struct VirtioGpu {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) device_features_sel: u32,
    pub(crate) driver_features_sel: u32,
    pub(crate) driver_features: [u32; 2],
    pub(crate) config_msix_vector: u16,
    pub(crate) queue_sel: u32,
    pub(crate) queues: [VirtioGpuQueue; QUEUE_COUNT],
    pub(crate) pending_msix_queue_bits: u8,
    pub(crate) status: u32,
    pub(crate) interrupt_status: u32,
    pub(crate) events_read: u32,
    pub(crate) events_clear: u32,
    pub(crate) pending_config_change: bool,
    pub(crate) resources: BTreeMap<u32, GpuResource>,
    pub(crate) scanout_resource: Option<u32>,
    pub(crate) blob_scanout: Option<BlobScanout>,
    pub(crate) scanout: Vec<u8>,
    pub(crate) fb_sink: Option<FbSink>,
    pub(crate) three_d: VirtioGpu3d,
    pub(crate) pending_fenced: Vec<PendingFencedResponse>,
    pub(crate) pending_vblank: Vec<PendingVblankResponse>,
    pub(crate) completed_fences_scratch: Vec<CompletedFence>,
    pub(crate) descriptor_scratch: Vec<Descriptor>,
    pub(crate) parked_descriptor_scratch: Vec<Vec<Descriptor>>,
    pub(crate) request_scratch: Vec<u8>,
    pub(crate) response_scratch: Vec<u8>,
    pub(crate) parked_response_scratch: Vec<Vec<u8>>,
    pub(crate) blob_row_scratch: Vec<u8>,
    pub(crate) scanout_readback_scratch: Vec<u8>,
    pub(crate) trace_fields_scratch: String,
    pub(crate) trace: VirtioGpuTraceRecorder,
    pub(crate) trace_queue_notify_count: u64,
    pub(crate) trace_submit_success_count: u64,
    pub(crate) trace_fence_create_count: u64,
    pub(crate) trace_fence_complete_count: u64,
    pub(crate) trace_fence_deliver_count: u64,
    pub(crate) vblank_interval: Duration,
    pub(crate) last_vblank: Option<Instant>,
    pub(crate) vblank_paced_count: u64,
    pub(crate) vblank_wake: Option<Arc<VblankWakeState>>,
    pub(crate) scanout_readback_interval: Duration,
    pub(crate) last_3d_scanout_readback: Option<Instant>,
    pub(crate) scanout_3d_flush_count: u64,
    pub(crate) scanout_readback_attempt_count: u64,
    pub(crate) scanout_readback_count: u64,
    pub(crate) scanout_readback_throttled_count: u64,
    pub(crate) scanout_readback_bytes: u64,
    pub(crate) scanout_readback_nanoseconds: u64,
    pub(crate) scanout_3d_deferred: bool,
    pub(crate) pending_3d_scanout: Option<(u32, Rect)>,
    pub(crate) pending_3d_scanout_fresh: bool,
    pub(crate) pending_3d_scanout_blitted: bool,
    pub(crate) deferred_scanout_flush_count: u64,
    pub(crate) deferred_scanout_serviced_count: u64,
    pub(crate) scanout_iosurface: bool,
    pub(crate) scanout_iosurface_verify: bool,
    pub(crate) scanout_iosurface_id: Option<u32>,
    pub(crate) scanout_iosurface_dumped: bool,
    pub(crate) scanout_blit_count: u64,
    pub(crate) scanout_blit_nanoseconds: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VirtioGpuQueue {
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

impl VirtioGpuQueue {
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

    /// Queue size the device must actually run at. The virtio driver may enable
    /// a queue without ever writing COMMON_QUEUE_SIZE, in which case the queue
    /// operates at the advertised maximum (`QUEUE_MAX`) rather than the reset
    /// value of 0. Reads of COMMON_QUEUE_SIZE already report this effective
    /// value, so descriptor processing must agree with it.
    pub(crate) fn effective_size(&self) -> u16 {
        if self.size == 0 {
            QUEUE_MAX
        } else {
            self.size
        }
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
    pub scanout_3d_flushes: u64,
    pub vblank_paced_count: u64,
    pub scanout_readback_attempts: u64,
    pub scanout_readbacks: u64,
    pub scanout_readback_throttled: u64,
    pub scanout_readback_bytes: u64,
    pub scanout_readback_nanoseconds: u64,
    pub deferred_scanout_flushes: u64,
    pub deferred_scanout_serviced: u64,
    pub scanout_blits: u64,
    pub three_d: VirtioGpu3dStats,
    pub queues: [VirtioGpuQueueStats; QUEUE_COUNT],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GpuResource {
    pub(crate) format: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) host_pixels: Vec<u8>,
    pub(crate) backing: Vec<BackingEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BackingEntry {
    pub(crate) addr: u64,
    pub(crate) len: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BlobScanout {
    pub(crate) resource_id: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) format: u32,
    pub(crate) stride: u32,
    pub(crate) offset: u32,
    pub(crate) mapping: Option<BlobScanoutMapping>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BlobScanoutMapping {
    pub(crate) ptr: *const u8,
    pub(crate) len: usize,
}

unsafe impl Send for BlobScanoutMapping {}

#[derive(Debug, Clone)]
pub(crate) struct PendingFencedResponse {
    pub(crate) queue_index: usize,
    pub(crate) queue: VirtioGpuQueue,
    pub(crate) head: u16,
    pub(crate) descs: Vec<Descriptor>,
    pub(crate) response: Vec<u8>,
    pub(crate) fence: CompletedFence,
}

#[derive(Debug, Clone)]
pub(crate) struct PendingVblankResponse {
    pub(crate) queue_index: usize,
    pub(crate) queue: VirtioGpuQueue,
    pub(crate) head: u16,
    pub(crate) descs: Vec<Descriptor>,
    pub(crate) response: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ChainCompletion {
    Immediate(u32),
    Parked,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct CtrlHdr {
    pub(crate) typ: u32,
    pub(crate) flags: u32,
    pub(crate) fence_id: u64,
    pub(crate) ctx_id: u32,
    pub(crate) padding: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScanoutReadbackOutcome {
    Done,
    NotDue,
    Gone,
}

pub(crate) fn fnv1a64(data: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &byte in data {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

pub(crate) fn union_rect(a: Rect, b: Rect) -> Rect {
    if a.width == 0 || a.height == 0 {
        return b;
    }
    if b.width == 0 || b.height == 0 {
        return a;
    }
    let x = a.x.min(b.x);
    let y = a.y.min(b.y);
    let right = a.x.saturating_add(a.width).max(b.x.saturating_add(b.width));
    let bottom =
        a.y.saturating_add(a.height)
            .max(b.y.saturating_add(b.height));
    Rect {
        x,
        y,
        width: right - x,
        height: bottom - y,
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Rect {
    pub(crate) x: u32,
    pub(crate) y: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

impl VirtioGpu {
    pub fn new(width: u32, height: u32) -> Self {
        assert!(
            width > 0 && height > 0,
            "virtio-gpu resolution must be non-zero"
        );
        let len = scanout_len(width, height);
        let mut gpu = Self {
            width,
            height,
            device_features_sel: 0,
            driver_features_sel: 0,
            driver_features: [0; 2],
            config_msix_vector: VIRTIO_MSI_NO_VECTOR,
            queue_sel: 0,
            queues: [VirtioGpuQueue::new(0), VirtioGpuQueue::new(1)],
            pending_msix_queue_bits: 0,
            status: 0,
            interrupt_status: 0,
            events_read: 0,
            events_clear: 0,
            pending_config_change: false,
            resources: BTreeMap::new(),
            scanout_resource: None,
            blob_scanout: None,
            scanout: vec![0; len],
            fb_sink: FbSink::from_env(),
            three_d: VirtioGpu3d::new(),
            pending_fenced: Vec::new(),
            pending_vblank: Vec::new(),
            completed_fences_scratch: Vec::new(),
            descriptor_scratch: Vec::new(),
            parked_descriptor_scratch: Vec::new(),
            request_scratch: Vec::new(),
            response_scratch: Vec::new(),
            parked_response_scratch: Vec::new(),
            blob_row_scratch: Vec::new(),
            scanout_readback_scratch: Vec::new(),
            trace_fields_scratch: String::new(),
            trace: VirtioGpuTraceRecorder::from_env(),
            trace_queue_notify_count: 0,
            trace_submit_success_count: 0,
            trace_fence_create_count: 0,
            trace_fence_complete_count: 0,
            trace_fence_deliver_count: 0,
            vblank_interval: Duration::ZERO,
            last_vblank: None,
            vblank_paced_count: 0,
            vblank_wake: None,
            scanout_readback_interval: Duration::ZERO,
            last_3d_scanout_readback: None,
            scanout_3d_flush_count: 0,
            scanout_readback_attempt_count: 0,
            scanout_readback_count: 0,
            scanout_readback_throttled_count: 0,
            scanout_3d_deferred: false,
            pending_3d_scanout: None,
            pending_3d_scanout_fresh: false,
            pending_3d_scanout_blitted: false,
            deferred_scanout_flush_count: 0,
            deferred_scanout_serviced_count: 0,
            scanout_iosurface: false,
            scanout_iosurface_verify: false,
            scanout_iosurface_id: None,
            scanout_iosurface_dumped: false,
            scanout_blit_count: 0,
            scanout_blit_nanoseconds: 0,
            scanout_readback_bytes: 0,
            scanout_readback_nanoseconds: 0,
        };
        gpu.trace_device_init(false);
        gpu
    }

    pub fn with_3d_backend(width: u32, height: u32, backend: Box<dyn VirtioGpu3dBackend>) -> Self {
        let mut gpu = Self::new(width, height);
        gpu.three_d = VirtioGpu3d::with_backend(backend);
        gpu.trace
            .record("backend_attached", ",\"backend\":\"virtio-gpu-3d\"");
        gpu
    }

    pub fn set_shm_map_port(&mut self, port: Box<dyn GpuShmMapPort>, window_size: u64) {
        self.three_d.set_shm_map_port(port, window_size);
    }

    pub fn set_vblank_interval(&mut self, interval: Duration) {
        self.vblank_interval = interval;
        self.last_vblank = None;
        self.publish_vblank_wake();
        let enabled = !interval.is_zero();
        let interval_ns = interval.as_nanos();
        self.record_trace_fields("vblank_pacing_config", |fields| {
            let _ = write!(
                fields,
                ",\"enabled\":{enabled},\"interval_ns\":{interval_ns}"
            );
        });
    }

    /// Share the lock-free wake signal a host waker thread polls to bound
    /// vblank retire latency while the guest idles (no vCPU exits).
    pub fn set_vblank_wake(&mut self, wake: Arc<VblankWakeState>) {
        self.vblank_wake = Some(wake);
        self.publish_vblank_wake();
    }

    pub fn vblank_wake(&self) -> Option<Arc<VblankWakeState>> {
        self.vblank_wake.clone()
    }

    pub(crate) fn publish_vblank_wake(&self) {
        let Some(wake) = self.vblank_wake.as_ref() else {
            return;
        };
        let parked = !self.vblank_interval.is_zero() && !self.pending_vblank.is_empty();
        let deadline = self.last_vblank.map(|last| last + self.vblank_interval);
        wake.publish(parked, deadline);
    }

    pub fn set_3d_scanout_readback_interval(&mut self, interval: Duration) {
        self.scanout_readback_interval = interval;
        self.last_3d_scanout_readback = None;
    }

    pub fn set_3d_scanout_deferred(&mut self, deferred: bool) {
        self.scanout_3d_deferred = deferred;
        if !deferred {
            self.pending_3d_scanout = None;
            self.pending_3d_scanout_fresh = false;
        }
    }

    pub fn set_3d_scanout_iosurface(&mut self, enabled: bool, verify: bool) {
        self.scanout_iosurface = enabled;
        self.scanout_iosurface_verify = enabled && verify;
    }

    /// GPU-blit the scanout into the shared IOSurface (display path); the
    /// CPU readback stays as the paced evidence/FbSink feed.
    pub(crate) fn blit_3d_scanout_iosurface(&mut self, resource_id: u32) {
        if !self.scanout_iosurface {
            return;
        }
        let Some(info) = self.three_d.scanout_3d_info(resource_id) else {
            return;
        };
        let width = info.width.min(self.width);
        let height = info.height.min(self.height);
        let started = Instant::now();
        let Some(surface_id) = self
            .three_d
            .blit_3d_scanout_iosurface(resource_id, width, height)
        else {
            return;
        };
        let duration_ns = started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
        self.scanout_blit_count = self.scanout_blit_count.saturating_add(1);
        self.scanout_blit_nanoseconds = self.scanout_blit_nanoseconds.saturating_add(duration_ns);
        if self.scanout_iosurface_id != Some(surface_id) {
            self.scanout_iosurface_id = Some(surface_id);
            eprintln!("virtio-gpu: scanout IOSurface global id={surface_id} ({width}x{height})");
            // Publish the global ID beside the shared framebuffer so a
            // windowed viewer can IOSurfaceLookup + bind layer.contents
            // instead of consuming the CPU framebuffer file.
            if let Ok(fb_path) = std::env::var("BRIDGEVM_DISPLAY_EXPORT_FB") {
                let _ = std::fs::write(
                    format!("{fb_path}.iosurface"),
                    format!("{surface_id} {width} {height}\n"),
                );
            }
        }
        let count = self.scanout_blit_count;
        self.record_trace_fields("scanout_blit", |fields| {
            let _ = write!(
                fields,
                ",\"resource_id\":{resource_id},\"surface_id\":{surface_id},\"width\":{width},\"height\":{height},\"duration_ns\":{duration_ns},\"count\":{count}"
            );
        });
    }

    pub(crate) fn defer_3d_scanout(&mut self, resource_id: u32, rect: Rect) {
        self.deferred_scanout_flush_count = self.deferred_scanout_flush_count.saturating_add(1);
        let pending = match self.pending_3d_scanout.take() {
            Some((pending_id, pending_rect)) if pending_id == resource_id => {
                (resource_id, union_rect(pending_rect, rect))
            }
            // A different resource means the scanout switched; the stale
            // pending frame is superseded, not unioned.
            _ => (resource_id, rect),
        };
        self.pending_3d_scanout = Some(pending);
        self.pending_3d_scanout_fresh = true;
        self.pending_3d_scanout_blitted = false;
        if self.deferred_scanout_flush_count <= 8 {
            let count = self.deferred_scanout_flush_count;
            self.record_trace_fields("scanout_readback_deferred", |fields| {
                let _ = write!(fields, ",\"resource_id\":{resource_id},\"count\":{count}");
            });
        }
    }

    /// Service a flush-deferred 3D scanout readback from the per-exit drain.
    /// The fresh flag skips the drain pass of the exit that armed the flush,
    /// so the guest sees its RESOURCE_FLUSH response and the vCPU resumes
    /// before this thread pays for the GL readback. A pacing-not-due pending
    /// frame is kept (delayed), never dropped.
    pub fn service_deferred_3d_scanout(&mut self) {
        let Some((resource_id, rect)) = self.pending_3d_scanout else {
            return;
        };
        if self.pending_3d_scanout_fresh {
            self.pending_3d_scanout_fresh = false;
            return;
        }
        if self.scanout_resource != Some(resource_id) || !self.three_d.is_3d_resource(resource_id) {
            self.pending_3d_scanout = None;
            return;
        }
        if !self.pending_3d_scanout_blitted || self.scanout_iosurface_verify {
            // One blit per armed frame: retries of a pacing-held pending
            // frame must not re-blit at vCPU-exit cadence. Verify mode
            // re-blits so the checksum compares the same frame the CPU
            // readback is about to capture (the guest animates between an
            // armed frame's blit and a pacing-held readback).
            self.blit_3d_scanout_iosurface(resource_id);
            self.pending_3d_scanout_blitted = true;
        }
        match self.try_3d_scanout_readback(resource_id, rect, true) {
            ScanoutReadbackOutcome::NotDue => {}
            ScanoutReadbackOutcome::Gone => {
                self.pending_3d_scanout = None;
            }
            ScanoutReadbackOutcome::Done => {
                self.pending_3d_scanout = None;
                self.deferred_scanout_serviced_count =
                    self.deferred_scanout_serviced_count.saturating_add(1);
                self.publish_scanout_fb();
            }
        }
    }

    pub(crate) fn try_3d_scanout_readback(
        &mut self,
        resource_id: u32,
        rect: Rect,
        deferred: bool,
    ) -> ScanoutReadbackOutcome {
        let now = Instant::now();
        let readback_due = self.last_3d_scanout_readback.map_or(true, |last| {
            now.saturating_duration_since(last) >= self.scanout_readback_interval
        });
        if !readback_due {
            return ScanoutReadbackOutcome::NotDue;
        }
        self.scanout_readback_attempt_count = self.scanout_readback_attempt_count.saturating_add(1);
        let started = Instant::now();
        let Some(info) = self.three_d.scanout_3d_info(resource_id) else {
            return ScanoutReadbackOutcome::Gone;
        };
        let readback_width = info.width.min(self.width);
        let readback_height = info.height.min(self.height);
        let readback_len = scanout_len(readback_width, readback_height);
        self.scanout_readback_scratch.resize(readback_len, 0);
        self.scanout_readback_scratch.fill(0);
        let transfer_started = Instant::now();
        let transfer_ok = self.three_d.read_3d_scanout(
            resource_id,
            readback_width,
            readback_height,
            &mut self.scanout_readback_scratch,
        );
        let transfer_ns = transfer_started
            .elapsed()
            .as_nanos()
            .min(u128::from(u64::MAX)) as u64;
        let composite_started = Instant::now();
        let readback_ok = transfer_ok
            && composite_host_3d_to_scanout(
                &self.scanout_readback_scratch,
                readback_width,
                readback_height,
                &mut self.scanout,
                self.width,
                self.height,
                rect,
            );
        let composite_ns = composite_started
            .elapsed()
            .as_nanos()
            .min(u128::from(u64::MAX)) as u64;
        let elapsed = started.elapsed();
        let duration_ns = elapsed.as_nanos().min(u128::from(u64::MAX)) as u64;
        self.scanout_readback_nanoseconds = self
            .scanout_readback_nanoseconds
            .saturating_add(duration_ns);
        if readback_ok && self.scanout_iosurface_verify {
            // Hash four orientations of the CPU readback so a single run
            // identifies the transform the GPU blit applied: identity,
            // y-flip, R<->B swap, and both.
            let scratch = &self.scanout_readback_scratch[..readback_len];
            let row_bytes = readback_width as usize * 4;
            let rows = readback_height as usize;
            let cpu_checksum = fnv1a64(scratch);
            let mut flip = 0xcbf2_9ce4_8422_2325u64;
            let mut swap = 0xcbf2_9ce4_8422_2325u64;
            let mut flip_swap = 0xcbf2_9ce4_8422_2325u64;
            let fnv_byte = |hash: &mut u64, byte: u8| {
                *hash ^= u64::from(byte);
                *hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            };
            for y in 0..rows {
                let row = &scratch[y * row_bytes..(y + 1) * row_bytes];
                let flipped_row = &scratch[(rows - 1 - y) * row_bytes..(rows - y) * row_bytes];
                for x in (0..row_bytes).step_by(4) {
                    for (hash, src) in [(&mut swap, row), (&mut flip_swap, flipped_row)] {
                        fnv_byte(hash, src[x + 2]);
                        fnv_byte(hash, src[x + 1]);
                        fnv_byte(hash, src[x]);
                        fnv_byte(hash, src[x + 3]);
                    }
                }
                for &byte in flipped_row {
                    fnv_byte(&mut flip, byte);
                }
            }
            if let Some(gpu_checksum) = self.three_d.scanout_iosurface_checksum() {
                let matched = cpu_checksum == gpu_checksum;
                let matched_flip = flip == gpu_checksum;
                let matched_swap = swap == gpu_checksum;
                let matched_flip_swap = flip_swap == gpu_checksum;
                if !(matched || matched_flip || matched_swap || matched_flip_swap)
                    && !self.scanout_iosurface_dumped
                {
                    // First unexplained mismatch: dump both buffers beside
                    // the trace JSONL for offline inspection.
                    self.scanout_iosurface_dumped = true;
                    if let Some(dir) = std::env::var("BRIDGEVM_VIRTIO_GPU_TRACE_JSONL")
                        .ok()
                        .and_then(|p| std::path::Path::new(&p).parent().map(PathBuf::from))
                    {
                        let _ = self
                            .three_d
                            .scanout_iosurface_dump(&dir.join("iosurface-gpu.bin"));
                        let mut cpu_dump = Vec::with_capacity(8 + scratch.len());
                        cpu_dump.extend_from_slice(&readback_width.to_le_bytes());
                        cpu_dump.extend_from_slice(&readback_height.to_le_bytes());
                        cpu_dump.extend_from_slice(scratch);
                        let _ = std::fs::write(dir.join("iosurface-cpu.bin"), &cpu_dump);
                    }
                }
                self.record_trace_fields("scanout_iosurface_verify", |fields| {
                    let _ = write!(
                        fields,
                        ",\"matched\":{matched},\"matched_flip\":{matched_flip},\"matched_swap\":{matched_swap},\"matched_flip_swap\":{matched_flip_swap},\"cpu\":{cpu_checksum},\"gpu\":{gpu_checksum}"
                    );
                });
            }
        }
        if readback_ok {
            self.last_3d_scanout_readback = Some(Instant::now());
            self.scanout_readback_count = self.scanout_readback_count.saturating_add(1);
            let bytes = u64::from(readback_width)
                .saturating_mul(u64::from(readback_height))
                .saturating_mul(4);
            self.scanout_readback_bytes = self.scanout_readback_bytes.saturating_add(bytes);
            let count = self.scanout_readback_count;
            let width = readback_width;
            let height = readback_height;
            let deferred_flag = u8::from(deferred);
            // duration_ns spans scratch prep + GL transfer + CPU composite;
            // transfer_ns/composite_ns isolate the two phases.
            self.record_trace_fields("scanout_readback", |fields| {
                let _ = write!(
                    fields,
                    ",\"resource_id\":{resource_id},\"width\":{width},\"height\":{height},\"bytes\":{bytes},\"duration_ns\":{duration_ns},\"transfer_ns\":{transfer_ns},\"composite_ns\":{composite_ns},\"deferred\":{deferred_flag},\"count\":{count}"
                );
            });
        } else {
            let flush_count = self.scanout_3d_flush_count;
            if flush_count <= 8 {
                self.record_trace_fields("scanout_readback_failed", |fields| {
                    let _ = write!(
                        fields,
                        ",\"resource_id\":{resource_id},\"width\":{readback_width},\"height\":{readback_height},\"count\":{flush_count}"
                    );
                });
            }
        }
        ScanoutReadbackOutcome::Done
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
            scanout_active: self.scanout_resource.is_some() || self.blob_scanout.is_some(),
            scanout_3d_flushes: self.scanout_3d_flush_count,
            vblank_paced_count: self.vblank_paced_count,
            scanout_readback_attempts: self.scanout_readback_attempt_count,
            scanout_readbacks: self.scanout_readback_count,
            scanout_readback_throttled: self.scanout_readback_throttled_count,
            scanout_readback_bytes: self.scanout_readback_bytes,
            scanout_readback_nanoseconds: self.scanout_readback_nanoseconds,
            deferred_scanout_flushes: self.deferred_scanout_flush_count,
            deferred_scanout_serviced: self.deferred_scanout_serviced_count,
            scanout_blits: self.scanout_blit_count,
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
        self.pending_msix_queue_bits = 0;
        self.status = 0;
        self.interrupt_status = 0;
        self.events_read = 0;
        self.events_clear = 0;
        self.pending_config_change = false;
        self.resources.clear();
        self.scanout_resource = None;
        self.unbind_blob_scanout();
        self.scanout.clear();
        self.scanout.resize(scanout_len(width, height), 0);
        self.three_d.reset();
        self.pending_fenced.clear();
        self.pending_vblank.clear();
        self.completed_fences_scratch.clear();
        self.descriptor_scratch.clear();
        self.parked_descriptor_scratch.clear();
        self.request_scratch.clear();
        self.response_scratch.clear();
        self.parked_response_scratch.clear();
        self.blob_row_scratch.clear();
        self.scanout_readback_scratch.clear();
        self.trace_fields_scratch.clear();
        self.last_vblank = None;
        self.vblank_paced_count = 0;
        self.publish_vblank_wake();
        self.last_3d_scanout_readback = None;
        self.scanout_3d_flush_count = 0;
        self.scanout_readback_attempt_count = 0;
        self.scanout_readback_count = 0;
        self.scanout_readback_throttled_count = 0;
        self.scanout_readback_bytes = 0;
        self.scanout_readback_nanoseconds = 0;
    }

    pub fn scanout(&self) -> Option<VirtioGpuScanout<'_>> {
        (self.scanout_resource.is_some() || self.blob_scanout.is_some()).then_some(
            VirtioGpuScanout {
                bytes: &self.scanout,
                width: self.width,
                height: self.height,
                stride: self.width * 4,
                fourcc: DRM_FORMAT_XRGB8888,
            },
        )
    }

    pub(crate) fn access_common(
        &mut self,
        offset: u64,
        is_write: bool,
        size: u8,
        value: u64,
        mem: &mut dyn GuestMemoryMut,
    ) -> VirtioGpuResult {
        if !is_write {
            let value = self.read_common(offset, size);
            self.trace_common_read(offset, size, value);
            return VirtioGpuResult::ReadValue(value);
        }
        self.write_common(offset, size, value, mem);
        VirtioGpuResult::WriteAck
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
            self.driver_features[index] = (value as u32) & self.offered_features_word(index as u32);
            let select = self.driver_features_sel;
            let raw = value as u32;
            let accepted = self.driver_features[index];
            if venus_start_trace_enabled() {
                println!(
                    "venus-start: driver_features select={select} raw={raw:#x} accepted={accepted:#x} offered={:#x}",
                    self.offered_features_word(select)
                );
            }
            self.record_trace_fields("driver_features", |fields| {
                let _ = write!(
                    fields,
                    ",\"select\":{},\"raw\":{},\"accepted\":{},\"accepted_hex\":\"{:#x}\"",
                    select, raw, accepted, accepted
                );
            });
        }
    }

    pub(crate) fn offered_features_word(&self, select: u32) -> u32 {
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
            let vector = write_common_register(
                self.config_msix_vector.into(),
                COMMON_CONFIG_MSIX_VECTOR,
                2,
                offset,
                size,
                value,
            ) as u16;
            self.config_msix_vector = valid_msix_vector(vector);
            if venus_start_trace_enabled() {
                println!(
                    "venus-start: config_msix_vector write raw={vector} accepted={}",
                    self.config_msix_vector
                );
            }
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
            let vector = write_common_register(
                u64::from(queue.msix_vector),
                COMMON_QUEUE_MSIX_VECTOR,
                2,
                offset,
                size,
                value,
            ) as u16;
            queue.msix_vector = valid_msix_vector(vector);
            if venus_start_trace_enabled() {
                println!(
                    "venus-start: queue={} msix_vector write raw={vector} accepted={}",
                    self.queue_sel, queue.msix_vector
                );
            }
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
            if venus_start_trace_enabled() {
                println!(
                    "venus-start: queue={} enable write {} size={} desc={:#x} driver={:#x} device={:#x} msix_vector={}",
                    self.queue_sel, enable, queue.size, queue.desc, queue.driver, queue.device, queue.msix_vector
                );
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
        let raw = value as u32;
        let previous = self.status;
        let driver_features_word0 = self.driver_features[0];
        let driver_features_word1 = self.driver_features[1];
        let resources = self.resources.len();
        let scanout_active = self.scanout_resource.is_some() || self.blob_scanout.is_some();
        if venus_start_trace_enabled() {
            println!("venus-start: device_status write {raw:#x}");
        }
        self.record_trace_fields("device_status", |fields| {
            let _ = write!(
                fields,
                ",\"raw\":{},\"raw_hex\":\"{:#x}\",\"previous\":{},\"previous_hex\":\"{:#x}\",\"reset\":{},\"driver_features_word0\":{},\"driver_features_word0_hex\":\"{:#x}\",\"driver_features_word1\":{},\"driver_features_word1_hex\":\"{:#x}\",\"resources\":{},\"scanout_active\":{}",
                raw,
                raw,
                previous,
                previous,
                raw == 0,
                driver_features_word0,
                driver_features_word0,
                driver_features_word1,
                driver_features_word1,
                resources,
                scanout_active
            );
        });
        self.status = value as u32;
        if value == 0 {
            self.reset_runtime_state();
        }
    }

    pub(crate) fn selected_queue(&self) -> Option<VirtioGpuQueue> {
        self.queues.get(self.queue_sel as usize).copied()
    }

    pub(crate) fn write_selected_queue(&mut self, write: impl FnOnce(&mut VirtioGpuQueue)) {
        if let Some(queue) = self.queues.get_mut(self.queue_sel as usize) {
            write(queue);
        }
    }

    pub(crate) fn config_read(&self, offset: u64, size: u8) -> u64 {
        // struct virtio_gpu_config: le32 events_read @0, le32 events_clear @4,
        // le32 num_scanouts @8, le32 num_capsets @12. num_capsets was being
        // written into the num_scanouts slot, so Linux saw "number of cap
        // sets: 0" and never queried the venus capset (and a 2D-only device
        // reported zero scanouts).
        let mut config = [0u8; 16];
        config[0..4].copy_from_slice(&self.events_read.to_le_bytes());
        config[4..8].copy_from_slice(&self.events_clear.to_le_bytes());
        config[8..12].copy_from_slice(&1u32.to_le_bytes());
        let num_capsets = self.three_d.capset_count();
        config[12..16].copy_from_slice(&num_capsets.to_le_bytes());
        let value = read_le_from_bytes(&config, offset, size).unwrap_or(0);
        if venus_start_trace_enabled() {
            static COUNT: AtomicU64 = AtomicU64::new(0);
            let n = COUNT.fetch_add(1, Ordering::Relaxed) + 1;
            if trace_sample(n) {
                println!(
                    "venus-start: config_read n={n} off={offset:#x} size={size} value={value:#x} num_capsets={num_capsets}"
                );
            }
        }
        value
    }

    pub(crate) fn config_write(&mut self, offset: u64, size: u8, value: u64) {
        if common_access_touches(4, 4, offset, size) {
            self.events_clear =
                write_common_register(self.events_clear.into(), 4, 4, offset, size, value) as u32;
            // The driver acks a display event by writing its bit to
            // events_clear; clear the matching events_read bits so the next
            // GET_DISPLAY_INFO does not re-report a stale change.
            self.events_read &= !self.events_clear;
        }
    }

    /// Host-driven scanout resize. Updates the reported resolution and raises a
    /// virtio-gpu DISPLAY event + config-change interrupt so the guest WDDM
    /// driver re-queries GET_DISPLAY_INFO/GET_EDID and switches modes. No-op
    /// (returns false) when the size is unchanged or out of range; the caller
    /// delivers the config interrupt via the device wrapper's drain path.
    pub(crate) fn request_display_resolution(&mut self, width: u32, height: u32) -> bool {
        if width == 0
            || height == 0
            || width > MAX_SCANOUT_DIMENSION
            || height > MAX_SCANOUT_DIMENSION
        {
            return false;
        }
        if width == self.width && height == self.height {
            return false;
        }
        self.width = width;
        self.height = height;
        // Grow the 2D scanout backing to the new geometry; the guest re-creates
        // its scanout resource after the mode switch, so drop the stale binding.
        self.scanout.clear();
        self.scanout.resize(scanout_len(width, height), 0);
        self.scanout_resource = None;
        self.unbind_blob_scanout();
        self.events_read |= VIRTIO_GPU_EVENT_DISPLAY;
        self.pending_config_change = true;
        self.interrupt_status |= 2;
        true
    }

    pub(crate) fn notify_queue(&mut self, queue_index: u16, mem: &mut dyn GuestMemoryMut) {
        self.trace_queue_notify(queue_index);
        match usize::from(queue_index) {
            QUEUE_CONTROL => self.process_control_queue(mem),
            QUEUE_CURSOR => self.process_cursor_queue(mem),
            _ => {}
        }
    }

    pub(crate) fn process_control_queue(&mut self, mem: &mut dyn GuestMemoryMut) {
        self.process_queue(QUEUE_CONTROL, mem, true);
    }

    pub(crate) fn process_cursor_queue(&mut self, mem: &mut dyn GuestMemoryMut) {
        self.process_queue(QUEUE_CURSOR, mem, false);
    }

    pub(crate) fn process_queue(
        &mut self,
        queue_index: usize,
        mem: &mut dyn GuestMemoryMut,
        control: bool,
    ) {
        let queue = self.queues[queue_index];
        if !queue.ready || queue.desc == 0 || queue.driver == 0 {
            return;
        }
        // A driver may enable the queue without writing COMMON_QUEUE_SIZE (EDK2's
        // VirtioGpuDxe reads the advertised size but never writes it back). Gating
        // on the raw stored size left `queue.size == 0`, so the control queue was
        // never drained: firmware submitted GET_DISPLAY_INFO, polled the used ring
        // forever, and the guest hung before reaching the boot manager.
        let queue_size = queue.effective_size();
        let Some(avail_idx) = read_u16(mem, queue.driver + 2) else {
            return;
        };
        while self.queues[queue_index].last_avail_idx != avail_idx {
            let last_avail_idx = self.queues[queue_index].last_avail_idx;
            let ring_off = 4 + u64::from(last_avail_idx % queue_size) * 2;
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
        self.drain_completed_fences_after_queue(mem);
    }

    pub(crate) fn process_chain(
        &mut self,
        mem: &mut dyn GuestMemoryMut,
        queue: &VirtioGpuQueue,
        queue_index: usize,
        head: u16,
        control: bool,
    ) -> ChainCompletion {
        let mut descs = self.take_descriptor_scratch();
        if !Self::descriptor_chain_into(mem, queue, head, &mut descs) {
            self.descriptor_scratch = descs;
            return ChainCompletion::Immediate(0);
        }
        let mut request = std::mem::take(&mut self.request_scratch);
        Self::gather_readable_into(mem, &descs, &mut request);
        let mut response = self.take_response_scratch();
        response.clear();
        let handle_started = Instant::now();
        if control {
            self.handle_control_request_into(mem, &request, &mut response);
        } else {
            self.handle_cursor_request_into(&request, &mut response);
        }
        let handle_ns = handle_started
            .elapsed()
            .as_nanos()
            .min(u128::from(u64::MAX)) as u64;
        let Some(hdr) = CtrlHdr::parse(&request) else {
            let request_len = request.len();
            let response_len = response.len();
            self.record_trace_fields("command_parse_error", |fields| {
                let _ = write!(
                    fields,
                    ",\"queue\":{},\"head\":{},\"request_len\":{},\"response_len\":{}",
                    queue_index, head, request_len, response_len
                );
            });
            let used_len = Self::scatter_write(mem, &descs, &response);
            self.recycle_queue_scratch(descs, request, response);
            return ChainCompletion::Immediate(used_len);
        };
        self.trace_command(
            queue_index,
            head,
            control,
            &descs,
            &request,
            hdr,
            &response,
            handle_ns,
        );
        // viogpu3d uses an empty context-0 SUBMIT_3D as its control-queue
        // synchronization NOP. Its used-ring completion drives the guest's
        // DXGK CRTC_VSYNC notification, so park that completion when host
        // vblank pacing is enabled.
        if control
            && !self.vblank_interval.is_zero()
            && hdr.typ == VIRTIO_GPU_CMD_SUBMIT_3D
            && hdr.ctx_id == 0
            && read_le_u32(&request, 24) == Some(0)
            && read_le_u32(&response, 0) == Some(VIRTIO_GPU_RESP_OK_NODATA)
        {
            self.pending_vblank.push(PendingVblankResponse {
                queue_index,
                queue: *queue,
                head,
                descs,
                response,
            });
            self.publish_vblank_wake();
            request.clear();
            self.request_scratch = request;
            self.response_scratch = Vec::new();
            return ChainCompletion::Parked;
        }
        // Defer only commands that can leave GPU work in flight. Resource/context
        // lifecycle, capset, and map operations are complete when their backend
        // call returns, so their fence is already satisfied. In particular,
        // RESOURCE_CREATE_3D normally carries ctx_id=0; trying to create a context
        // fence for it is invalid in virglrenderer and floods the host log.
        if control
            && hdr.flags & VIRTIO_GPU_FLAG_FENCE != 0
            && hdr.ctx_id != 0
            && self.three_d.has_backend()
            // The WDDM KMD uses numeric context ids for its display-copy path
            // before any UMD VIOGPU_CTX_INIT/CTX_CREATE. Those commands are
            // handled synchronously by the local scanout path and have no
            // renderer context on which virglrenderer could create a fence.
            && self.three_d.has_live_context(hdr.ctx_id)
            && command_requires_backend_fence(hdr.typ)
        {
            let fence = CompletedFence {
                ctx_id: hdr.ctx_id,
                ring_idx: hdr.ring_idx(),
                fence_id: hdr.fence_id,
            };
            if self.three_d.create_fence(fence) {
                self.trace_fence_create(fence, true, "parked");
                self.pending_fenced.push(PendingFencedResponse {
                    queue_index,
                    queue: *queue,
                    head,
                    descs,
                    response,
                    fence,
                });
                request.clear();
                self.request_scratch = request;
                self.response_scratch = Vec::new();
                return ChainCompletion::Parked;
            }
            // If virgl rejects the requested timeline, the command response is
            // still delivered; there is no backend fence that can retire it.
            self.trace_fence_create(fence, false, "immediate");
        }
        let used_len = Self::scatter_write(mem, &descs, &response);
        self.recycle_queue_scratch(descs, request, response);
        ChainCompletion::Immediate(used_len)
    }

    pub(crate) fn recycle_queue_scratch(
        &mut self,
        mut descs: Vec<Descriptor>,
        mut request: Vec<u8>,
        mut response: Vec<u8>,
    ) {
        descs.clear();
        request.clear();
        response.clear();
        self.descriptor_scratch = descs;
        self.request_scratch = request;
        self.response_scratch = response;
    }

    pub(crate) fn take_descriptor_scratch(&mut self) -> Vec<Descriptor> {
        let scratch = std::mem::take(&mut self.descriptor_scratch);
        if scratch.capacity() == 0 {
            self.parked_descriptor_scratch.pop().unwrap_or(scratch)
        } else {
            scratch
        }
    }

    pub(crate) fn take_response_scratch(&mut self) -> Vec<u8> {
        let scratch = std::mem::take(&mut self.response_scratch);
        if scratch.capacity() == 0 {
            self.parked_response_scratch.pop().unwrap_or(scratch)
        } else {
            scratch
        }
    }

    pub(crate) fn recycle_parked_response_buffers(
        &mut self,
        mut descs: Vec<Descriptor>,
        mut response: Vec<u8>,
    ) {
        descs.clear();
        response.clear();
        self.recycle_descriptor_scratch(descs);
        self.recycle_response_scratch(response);
    }

    pub(crate) fn recycle_descriptor_scratch(&mut self, mut descs: Vec<Descriptor>) {
        if descs.capacity() > self.descriptor_scratch.capacity() {
            std::mem::swap(&mut self.descriptor_scratch, &mut descs);
        }
        self.recycle_extra_descriptor_scratch(descs);
    }

    pub(crate) fn recycle_response_scratch(&mut self, mut response: Vec<u8>) {
        if response.capacity() > self.response_scratch.capacity() {
            std::mem::swap(&mut self.response_scratch, &mut response);
        }
        self.recycle_extra_response_scratch(response);
    }

    pub(crate) fn recycle_extra_descriptor_scratch(&mut self, descs: Vec<Descriptor>) {
        if descs.capacity() != 0
            && self.parked_descriptor_scratch.len() < PARKED_RESPONSE_BUFFER_POOL_LIMIT
        {
            self.parked_descriptor_scratch.push(descs);
        }
    }

    pub(crate) fn recycle_extra_response_scratch(&mut self, response: Vec<u8>) {
        if response.capacity() != 0
            && self.parked_response_scratch.len() < PARKED_RESPONSE_BUFFER_POOL_LIMIT
        {
            self.parked_response_scratch.push(response);
        }
    }

    pub(crate) fn handle_cursor_request_into(&mut self, request: &[u8], out: &mut Vec<u8>) {
        let hdr = CtrlHdr::parse(request);
        match hdr.map(|h| h.typ) {
            Some(VIRTIO_GPU_CMD_UPDATE_CURSOR | VIRTIO_GPU_CMD_MOVE_CURSOR) => {
                response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, hdr);
            }
            _ => response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, hdr),
        }
    }

    pub(crate) fn handle_control_request_into(
        &mut self,
        mem: &dyn GuestMemoryMut,
        request: &[u8],
        out: &mut Vec<u8>,
    ) {
        let Some(hdr) = CtrlHdr::parse(request) else {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, None);
            return;
        };
        match hdr.typ {
            VIRTIO_GPU_CMD_GET_DISPLAY_INFO => self.response_display_info_into(Some(hdr), out),
            VIRTIO_GPU_CMD_GET_EDID => self.response_edid_into(Some(hdr), out),
            VIRTIO_GPU_CMD_RESOURCE_CREATE_2D => {
                self.resource_create_2d_into(request, Some(hdr), out)
            }
            VIRTIO_GPU_CMD_RESOURCE_UNREF => self.resource_unref_into(request, Some(hdr), out),
            VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING => {
                self.attach_backing_into(mem, request, Some(hdr), out)
            }
            VIRTIO_GPU_CMD_RESOURCE_DETACH_BACKING => {
                self.detach_backing_into(request, Some(hdr), out)
            }
            VIRTIO_GPU_CMD_SET_SCANOUT => self.set_scanout_into(request, Some(hdr), out),
            VIRTIO_GPU_CMD_SET_SCANOUT_BLOB => self.set_scanout_blob_into(request, Some(hdr), out),
            VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D => {
                self.transfer_to_host_2d_into(mem, request, Some(hdr), out)
            }
            VIRTIO_GPU_CMD_RESOURCE_FLUSH => self.resource_flush_into(mem, request, Some(hdr), out),
            VIRTIO_GPU_CMD_GET_CAPSET_INFO
            | VIRTIO_GPU_CMD_GET_CAPSET
            | VIRTIO_GPU_CMD_RESOURCE_CREATE_BLOB
            | VIRTIO_GPU_CMD_CTX_CREATE
            | VIRTIO_GPU_CMD_CTX_DESTROY
            | VIRTIO_GPU_CMD_CTX_ATTACH_RESOURCE
            | VIRTIO_GPU_CMD_CTX_DETACH_RESOURCE
            | VIRTIO_GPU_CMD_RESOURCE_CREATE_3D
            | VIRTIO_GPU_CMD_TRANSFER_TO_HOST_3D
            | VIRTIO_GPU_CMD_TRANSFER_FROM_HOST_3D
            | VIRTIO_GPU_CMD_SUBMIT_3D
            | VIRTIO_GPU_CMD_RESOURCE_MAP_BLOB
            | VIRTIO_GPU_CMD_RESOURCE_UNMAP_BLOB => {
                let hdr3d = CtrlHdr3d::parse(request).unwrap();
                if hdr3d.typ == VIRTIO_GPU_CMD_CTX_DESTROY {
                    if let Some(resource_id) = self
                        .blob_scanout
                        .as_ref()
                        .map(|scanout| scanout.resource_id)
                    {
                        if self.three_d.ctx_has_resource(hdr3d.ctx_id, resource_id) {
                            self.unbind_blob_scanout();
                        }
                    }
                }
                if !self
                    .three_d
                    .handle_with_mem_into(Some(mem), request, hdr3d, out)
                {
                    virtio_gpu_3d::response_hdr_into(
                        out,
                        virtio_gpu_3d::VIRTIO_GPU_RESP_ERR_UNSPEC,
                        Some(hdr3d),
                    );
                }
            }
            _ => response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, Some(hdr)),
        }
    }

    pub fn drain_host_vblank(&mut self, mem: &mut dyn GuestMemoryMut) {
        self.drain_host_vblank_at(mem, Instant::now());
    }

    pub(crate) fn drain_host_vblank_at(&mut self, mem: &mut dyn GuestMemoryMut, now: Instant) {
        if self.vblank_interval.is_zero() || self.pending_vblank.is_empty() {
            return;
        }
        if self
            .last_vblank
            .is_some_and(|last| now.saturating_duration_since(last) < self.vblank_interval)
        {
            return;
        }

        // Retire exactly one response. Even if the vCPU did not exit for several
        // intervals, do not catch up in a burst.
        let pending_response = self.pending_vblank.remove(0);
        let used_len =
            Self::scatter_write(mem, &pending_response.descs, &pending_response.response);
        Self::write_used(
            mem,
            &pending_response.queue,
            pending_response.head,
            used_len,
        );
        self.mark_queue_interrupt(pending_response.queue_index);
        // Anchor the next deadline on the absolute schedule, not the (late)
        // retire time, so wake/drain latency does not accumulate into a lower
        // long-run rate. Re-anchor at `now` only after a gap of more than one
        // interval (guest asleep) — never catch up in a burst.
        self.last_vblank = Some(match self.last_vblank {
            Some(last)
                if now.saturating_duration_since(last + self.vblank_interval)
                    <= self.vblank_interval =>
            {
                last + self.vblank_interval
            }
            _ => now,
        });
        self.vblank_paced_count = self.vblank_paced_count.saturating_add(1);
        self.publish_vblank_wake();

        let count = self.vblank_paced_count;
        let interval_ns = self.vblank_interval.as_nanos();
        let pending = self.pending_vblank.len();
        self.record_trace_fields("vblank_paced", |fields| {
            let _ = write!(
                fields,
                ",\"vblank_paced_count\":{count},\"interval_ns\":{interval_ns},\"used_len\":{used_len},\"pending\":{pending}"
            );
        });
        self.recycle_parked_response_buffers(pending_response.descs, pending_response.response);
    }

    pub fn drain_completed_fences(&mut self, mem: &mut dyn GuestMemoryMut) {
        self.drain_completed_fences_inner(mem, false);
    }

    pub(crate) fn drain_completed_fences_after_queue(&mut self, mem: &mut dyn GuestMemoryMut) {
        self.drain_completed_fences_inner(mem, true);
    }

    pub(crate) fn drain_completed_fences_inner(
        &mut self,
        mem: &mut dyn GuestMemoryMut,
        after_queue: bool,
    ) {
        let mut completed = std::mem::take(&mut self.completed_fences_scratch);
        completed.clear();
        if after_queue {
            self.three_d
                .drain_completed_fences_after_queue_into(&mut completed);
        } else {
            self.three_d.drain_completed_fences_into(&mut completed);
        }
        if completed.is_empty() || self.pending_fenced.is_empty() {
            for fence in &completed {
                self.trace_fence_complete(*fence);
            }
            completed.clear();
            self.completed_fences_scratch = completed;
            return;
        }
        for fence in &completed {
            self.trace_fence_complete(*fence);
        }
        let mut index = 0;
        while index < self.pending_fenced.len() {
            let ready = completed.iter().any(|completed| {
                let pending_response = &self.pending_fenced[index];
                completed.ctx_id == pending_response.fence.ctx_id
                    && completed.ring_idx == pending_response.fence.ring_idx
                    && completed.fence_id >= pending_response.fence.fence_id
            });
            if !ready {
                index += 1;
                continue;
            }

            let pending_response = self.pending_fenced.remove(index);
            let used_len =
                Self::scatter_write(mem, &pending_response.descs, &pending_response.response);
            self.trace_fence_delivery(pending_response.fence, used_len);
            Self::write_used(
                mem,
                &pending_response.queue,
                pending_response.head,
                used_len,
            );
            self.mark_queue_interrupt(pending_response.queue_index);
            self.recycle_parked_response_buffers(pending_response.descs, pending_response.response);
        }
        completed.clear();
        self.completed_fences_scratch = completed;
    }

    pub(crate) fn response_display_info_into(&self, hdr: Option<CtrlHdr>, out: &mut Vec<u8>) {
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_DISPLAY_INFO, hdr);
        for scanout in 0..16 {
            if scanout == 0 {
                push_rect(
                    out,
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
    }

    pub(crate) fn response_edid_into(&self, hdr: Option<CtrlHdr>, out: &mut Vec<u8>) {
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_EDID, hdr);
        out.extend_from_slice(&128u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        let edid = build_edid(self.width, self.height);
        out.extend_from_slice(&edid);
        out.resize(out.len() + (1024 - 128), 0);
    }

    pub(crate) fn resource_create_2d_into(
        &mut self,
        request: &[u8],
        hdr: Option<CtrlHdr>,
        out: &mut Vec<u8>,
    ) {
        let Some(resource_id) = read_le_u32(request, 24) else {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, hdr);
            return;
        };
        let format = read_le_u32(request, 28).unwrap_or(0);
        let width = read_le_u32(request, 32).unwrap_or(0);
        let height = read_le_u32(request, 36).unwrap_or(0);
        if resource_id == 0 || width == 0 || height == 0 || !format_supported(format) {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, hdr);
            return;
        }
        let Some(len) = u64::from(width)
            .checked_mul(u64::from(height))
            .and_then(|pixels| pixels.checked_mul(4))
            .and_then(|bytes| usize::try_from(bytes).ok())
        else {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, hdr);
            return;
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
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, hdr);
    }

    pub(crate) fn resource_unref_into(
        &mut self,
        request: &[u8],
        hdr: Option<CtrlHdr>,
        out: &mut Vec<u8>,
    ) {
        if let Some(resource_id) = read_le_u32(request, 24) {
            if self
                .blob_scanout
                .as_ref()
                .map(|scanout| scanout.resource_id)
                == Some(resource_id)
            {
                self.unbind_blob_scanout();
            }
            self.resources.remove(&resource_id);
            self.three_d.unref_resource(resource_id);
            if self.scanout_resource == Some(resource_id) {
                self.scanout_resource = None;
            }
        }
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, hdr);
    }

    pub(crate) fn attach_backing_into(
        &mut self,
        mem: &dyn GuestMemoryMut,
        request: &[u8],
        hdr: Option<CtrlHdr>,
        out: &mut Vec<u8>,
    ) {
        let Some(resource_id) = read_le_u32(request, 24) else {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, hdr);
            return;
        };
        let nr_entries = read_le_u32(request, 28).unwrap_or(0);
        let Some(entries_len) = (nr_entries as usize).checked_mul(16) else {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, hdr);
            return;
        };
        if request.len().saturating_sub(32) < entries_len {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, hdr);
            return;
        }
        let mut backing = Vec::with_capacity(nr_entries as usize);
        let mut offset = 32usize;
        for _ in 0..nr_entries {
            let addr = read_le_u64(request, offset).unwrap();
            let len = read_le_u32(request, offset + 8).unwrap();
            backing.push(BlobMemEntry { addr, len });
            offset += 16;
        }
        if let Some(resource) = self.resources.get_mut(&resource_id) {
            resource.backing.clear();
            resource
                .backing
                .extend(backing.iter().map(|entry| BackingEntry {
                    addr: entry.addr,
                    len: entry.len,
                }));
        } else if self.three_d.is_3d_resource(resource_id) {
            if !self.three_d.attach_3d_backing(mem, resource_id, &backing) {
                response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, hdr);
                return;
            }
        } else {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, hdr);
            return;
        }
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, hdr);
    }

    pub(crate) fn detach_backing_into(
        &mut self,
        request: &[u8],
        hdr: Option<CtrlHdr>,
        out: &mut Vec<u8>,
    ) {
        if let Some(resource_id) = read_le_u32(request, 24) {
            if let Some(resource) = self.resources.get_mut(&resource_id) {
                resource.backing.clear();
            } else if self.three_d.is_3d_resource(resource_id)
                && !self.three_d.detach_3d_backing(resource_id)
            {
                response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, hdr);
                return;
            }
        }
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, hdr);
    }

    pub(crate) fn set_scanout_into(
        &mut self,
        request: &[u8],
        hdr: Option<CtrlHdr>,
        out: &mut Vec<u8>,
    ) {
        let rect = read_rect(request, 24).unwrap_or(Rect {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        });
        let scanout_id = read_le_u32(request, 40).unwrap_or(u32::MAX);
        let resource_id = read_le_u32(request, 44).unwrap_or(0);
        if scanout_id != 0 {
            response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, hdr);
            return;
        }
        if resource_id == 0 {
            self.scanout_resource = None;
            self.unbind_blob_scanout();
        } else {
            let valid_resource = self.resources.contains_key(&resource_id)
                || self
                    .three_d
                    .scanout_3d_info(resource_id)
                    .is_some_and(|info| {
                        format_supported(info.format)
                            && rect.width > 0
                            && rect.height > 0
                            && rect.width <= self.width
                            && rect.height <= self.height
                            && rect
                                .x
                                .checked_add(rect.width)
                                .is_some_and(|end| end <= info.width)
                            && rect
                                .y
                                .checked_add(rect.height)
                                .is_some_and(|end| end <= info.height)
                    });
            if !valid_resource {
                response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, hdr);
                return;
            }
            self.unbind_blob_scanout();
            self.scanout_resource = Some(resource_id);
        }
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, hdr);
    }

    pub(crate) fn set_scanout_blob_into(
        &mut self,
        request: &[u8],
        hdr: Option<CtrlHdr>,
        out: &mut Vec<u8>,
    ) {
        if request.len() < SET_SCANOUT_BLOB_LEN {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, hdr);
            return;
        }
        let scanout_id = read_le_u32(request, 40).unwrap_or(u32::MAX);
        let resource_id = read_le_u32(request, 44).unwrap_or(0);
        if scanout_id != 0 {
            response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, hdr);
            return;
        }
        if resource_id == 0 {
            self.unbind_blob_scanout();
            self.scanout_resource = None;
            response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, hdr);
            return;
        }

        let width = read_le_u32(request, 48).unwrap_or(0);
        let height = read_le_u32(request, 52).unwrap_or(0);
        let format = read_le_u32(request, 56).unwrap_or(0);
        let stride = read_le_u32(request, 64).unwrap_or(0);
        let offset = read_le_u32(request, 80).unwrap_or(0);
        if width == 0
            || height == 0
            || width > self.width
            || height > self.height
            || !format_supported(format)
            || stride < width.saturating_mul(4)
        {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, hdr);
            return;
        }

        let Some(info) = self.three_d.blob_resource_info_ref(resource_id) else {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, hdr);
            return;
        };
        let blob_mem = info.blob_mem;
        let blob_size = info.size;
        if blob_mem != VIRTIO_GPU_BLOB_MEM_GUEST && blob_mem != VIRTIO_GPU_BLOB_MEM_HOST3D {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, hdr);
            return;
        }
        let Some(footprint) = blob_surface_footprint(width, height, stride, offset) else {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, hdr);
            return;
        };
        if footprint > blob_size {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, hdr);
            return;
        }

        self.unbind_blob_scanout();
        let mapping = if blob_mem == VIRTIO_GPU_BLOB_MEM_HOST3D {
            let Some(mapped) = self.three_d.scanout_map_blob(resource_id) else {
                response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, hdr);
                return;
            };
            if mapped.host_ptr.is_null() || (mapped.size as u64) < footprint {
                self.three_d.scanout_unmap_blob(resource_id);
                response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, hdr);
                return;
            }
            Some(BlobScanoutMapping {
                ptr: mapped.host_ptr,
                len: mapped.size,
            })
        } else {
            None
        };
        self.scanout_resource = None;
        self.blob_scanout = Some(BlobScanout {
            resource_id,
            width,
            height,
            format,
            stride,
            offset,
            mapping,
        });
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, hdr);
    }

    pub(crate) fn transfer_to_host_2d_into(
        &mut self,
        mem: &dyn GuestMemoryMut,
        request: &[u8],
        hdr: Option<CtrlHdr>,
        out: &mut Vec<u8>,
    ) {
        let rect = read_rect(request, 24).unwrap_or(Rect {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        });
        let offset = read_le_u64(request, 40).unwrap_or(0);
        let resource_id = read_le_u32(request, 48).unwrap_or(0);
        let Some(resource) = self.resources.get_mut(&resource_id) else {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, hdr);
            return;
        };
        copy_backing_to_resource(mem, resource, rect, offset);
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, hdr);
    }

    pub(crate) fn publish_scanout_fb(&mut self) {
        if self.scanout_resource.is_none() && self.blob_scanout.is_none() {
            return;
        }
        self.publish_scanout_fb_unconditionally();
    }

    /// Write the current `scanout` pixels to the export sink even without an
    /// active scanout binding. Restore uses this: the checkpointed pixels are
    /// the last frame the guest presented, but the blob scanout that produced
    /// them is not serializable, so without this one-shot publish the display
    /// export stays black until the guest's WDDM TDR re-establishes the
    /// scanout and presents fresh.
    pub(crate) fn publish_scanout_fb_unconditionally(&mut self) {
        let width = self.width;
        let height = self.height;
        let stride = width * 4;
        if self.scanout.len() < (stride as usize) * (height as usize) {
            return;
        }
        let (fb_sink, scanout) = (&mut self.fb_sink, &self.scanout);
        if let Some(sink) = fb_sink.as_mut() {
            sink.write(width, height, stride, DRM_FORMAT_XRGB8888, scanout);
        }
    }

    pub(crate) fn resource_flush_into(
        &mut self,
        mem: &dyn GuestMemoryMut,
        request: &[u8],
        hdr: Option<CtrlHdr>,
        out: &mut Vec<u8>,
    ) {
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
            } else if self.three_d.is_3d_resource(resource_id) {
                self.scanout_3d_flush_count = self.scanout_3d_flush_count.saturating_add(1);
                let flush_count = self.scanout_3d_flush_count;
                if flush_count <= 8 {
                    let info = self.three_d.scanout_3d_info(resource_id);
                    let local_backing = self.three_d.local_3d_backing(resource_id).is_some();
                    let display_width = self.width;
                    let display_height = self.height;
                    self.record_trace_fields("scanout_3d_flush", |fields| {
                        let _ = write!(
                            fields,
                            ",\"resource_id\":{resource_id},\"resource_width\":{},\"resource_height\":{},\"display_width\":{},\"display_height\":{},\"local_backing\":{local_backing},\"count\":{flush_count}",
                            info.map_or(0, |info| info.width),
                            info.map_or(0, |info| info.height),
                            display_width,
                            display_height
                        );
                    });
                }
                let local_readback = self
                    .three_d
                    .scanout_3d_info(resource_id)
                    .zip(self.three_d.local_3d_backing(resource_id))
                    .map(|(info, backing)| {
                        let started = Instant::now();
                        let copied = composite_local_3d_to_scanout(
                            mem,
                            backing,
                            info,
                            &mut self.scanout,
                            self.width,
                            self.height,
                            rect,
                            &mut self.blob_row_scratch,
                        );
                        (copied, started.elapsed())
                    });
                if let Some((readback_ok, elapsed)) = local_readback {
                    self.scanout_readback_attempt_count =
                        self.scanout_readback_attempt_count.saturating_add(1);
                    let duration_ns = elapsed.as_nanos().min(u128::from(u64::MAX)) as u64;
                    self.scanout_readback_nanoseconds = self
                        .scanout_readback_nanoseconds
                        .saturating_add(duration_ns);
                    if readback_ok {
                        self.scanout_readback_count = self.scanout_readback_count.saturating_add(1);
                        let bytes = u64::from(rect.width)
                            .saturating_mul(u64::from(rect.height))
                            .saturating_mul(4);
                        self.scanout_readback_bytes =
                            self.scanout_readback_bytes.saturating_add(bytes);
                        let count = self.scanout_readback_count;
                        self.record_trace_fields("scanout_guest_backing", |fields| {
                            let _ = write!(
                                fields,
                                ",\"resource_id\":{resource_id},\"width\":{},\"height\":{},\"bytes\":{bytes},\"duration_ns\":{duration_ns},\"count\":{count}",
                                rect.width, rect.height
                            );
                        });
                    }
                    self.publish_scanout_fb();
                    response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, hdr);
                    return;
                }
                if self.scanout_3d_deferred {
                    // Decouple the GL readback from the guest's flush: arm a
                    // pending readback and respond OK now; the per-exit drain
                    // services it after the vCPU has resumed at least once.
                    self.defer_3d_scanout(resource_id, rect);
                    response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, hdr);
                    return;
                }
                self.blit_3d_scanout_iosurface(resource_id);
                match self.try_3d_scanout_readback(resource_id, rect, false) {
                    ScanoutReadbackOutcome::Done | ScanoutReadbackOutcome::Gone => {}
                    ScanoutReadbackOutcome::NotDue => {
                        self.scanout_readback_throttled_count =
                            self.scanout_readback_throttled_count.saturating_add(1);
                        let throttled = self.scanout_readback_throttled_count;
                        let width = self.width;
                        let height = self.height;
                        self.record_trace_fields("scanout_readback_throttled", |fields| {
                            let _ = write!(
                                fields,
                                ",\"resource_id\":{resource_id},\"width\":{width},\"height\":{height},\"count\":{throttled}"
                            );
                        });
                    }
                }
            }
        } else if self
            .blob_scanout
            .as_ref()
            .is_some_and(|scanout| scanout.resource_id == resource_id)
        {
            self.composite_blob_scanout(mem, rect);
        }
        self.publish_scanout_fb();
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, hdr);
    }

    pub(crate) fn composite_blob_scanout(&mut self, mem: &dyn GuestMemoryMut, rect: Rect) {
        let Some(scanout) = self.blob_scanout.as_ref() else {
            return;
        };
        let Some(info) = self.three_d.blob_resource_info_ref(scanout.resource_id) else {
            return;
        };
        let x_end = rect
            .x
            .saturating_add(rect.width)
            .min(self.width)
            .min(scanout.width);
        let y_end = rect
            .y
            .saturating_add(rect.height)
            .min(self.height)
            .min(scanout.height);

        match info.blob_mem {
            VIRTIO_GPU_BLOB_MEM_GUEST => composite_guest_blob_to_scanout(
                GuestBlobComposite {
                    mem,
                    backing: info.backing,
                    scanout: &mut self.scanout,
                    scanout_width: self.width,
                    blob: scanout,
                    row_pixels: &mut self.blob_row_scratch,
                },
                rect,
                x_end,
                y_end,
            ),
            VIRTIO_GPU_BLOB_MEM_HOST3D => {
                let Some(mapping) = scanout.mapping else {
                    return;
                };
                let pixels = unsafe { std::slice::from_raw_parts(mapping.ptr, mapping.len) };
                composite_host_blob_to_scanout(
                    pixels,
                    &mut self.scanout,
                    self.width,
                    scanout,
                    rect,
                    x_end,
                    y_end,
                );
            }
            _ => {}
        }
    }

    pub(crate) fn unbind_blob_scanout(&mut self) {
        if let Some(scanout) = self.blob_scanout.take() {
            if scanout.mapping.is_some() {
                self.three_d.scanout_unmap_blob(scanout.resource_id);
            }
        }
    }

    pub(crate) fn trace_device_init(&mut self, backend_3d: bool) {
        let width = self.width;
        let height = self.height;
        self.record_trace_fields("device_init", |fields| {
            let _ = write!(
                fields,
                ",\"width\":{},\"height\":{},\"device_id\":{},\"vendor_id\":{},\"queue_count\":{},\"queue_max\":{},\"msix_vectors\":{},\"backend_3d\":{},\"common_cfg_offset\":{},\"device_cfg_offset\":{},\"notify_cfg_offset\":{}",
                width,
                height,
                DEVICE_ID_GPU,
                VENDOR_ID_QEMU,
                QUEUE_COUNT,
                QUEUE_MAX,
                VIRTIO_GPU_MSIX_VECTOR_COUNT,
                backend_3d,
                PCI_COMMON_CFG_OFFSET,
                PCI_DEVICE_CFG_OFFSET,
                PCI_NOTIFY_CFG_OFFSET
            );
        });
    }

    pub(crate) fn trace_common_read(&mut self, offset: u64, size: u8, value: u64) {
        if !self.trace.enabled() {
            return;
        }
        let field = match offset {
            COMMON_DEVICE_FEATURE | REG_DEVICE_FEATURES => "device_features",
            COMMON_DRIVER_FEATURE | REG_DRIVER_FEATURES => "driver_features",
            COMMON_DEVICE_STATUS | REG_STATUS => "device_status",
            COMMON_QUEUE_SIZE | REG_QUEUE_NUM => "queue_size",
            COMMON_QUEUE_ENABLE | REG_QUEUE_READY => "queue_enable",
            _ => return,
        };
        let device_features_sel = self.device_features_sel;
        let driver_features_sel = self.driver_features_sel;
        let queue_sel = self.queue_sel;
        self.record_trace_fields("common_read", |fields| {
            fields.push_str(",\"field\":");
            write_json_string(fields, field);
            let _ = write!(
                fields,
                ",\"offset\":{},\"size\":{},\"value\":{},\"value_hex\":\"{:#x}\",\"device_features_sel\":{},\"driver_features_sel\":{},\"queue_sel\":{}",
                offset,
                size,
                value,
                value,
                device_features_sel,
                driver_features_sel,
                queue_sel
            );
        });
    }

    pub(crate) fn trace_queue_notify(&mut self, queue_index: u16) {
        if !self.trace.enabled() {
            return;
        }
        self.trace_queue_notify_count = self.trace_queue_notify_count.saturating_add(1);
        if !trace_sample(self.trace_queue_notify_count) {
            return;
        }
        let Some(queue) = self.queues.get(queue_index as usize).copied() else {
            self.record_trace_fields("queue_notify", |fields| {
                let _ = write!(fields, ",\"queue\":{},\"valid\":false", queue_index);
            });
            return;
        };
        self.record_trace_fields("queue_notify", |fields| {
            let _ = write!(
                fields,
                ",\"queue\":{},\"valid\":true,\"size\":{},\"ready\":{},\"desc\":{},\"driver\":{},\"device\":{},\"msix_vector\":{},\"last_avail_idx\":{}",
                queue_index,
                queue.size,
                queue.ready,
                queue.desc,
                queue.driver,
                queue.device,
                queue.msix_vector,
                queue.last_avail_idx
            );
        });
    }

    pub(crate) fn record_trace_fields(
        &mut self,
        event: &str,
        write_fields: impl FnOnce(&mut String),
    ) {
        if !self.trace.enabled() {
            return;
        }
        let mut fields = std::mem::take(&mut self.trace_fields_scratch);
        fields.clear();
        write_fields(&mut fields);
        self.trace.record(event, &fields);
        fields.clear();
        self.trace_fields_scratch = fields;
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn trace_command(
        &mut self,
        queue_index: usize,
        head: u16,
        control: bool,
        descs: &[Descriptor],
        request: &[u8],
        hdr: CtrlHdr,
        response: &[u8],
        handle_ns: u64,
    ) {
        venus_start_trace_command(request, hdr, response);
        if !self.trace.enabled() {
            return;
        }
        let response_type = read_le_u32(response, 0).unwrap_or(0);
        if hdr.typ == VIRTIO_GPU_CMD_SUBMIT_3D && response_type == VIRTIO_GPU_RESP_OK_NODATA {
            // Sample only EMPTY submissions: the Windows KMD's 60 Hz vsync
            // heartbeat floods this counter with size-0 no-ops, and sampling
            // everything let those consume the un-sampled budget so real
            // application command buffers vanished from the trace minutes
            // into a boot. A nonempty SUBMIT_3D is exactly what the P3 gate
            // exists to witness; record every one of them.
            let submit_size = read_le_u32(request, 24).unwrap_or(0);
            if submit_size == 0 {
                self.trace_submit_success_count = self.trace_submit_success_count.saturating_add(1);
                if !trace_sample(self.trace_submit_success_count) {
                    return;
                }
            }
        }
        let readable_descriptor_count = descs
            .iter()
            .filter(|desc| desc.flags & DESC_F_WRITE == 0)
            .count();
        let writable_descriptor_count = descs.len().saturating_sub(readable_descriptor_count);
        let readable_descriptor_bytes = descs
            .iter()
            .filter(|desc| desc.flags & DESC_F_WRITE == 0)
            .fold(0u64, |total, desc| {
                total.saturating_add(u64::from(desc.len))
            });
        let writable_descriptor_bytes = descs
            .iter()
            .filter(|desc| desc.flags & DESC_F_WRITE != 0)
            .fold(0u64, |total, desc| {
                total.saturating_add(u64::from(desc.len))
            });
        let response_planned_write_len = writable_descriptor_bytes.min(response.len() as u64);
        let response_header = CtrlHdr::parse(response);
        self.record_trace_fields("command", |fields| {
            let _ = write!(
                fields,
                ",\"queue\":{},\"head\":{},\"control\":{},\"typ\":{},\"duration_ns\":{handle_ns},\"name\":",
                queue_index, head, control, hdr.typ
            );
            write_json_string(fields, command_name(hdr.typ));
            let _ = write!(
                fields,
                ",\"flags\":{},\"fenced\":{},\"fence_id\":{},\"ctx_id\":{},\"ring_idx\":{},\"request_len\":{},\"response_type\":{},\"response_name\":",
                hdr.flags,
                hdr.flags & VIRTIO_GPU_FLAG_FENCE != 0,
                hdr.fence_id,
                hdr.ctx_id,
                hdr.ring_idx(),
                request.len(),
                response_type
            );
            write_json_string(fields, response_name(response_type));
            let _ = write!(
                fields,
                ",\"response_len\":{},\"descriptor_count\":{},\"readable_descriptor_count\":{},\"readable_descriptor_bytes\":{},\"writable_descriptor_count\":{},\"writable_descriptor_bytes\":{},\"response_planned_write_len\":{},\"response_truncated\":{}",
                response.len(),
                descs.len(),
                readable_descriptor_count,
                readable_descriptor_bytes,
                writable_descriptor_count,
                writable_descriptor_bytes,
                response_planned_write_len,
                response.len() as u64 > writable_descriptor_bytes
            );
            fields.push_str(",\"readable_descriptor_lengths\":[");
            write_descriptor_lengths(fields, descs, false);
            fields.push_str("],\"writable_descriptor_lengths\":[");
            write_descriptor_lengths(fields, descs, true);
            fields.push(']');
            if let Some(response_header) = response_header {
                let _ = write!(
                    fields,
                    ",\"response_header_valid\":true,\"response_flags\":{},\"response_fenced\":{},\"response_fence_id\":{},\"response_ctx_id\":{},\"response_ring_idx\":{}",
                    response_header.flags,
                    response_header.flags & VIRTIO_GPU_FLAG_FENCE != 0,
                    response_header.fence_id,
                    response_header.ctx_id,
                    response_header.ring_idx()
                );
            } else {
                fields.push_str(",\"response_header_valid\":false");
            }
            write_trace_command_details(fields, request, hdr);
            write_trace_command_response_details(fields, response_type, response);
        });
    }

    pub(crate) fn trace_fence_create(
        &mut self,
        fence: CompletedFence,
        backend_accepted: bool,
        outcome: &str,
    ) {
        self.trace_fence_create_count = self.trace_fence_create_count.saturating_add(1);
        if !trace_sample(self.trace_fence_create_count) {
            return;
        }
        self.record_trace_fields("fence_create", |fields| {
            let _ = write!(
                fields,
                ",\"ctx_id\":{},\"ring_idx\":{},\"fence_id\":{},\"backend_accepted\":{}",
                fence.ctx_id, fence.ring_idx, fence.fence_id, backend_accepted
            );
            fields.push_str(",\"outcome\":");
            write_json_string(fields, outcome);
        });
    }

    pub(crate) fn trace_fence_complete(&mut self, fence: CompletedFence) {
        self.trace_fence_complete_count = self.trace_fence_complete_count.saturating_add(1);
        if !trace_sample(self.trace_fence_complete_count) {
            return;
        }
        self.record_trace_fields("fence_complete", |fields| {
            let _ = write!(
                fields,
                ",\"ctx_id\":{},\"ring_idx\":{},\"fence_id\":{}",
                fence.ctx_id, fence.ring_idx, fence.fence_id
            );
        });
    }

    pub(crate) fn trace_fence_delivery(&mut self, fence: CompletedFence, used_len: u32) {
        self.trace_fence_deliver_count = self.trace_fence_deliver_count.saturating_add(1);
        if !trace_sample(self.trace_fence_deliver_count) {
            return;
        }
        self.record_trace_fields("fence_deliver", |fields| {
            let _ = write!(
                fields,
                ",\"ctx_id\":{},\"ring_idx\":{},\"fence_id\":{},\"used_len\":{}",
                fence.ctx_id, fence.ring_idx, fence.fence_id, used_len
            );
        });
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
        queue: &VirtioGpuQueue,
        head: u16,
        out: &mut Vec<Descriptor>,
    ) -> bool {
        out.clear();
        let queue_size = queue.effective_size();
        if head >= queue_size {
            return false;
        }
        let mut index = head;
        for _ in 0..queue_size {
            let Some(desc) = Descriptor::read(mem, queue.desc + u64::from(index) * DESC_SIZE)
            else {
                return false;
            };
            let has_next = desc.flags & DESC_F_NEXT != 0;
            out.push(desc);
            if !has_next {
                return true;
            }
            index = desc.next;
            if index >= queue_size {
                return false;
            }
        }
        false
    }

    pub(crate) fn gather_readable_into(
        mem: &dyn GuestMemoryMut,
        descs: &[Descriptor],
        out: &mut Vec<u8>,
    ) {
        out.clear();
        for desc in descs {
            if desc.flags & DESC_F_WRITE != 0 {
                continue;
            }
            let start = out.len();
            let Some(end) = start.checked_add(desc.len as usize) else {
                out.clear();
                return;
            };
            if end > MAX_GPU_REQUEST_LEN {
                out.clear();
                return;
            }
            if let Some(bytes) = mem.read_bytes(desc.addr, desc.len as usize) {
                out.extend_from_slice(&bytes);
            }
        }
    }

    pub(crate) fn scatter_write(
        mem: &mut dyn GuestMemoryMut,
        descs: &[Descriptor],
        bytes: &[u8],
    ) -> u32 {
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

    pub(crate) fn write_used(
        mem: &mut dyn GuestMemoryMut,
        queue: &VirtioGpuQueue,
        id: u16,
        len: u32,
    ) {
        if queue.device == 0 {
            return;
        }
        let queue_size = queue.effective_size();
        let Some(used_idx) = read_u16(mem, queue.device + 2) else {
            return;
        };
        let elem = queue.device + 4 + u64::from(used_idx % queue_size) * 8;
        let _ = mem.write_bytes(elem, &u32::from(id).to_le_bytes());
        let _ = mem.write_bytes(elem + 4, &len.to_le_bytes());
        let _ = mem.write_bytes(queue.device + 2, &used_idx.wrapping_add(1).to_le_bytes());
    }
}
