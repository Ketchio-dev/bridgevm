//! Split out of magic_value.rs to keep files under 600 lines.

use super::*;
use crate::virtio_gpu_3d::CompletedFence;
use crate::virtio_gpu_3d::GpuShmMapPort;
use crate::virtio_gpu_3d::VirtioGpu3d;
use crate::virtio_gpu_3d::VirtioGpu3dBackend;
use crate::virtio_gpu_3d::VirtioGpu3dStats;
use crate::virtio_gpu_trace::VirtioGpuTraceRecorder;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

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
}
