//! Modern virtio-gpu PCI device model with a 2D scanout and optional 3D backend.
//!
//! It deliberately mirrors the proven modern virtio-pci transport in
//! `virtio_net.rs` instead of sharing transport code, so existing net/block
//! paths keep their validated behavior.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs::{File, OpenOptions};
use std::os::unix::io::AsRawFd;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use crate::{
    fwcfg::GuestMemoryMut,
    msix::{MsixMessage, MsixTable},
    pcie::{
        VIRTIO_GPU_MSIX_PBA_OFFSET, VIRTIO_GPU_MSIX_TABLE_OFFSET, VIRTIO_GPU_MSIX_VECTOR_COUNT,
    },
    ramfb::DRM_FORMAT_XRGB8888,
    virtio_gpu_3d::{
        self, BlobMemEntry, CompletedFence, Create3dArgs, CtrlHdr3d, GpuShmMapPort, VirtioGpu3d,
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

/// virtio-gpu config `events_read` bit: the host changed the scanout layout
/// (resolution), so the guest should re-query GET_DISPLAY_INFO/GET_EDID.
const VIRTIO_GPU_EVENT_DISPLAY: u32 = 1 << 0;
/// Largest scanout the resize path accepts, matching the EDID/mode range the
/// viogpu3d driver advertises. Guards the scanout allocation.
const MAX_SCANOUT_DIMENSION: u32 = 7680;

const QUEUE_CONTROL: usize = 0;
const QUEUE_CURSOR: usize = 1;
const QUEUE_COUNT: usize = 2;
const PARKED_RESPONSE_BUFFER_POOL_LIMIT: usize = 4;
const QUEUE_MAX: u16 = 64;
const DESC_SIZE: u64 = 16;
const DESC_F_NEXT: u16 = 1;
const DESC_F_WRITE: u16 = 2;
const MAX_GPU_REQUEST_LEN: usize = 64 * 1024 * 1024;

const VIRTIO_GPU_CMD_GET_DISPLAY_INFO: u32 = 0x0100;
const VIRTIO_GPU_CMD_RESOURCE_CREATE_2D: u32 = 0x0101;
const VIRTIO_GPU_CMD_RESOURCE_UNREF: u32 = 0x0102;
const VIRTIO_GPU_CMD_SET_SCANOUT: u32 = 0x0103;
const VIRTIO_GPU_CMD_RESOURCE_FLUSH: u32 = 0x0104;
const VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D: u32 = 0x0105;
const VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING: u32 = 0x0106;
const VIRTIO_GPU_CMD_RESOURCE_DETACH_BACKING: u32 = 0x0107;
const VIRTIO_GPU_CMD_GET_EDID: u32 = 0x010a;
const VIRTIO_GPU_CMD_SET_SCANOUT_BLOB: u32 = 0x010d;
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
const SET_SCANOUT_BLOB_LEN: usize = 24 + 16 + 4 + 4 + 4 + 4 + 4 + 4 + 16 + 16;

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
    base: Instant,
    parked: AtomicBool,
    deadline_ns: AtomicU64,
}

impl VblankWakeState {
    pub fn new() -> Self {
        Self {
            base: Instant::now(),
            parked: AtomicBool::new(false),
            deadline_ns: AtomicU64::new(0),
        }
    }

    fn publish(&self, parked: bool, deadline: Option<Instant>) {
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
    width: u32,
    height: u32,
    device_features_sel: u32,
    driver_features_sel: u32,
    driver_features: [u32; 2],
    config_msix_vector: u16,
    queue_sel: u32,
    queues: [VirtioGpuQueue; QUEUE_COUNT],
    pending_msix_queue_bits: u8,
    status: u32,
    interrupt_status: u32,
    events_read: u32,
    events_clear: u32,
    pending_config_change: bool,
    resources: BTreeMap<u32, GpuResource>,
    scanout_resource: Option<u32>,
    blob_scanout: Option<BlobScanout>,
    scanout: Vec<u8>,
    fb_sink: Option<FbSink>,
    three_d: VirtioGpu3d,
    pending_fenced: Vec<PendingFencedResponse>,
    pending_vblank: Vec<PendingVblankResponse>,
    completed_fences_scratch: Vec<CompletedFence>,
    descriptor_scratch: Vec<Descriptor>,
    parked_descriptor_scratch: Vec<Vec<Descriptor>>,
    request_scratch: Vec<u8>,
    response_scratch: Vec<u8>,
    parked_response_scratch: Vec<Vec<u8>>,
    blob_row_scratch: Vec<u8>,
    scanout_readback_scratch: Vec<u8>,
    trace_fields_scratch: String,
    trace: VirtioGpuTraceRecorder,
    trace_queue_notify_count: u64,
    trace_submit_success_count: u64,
    trace_fence_create_count: u64,
    trace_fence_complete_count: u64,
    trace_fence_deliver_count: u64,
    vblank_interval: Duration,
    last_vblank: Option<Instant>,
    vblank_paced_count: u64,
    vblank_wake: Option<Arc<VblankWakeState>>,
    scanout_readback_interval: Duration,
    last_3d_scanout_readback: Option<Instant>,
    scanout_3d_flush_count: u64,
    scanout_readback_attempt_count: u64,
    scanout_readback_count: u64,
    scanout_readback_throttled_count: u64,
    scanout_readback_bytes: u64,
    scanout_readback_nanoseconds: u64,
    scanout_3d_deferred: bool,
    pending_3d_scanout: Option<(u32, Rect)>,
    pending_3d_scanout_fresh: bool,
    deferred_scanout_flush_count: u64,
    deferred_scanout_serviced_count: u64,
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

    /// Queue size the device must actually run at. The virtio driver may enable
    /// a queue without ever writing COMMON_QUEUE_SIZE, in which case the queue
    /// operates at the advertised maximum (`QUEUE_MAX`) rather than the reset
    /// value of 0. Reads of COMMON_QUEUE_SIZE already report this effective
    /// value, so descriptor processing must agree with it.
    fn effective_size(&self) -> u16 {
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct BlobScanout {
    resource_id: u32,
    width: u32,
    height: u32,
    format: u32,
    stride: u32,
    offset: u32,
    mapping: Option<BlobScanoutMapping>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BlobScanoutMapping {
    ptr: *const u8,
    len: usize,
}

unsafe impl Send for BlobScanoutMapping {}

#[derive(Debug, Clone)]
struct PendingFencedResponse {
    queue_index: usize,
    queue: VirtioGpuQueue,
    head: u16,
    descs: Vec<Descriptor>,
    response: Vec<u8>,
    fence: CompletedFence,
}

#[derive(Debug, Clone)]
struct PendingVblankResponse {
    queue_index: usize,
    queue: VirtioGpuQueue,
    head: u16,
    descs: Vec<Descriptor>,
    response: Vec<u8>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScanoutReadbackOutcome {
    Done,
    NotDue,
    Gone,
}

fn union_rect(a: Rect, b: Rect) -> Rect {
    if a.width == 0 || a.height == 0 {
        return b;
    }
    if b.width == 0 || b.height == 0 {
        return a;
    }
    let x = a.x.min(b.x);
    let y = a.y.min(b.y);
    let right = a.x.saturating_add(a.width).max(b.x.saturating_add(b.width));
    let bottom = a
        .y
        .saturating_add(a.height)
        .max(b.y.saturating_add(b.height));
    Rect {
        x,
        y,
        width: right - x,
        height: bottom - y,
    }
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
            deferred_scanout_flush_count: 0,
            deferred_scanout_serviced_count: 0,
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

    fn publish_vblank_wake(&self) {
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

    fn defer_3d_scanout(&mut self, resource_id: u32, rect: Rect) {
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
        if self.scanout_resource != Some(resource_id) || !self.three_d.is_3d_resource(resource_id)
        {
            self.pending_3d_scanout = None;
            return;
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

    fn try_3d_scanout_readback(
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
        self.scanout_readback_attempt_count =
            self.scanout_readback_attempt_count.saturating_add(1);
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
        let transfer_ns = transfer_started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
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
        let composite_ns = composite_started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
        let elapsed = started.elapsed();
        let duration_ns = elapsed.as_nanos().min(u128::from(u64::MAX)) as u64;
        self.scanout_readback_nanoseconds = self
            .scanout_readback_nanoseconds
            .saturating_add(duration_ns);
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

    fn access_common(
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

    fn write_status(&mut self, value: u64) {
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

    fn config_write(&mut self, offset: u64, size: u8, value: u64) {
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
    fn request_display_resolution(&mut self, width: u32, height: u32) -> bool {
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

    fn notify_queue(&mut self, queue_index: u16, mem: &mut dyn GuestMemoryMut) {
        self.trace_queue_notify(queue_index);
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
        let mut descs = self.take_descriptor_scratch();
        if !Self::descriptor_chain_into(mem, queue, head, &mut descs) {
            self.descriptor_scratch = descs;
            return ChainCompletion::Immediate(0);
        }
        let mut request = std::mem::take(&mut self.request_scratch);
        Self::gather_readable_into(mem, &descs, &mut request);
        let mut response = self.take_response_scratch();
        response.clear();
        if control {
            self.handle_control_request_into(mem, &request, &mut response);
        } else {
            self.handle_cursor_request_into(&request, &mut response);
        }
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
        self.trace_command(queue_index, head, control, &descs, &request, hdr, &response);
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

    fn recycle_queue_scratch(
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

    fn take_descriptor_scratch(&mut self) -> Vec<Descriptor> {
        let scratch = std::mem::take(&mut self.descriptor_scratch);
        if scratch.capacity() == 0 {
            self.parked_descriptor_scratch.pop().unwrap_or(scratch)
        } else {
            scratch
        }
    }

    fn take_response_scratch(&mut self) -> Vec<u8> {
        let scratch = std::mem::take(&mut self.response_scratch);
        if scratch.capacity() == 0 {
            self.parked_response_scratch.pop().unwrap_or(scratch)
        } else {
            scratch
        }
    }

    fn recycle_parked_response_buffers(
        &mut self,
        mut descs: Vec<Descriptor>,
        mut response: Vec<u8>,
    ) {
        descs.clear();
        response.clear();
        self.recycle_descriptor_scratch(descs);
        self.recycle_response_scratch(response);
    }

    fn recycle_descriptor_scratch(&mut self, mut descs: Vec<Descriptor>) {
        if descs.capacity() > self.descriptor_scratch.capacity() {
            std::mem::swap(&mut self.descriptor_scratch, &mut descs);
        }
        self.recycle_extra_descriptor_scratch(descs);
    }

    fn recycle_response_scratch(&mut self, mut response: Vec<u8>) {
        if response.capacity() > self.response_scratch.capacity() {
            std::mem::swap(&mut self.response_scratch, &mut response);
        }
        self.recycle_extra_response_scratch(response);
    }

    fn recycle_extra_descriptor_scratch(&mut self, descs: Vec<Descriptor>) {
        if descs.capacity() != 0
            && self.parked_descriptor_scratch.len() < PARKED_RESPONSE_BUFFER_POOL_LIMIT
        {
            self.parked_descriptor_scratch.push(descs);
        }
    }

    fn recycle_extra_response_scratch(&mut self, response: Vec<u8>) {
        if response.capacity() != 0
            && self.parked_response_scratch.len() < PARKED_RESPONSE_BUFFER_POOL_LIMIT
        {
            self.parked_response_scratch.push(response);
        }
    }

    fn handle_cursor_request_into(&mut self, request: &[u8], out: &mut Vec<u8>) {
        let hdr = CtrlHdr::parse(request);
        match hdr.map(|h| h.typ) {
            Some(VIRTIO_GPU_CMD_UPDATE_CURSOR | VIRTIO_GPU_CMD_MOVE_CURSOR) => {
                response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, hdr);
            }
            _ => response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, hdr),
        }
    }

    fn handle_control_request_into(
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

    fn drain_host_vblank_at(&mut self, mem: &mut dyn GuestMemoryMut, now: Instant) {
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
        let mut completed = std::mem::take(&mut self.completed_fences_scratch);
        completed.clear();
        self.three_d.drain_completed_fences_into(&mut completed);
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

    fn response_display_info_into(&self, hdr: Option<CtrlHdr>, out: &mut Vec<u8>) {
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

    fn response_edid_into(&self, hdr: Option<CtrlHdr>, out: &mut Vec<u8>) {
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_EDID, hdr);
        out.extend_from_slice(&128u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        let edid = build_edid(self.width, self.height);
        out.extend_from_slice(&edid);
        out.resize(out.len() + (1024 - 128), 0);
    }

    fn resource_create_2d_into(&mut self, request: &[u8], hdr: Option<CtrlHdr>, out: &mut Vec<u8>) {
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

    fn resource_unref_into(&mut self, request: &[u8], hdr: Option<CtrlHdr>, out: &mut Vec<u8>) {
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

    fn attach_backing_into(
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

    fn detach_backing_into(&mut self, request: &[u8], hdr: Option<CtrlHdr>, out: &mut Vec<u8>) {
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

    fn set_scanout_into(&mut self, request: &[u8], hdr: Option<CtrlHdr>, out: &mut Vec<u8>) {
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

    fn set_scanout_blob_into(&mut self, request: &[u8], hdr: Option<CtrlHdr>, out: &mut Vec<u8>) {
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

    fn transfer_to_host_2d_into(
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

    fn publish_scanout_fb(&mut self) {
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
    fn publish_scanout_fb_unconditionally(&mut self) {
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

    fn resource_flush_into(
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

    fn composite_blob_scanout(&mut self, mem: &dyn GuestMemoryMut, rect: Rect) {
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

    fn unbind_blob_scanout(&mut self) {
        if let Some(scanout) = self.blob_scanout.take() {
            if scanout.mapping.is_some() {
                self.three_d.scanout_unmap_blob(scanout.resource_id);
            }
        }
    }

    fn trace_device_init(&mut self, backend_3d: bool) {
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

    fn trace_common_read(&mut self, offset: u64, size: u8, value: u64) {
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

    fn trace_queue_notify(&mut self, queue_index: u16) {
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

    fn record_trace_fields(&mut self, event: &str, write_fields: impl FnOnce(&mut String)) {
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

    fn trace_command(
        &mut self,
        queue_index: usize,
        head: u16,
        control: bool,
        descs: &[Descriptor],
        request: &[u8],
        hdr: CtrlHdr,
        response: &[u8],
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
                ",\"queue\":{},\"head\":{},\"control\":{},\"typ\":{},\"name\":",
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

    fn trace_fence_create(&mut self, fence: CompletedFence, backend_accepted: bool, outcome: &str) {
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

    fn trace_fence_complete(&mut self, fence: CompletedFence) {
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

    fn trace_fence_delivery(&mut self, fence: CompletedFence, used_len: u32) {
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

    fn gather_readable_into(mem: &dyn GuestMemoryMut, descs: &[Descriptor], out: &mut Vec<u8>) {
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

    pub fn set_vblank_interval(&mut self, interval: Duration) {
        self.gpu.set_vblank_interval(interval);
    }

    /// Host-driven scanout resize. Returns true when the geometry changed and a
    /// DISPLAY event + config-change interrupt were armed; the caller flushes
    /// the resulting MSI-X via `drain_pending_msix_into`.
    pub fn request_display_resolution(&mut self, width: u32, height: u32) -> bool {
        self.gpu.request_display_resolution(width, height)
    }

    /// Current reported scanout geometry.
    pub fn display_resolution(&self) -> (u32, u32) {
        (self.gpu.width, self.gpu.height)
    }

    pub fn set_vblank_wake(&mut self, wake: Arc<VblankWakeState>) {
        self.gpu.set_vblank_wake(wake);
    }

    pub fn vblank_wake(&self) -> Option<Arc<VblankWakeState>> {
        self.gpu.vblank_wake()
    }

    pub fn set_3d_scanout_readback_interval(&mut self, interval: Duration) {
        self.gpu.set_3d_scanout_readback_interval(interval);
    }

    pub fn set_3d_scanout_deferred(&mut self, deferred: bool) {
        self.gpu.set_3d_scanout_deferred(deferred);
    }

    pub fn service_deferred_3d_scanout(&mut self) {
        self.gpu.service_deferred_3d_scanout();
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

    pub fn drain_host_vblank(&mut self, mem: &mut dyn GuestMemoryMut) {
        self.gpu.drain_host_vblank(mem);
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
        self.raise_pending_msix_into(function_enabled, function_masked, &mut messages);
        messages
    }

    pub fn raise_pending_msix_into(
        &mut self,
        function_enabled: bool,
        function_masked: bool,
        out: &mut Vec<MsixMessage>,
    ) {
        if self.gpu.pending_config_change {
            let vector = self.gpu.config_msix_vector;
            if vector != VIRTIO_MSI_NO_VECTOR {
                if let Some(message) = self.msix.raise(vector, function_enabled, function_masked) {
                    self.gpu.pending_config_change = false;
                    venus_start_trace_msix(
                        "config raised",
                        vector,
                        function_enabled,
                        function_masked,
                    );
                    out.push(message);
                } else {
                    venus_start_trace_msix(
                        "config held",
                        vector,
                        function_enabled,
                        function_masked,
                    );
                }
            } else {
                // No config vector programmed (INTx path): the ISR config bit
                // is already set; nothing MSI-X to raise.
                self.gpu.pending_config_change = false;
            }
        }
        let mut pending = self.gpu.pending_msix_queue_bits;
        while pending != 0 {
            let queue_index = pending.trailing_zeros() as usize;
            let vector = self.gpu.queues[queue_index].msix_vector;
            if vector == VIRTIO_MSI_NO_VECTOR {
                venus_start_trace_msix_queue("no-vector (ISR path)", queue_index, vector);
                pending &= !(1u8 << queue_index);
                continue;
            }
            if let Some(message) = self.msix.raise(vector, function_enabled, function_masked) {
                self.gpu.queues[queue_index].pending_msix = false;
                self.gpu.pending_msix_queue_bits &= !(1u8 << queue_index);
                venus_start_trace_msix_queue("raised", queue_index, vector);
                out.push(message);
            } else {
                venus_start_trace_msix_queue("held (disabled/masked)", queue_index, vector);
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
        for (queue_index, queue) in self.gpu.queues.iter_mut().enumerate() {
            if queue.msix_vector == vector {
                queue.pending_msix = false;
                if let Some(bit) = queue_bit(queue_index) {
                    self.gpu.pending_msix_queue_bits &= !bit;
                }
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

    pub fn snapshot_state(&self) -> Vec<u8> {
        let gpu = &self.gpu;
        let mut out = crate::checkpoint::StateWriter::new();
        out.write_u32(1);
        out.write_u32(gpu.width);
        out.write_u32(gpu.height);
        out.write_u32(gpu.device_features_sel);
        out.write_u32(gpu.driver_features_sel);
        out.write_u32(gpu.driver_features[0]);
        out.write_u32(gpu.driver_features[1]);
        out.write_u16(gpu.config_msix_vector);
        out.write_u16(0);
        out.write_u32(gpu.queue_sel);
        out.write_u8(gpu.pending_msix_queue_bits);
        out.write_u8(0);
        out.write_u16(0);
        out.write_u32(gpu.status);
        out.write_u32(gpu.interrupt_status);
        out.write_u32(gpu.events_clear);

        for queue in &gpu.queues {
            out.write_u16(queue.size);
            out.write_bool(queue.ready);
            out.write_bool(queue.pending_msix);
            out.write_u64(queue.desc);
            out.write_u64(queue.driver);
            out.write_u64(queue.device);
            out.write_u16(queue.msix_vector);
            out.write_u16(queue.notify_off);
            out.write_u16(queue.last_avail_idx);
            out.write_u16(0);
        }

        out.write_u32(gpu.resources.len() as u32);
        for (&resource_id, resource) in &gpu.resources {
            out.write_u32(resource_id);
            out.write_u32(resource.format);
            out.write_u32(resource.width);
            out.write_u32(resource.height);
            out.write_blob(&resource.host_pixels);
            out.write_u32(resource.backing.len() as u32);
            for backing in &resource.backing {
                out.write_u64(backing.addr);
                out.write_u32(backing.len);
                out.write_u32(0);
            }
        }

        out.write_bool(gpu.scanout_resource.is_some());
        if let Some(resource_id) = gpu.scanout_resource {
            out.write_u32(resource_id);
        }
        out.write_blob(&gpu.scanout);
        out.write_blob(&self.msix.snapshot_state());
        out.into_inner()
    }

    pub fn restore_state(&mut self, data: &[u8]) {
        let mut input = crate::checkpoint::StateReader::new(data);
        assert_eq!(
            input.read_u32(),
            1,
            "unsupported virtio-gpu snapshot version"
        );

        let width = input.read_u32();
        let height = input.read_u32();
        assert_eq!(
            (width, height),
            (self.gpu.width, self.gpu.height),
            "virtio-gpu resolution mismatch on restore"
        );

        self.gpu.device_features_sel = input.read_u32();
        self.gpu.driver_features_sel = input.read_u32();
        self.gpu.driver_features = [input.read_u32(), input.read_u32()];
        self.gpu.config_msix_vector = input.read_u16();
        assert_eq!(input.read_u16(), 0, "invalid virtio-gpu snapshot");
        self.gpu.queue_sel = input.read_u32();
        self.gpu.pending_msix_queue_bits = input.read_u8();
        assert_eq!(input.read_u8(), 0, "invalid virtio-gpu snapshot");
        assert_eq!(input.read_u16(), 0, "invalid virtio-gpu snapshot");
        self.gpu.status = input.read_u32();
        self.gpu.interrupt_status = input.read_u32();
        self.gpu.events_clear = input.read_u32();

        for queue in &mut self.gpu.queues {
            queue.size = input.read_u16();
            queue.ready = input.read_bool();
            queue.pending_msix = input.read_bool();
            queue.desc = input.read_u64();
            queue.driver = input.read_u64();
            queue.device = input.read_u64();
            queue.msix_vector = input.read_u16();
            queue.notify_off = input.read_u16();
            queue.last_avail_idx = input.read_u16();
            assert_eq!(input.read_u16(), 0, "invalid virtio-gpu queue snapshot");
        }

        self.gpu.resources.clear();
        let resource_count = input.read_u32() as usize;
        for _ in 0..resource_count {
            let resource_id = input.read_u32();
            let format = input.read_u32();
            let width = input.read_u32();
            let height = input.read_u32();
            let host_pixels = input.read_blob();

            let backing_count = input.read_u32() as usize;
            let mut backing = Vec::with_capacity(backing_count);
            for _ in 0..backing_count {
                backing.push(BackingEntry {
                    addr: input.read_u64(),
                    len: input.read_u32(),
                });
                assert_eq!(input.read_u32(), 0, "invalid GPU backing snapshot");
            }

            self.gpu.resources.insert(
                resource_id,
                GpuResource {
                    format,
                    width,
                    height,
                    host_pixels,
                    backing,
                },
            );
        }

        self.gpu.scanout_resource = if input.read_bool() {
            Some(input.read_u32())
        } else {
            None
        };
        if let Some(resource_id) = self.gpu.scanout_resource {
            if !self.gpu.resources.contains_key(&resource_id) {
                // The active desktop scanout is normally backed by a 3D/blob resource
                // whose pixels live in the (non-serializable) virglrenderer host
                // context, so it is absent from the restored 2D resource map. Drop the
                // dangling reference rather than panicking; on resume the guest WDDM
                // driver detects the lost adapter, TDR-resets, and re-establishes the
                // scanout (the documented "3D contexts lost on restore" behavior).
                self.gpu.scanout_resource = None;
            }
        }
        self.gpu.scanout = input.read_blob();

        self.gpu.unbind_blob_scanout();
        self.gpu.three_d.reset();
        self.gpu.pending_fenced.clear();
        self.gpu.pending_vblank.clear();
        self.gpu.completed_fences_scratch.clear();
        self.gpu.descriptor_scratch.clear();
        self.gpu.parked_descriptor_scratch.clear();
        self.gpu.request_scratch.clear();
        self.gpu.response_scratch.clear();
        self.gpu.parked_response_scratch.clear();
        self.gpu.blob_row_scratch.clear();
        self.gpu.last_vblank = None;
        self.gpu.last_3d_scanout_readback = None;
        self.gpu.publish_vblank_wake();
        self.gpu.publish_scanout_fb_unconditionally();

        self.msix.restore_state(&input.read_blob());
        input.finish();
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Descriptor {
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
}

impl Descriptor {
    fn read(mem: &dyn GuestMemoryMut, gpa: u64) -> Option<Self> {
        let mut bytes = [0u8; DESC_SIZE as usize];
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

fn response_hdr_into(out: &mut Vec<u8>, typ: u32, request: Option<CtrlHdr>) {
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
    out.clear();
    out.reserve(24);
    hdr.append_to(out);
}

/// Commands whose backend call can leave rendering or transfer work in flight.
/// Every other command is synchronous and may complete its virtqueue fence as
/// soon as the call returns, even when it was routed through the 3D backend.
fn command_requires_backend_fence(typ: u32) -> bool {
    matches!(
        typ,
        VIRTIO_GPU_CMD_TRANSFER_TO_HOST_3D
            | VIRTIO_GPU_CMD_TRANSFER_FROM_HOST_3D
            | VIRTIO_GPU_CMD_SUBMIT_3D
    )
}

fn command_name(typ: u32) -> &'static str {
    match typ {
        VIRTIO_GPU_CMD_GET_DISPLAY_INFO => "GET_DISPLAY_INFO",
        VIRTIO_GPU_CMD_RESOURCE_CREATE_2D => "RESOURCE_CREATE_2D",
        VIRTIO_GPU_CMD_RESOURCE_UNREF => "RESOURCE_UNREF",
        VIRTIO_GPU_CMD_SET_SCANOUT => "SET_SCANOUT",
        VIRTIO_GPU_CMD_RESOURCE_FLUSH => "RESOURCE_FLUSH",
        VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D => "TRANSFER_TO_HOST_2D",
        VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING => "RESOURCE_ATTACH_BACKING",
        VIRTIO_GPU_CMD_RESOURCE_DETACH_BACKING => "RESOURCE_DETACH_BACKING",
        VIRTIO_GPU_CMD_GET_CAPSET_INFO => "GET_CAPSET_INFO",
        VIRTIO_GPU_CMD_GET_CAPSET => "GET_CAPSET",
        VIRTIO_GPU_CMD_GET_EDID => "GET_EDID",
        VIRTIO_GPU_CMD_RESOURCE_CREATE_BLOB => "RESOURCE_CREATE_BLOB",
        VIRTIO_GPU_CMD_SET_SCANOUT_BLOB => "SET_SCANOUT_BLOB",
        VIRTIO_GPU_CMD_CTX_CREATE => "CTX_CREATE",
        VIRTIO_GPU_CMD_CTX_DESTROY => "CTX_DESTROY",
        VIRTIO_GPU_CMD_CTX_ATTACH_RESOURCE => "CTX_ATTACH_RESOURCE",
        VIRTIO_GPU_CMD_CTX_DETACH_RESOURCE => "CTX_DETACH_RESOURCE",
        VIRTIO_GPU_CMD_RESOURCE_CREATE_3D => "RESOURCE_CREATE_3D",
        VIRTIO_GPU_CMD_TRANSFER_TO_HOST_3D => "TRANSFER_TO_HOST_3D",
        VIRTIO_GPU_CMD_TRANSFER_FROM_HOST_3D => "TRANSFER_FROM_HOST_3D",
        VIRTIO_GPU_CMD_SUBMIT_3D => "SUBMIT_3D",
        VIRTIO_GPU_CMD_RESOURCE_MAP_BLOB => "RESOURCE_MAP_BLOB",
        VIRTIO_GPU_CMD_RESOURCE_UNMAP_BLOB => "RESOURCE_UNMAP_BLOB",
        VIRTIO_GPU_CMD_UPDATE_CURSOR => "UPDATE_CURSOR",
        VIRTIO_GPU_CMD_MOVE_CURSOR => "MOVE_CURSOR",
        _ => "UNKNOWN",
    }
}

fn trace_sample(count: u64) -> bool {
    count <= 64 || count % 1024 == 0
}

fn venus_start_trace_msix(what: &str, vector: u16, enabled: bool, masked: bool) {
    if !venus_start_trace_enabled() {
        return;
    }
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let n = COUNT.fetch_add(1, Ordering::Relaxed) + 1;
    if trace_sample(n) {
        println!(
            "venus-start: msix {what} vector={vector} fn_enabled={enabled} fn_masked={masked} n={n}"
        );
    }
}

fn venus_start_trace_msix_queue(what: &str, queue_index: usize, vector: u16) {
    if !venus_start_trace_enabled() {
        return;
    }
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let n = COUNT.fetch_add(1, Ordering::Relaxed) + 1;
    if trace_sample(n) {
        println!("venus-start: msix queue={queue_index} {what} vector={vector} n={n}");
    }
}

/// Stdout mirror of the command trace for the venus KMD start path
/// (`BRIDGEVM_TRACE_VENUS_START=1`). Capset/blob/context lifecycle commands
/// and every error response print unconditionally — those are exactly the
/// accesses DxgkDdiStartDevice makes before the crash — while the high-rate
/// steady-state commands (SUBMIT_3D NOPs, transfers, flushes) are sampled.
fn venus_start_trace_command(request: &[u8], hdr: CtrlHdr, response: &[u8]) {
    if !venus_start_trace_enabled() {
        return;
    }
    let response_type = read_le_u32(response, 0).unwrap_or(0);
    let is_error = response_type >= VIRTIO_GPU_RESP_ERR_UNSPEC;
    let always = is_error
        || matches!(
            hdr.typ,
            VIRTIO_GPU_CMD_GET_CAPSET_INFO
                | VIRTIO_GPU_CMD_GET_CAPSET
                | VIRTIO_GPU_CMD_RESOURCE_CREATE_BLOB
                | VIRTIO_GPU_CMD_RESOURCE_MAP_BLOB
                | VIRTIO_GPU_CMD_RESOURCE_UNMAP_BLOB
                | VIRTIO_GPU_CMD_CTX_CREATE
                | VIRTIO_GPU_CMD_CTX_DESTROY
        );
    if !always {
        static COUNT: AtomicU64 = AtomicU64::new(0);
        let n = COUNT.fetch_add(1, Ordering::Relaxed) + 1;
        if !trace_sample(n) {
            return;
        }
    }
    let mut line = format!(
        "venus-start: cmd {} typ={:#x} ctx={} flags={:#x} -> {} typ={:#x}",
        command_name(hdr.typ),
        hdr.typ,
        hdr.ctx_id,
        hdr.flags,
        response_name(response_type),
        response_type
    );
    match hdr.typ {
        VIRTIO_GPU_CMD_GET_CAPSET_INFO => {
            let _ = write!(
                line,
                " capset_index={}",
                read_le_u32(request, 24).unwrap_or(u32::MAX)
            );
            if response_type == virtio_gpu_3d::VIRTIO_GPU_RESP_OK_CAPSET_INFO {
                let _ = write!(
                    line,
                    " capset_id={} max_version={} max_size={}",
                    read_le_u32(response, 24).unwrap_or(0),
                    read_le_u32(response, 28).unwrap_or(0),
                    read_le_u32(response, 32).unwrap_or(0)
                );
            }
        }
        VIRTIO_GPU_CMD_GET_CAPSET => {
            let _ = write!(
                line,
                " capset_id={} version={} response_bytes={}",
                read_le_u32(request, 24).unwrap_or(0),
                read_le_u32(request, 28).unwrap_or(0),
                response.len().saturating_sub(24)
            );
        }
        VIRTIO_GPU_CMD_RESOURCE_CREATE_BLOB => {
            let _ = write!(
                line,
                " resource_id={} blob_mem={} blob_flags={:#x} blob_id={} size={} nr_entries={}",
                read_le_u32(request, 24).unwrap_or(0),
                read_le_u32(request, 28).unwrap_or(0),
                read_le_u32(request, 32).unwrap_or(0),
                read_le_u64(request, 40).unwrap_or(0),
                read_le_u64(request, 48).unwrap_or(0),
                read_le_u32(request, 36).unwrap_or(0)
            );
        }
        VIRTIO_GPU_CMD_RESOURCE_MAP_BLOB => {
            let _ = write!(
                line,
                " resource_id={} shm_offset={:#x}",
                read_le_u32(request, 24).unwrap_or(0),
                read_le_u64(request, 32).unwrap_or(0)
            );
            if response_type == virtio_gpu_3d::VIRTIO_GPU_RESP_OK_MAP_INFO {
                let _ = write!(
                    line,
                    " map_info={:#x}",
                    read_le_u32(response, 24).unwrap_or(0)
                );
            }
        }
        VIRTIO_GPU_CMD_RESOURCE_UNMAP_BLOB => {
            let _ = write!(
                line,
                " resource_id={}",
                read_le_u32(request, 24).unwrap_or(0)
            );
        }
        _ => {}
    }
    println!("{line}");
}

fn response_name(typ: u32) -> &'static str {
    match typ {
        VIRTIO_GPU_RESP_OK_NODATA => "OK_NODATA",
        VIRTIO_GPU_RESP_OK_DISPLAY_INFO => "OK_DISPLAY_INFO",
        virtio_gpu_3d::VIRTIO_GPU_RESP_OK_CAPSET_INFO => "OK_CAPSET_INFO",
        virtio_gpu_3d::VIRTIO_GPU_RESP_OK_CAPSET => "OK_CAPSET",
        VIRTIO_GPU_RESP_OK_EDID => "OK_EDID",
        virtio_gpu_3d::VIRTIO_GPU_RESP_OK_MAP_INFO => "OK_MAP_INFO",
        VIRTIO_GPU_RESP_ERR_UNSPEC => "ERR_UNSPEC",
        virtio_gpu_3d::VIRTIO_GPU_RESP_ERR_OUT_OF_MEMORY => "ERR_OUT_OF_MEMORY",
        virtio_gpu_3d::VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER => "ERR_INVALID_PARAMETER",
        _ => "UNKNOWN",
    }
}

fn write_trace_command_details(out: &mut String, request: &[u8], hdr: CtrlHdr) {
    match hdr.typ {
        VIRTIO_GPU_CMD_RESOURCE_CREATE_2D => {
            let _ = write!(
                out,
                ",\"resource_id\":{},\"format\":{},\"width\":{},\"height\":{}",
                read_le_u32(request, 24).unwrap_or(0),
                read_le_u32(request, 28).unwrap_or(0),
                read_le_u32(request, 32).unwrap_or(0),
                read_le_u32(request, 36).unwrap_or(0)
            );
        }
        VIRTIO_GPU_CMD_RESOURCE_UNREF
        | VIRTIO_GPU_CMD_RESOURCE_DETACH_BACKING
        | VIRTIO_GPU_CMD_RESOURCE_UNMAP_BLOB => {
            let _ = write!(
                out,
                ",\"resource_id\":{}",
                read_le_u32(request, 24).unwrap_or(0)
            );
        }
        VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING => {
            let _ = write!(
                out,
                ",\"resource_id\":{},\"nr_entries\":{}",
                read_le_u32(request, 24).unwrap_or(0),
                read_le_u32(request, 28).unwrap_or(0)
            );
        }
        VIRTIO_GPU_CMD_SET_SCANOUT => {
            let rect = read_rect(request, 24).unwrap_or(Rect {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            });
            let _ = write!(
                out,
                ",\"scanout_id\":{},\"resource_id\":{},\"rect_x\":{},\"rect_y\":{},\"rect_w\":{},\"rect_h\":{}",
                read_le_u32(request, 40).unwrap_or(u32::MAX),
                read_le_u32(request, 44).unwrap_or(0),
                rect.x,
                rect.y,
                rect.width,
                rect.height
            );
        }
        VIRTIO_GPU_CMD_RESOURCE_FLUSH => {
            let rect = read_rect(request, 24).unwrap_or(Rect {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            });
            let _ = write!(
                out,
                ",\"resource_id\":{},\"rect_x\":{},\"rect_y\":{},\"rect_w\":{},\"rect_h\":{}",
                read_le_u32(request, 40).unwrap_or(0),
                rect.x,
                rect.y,
                rect.width,
                rect.height
            );
        }
        VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D => {
            let rect = read_rect(request, 24).unwrap_or(Rect {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            });
            let _ = write!(
                out,
                ",\"resource_id\":{},\"offset\":{},\"rect_x\":{},\"rect_y\":{},\"rect_w\":{},\"rect_h\":{}",
                read_le_u32(request, 48).unwrap_or(0),
                read_le_u64(request, 40).unwrap_or(0),
                rect.x,
                rect.y,
                rect.width,
                rect.height
            );
        }
        VIRTIO_GPU_CMD_GET_CAPSET_INFO => {
            let _ = write!(
                out,
                ",\"capset_index\":{}",
                read_le_u32(request, 24).unwrap_or(u32::MAX)
            );
        }
        VIRTIO_GPU_CMD_GET_CAPSET => {
            let _ = write!(
                out,
                ",\"capset_id\":{},\"capset_version\":{}",
                read_le_u32(request, 24).unwrap_or(0),
                read_le_u32(request, 28).unwrap_or(0)
            );
        }
        VIRTIO_GPU_CMD_RESOURCE_CREATE_BLOB => {
            let _ = write!(
                out,
                ",\"resource_id\":{},\"blob_mem\":{},\"blob_flags\":{},\"nr_entries\":{},\"blob_id\":{},\"blob_size\":{}",
                read_le_u32(request, 24).unwrap_or(0),
                read_le_u32(request, 28).unwrap_or(0),
                read_le_u32(request, 32).unwrap_or(0),
                read_le_u32(request, 36).unwrap_or(0),
                read_le_u64(request, 40).unwrap_or(0),
                read_le_u64(request, 48).unwrap_or(0)
            );
        }
        VIRTIO_GPU_CMD_SET_SCANOUT_BLOB => {
            let rect = read_rect(request, 24).unwrap_or(Rect {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            });
            let _ = write!(
                out,
                ",\"scanout_id\":{},\"resource_id\":{},\"width\":{},\"height\":{},\"format\":{},\"stride0\":{},\"offset0\":{},\"rect_x\":{},\"rect_y\":{},\"rect_w\":{},\"rect_h\":{}",
                read_le_u32(request, 40).unwrap_or(u32::MAX),
                read_le_u32(request, 44).unwrap_or(0),
                read_le_u32(request, 48).unwrap_or(0),
                read_le_u32(request, 52).unwrap_or(0),
                read_le_u32(request, 56).unwrap_or(0),
                read_le_u32(request, 64).unwrap_or(0),
                read_le_u32(request, 80).unwrap_or(0),
                rect.x,
                rect.y,
                rect.width,
                rect.height
            );
        }
        VIRTIO_GPU_CMD_CTX_CREATE => {
            let nlen = read_le_u32(request, 24).unwrap_or(0).min(64) as usize;
            let name_end = 32usize.saturating_add(nlen).min(request.len());
            let _ = write!(
                out,
                ",\"context_init\":{},\"name_len\":{},\"debug_name\":",
                read_le_u32(request, 28).unwrap_or(0),
                nlen
            );
            let name = request
                .get(32..name_end)
                .map(String::from_utf8_lossy)
                .unwrap_or_default();
            write_json_string(out, name.as_ref());
        }
        VIRTIO_GPU_CMD_CTX_ATTACH_RESOURCE | VIRTIO_GPU_CMD_CTX_DETACH_RESOURCE => {
            let _ = write!(
                out,
                ",\"resource_id\":{}",
                read_le_u32(request, 24).unwrap_or(0)
            );
        }
        VIRTIO_GPU_CMD_RESOURCE_CREATE_3D => {
            let _ = write!(
                out,
                ",\"resource_id\":{},\"target\":{},\"format\":{},\"bind\":{},\"width\":{},\"height\":{},\"depth\":{},\"array_size\":{},\"last_level\":{},\"nr_samples\":{},\"resource_flags\":{}",
                read_le_u32(request, 24).unwrap_or(0),
                read_le_u32(request, 28).unwrap_or(0),
                read_le_u32(request, 32).unwrap_or(0),
                read_le_u32(request, 36).unwrap_or(0),
                read_le_u32(request, 40).unwrap_or(0),
                read_le_u32(request, 44).unwrap_or(0),
                read_le_u32(request, 48).unwrap_or(0),
                read_le_u32(request, 52).unwrap_or(0),
                read_le_u32(request, 56).unwrap_or(0),
                read_le_u32(request, 60).unwrap_or(0),
                read_le_u32(request, 64).unwrap_or(0)
            );
        }
        VIRTIO_GPU_CMD_TRANSFER_TO_HOST_3D | VIRTIO_GPU_CMD_TRANSFER_FROM_HOST_3D => {
            let _ = write!(
                out,
                ",\"resource_id\":{},\"level\":{},\"stride\":{},\"layer_stride\":{},\"transfer_offset\":{},\"box_x\":{},\"box_y\":{},\"box_z\":{},\"box_w\":{},\"box_h\":{},\"box_d\":{}",
                read_le_u32(request, 56).unwrap_or(0),
                read_le_u32(request, 60).unwrap_or(0),
                read_le_u32(request, 64).unwrap_or(0),
                read_le_u32(request, 68).unwrap_or(0),
                read_le_u64(request, 48).unwrap_or(0),
                read_le_u32(request, 24).unwrap_or(0),
                read_le_u32(request, 28).unwrap_or(0),
                read_le_u32(request, 32).unwrap_or(0),
                read_le_u32(request, 36).unwrap_or(0),
                read_le_u32(request, 40).unwrap_or(0),
                read_le_u32(request, 44).unwrap_or(0)
            );
        }
        VIRTIO_GPU_CMD_SUBMIT_3D => {
            let size = read_le_u32(request, 24).unwrap_or(0) as usize;
            let payload_start = 32usize.min(request.len());
            let payload_end = payload_start.saturating_add(size).min(request.len());
            let payload = request.get(payload_start..payload_end).unwrap_or(&[]);
            let _ = write!(
                out,
                ",\"submit_size\":{},\"submit_dwords\":{},\"submit_prefix_hex\":",
                size,
                size.div_ceil(4)
            );
            write_hex_prefix_json(out, payload, submit_trace_prefix_len());
        }
        VIRTIO_GPU_CMD_RESOURCE_MAP_BLOB => {
            let _ = write!(
                out,
                ",\"resource_id\":{},\"shm_offset\":{}",
                read_le_u32(request, 24).unwrap_or(0),
                read_le_u64(request, 32).unwrap_or(0)
            );
        }
        _ => {}
    }
}

fn write_trace_command_response_details(out: &mut String, response_type: u32, response: &[u8]) {
    match response_type {
        VIRTIO_GPU_RESP_OK_DISPLAY_INFO => {
            let _ = write!(
                out,
                ",\"response_scanout0_x\":{},\"response_scanout0_y\":{},\"response_scanout0_width\":{},\"response_scanout0_height\":{},\"response_scanout0_enabled\":{},\"response_scanout0_flags\":{}",
                read_le_u32(response, 24).unwrap_or(0),
                read_le_u32(response, 28).unwrap_or(0),
                read_le_u32(response, 32).unwrap_or(0),
                read_le_u32(response, 36).unwrap_or(0),
                read_le_u32(response, 40).unwrap_or(0),
                read_le_u32(response, 44).unwrap_or(0)
            );
        }
        VIRTIO_GPU_RESP_OK_EDID => {
            let edid_size = read_le_u32(response, 24).unwrap_or(0) as usize;
            let available = response.len().saturating_sub(32);
            let checksum_valid = edid_size > 0
                && edid_size <= available
                && response[32..32 + edid_size]
                    .iter()
                    .fold(0u8, |sum, byte| sum.wrapping_add(*byte))
                    == 0;
            let _ = write!(
                out,
                ",\"response_edid_size\":{},\"response_edid_checksum_valid\":{}",
                edid_size, checksum_valid
            );
        }
        virtio_gpu_3d::VIRTIO_GPU_RESP_OK_CAPSET_INFO => {
            let _ = write!(
                out,
                ",\"response_capset_id\":{},\"response_capset_max_version\":{},\"response_capset_max_size\":{}",
                read_le_u32(response, 24).unwrap_or(0),
                read_le_u32(response, 28).unwrap_or(0),
                read_le_u32(response, 32).unwrap_or(0)
            );
        }
        virtio_gpu_3d::VIRTIO_GPU_RESP_OK_CAPSET => {
            let _ = write!(
                out,
                ",\"response_capset_bytes\":{}",
                response.len().saturating_sub(24)
            );
        }
        _ => {}
    }
}

fn write_descriptor_lengths(out: &mut String, descs: &[Descriptor], writable: bool) {
    let mut first = true;
    for desc in descs {
        if (desc.flags & DESC_F_WRITE != 0) != writable {
            continue;
        }
        if !first {
            out.push(',');
        }
        first = false;
        let _ = write!(out, "{}", desc.len);
    }
}

/// Bytes of SUBMIT_3D payload preserved in the JSONL trace. The 32-byte
/// default identifies the leading command; raising it via
/// BRIDGEVM_VIRTIO_GPU_TRACE_SUBMIT_PREFIX captures whole command streams for
/// offline decoding when diagnosing renderer-level divergence.
fn submit_trace_prefix_len() -> usize {
    static LEN: OnceLock<usize> = OnceLock::new();
    *LEN.get_or_init(|| {
        std::env::var("BRIDGEVM_VIRTIO_GPU_TRACE_SUBMIT_PREFIX")
            .ok()
            .and_then(|value| value.trim().parse::<usize>().ok())
            .filter(|&value| value > 0)
            .unwrap_or(32)
    })
}

fn write_hex_prefix_json(out: &mut String, bytes: &[u8], max_len: usize) {
    out.push('"');
    write_hex_prefix(out, bytes, max_len);
    out.push('"');
}

fn write_hex_prefix(out: &mut String, bytes: &[u8], max_len: usize) {
    for (index, byte) in bytes.iter().take(max_len).enumerate() {
        if index > 0 {
            out.push(' ');
        }
        let _ = write!(out, "{byte:02x}");
    }
    if bytes.len() > max_len {
        out.push_str(" ...");
    }
}

#[cfg(test)]
fn hex_prefix(bytes: &[u8], max_len: usize) -> String {
    let prefix_len = bytes.len().min(max_len);
    let mut out = String::with_capacity(prefix_len.saturating_mul(3).saturating_add(4));
    write_hex_prefix(&mut out, bytes, max_len);
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
    if x_end <= rect.x || y_end <= rect.y {
        return;
    }
    let stride = u64::from(resource.width) * 4;
    let row_bytes = ((x_end - rect.x) as usize) * 4;
    // Per the virtio-gpu spec (and QEMU), `offset` locates the box's top-left
    // (rect.x, rect.y) in the backing; source rows advance by `stride` from
    // there. So the backing offset for absolute pixel (x, y) is
    // offset + (y - rect.y) * stride + (x - rect.x) * 4 — NOT offset + y*stride
    // + x*4, which double-counts rect.{x,y} and sends every non-origin partial
    // update (taskbar, clock, cursor) out of bounds so it silently vanishes.
    for y in rect.y..y_end {
        let guest_row_off = offset + u64::from(y - rect.y) * stride;
        let dst_row = ((y as usize) * (resource.width as usize) + (rect.x as usize)) * 4;
        if read_from_backing_into(
            mem,
            &resource.backing,
            guest_row_off,
            &mut resource.host_pixels[dst_row..dst_row + row_bytes],
        ) {
            continue;
        }
        for x in rect.x..x_end {
            let guest_off = guest_row_off + u64::from(x - rect.x) * 4;
            let mut pixel = [0u8; 4];
            if !read_from_backing_into(mem, &resource.backing, guest_off, &mut pixel) {
                continue;
            }
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

fn composite_host_3d_to_scanout(
    pixels: &[u8],
    resource_width: u32,
    resource_height: u32,
    scanout: &mut [u8],
    scanout_width: u32,
    scanout_height: u32,
    rect: Rect,
) -> bool {
    if pixels.len() < scanout_len(resource_width, resource_height)
        || scanout.len() < scanout_len(scanout_width, scanout_height)
    {
        return false;
    }
    let x_end = rect
        .x
        .saturating_add(rect.width)
        .min(resource_width)
        .min(scanout_width);
    let y_end = rect
        .y
        .saturating_add(rect.height)
        .min(resource_height)
        .min(scanout_height);
    if x_end <= rect.x || y_end <= rect.y {
        return false;
    }

    let row_bytes = ((x_end - rect.x) as usize) * 4;
    for y in rect.y..y_end {
        let src = ((y as usize) * (resource_width as usize) + (rect.x as usize)) * 4;
        let dst = ((y as usize) * (scanout_width as usize) + (rect.x as usize)) * 4;
        scanout[dst..dst + row_bytes].copy_from_slice(&pixels[src..src + row_bytes]);
    }
    true
}

fn composite_local_3d_to_scanout(
    mem: &dyn GuestMemoryMut,
    backing: &[BlobMemEntry],
    info: Create3dArgs,
    scanout: &mut [u8],
    scanout_width: u32,
    scanout_height: u32,
    rect: Rect,
    row_pixels: &mut Vec<u8>,
) -> bool {
    if backing.is_empty() || !format_supported(info.format) {
        return false;
    }
    let x_end = rect
        .x
        .saturating_add(rect.width)
        .min(info.width)
        .min(scanout_width);
    let y_end = rect
        .y
        .saturating_add(rect.height)
        .min(info.height)
        .min(scanout_height);
    if x_end <= rect.x || y_end <= rect.y {
        return false;
    }

    let resource_stride = u64::from(info.width) * 4;
    let row_bytes = ((x_end - rect.x) as usize) * 4;
    row_pixels.resize(row_bytes, 0);
    let mut copied_any = false;
    for y in rect.y..y_end {
        let row_offset = u64::from(y) * resource_stride + u64::from(rect.x) * 4;
        if read_from_blob_backing_into(mem, backing, row_offset, row_pixels) {
            for x in rect.x..x_end {
                let src = ((x - rect.x) as usize) * 4;
                let dst = ((y as usize) * (scanout_width as usize) + (x as usize)) * 4;
                scanout[dst..dst + 4]
                    .copy_from_slice(&to_xrgb8888(&row_pixels[src..src + 4], info.format));
            }
            copied_any = true;
            continue;
        }
        for x in rect.x..x_end {
            let offset = u64::from(y) * resource_stride + u64::from(x) * 4;
            let mut pixel = [0u8; 4];
            if !read_from_blob_backing_into(mem, backing, offset, &mut pixel) {
                continue;
            }
            let dst = ((y as usize) * (scanout_width as usize) + (x as usize)) * 4;
            scanout[dst..dst + 4].copy_from_slice(&to_xrgb8888(&pixel, info.format));
            copied_any = true;
        }
    }
    row_pixels.clear();
    copied_any
}

struct GuestBlobComposite<'a> {
    mem: &'a dyn GuestMemoryMut,
    backing: &'a [virtio_gpu_3d::BlobMemEntry],
    scanout: &'a mut [u8],
    scanout_width: u32,
    blob: &'a BlobScanout,
    row_pixels: &'a mut Vec<u8>,
}

fn composite_guest_blob_to_scanout(
    composite: GuestBlobComposite<'_>,
    rect: Rect,
    x_end: u32,
    y_end: u32,
) {
    composite.row_pixels.clear();
    if x_end <= rect.x || y_end <= rect.y {
        return;
    }
    let row_bytes = ((x_end - rect.x) as usize) * 4;
    composite.row_pixels.resize(row_bytes, 0);
    for y in rect.y..y_end {
        let row_src = u64::from(composite.blob.offset)
            + u64::from(y) * u64::from(composite.blob.stride)
            + u64::from(rect.x) * 4;
        if read_from_blob_backing_into(
            composite.mem,
            composite.backing,
            row_src,
            composite.row_pixels,
        ) {
            for x in rect.x..x_end {
                let src = ((x - rect.x) as usize) * 4;
                let dst = ((y as usize) * (composite.scanout_width as usize) + (x as usize)) * 4;
                composite.scanout[dst..dst + 4].copy_from_slice(&to_xrgb8888(
                    &composite.row_pixels[src..src + 4],
                    composite.blob.format,
                ));
            }
            continue;
        }
        for x in rect.x..x_end {
            let src = u64::from(composite.blob.offset)
                + u64::from(y) * u64::from(composite.blob.stride)
                + u64::from(x) * 4;
            let mut pixel = [0u8; 4];
            if !read_from_blob_backing_into(composite.mem, composite.backing, src, &mut pixel) {
                continue;
            }
            let dst = ((y as usize) * (composite.scanout_width as usize) + (x as usize)) * 4;
            composite.scanout[dst..dst + 4]
                .copy_from_slice(&to_xrgb8888(&pixel, composite.blob.format));
        }
    }
    composite.row_pixels.clear();
}

fn composite_host_blob_to_scanout(
    pixels: &[u8],
    scanout: &mut [u8],
    scanout_width: u32,
    blob: &BlobScanout,
    rect: Rect,
    x_end: u32,
    y_end: u32,
) {
    for y in rect.y..y_end {
        for x in rect.x..x_end {
            let src = (blob.offset as usize)
                .saturating_add((y as usize).saturating_mul(blob.stride as usize))
                .saturating_add((x as usize).saturating_mul(4));
            if !matches!(src.checked_add(4), Some(end) if end <= pixels.len()) {
                continue;
            }
            let dst = ((y as usize) * (scanout_width as usize) + (x as usize)) * 4;
            scanout[dst..dst + 4].copy_from_slice(&to_xrgb8888(&pixels[src..src + 4], blob.format));
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

fn read_from_blob_backing_into(
    mem: &dyn GuestMemoryMut,
    backing: &[virtio_gpu_3d::BlobMemEntry],
    offset: u64,
    dst: &mut [u8],
) -> bool {
    let mut base = 0u64;
    let Ok(len_u64) = u64::try_from(dst.len()) else {
        return false;
    };
    for entry in backing {
        let Some(entry_end) = base.checked_add(u64::from(entry.len)) else {
            return false;
        };
        if offset >= base
            && offset
                .checked_add(len_u64)
                .is_some_and(|range_end| range_end <= entry_end)
        {
            let rel = offset - base;
            return mem.read_into(entry.addr + rel, dst);
        }
        base = entry_end;
    }
    false
}

fn read_from_backing_into(
    mem: &dyn GuestMemoryMut,
    backing: &[BackingEntry],
    offset: u64,
    dst: &mut [u8],
) -> bool {
    let mut base = 0u64;
    let Ok(len_u64) = u64::try_from(dst.len()) else {
        return false;
    };
    for entry in backing {
        let Some(entry_end) = base.checked_add(u64::from(entry.len)) else {
            return false;
        };
        if offset >= base
            && offset
                .checked_add(len_u64)
                .is_some_and(|range_end| range_end <= entry_end)
        {
            let rel = offset - base;
            return mem.read_into(entry.addr + rel, dst);
        }
        base = entry_end;
    }
    false
}

fn blob_surface_footprint(width: u32, height: u32, stride: u32, offset: u32) -> Option<u64> {
    u64::from(offset)
        .checked_add(u64::from(height.saturating_sub(1)).checked_mul(u64::from(stride))?)?
        .checked_add(u64::from(width).checked_mul(4)?)
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

    let dtd = detailed_timing_descriptor(width, height, 120);
    let pixel_clock_10khz = u16::from_le_bytes([dtd[0], dtd[1]]);
    let max_pixel_clock_10mhz = pixel_clock_10khz.div_ceil(1_000) as u8;
    edid[54..72].copy_from_slice(&dtd);
    edid[72..90].copy_from_slice(&monitor_descriptor(
        0xfd,
        &[
            48,
            144,
            30,
            160,
            max_pixel_clock_10mhz,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
        ],
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

fn detailed_timing_descriptor(width: u32, height: u32, refresh_hz: u32) -> [u8; 18] {
    let h_blank = 160u32.max(width / 8);
    let v_blank = 45u32.max(height / 20);
    let h_sync_offset = 48u32.min(h_blank / 3);
    let h_sync_width = 32u32.min(h_blank.saturating_sub(h_sync_offset).max(1));
    let v_sync_offset = 3u32;
    let v_sync_width = 5u32;
    let requested_pixel_clock_10khz = ((u64::from(width) + u64::from(h_blank))
        * (u64::from(height) + u64::from(v_blank))
        * u64::from(refresh_hz)
        / 10_000)
        .max(1);
    let pixel_clock_10khz = requested_pixel_clock_10khz.min(u64::from(u16::MAX));
    if requested_pixel_clock_10khz > u64::from(u16::MAX) {
        eprintln!(
            "virtio-gpu EDID: {width}x{height}@{refresh_hz} requires pixel clock \
             {requested_pixel_clock_10khz}0 kHz; clamping to {}0 kHz",
            u16::MAX
        );
    }

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

fn valid_msix_vector(vector: u16) -> u16 {
    if vector < VIRTIO_GPU_MSIX_VECTOR_COUNT || vector == VIRTIO_MSI_NO_VECTOR {
        vector
    } else {
        VIRTIO_MSI_NO_VECTOR
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
    mem.read_into(gpa, &mut bytes)
        .then(|| u16::from_le_bytes(bytes))
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

struct FbSink {
    path: PathBuf,
    file: Option<File>,
    map: *mut u8,
    map_len: usize,
    capacity: usize,
    seq: u64,
}

// The device owns FbSink single-threadedly on the vCPU thread. The raw mmap
// pointer is never shared across threads; this only satisfies VirtioGpu's Send bound.
unsafe impl Send for FbSink {}

impl std::fmt::Debug for FbSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FbSink")
            .field("path", &self.path)
            .field("capacity", &self.capacity)
            .field("seq", &self.seq)
            .finish()
    }
}

impl FbSink {
    fn from_env() -> Option<FbSink> {
        let path = std::env::var_os("BRIDGEVM_DISPLAY_EXPORT_FB")?;
        if path.is_empty() {
            return None;
        }

        Some(FbSink {
            path: PathBuf::from(path),
            file: None,
            map: std::ptr::null_mut(),
            map_len: 0,
            capacity: 0,
            seq: 0,
        })
    }

    fn write(&mut self, width: u32, height: u32, stride: u32, fourcc: u32, bytes: &[u8]) {
        let needed = 64 + (height as usize) * (stride as usize);

        if self.map.is_null() || self.capacity < needed {
            if !self.map.is_null() {
                unsafe {
                    libc::munmap(self.map.cast(), self.map_len);
                }
            }
            self.map = std::ptr::null_mut();
            self.map_len = 0;
            self.capacity = 0;
            self.file = None;

            if let Some(parent) = self.path.parent() {
                if !parent.as_os_str().is_empty() {
                    if let Err(err) = std::fs::create_dir_all(parent) {
                        eprintln!("virtio-gpu fb export failed: {err}");
                        return;
                    }
                }
            }

            let file = match OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(true)
                .open(&self.path)
            {
                Ok(file) => file,
                Err(err) => {
                    eprintln!("virtio-gpu fb export failed: {err}");
                    return;
                }
            };

            if let Err(err) = file.set_len(needed as u64) {
                eprintln!("virtio-gpu fb export failed: {err}");
                return;
            }

            let map = unsafe {
                libc::mmap(
                    std::ptr::null_mut(),
                    needed,
                    libc::PROT_READ | libc::PROT_WRITE,
                    libc::MAP_SHARED,
                    file.as_raw_fd(),
                    0,
                )
            };
            if map == libc::MAP_FAILED {
                eprintln!(
                    "virtio-gpu fb export failed: {}",
                    std::io::Error::last_os_error()
                );
                self.map = std::ptr::null_mut();
                self.map_len = 0;
                self.capacity = 0;
                self.file = None;
                return;
            }

            self.file = Some(file);
            self.map = map.cast();
            self.map_len = needed;
            self.capacity = needed;
        }

        self.seq = self.seq.wrapping_add(1);

        let mut header = [0u8; 24];
        header[0..4].copy_from_slice(&0x4256_4642u32.to_le_bytes());
        header[4..8].copy_from_slice(&1u32.to_le_bytes());
        header[8..12].copy_from_slice(&width.to_le_bytes());
        header[12..16].copy_from_slice(&height.to_le_bytes());
        header[16..20].copy_from_slice(&stride.to_le_bytes());
        header[20..24].copy_from_slice(&fourcc.to_le_bytes());

        unsafe {
            std::ptr::copy_nonoverlapping(header.as_ptr(), self.map, header.len());
            (&*(self.map.add(24) as *const std::sync::atomic::AtomicU64))
                .store(self.seq, Ordering::Release);
        }
        std::sync::atomic::fence(Ordering::Release);

        unsafe {
            std::ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                self.map.add(64),
                bytes.len().min(needed - 64),
            );
        }

        std::sync::atomic::fence(Ordering::Release);
        self.seq = self.seq.wrapping_add(1);
        unsafe {
            (&*(self.map.add(24) as *const std::sync::atomic::AtomicU64))
                .store(self.seq, Ordering::Release);
        }
    }
}

impl Drop for FbSink {
    fn drop(&mut self) {
        if !self.map.is_null() {
            unsafe {
                libc::munmap(self.map.cast(), self.map_len);
            }
            self.map = std::ptr::null_mut();
            self.map_len = 0;
            self.capacity = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::virtio_gpu_3d::MockBackend;
    use std::sync::{Arc, Mutex};

    #[test]
    fn edid_preferred_timing_is_120_hz_with_valid_ranges_and_checksum() {
        let edid = build_edid(1280, 800);
        let dtd = &edid[54..72];
        let pixel_clock_10khz = u32::from(u16::from_le_bytes([dtd[0], dtd[1]]));
        let h_active = u32::from(dtd[2]) | (u32::from(dtd[4] >> 4) << 8);
        let h_blank = u32::from(dtd[3]) | (u32::from(dtd[4] & 0x0f) << 8);
        let v_active = u32::from(dtd[5]) | (u32::from(dtd[7] >> 4) << 8);
        let v_blank = u32::from(dtd[6]) | (u32::from(dtd[7] & 0x0f) << 8);
        let refresh_hz = pixel_clock_10khz * 10_000 / ((h_active + h_blank) * (v_active + v_blank));

        assert_eq!((h_active, v_active), (1280, 800));
        assert_eq!(refresh_hz, 119); // Integer 10 kHz clock encoding rounds just below 120 Hz.
        assert_eq!(&edid[75..82], &[0xfd, 0, 48, 144, 30, 160, 15]);
        assert_eq!(
            edid.iter().fold(0u8, |sum, byte| sum.wrapping_add(*byte)),
            0
        );
    }

    #[test]
    fn trace_sampling_keeps_initial_evidence_and_sparse_long_run_checkpoints() {
        assert!(trace_sample(1));
        assert!(trace_sample(64));
        assert!(!trace_sample(65));
        assert!(!trace_sample(1023));
        assert!(trace_sample(1024));
        assert!(!trace_sample(1025));
    }

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

        fn host_ptr(&self, gpa: u64, len: usize) -> Option<*mut u8> {
            let off = gpa.checked_sub(self.base)? as usize;
            let end = off.checked_add(len)?;
            (end <= self.bytes.len()).then(|| self.bytes.as_ptr().wrapping_add(off) as *mut u8)
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

    fn program_msix_vector(dev: &mut VirtioPciGpu, vector: u16, address: u64, data: u32) {
        let off = u64::from(VIRTIO_GPU_MSIX_TABLE_OFFSET) + u64::from(vector) * 16;
        assert_eq!(
            dev.msix_bar_access(
                off,
                VirtioPciGpuOp::Write {
                    size: 8,
                    value: address,
                },
            ),
            VirtioGpuResult::WriteAck
        );
        assert_eq!(
            dev.msix_bar_access(
                off + 8,
                VirtioPciGpuOp::Write {
                    size: 4,
                    value: u64::from(data),
                },
            ),
            VirtioGpuResult::WriteAck
        );
        assert_eq!(
            dev.msix_bar_access(off + 12, VirtioPciGpuOp::Write { size: 4, value: 0 },),
            VirtioGpuResult::WriteAck
        );
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

    #[test]
    fn hex_prefix_formats_bounded_payloads() {
        assert_eq!(hex_prefix(&[], 32), "");
        assert_eq!(hex_prefix(&[0x00, 0x0f, 0xa5], 32), "00 0f a5");
        assert_eq!(hex_prefix(&[0x00, 0x01, 0x02, 0x03], 3), "00 01 02 ...");
        assert_eq!(hex_prefix(&[0x7f], 0), " ...");
    }

    fn create_blob_req(
        resource_id: u32,
        blob_mem: u32,
        size: u64,
        entries: &[(u64, u32)],
    ) -> Vec<u8> {
        let mut out = ctrl_req_ctx(VIRTIO_GPU_CMD_RESOURCE_CREATE_BLOB, 1);
        out.extend_from_slice(&resource_id.to_le_bytes());
        out.extend_from_slice(&blob_mem.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&(entries.len() as u32).to_le_bytes());
        out.extend_from_slice(&0u64.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        for (addr, len) in entries {
            out.extend_from_slice(&addr.to_le_bytes());
            out.extend_from_slice(&len.to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes());
        }
        out
    }

    fn set_scanout_blob_req(
        resource_id: u32,
        width: u32,
        height: u32,
        format: u32,
        stride: u32,
        offset: u32,
    ) -> Vec<u8> {
        let mut out = ctrl_req(VIRTIO_GPU_CMD_SET_SCANOUT_BLOB);
        push_rect(
            &mut out,
            Rect {
                x: 0,
                y: 0,
                width,
                height,
            },
        );
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&resource_id.to_le_bytes());
        out.extend_from_slice(&width.to_le_bytes());
        out.extend_from_slice(&height.to_le_bytes());
        out.extend_from_slice(&format.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        for index in 0..4 {
            out.extend_from_slice(&(if index == 0 { stride } else { 0 }).to_le_bytes());
        }
        for index in 0..4 {
            out.extend_from_slice(&(if index == 0 { offset } else { 0 }).to_le_bytes());
        }
        out
    }

    fn flush_req(resource_id: u32, rect: Rect) -> Vec<u8> {
        let mut out = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_FLUSH);
        push_rect(&mut out, rect);
        out.extend_from_slice(&resource_id.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
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

    fn trace_test_path(label: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "bridgevm-virtio-gpu-{label}-{}-{nanos}.jsonl",
            std::process::id()
        ))
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
        submit_control_readable_descs_at(
            dev,
            mem,
            readable,
            response_len,
            0x4000_1000,
            0x4000_4000,
            0x4000_9000,
        )
    }

    fn submit_control_readable_descs_at(
        dev: &mut VirtioPciGpu,
        mem: &mut TestMem,
        readable: &[&[u8]],
        response_len: u32,
        desc: u64,
        req: u64,
        resp: u64,
    ) -> (Vec<u8>, u16) {
        let avail = 0x4000_2000;
        let used = 0x4000_3000;
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
    fn viogpu3d_msix_contract_accepts_config_control_and_cursor_vectors() {
        let mut dev = VirtioPciGpu::new(1280, 800);
        let mut mem = TestMem::new(0x4000_0000, 0x10000);

        pci_write(&mut dev, COMMON_CONFIG_MSIX_VECTOR, 2, 0, &mut mem);
        assert_eq!(
            pci_read(&mut dev, COMMON_CONFIG_MSIX_VECTOR, 2, &mut mem),
            0
        );

        for (queue, vector) in [(0u16, 1u16), (1, 2)] {
            pci_write(&mut dev, COMMON_QUEUE_SELECT, 2, u64::from(queue), &mut mem);
            pci_write(
                &mut dev,
                COMMON_QUEUE_MSIX_VECTOR,
                2,
                u64::from(vector),
                &mut mem,
            );
            assert_eq!(
                pci_read(&mut dev, COMMON_QUEUE_MSIX_VECTOR, 2, &mut mem),
                u64::from(vector)
            );
        }

        pci_write(
            &mut dev,
            COMMON_QUEUE_MSIX_VECTOR,
            2,
            u64::from(VIRTIO_GPU_MSIX_VECTOR_COUNT),
            &mut mem,
        );
        assert_eq!(
            pci_read(&mut dev, COMMON_QUEUE_MSIX_VECTOR, 2, &mut mem),
            u64::from(VIRTIO_MSI_NO_VECTOR)
        );
    }

    #[test]
    fn trace_recorder_writes_command_details_for_p3_gpu_bringup() {
        let path = trace_test_path("p3-command-details");
        let (mut dev, _backend) = dev_with_mock();
        dev.gpu.trace = crate::virtio_gpu_trace::VirtioGpuTraceRecorder::test_file(&path);
        let mut mem = TestMem::new(0x4000_0000, 0x10000);

        let capset_req = {
            let mut req = ctrl_req(VIRTIO_GPU_CMD_GET_CAPSET_INFO);
            req.extend_from_slice(&0u32.to_le_bytes());
            req.extend_from_slice(&0u32.to_le_bytes());
            req
        };
        let _ = submit_control(&mut dev, &mut mem, &capset_req, 40);
        let _ = submit_control(&mut dev, &mut mem, &ctx_create_req(1, 4, b"venus"), 24);
        let _ = submit_control(
            &mut dev,
            &mut mem,
            &create_blob_req(7, VIRTIO_GPU_BLOB_MEM_HOST3D, 4096, &[]),
            24,
        );
        let _ = submit_control(
            &mut dev,
            &mut mem,
            &submit_3d_req(1, &[0xaa, 0xbb, 0xcc, 0xdd]),
            24,
        );
        drop(dev);

        let contents = std::fs::read_to_string(&path).unwrap();
        let _ = std::fs::remove_file(path);
        assert!(contents.contains("\"event\":\"queue_notify\""));
        assert!(contents.contains("\"name\":\"GET_CAPSET_INFO\""));
        assert!(contents.contains("\"capset_index\":0"));
        assert!(contents.contains("\"response_name\":\"OK_CAPSET_INFO\""));
        assert!(contents.contains("\"response_capset_id\":4"));
        assert!(contents.contains("\"response_capset_max_size\""));
        assert!(contents.contains("\"name\":\"CTX_CREATE\""));
        assert!(contents.contains("\"context_init\":4"));
        assert!(contents.contains("\"debug_name\":\"venus\""));
        assert!(contents.contains("\"name\":\"RESOURCE_CREATE_BLOB\""));
        assert!(contents.contains("\"resource_id\":7"));
        assert!(contents.contains("\"blob_mem\":2"));
        assert!(contents.contains("\"blob_size\":4096"));
        assert!(contents.contains("\"name\":\"SUBMIT_3D\""));
        assert!(contents.contains("\"submit_prefix_hex\":\"aa bb cc dd\""));
    }

    #[test]
    fn trace_never_samples_away_nonempty_submits() {
        let path = trace_test_path("nonempty-submit-sampling");
        let (mut dev, _backend) = dev_with_mock();
        dev.gpu.trace = crate::virtio_gpu_trace::VirtioGpuTraceRecorder::test_file(&path);
        // Deep past the always-record window: a boot's 60 Hz vsync no-ops put
        // real application submissions thousands deep into this counter.
        dev.gpu.trace_submit_success_count = 5000;
        let mut mem = TestMem::new(0x4000_0000, 0x10000);

        let _ = submit_control(&mut dev, &mut mem, &ctx_create_req(1, 4, b"venus"), 24);
        let _ = submit_control(&mut dev, &mut mem, &submit_3d_req(0, &[]), 24);
        let _ = submit_control(
            &mut dev,
            &mut mem,
            &submit_3d_req(1, &[0x11, 0x22, 0x33, 0x44]),
            24,
        );
        drop(dev);

        let contents = std::fs::read_to_string(&path).unwrap();
        let _ = std::fs::remove_file(path);
        // The empty synchronization no-op is sampled out at this depth...
        assert!(!contents.contains("\"submit_size\":0"));
        // ...but the nonempty application submission must always be recorded.
        assert!(contents.contains("\"submit_prefix_hex\":\"11 22 33 44\""));
    }

    #[test]
    fn trace_command_reuses_field_scratch_across_records() {
        let path = trace_test_path("command-field-scratch");
        let (mut dev, _backend) = dev_with_mock();
        dev.gpu.trace = crate::virtio_gpu_trace::VirtioGpuTraceRecorder::test_file(&path);
        let mut mem = TestMem::new(0x4000_0000, 0x10000);
        let req = {
            let mut req = ctrl_req(VIRTIO_GPU_CMD_GET_CAPSET_INFO);
            req.extend_from_slice(&0u32.to_le_bytes());
            req.extend_from_slice(&0u32.to_le_bytes());
            req
        };

        let _ = submit_control(&mut dev, &mut mem, &req, 40);
        let cap = dev.gpu.trace_fields_scratch.capacity();
        let ptr = dev.gpu.trace_fields_scratch.as_ptr();
        assert!(cap > 0);
        assert!(dev.gpu.trace_fields_scratch.is_empty());

        let _ = submit_control(&mut dev, &mut mem, &req, 40);

        assert_eq!(dev.gpu.trace_fields_scratch.capacity(), cap);
        assert_eq!(dev.gpu.trace_fields_scratch.as_ptr(), ptr);
        assert!(dev.gpu.trace_fields_scratch.is_empty());
        drop(dev);
        let contents = std::fs::read_to_string(&path).unwrap();
        let _ = std::fs::remove_file(path);
        assert!(contents.matches("\"event\":\"command\"").count() >= 2);
    }

    #[test]
    fn trace_non_command_events_reuse_field_scratch() {
        let path = trace_test_path("non-command-field-scratch");
        let mut dev = VirtioPciGpu::new(1600, 900);
        dev.gpu.trace = crate::virtio_gpu_trace::VirtioGpuTraceRecorder::disabled();
        dev.gpu.trace_fields_scratch = String::new();

        dev.gpu.write_status(1);
        dev.gpu.write_driver_features(0xffff);
        dev.gpu.trace_queue_notify(42);
        assert_eq!(dev.gpu.trace_fields_scratch.capacity(), 0);

        dev.gpu.trace = crate::virtio_gpu_trace::VirtioGpuTraceRecorder::test_file(&path);
        dev.gpu.write_status(1);
        dev.gpu.write_driver_features(0xffff);
        dev.gpu.trace_common_read(REG_STATUS, 4, 1);
        dev.gpu.trace_queue_notify(42);
        dev.gpu.trace_fence_create(
            CompletedFence {
                ctx_id: 1,
                ring_idx: 2,
                fence_id: 3,
            },
            true,
            "accepted",
        );
        dev.gpu.trace_fence_complete(CompletedFence {
            ctx_id: 1,
            ring_idx: 2,
            fence_id: 3,
        });
        dev.gpu.trace_fence_delivery(
            CompletedFence {
                ctx_id: 1,
                ring_idx: 2,
                fence_id: 3,
            },
            24,
        );
        let cap = dev.gpu.trace_fields_scratch.capacity();
        let ptr = dev.gpu.trace_fields_scratch.as_ptr();
        assert!(cap > 0);
        assert!(dev.gpu.trace_fields_scratch.is_empty());

        dev.gpu.write_status(1);
        dev.gpu.write_driver_features(0xffff);
        dev.gpu.trace_common_read(REG_STATUS, 4, 1);
        dev.gpu.trace_queue_notify(42);
        dev.gpu.trace_fence_create(
            CompletedFence {
                ctx_id: 1,
                ring_idx: 2,
                fence_id: 3,
            },
            true,
            "accepted",
        );
        dev.gpu.trace_fence_complete(CompletedFence {
            ctx_id: 1,
            ring_idx: 2,
            fence_id: 3,
        });
        dev.gpu.trace_fence_delivery(
            CompletedFence {
                ctx_id: 1,
                ring_idx: 2,
                fence_id: 3,
            },
            24,
        );

        assert_eq!(dev.gpu.trace_fields_scratch.capacity(), cap);
        assert_eq!(dev.gpu.trace_fields_scratch.as_ptr(), ptr);
        assert!(dev.gpu.trace_fields_scratch.is_empty());
        drop(dev);
        let contents = std::fs::read_to_string(&path).unwrap();
        let _ = std::fs::remove_file(path);
        assert!(contents.contains("\"event\":\"device_status\""));
        assert!(contents.contains("\"event\":\"driver_features\""));
        assert!(contents.contains("\"event\":\"common_read\""));
        assert!(contents.contains("\"event\":\"queue_notify\""));
        assert!(contents.contains("\"event\":\"fence_create\""));
        assert!(contents.contains("\"event\":\"fence_complete\""));
        assert!(contents.contains("\"event\":\"fence_deliver\""));
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
    fn trace_records_display_edid_and_pre_reset_state() {
        let path = trace_test_path("display-edid-reset-details");
        let mut dev = VirtioPciGpu::new(1600, 900);
        dev.gpu.trace = crate::virtio_gpu_trace::VirtioGpuTraceRecorder::test_file(&path);
        let mut mem = TestMem::new(0x4000_0000, 0x20000);

        let _ = submit_control(
            &mut dev,
            &mut mem,
            &ctrl_req(VIRTIO_GPU_CMD_GET_DISPLAY_INFO),
            408,
        );
        let mut edid_request = ctrl_req(VIRTIO_GPU_CMD_GET_EDID);
        edid_request.extend_from_slice(&0u32.to_le_bytes());
        edid_request.extend_from_slice(&0u32.to_le_bytes());
        let _ = submit_control(&mut dev, &mut mem, &edid_request, 1056);
        dev.gpu.write_driver_features(u64::MAX);
        dev.gpu.write_status(0xf);
        dev.gpu.write_status(0);

        drop(dev);
        let contents = std::fs::read_to_string(&path).unwrap();
        let _ = std::fs::remove_file(path);
        assert!(contents.contains("\"response_scanout0_width\":1600"));
        assert!(contents.contains("\"response_scanout0_height\":900"));
        assert!(contents.contains("\"response_scanout0_enabled\":1"));
        assert!(contents.contains("\"response_edid_size\":128"));
        assert!(contents.contains("\"response_edid_checksum_valid\":true"));
        assert!(contents.contains(
            "\"readable_descriptor_lengths\":[24],\"writable_descriptor_lengths\":[408]"
        ));
        assert!(contents.contains(
            "\"readable_descriptor_lengths\":[32],\"writable_descriptor_lengths\":[1056]"
        ));
        assert!(contents.contains(
            "\"writable_descriptor_bytes\":1056,\"response_planned_write_len\":1056,\"response_truncated\":false"
        ));
        assert!(contents.contains(
            "\"response_header_valid\":true,\"response_flags\":0,\"response_fenced\":false,\"response_fence_id\":0,\"response_ctx_id\":0,\"response_ring_idx\":0"
        ));
        assert!(contents.contains("\"raw\":0,\"raw_hex\":\"0x0\",\"previous\":15"));
        assert!(contents.contains("\"driver_features_word0_hex\":\"0x2\""));
        assert!(contents.contains("\"reset\":true"));
    }

    fn program_config_msix_vector(dev: &mut VirtioPciGpu, vector: u16) {
        let mut mem = TestMem::new(0x4000_0000, 0x1000);
        assert_eq!(
            dev.access(
                PCI_COMMON_CFG_OFFSET + COMMON_CONFIG_MSIX_VECTOR,
                VirtioPciGpuOp::Write {
                    size: 2,
                    value: u64::from(vector),
                },
                &mut mem,
            ),
            VirtioGpuResult::WriteAck
        );
    }

    #[test]
    fn host_resize_reports_new_geometry_and_raises_config_change_interrupt() {
        let mut dev = VirtioPciGpu::new(1280, 800);
        program_config_msix_vector(&mut dev, 0);
        program_msix_vector(&mut dev, 0, 0xfee0_1000, 0x71);

        // Config reads start with no pending display event.
        assert_eq!(dev.display_resolution(), (1280, 800));

        assert!(dev.request_display_resolution(1920, 1080));
        assert_eq!(dev.display_resolution(), (1920, 1080));
        // ISR config-change bit (0x2) is set and events_read advertises DISPLAY.
        assert_eq!(dev.stats().interrupt_status & 0x2, 0x2);

        // The armed config-change interrupt is delivered on the config vector.
        assert_eq!(
            dev.drain_pending_msix(true, false),
            vec![MsixMessage {
                vector: 0,
                address: 0xfee0_1000,
                data: 0x71,
            }]
        );
        // Delivered once only.
        assert!(dev.drain_pending_msix(true, false).is_empty());

        // GET_DISPLAY_INFO now reports the new geometry.
        let mut mem = TestMem::new(0x4000_0000, 0x40000);
        assert_eq!(
            read_le_u32(
                &submit_control(
                    &mut dev,
                    &mut mem,
                    &ctrl_req(VIRTIO_GPU_CMD_GET_DISPLAY_INFO),
                    408
                ),
                24 + 8
            ),
            Some(1920)
        );

        // A no-op resize to the same geometry does not re-arm.
        assert!(!dev.request_display_resolution(1920, 1080));
        // Out-of-range is rejected.
        assert!(!dev.request_display_resolution(0, 1080));
        assert!(!dev.request_display_resolution(1920, MAX_SCANOUT_DIMENSION + 1));
    }

    #[test]
    fn host_resize_display_event_clears_when_guest_acks() {
        let mut dev = VirtioPciGpu::new(1280, 800);
        assert!(dev.request_display_resolution(1600, 900));

        let mut mem = TestMem::new(0x4000_0000, 0x1000);
        // events_read (config offset 0) advertises the DISPLAY event.
        assert_eq!(
            dev.access(
                PCI_DEVICE_CFG_OFFSET + 0,
                VirtioPciGpuOp::Read { size: 4 },
                &mut mem,
            ),
            VirtioGpuResult::ReadValue(u64::from(VIRTIO_GPU_EVENT_DISPLAY))
        );

        // The driver acks by writing the bit into events_clear (config offset 4).
        assert_eq!(
            dev.access(
                PCI_DEVICE_CFG_OFFSET + 4,
                VirtioPciGpuOp::Write {
                    size: 4,
                    value: u64::from(VIRTIO_GPU_EVENT_DISPLAY),
                },
                &mut mem,
            ),
            VirtioGpuResult::WriteAck
        );

        // events_read no longer reports the acked event.
        assert_eq!(
            dev.access(
                PCI_DEVICE_CFG_OFFSET + 0,
                VirtioPciGpuOp::Read { size: 4 },
                &mut mem,
            ),
            VirtioGpuResult::ReadValue(0)
        );
    }

    #[test]
    fn control_queue_pending_msix_survives_until_table_entry_is_programmed() {
        let mut dev = VirtioPciGpu::new(1600, 900);
        let mut mem = TestMem::new(0x4000_0000, 0x20000);
        let resp = submit_control(
            &mut dev,
            &mut mem,
            &ctrl_req(VIRTIO_GPU_CMD_GET_DISPLAY_INFO),
            408,
        );

        assert_eq!(read_le_u32(&resp, 0), Some(VIRTIO_GPU_RESP_OK_DISPLAY_INFO));
        assert!(dev.stats().queues[0].pending_msix);
        assert_eq!(dev.drain_pending_msix(true, false), Vec::new());
        assert!(dev.stats().queues[0].pending_msix);

        program_msix_vector(&mut dev, 0, 0xfee0_0000, 0x40);

        assert_eq!(
            dev.drain_pending_msix(true, false),
            vec![MsixMessage {
                vector: 0,
                address: 0xfee0_0000,
                data: 0x40,
            }]
        );
        assert!(!dev.stats().queues[0].pending_msix);
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
    fn gather_readable_skips_writable_and_unbacked_descriptors() {
        let mut mem = TestMem::new(0x4000_0000, 0x20000);
        mem.write(0x4000_1000, b"head");
        mem.write(0x4000_2000, b"skip");
        mem.write(0x4000_3000, b"tail");

        let mut gathered = Vec::new();
        VirtioGpu::gather_readable_into(
            &mem,
            &[
                Descriptor {
                    addr: 0x4000_1000,
                    len: 4,
                    flags: 0,
                    next: 0,
                },
                Descriptor {
                    addr: 0x4000_2000,
                    len: 4,
                    flags: DESC_F_WRITE,
                    next: 0,
                },
                Descriptor {
                    addr: 0x3fff_ff00,
                    len: 4,
                    flags: 0,
                    next: 0,
                },
                Descriptor {
                    addr: 0x4000_3000,
                    len: 4,
                    flags: 0,
                    next: 0,
                },
            ],
            &mut gathered,
        );

        assert_eq!(gathered, b"headtail");
    }

    #[test]
    fn gather_readable_rejects_oversized_guest_length_before_growing_scratch() {
        let mem = TestMem::new(0x4000_0000, 0x1000);
        let mut gathered = Vec::with_capacity(32);
        let capacity = gathered.capacity();

        VirtioGpu::gather_readable_into(
            &mem,
            &[Descriptor {
                addr: 0x4000_0800,
                len: u32::MAX,
                flags: 0,
                next: 0,
            }],
            &mut gathered,
        );

        assert!(gathered.is_empty());
        assert_eq!(gathered.capacity(), capacity);
    }

    #[test]
    fn control_queue_reuses_descriptor_request_and_response_scratch_for_immediate_commands() {
        let mut dev = VirtioPciGpu::new(4, 3);
        let mut mem = TestMem::new(0x4000_0000, 0x20000);
        let request = ctrl_req(VIRTIO_GPU_CMD_GET_DISPLAY_INFO);

        let first = submit_control(&mut dev, &mut mem, &request, 408);
        assert_eq!(
            read_le_u32(&first, 0),
            Some(VIRTIO_GPU_RESP_OK_DISPLAY_INFO)
        );
        let first_desc_capacity = dev.gpu.descriptor_scratch.capacity();
        let first_request_capacity = dev.gpu.request_scratch.capacity();
        let first_response_capacity = dev.gpu.response_scratch.capacity();
        let first_response_ptr = dev.gpu.response_scratch.as_ptr();
        assert!(dev.gpu.descriptor_scratch.is_empty());
        assert!(dev.gpu.request_scratch.is_empty());
        assert!(dev.gpu.response_scratch.is_empty());
        assert!(first_desc_capacity >= 2);
        assert!(first_request_capacity >= request.len());
        assert!(first_response_capacity >= first.len());

        let second = submit_control(&mut dev, &mut mem, &request, 408);
        assert_eq!(
            read_le_u32(&second, 0),
            Some(VIRTIO_GPU_RESP_OK_DISPLAY_INFO)
        );
        assert_eq!(dev.gpu.descriptor_scratch.capacity(), first_desc_capacity);
        assert_eq!(dev.gpu.request_scratch.capacity(), first_request_capacity);
        assert_eq!(dev.gpu.response_scratch.capacity(), first_response_capacity);
        assert_eq!(dev.gpu.response_scratch.as_ptr(), first_response_ptr);
        assert!(dev.gpu.descriptor_scratch.is_empty());
        assert!(dev.gpu.request_scratch.is_empty());
        assert!(dev.gpu.response_scratch.is_empty());
    }

    #[test]
    fn attach_backing_reuses_resource_backing_and_preserves_on_malformed_request() {
        let mut gpu = VirtioGpu::new(4, 3);
        let mem = TestMem::new(0x4000_0000, 0x20000);
        let mut response = Vec::new();
        let mut create = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_CREATE_2D);
        create.extend_from_slice(&1u32.to_le_bytes());
        create.extend_from_slice(&FORMAT_B8G8R8A8_UNORM.to_le_bytes());
        create.extend_from_slice(&4u32.to_le_bytes());
        create.extend_from_slice(&3u32.to_le_bytes());
        let hdr = CtrlHdr::parse(&create).unwrap();
        gpu.resource_create_2d_into(&create, Some(hdr), &mut response);
        assert_eq!(read_le_u32(&response, 0), Some(VIRTIO_GPU_RESP_OK_NODATA));

        let mut attach = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING);
        attach.extend_from_slice(&1u32.to_le_bytes());
        attach.extend_from_slice(&2u32.to_le_bytes());
        attach.extend_from_slice(&0x4000_8000u64.to_le_bytes());
        attach.extend_from_slice(&4u32.to_le_bytes());
        attach.extend_from_slice(&0u32.to_le_bytes());
        attach.extend_from_slice(&0x4000_9000u64.to_le_bytes());
        attach.extend_from_slice(&8u32.to_le_bytes());
        attach.extend_from_slice(&0u32.to_le_bytes());
        let hdr = CtrlHdr::parse(&attach).unwrap();
        gpu.attach_backing_into(&mem, &attach, Some(hdr), &mut response);
        assert_eq!(read_le_u32(&response, 0), Some(VIRTIO_GPU_RESP_OK_NODATA));
        let resource = gpu.resources.get(&1).unwrap();
        assert_eq!(
            resource.backing,
            vec![
                BackingEntry {
                    addr: 0x4000_8000,
                    len: 4
                },
                BackingEntry {
                    addr: 0x4000_9000,
                    len: 8
                },
            ]
        );
        let backing_ptr = resource.backing.as_ptr();
        let backing_capacity = resource.backing.capacity();

        let mut malformed = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING);
        malformed.extend_from_slice(&1u32.to_le_bytes());
        malformed.extend_from_slice(&2u32.to_le_bytes());
        malformed.extend_from_slice(&0x4000_a000u64.to_le_bytes());
        malformed.extend_from_slice(&4u32.to_le_bytes());
        malformed.extend_from_slice(&0u32.to_le_bytes());
        let hdr = CtrlHdr::parse(&malformed).unwrap();
        gpu.attach_backing_into(&mem, &malformed, Some(hdr), &mut response);
        assert_eq!(read_le_u32(&response, 0), Some(VIRTIO_GPU_RESP_ERR_UNSPEC));
        let resource = gpu.resources.get(&1).unwrap();
        assert_eq!(resource.backing.as_ptr(), backing_ptr);
        assert_eq!(resource.backing.capacity(), backing_capacity);
        assert_eq!(
            resource.backing,
            vec![
                BackingEntry {
                    addr: 0x4000_8000,
                    len: 4
                },
                BackingEntry {
                    addr: 0x4000_9000,
                    len: 8
                },
            ]
        );

        let mut reattach = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING);
        reattach.extend_from_slice(&1u32.to_le_bytes());
        reattach.extend_from_slice(&2u32.to_le_bytes());
        reattach.extend_from_slice(&0x4000_b000u64.to_le_bytes());
        reattach.extend_from_slice(&16u32.to_le_bytes());
        reattach.extend_from_slice(&0u32.to_le_bytes());
        reattach.extend_from_slice(&0x4000_c000u64.to_le_bytes());
        reattach.extend_from_slice(&32u32.to_le_bytes());
        reattach.extend_from_slice(&0u32.to_le_bytes());
        let hdr = CtrlHdr::parse(&reattach).unwrap();
        gpu.attach_backing_into(&mem, &reattach, Some(hdr), &mut response);
        assert_eq!(read_le_u32(&response, 0), Some(VIRTIO_GPU_RESP_OK_NODATA));
        let resource = gpu.resources.get(&1).unwrap();
        assert_eq!(resource.backing.as_ptr(), backing_ptr);
        assert_eq!(resource.backing.capacity(), backing_capacity);
        assert_eq!(
            resource.backing,
            vec![
                BackingEntry {
                    addr: 0x4000_b000,
                    len: 16
                },
                BackingEntry {
                    addr: 0x4000_c000,
                    len: 32
                },
            ]
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
    fn resource_transfer_split_backing_row_falls_back_to_pixel_reads() {
        let mut dev = VirtioPciGpu::new(2, 1);
        let mut mem = TestMem::new(0x4000_0000, 0x30000);
        let backing_a = 0x4000_8000;
        let backing_b = 0x4000_9000;
        mem.write(backing_a, &[0x11, 0x22, 0x33, 0xff]);
        mem.write(backing_b, &[0x44, 0x55, 0x66, 0xee]);

        let mut create = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_CREATE_2D);
        create.extend_from_slice(&1u32.to_le_bytes());
        create.extend_from_slice(&FORMAT_B8G8R8A8_UNORM.to_le_bytes());
        create.extend_from_slice(&2u32.to_le_bytes());
        create.extend_from_slice(&1u32.to_le_bytes());
        assert_eq!(
            read_le_u32(&submit_control(&mut dev, &mut mem, &create, 24), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );

        let mut attach = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING);
        attach.extend_from_slice(&1u32.to_le_bytes());
        attach.extend_from_slice(&2u32.to_le_bytes());
        attach.extend_from_slice(&backing_a.to_le_bytes());
        attach.extend_from_slice(&4u32.to_le_bytes());
        attach.extend_from_slice(&0u32.to_le_bytes());
        attach.extend_from_slice(&backing_b.to_le_bytes());
        attach.extend_from_slice(&4u32.to_le_bytes());
        attach.extend_from_slice(&0u32.to_le_bytes());
        assert_eq!(
            read_le_u32(&submit_control(&mut dev, &mut mem, &attach, 24), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );

        let mut set_scanout = ctrl_req(VIRTIO_GPU_CMD_SET_SCANOUT);
        push_rect(
            &mut set_scanout,
            Rect {
                x: 0,
                y: 0,
                width: 2,
                height: 1,
            },
        );
        set_scanout.extend_from_slice(&0u32.to_le_bytes());
        set_scanout.extend_from_slice(&1u32.to_le_bytes());
        assert_eq!(
            read_le_u32(&submit_control(&mut dev, &mut mem, &set_scanout, 24), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );

        let mut transfer = ctrl_req(VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D);
        push_rect(
            &mut transfer,
            Rect {
                x: 0,
                y: 0,
                width: 2,
                height: 1,
            },
        );
        transfer.extend_from_slice(&0u64.to_le_bytes());
        transfer.extend_from_slice(&1u32.to_le_bytes());
        transfer.extend_from_slice(&0u32.to_le_bytes());
        assert_eq!(
            read_le_u32(&submit_control(&mut dev, &mut mem, &transfer, 24), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );

        let flush = flush_req(
            1,
            Rect {
                x: 0,
                y: 0,
                width: 2,
                height: 1,
            },
        );
        assert_eq!(
            read_le_u32(&submit_control(&mut dev, &mut mem, &flush, 24), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );

        assert_eq!(
            &dev.scanout().unwrap().bytes[0..8],
            &[0x11, 0x22, 0x33, 0, 0x44, 0x55, 0x66, 0]
        );
    }

    #[test]
    fn set_scanout_blob_guest_flush_presents_pixels_with_stride_and_offset() {
        let (mut dev, _) = dev_with_mock();
        let mut mem = TestMem::new(0x4000_0000, 0x30000);
        let backing = 0x4000_8000;
        let mut backing_bytes = vec![0u8; 64];
        backing_bytes[4..8].copy_from_slice(&[0x10, 0x20, 0x30, 0xff]);
        backing_bytes[8..12].copy_from_slice(&[0x40, 0x50, 0x60, 0xee]);
        backing_bytes[20..24].copy_from_slice(&[0x70, 0x80, 0x90, 0xdd]);
        backing_bytes[24..28].copy_from_slice(&[0xa0, 0xb0, 0xc0, 0xcc]);
        mem.write(backing, &backing_bytes);

        let create = create_blob_req(7, VIRTIO_GPU_BLOB_MEM_GUEST, 64, &[(backing, 64)]);
        let resp = submit_control(&mut dev, &mut mem, &create, 24);
        assert_eq!(read_le_u32(&resp, 0), Some(VIRTIO_GPU_RESP_OK_NODATA));

        let set_scanout = set_scanout_blob_req(7, 2, 2, FORMAT_B8G8R8A8_UNORM, 16, 4);
        let resp = submit_control(&mut dev, &mut mem, &set_scanout, 24);
        assert_eq!(read_le_u32(&resp, 0), Some(VIRTIO_GPU_RESP_OK_NODATA));

        let flush = flush_req(
            7,
            Rect {
                x: 0,
                y: 0,
                width: 2,
                height: 2,
            },
        );
        let resp = submit_control(&mut dev, &mut mem, &flush, 24);
        assert_eq!(read_le_u32(&resp, 0), Some(VIRTIO_GPU_RESP_OK_NODATA));

        let scanout = dev.scanout().unwrap();
        assert_eq!(
            &scanout.bytes[0..8],
            &[0x10, 0x20, 0x30, 0, 0x40, 0x50, 0x60, 0]
        );
        let row1 = scanout.stride as usize;
        assert_eq!(
            &scanout.bytes[row1..row1 + 8],
            &[0x70, 0x80, 0x90, 0, 0xa0, 0xb0, 0xc0, 0]
        );
    }

    #[test]
    fn set_scanout_blob_guest_flush_reuses_row_scratch() {
        let (mut dev, _) = dev_with_mock();
        let mut mem = TestMem::new(0x4000_0000, 0x30000);
        let backing = 0x4000_8000;
        let backing_bytes = [
            0x10, 0x20, 0x30, 0xff, 0x40, 0x50, 0x60, 0xee, 0x70, 0x80, 0x90, 0xdd, 0xa0, 0xb0,
            0xc0, 0xcc,
        ];
        mem.write(backing, &backing_bytes);

        let create = create_blob_req(17, VIRTIO_GPU_BLOB_MEM_GUEST, 16, &[(backing, 16)]);
        assert_eq!(
            read_le_u32(&submit_control(&mut dev, &mut mem, &create, 24), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );

        let set_scanout = set_scanout_blob_req(17, 2, 2, FORMAT_B8G8R8A8_UNORM, 8, 0);
        assert_eq!(
            read_le_u32(&submit_control(&mut dev, &mut mem, &set_scanout, 24), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );
        let flush = flush_req(
            17,
            Rect {
                x: 0,
                y: 0,
                width: 2,
                height: 2,
            },
        );

        assert_eq!(
            read_le_u32(&submit_control(&mut dev, &mut mem, &flush, 24), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );
        assert!(dev.gpu.blob_row_scratch.is_empty());
        assert!(dev.gpu.blob_row_scratch.capacity() >= 8);
        let row_scratch = (
            dev.gpu.blob_row_scratch.as_ptr(),
            dev.gpu.blob_row_scratch.capacity(),
        );

        assert_eq!(
            read_le_u32(&submit_control(&mut dev, &mut mem, &flush, 24), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );
        assert!(dev.gpu.blob_row_scratch.is_empty());
        assert_eq!(
            (
                dev.gpu.blob_row_scratch.as_ptr(),
                dev.gpu.blob_row_scratch.capacity()
            ),
            row_scratch
        );
    }

    #[test]
    fn set_scanout_blob_guest_split_backing_row_falls_back_to_pixel_reads() {
        let (mut dev, _) = dev_with_mock();
        let mut mem = TestMem::new(0x4000_0000, 0x30000);
        let backing_a = 0x4000_8000;
        let backing_b = 0x4000_9000;
        mem.write(backing_a, &[0x12, 0x23, 0x34, 0xff]);
        mem.write(backing_b, &[0x45, 0x56, 0x67, 0xee]);

        let create = create_blob_req(
            7,
            VIRTIO_GPU_BLOB_MEM_GUEST,
            8,
            &[(backing_a, 4), (backing_b, 4)],
        );
        assert_eq!(
            read_le_u32(&submit_control(&mut dev, &mut mem, &create, 24), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );

        let set_scanout = set_scanout_blob_req(7, 2, 1, FORMAT_B8G8R8A8_UNORM, 8, 0);
        assert_eq!(
            read_le_u32(&submit_control(&mut dev, &mut mem, &set_scanout, 24), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );

        let flush = flush_req(
            7,
            Rect {
                x: 0,
                y: 0,
                width: 2,
                height: 1,
            },
        );
        assert_eq!(
            read_le_u32(&submit_control(&mut dev, &mut mem, &flush, 24), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );

        assert_eq!(
            &dev.scanout().unwrap().bytes[0..8],
            &[0x12, 0x23, 0x34, 0, 0x45, 0x56, 0x67, 0]
        );
    }

    #[test]
    fn set_scanout_blob_host3d_flush_presents_pixels_from_mock_mapping() {
        let (mut dev, backend) = dev_with_mock();
        let mut mem = TestMem::new(0x4000_0000, 0x30000);
        let mut host_pixels = vec![0u8; 32];
        host_pixels[0..4].copy_from_slice(&[0x11, 0x22, 0x33, 0xff]);
        host_pixels[4..8].copy_from_slice(&[0x44, 0x55, 0x66, 0xee]);
        backend.lock().unwrap().mapped.insert(
            9,
            virtio_gpu_3d::MappedBlob {
                host_ptr: host_pixels.as_mut_ptr(),
                size: host_pixels.len(),
                map_info: 0,
            },
        );

        let create = create_blob_req(9, VIRTIO_GPU_BLOB_MEM_HOST3D, 32, &[]);
        assert_eq!(
            read_le_u32(&submit_control(&mut dev, &mut mem, &create, 24), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );
        let set_scanout = set_scanout_blob_req(9, 2, 1, FORMAT_B8G8R8A8_UNORM, 8, 0);
        assert_eq!(
            read_le_u32(&submit_control(&mut dev, &mut mem, &set_scanout, 24), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );
        let flush = flush_req(
            9,
            Rect {
                x: 0,
                y: 0,
                width: 2,
                height: 1,
            },
        );
        assert_eq!(
            read_le_u32(&submit_control(&mut dev, &mut mem, &flush, 24), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );

        assert_eq!(
            &dev.scanout().unwrap().bytes[0..8],
            &[0x11, 0x22, 0x33, 0, 0x44, 0x55, 0x66, 0]
        );
        assert!(backend.lock().unwrap().unmapped.is_empty());
    }

    #[test]
    fn set_scanout_blob_unknown_resource_errors() {
        let (mut dev, _) = dev_with_mock();
        let mut mem = TestMem::new(0x4000_0000, 0x20000);
        let set_scanout = set_scanout_blob_req(99, 2, 1, FORMAT_B8G8R8A8_UNORM, 8, 0);
        let resp = submit_control(&mut dev, &mut mem, &set_scanout, 24);
        assert_eq!(read_le_u32(&resp, 0), Some(VIRTIO_GPU_RESP_ERR_UNSPEC));
    }

    #[test]
    fn set_scanout_blob_resource_zero_unbinds() {
        let (mut dev, backend) = dev_with_mock();
        let mut mem = TestMem::new(0x4000_0000, 0x30000);
        let mut host_pixels = vec![0u8; 16];
        backend.lock().unwrap().mapped.insert(
            10,
            virtio_gpu_3d::MappedBlob {
                host_ptr: host_pixels.as_mut_ptr(),
                size: host_pixels.len(),
                map_info: 0,
            },
        );
        let create = create_blob_req(10, VIRTIO_GPU_BLOB_MEM_HOST3D, 16, &[]);
        let set_scanout = set_scanout_blob_req(10, 1, 1, FORMAT_B8G8R8A8_UNORM, 4, 0);
        let unbind = set_scanout_blob_req(0, 1, 1, FORMAT_B8G8R8A8_UNORM, 4, 0);

        assert_eq!(
            read_le_u32(&submit_control(&mut dev, &mut mem, &create, 24), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );
        assert_eq!(
            read_le_u32(&submit_control(&mut dev, &mut mem, &set_scanout, 24), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );
        assert!(dev.scanout().is_some());
        assert_eq!(
            read_le_u32(&submit_control(&mut dev, &mut mem, &unbind, 24), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );
        assert!(dev.scanout().is_none());
        assert_eq!(backend.lock().unwrap().unmapped, vec![10]);
    }

    #[test]
    fn two_d_scanout_still_works_after_blob_unbind() {
        let (mut dev, _) = dev_with_mock();
        let mut mem = TestMem::new(0x4000_0000, 0x30000);
        let blob_backing = 0x4000_7000;
        mem.write(blob_backing, &[0u8; 16]);
        let create_blob = create_blob_req(12, VIRTIO_GPU_BLOB_MEM_GUEST, 16, &[(blob_backing, 16)]);
        let set_blob = set_scanout_blob_req(12, 1, 1, FORMAT_B8G8R8A8_UNORM, 4, 0);
        let unbind = set_scanout_blob_req(0, 1, 1, FORMAT_B8G8R8A8_UNORM, 4, 0);
        assert_eq!(
            read_le_u32(&submit_control(&mut dev, &mut mem, &create_blob, 24), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );
        assert_eq!(
            read_le_u32(&submit_control(&mut dev, &mut mem, &set_blob, 24), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );
        assert_eq!(
            read_le_u32(&submit_control(&mut dev, &mut mem, &unbind, 24), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );

        let backing = 0x4000_8000;
        mem.write(backing, &[0x21, 0x32, 0x43, 0xff]);
        let mut create_2d = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_CREATE_2D);
        create_2d.extend_from_slice(&1u32.to_le_bytes());
        create_2d.extend_from_slice(&FORMAT_B8G8R8A8_UNORM.to_le_bytes());
        create_2d.extend_from_slice(&1u32.to_le_bytes());
        create_2d.extend_from_slice(&1u32.to_le_bytes());
        assert_eq!(
            read_le_u32(&submit_control(&mut dev, &mut mem, &create_2d, 24), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );
        let mut attach = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING);
        attach.extend_from_slice(&1u32.to_le_bytes());
        attach.extend_from_slice(&1u32.to_le_bytes());
        attach.extend_from_slice(&backing.to_le_bytes());
        attach.extend_from_slice(&4u32.to_le_bytes());
        attach.extend_from_slice(&0u32.to_le_bytes());
        assert_eq!(
            read_le_u32(&submit_control(&mut dev, &mut mem, &attach, 24), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );
        let mut set_2d = ctrl_req(VIRTIO_GPU_CMD_SET_SCANOUT);
        push_rect(
            &mut set_2d,
            Rect {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            },
        );
        set_2d.extend_from_slice(&0u32.to_le_bytes());
        set_2d.extend_from_slice(&1u32.to_le_bytes());
        assert_eq!(
            read_le_u32(&submit_control(&mut dev, &mut mem, &set_2d, 24), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );
        let mut transfer = ctrl_req(VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D);
        push_rect(
            &mut transfer,
            Rect {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            },
        );
        transfer.extend_from_slice(&0u64.to_le_bytes());
        transfer.extend_from_slice(&1u32.to_le_bytes());
        transfer.extend_from_slice(&0u32.to_le_bytes());
        assert_eq!(
            read_le_u32(&submit_control(&mut dev, &mut mem, &transfer, 24), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );
        let flush = flush_req(
            1,
            Rect {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            },
        );
        assert_eq!(
            read_le_u32(&submit_control(&mut dev, &mut mem, &flush, 24), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );
        assert_eq!(&dev.scanout().unwrap().bytes[0..4], &[0x21, 0x32, 0x43, 0]);
    }

    #[test]
    fn reset_clears_blob_scanout_and_unmaps() {
        let (mut dev, backend) = dev_with_mock();
        let mut mem = TestMem::new(0x4000_0000, 0x30000);
        let mut host_pixels = vec![0u8; 16];
        backend.lock().unwrap().mapped.insert(
            13,
            virtio_gpu_3d::MappedBlob {
                host_ptr: host_pixels.as_mut_ptr(),
                size: host_pixels.len(),
                map_info: 0,
            },
        );
        let create = create_blob_req(13, VIRTIO_GPU_BLOB_MEM_HOST3D, 16, &[]);
        let set_scanout = set_scanout_blob_req(13, 1, 1, FORMAT_B8G8R8A8_UNORM, 4, 0);
        assert_eq!(
            read_le_u32(&submit_control(&mut dev, &mut mem, &create, 24), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );
        assert_eq!(
            read_le_u32(&submit_control(&mut dev, &mut mem, &set_scanout, 24), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );
        dev.gpu.scanout[0] = 0xff;
        let scanout_capacity = dev.gpu.scanout.capacity();
        let scanout_ptr = dev.gpu.scanout.as_ptr();

        dev.reset_runtime_state();

        assert!(dev.scanout().is_none());
        assert_eq!(backend.lock().unwrap().unmapped, vec![13]);
        assert!(!dev.stats().scanout_active);
        assert_eq!(dev.gpu.scanout.capacity(), scanout_capacity);
        assert_eq!(dev.gpu.scanout.as_ptr(), scanout_ptr);
        assert!(dev.gpu.scanout.iter().all(|byte| *byte == 0));
    }

    #[test]
    fn resource_unref_unbinds_bound_host3d_blob_scanout() {
        let (mut dev, backend) = dev_with_mock();
        let mut mem = TestMem::new(0x4000_0000, 0x30000);
        let mut host_pixels = vec![0u8; 16];
        backend.lock().unwrap().mapped.insert(
            14,
            virtio_gpu_3d::MappedBlob {
                host_ptr: host_pixels.as_mut_ptr(),
                size: host_pixels.len(),
                map_info: 0,
            },
        );
        let create = create_blob_req(14, VIRTIO_GPU_BLOB_MEM_HOST3D, 16, &[]);
        let set_scanout = set_scanout_blob_req(14, 1, 1, FORMAT_B8G8R8A8_UNORM, 4, 0);
        let mut unref = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_UNREF);
        unref.extend_from_slice(&14u32.to_le_bytes());
        unref.extend_from_slice(&0u32.to_le_bytes());

        assert_eq!(
            read_le_u32(&submit_control(&mut dev, &mut mem, &create, 24), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );
        assert_eq!(
            read_le_u32(&submit_control(&mut dev, &mut mem, &set_scanout, 24), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );
        assert_eq!(
            read_le_u32(&submit_control(&mut dev, &mut mem, &unref, 24), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );

        assert!(dev.scanout().is_none());
        assert_eq!(backend.lock().unwrap().unmapped, vec![14]);
        assert_eq!(backend.lock().unwrap().destroyed_resources, vec![14]);
    }

    #[test]
    fn ctx_destroy_unbinds_attached_blob_scanout_before_teardown() {
        let (mut dev, backend) = dev_with_mock();
        let mut mem = TestMem::new(0x4000_0000, 0x30000);
        let mut host_pixels = vec![0u8; 16];
        backend.lock().unwrap().mapped.insert(
            15,
            virtio_gpu_3d::MappedBlob {
                host_ptr: host_pixels.as_mut_ptr(),
                size: host_pixels.len(),
                map_info: 0,
            },
        );
        let create_ctx = ctx_create_req(1, 4, b"ctx");
        let create_blob = create_blob_req(15, VIRTIO_GPU_BLOB_MEM_HOST3D, 16, &[]);
        let mut attach = ctrl_req_ctx(VIRTIO_GPU_CMD_CTX_ATTACH_RESOURCE, 1);
        attach.extend_from_slice(&15u32.to_le_bytes());
        attach.extend_from_slice(&0u32.to_le_bytes());
        let set_scanout = set_scanout_blob_req(15, 1, 1, FORMAT_B8G8R8A8_UNORM, 4, 0);
        let destroy = ctrl_req_ctx(VIRTIO_GPU_CMD_CTX_DESTROY, 1);

        for request in [&create_ctx, &create_blob, &attach, &set_scanout, &destroy] {
            assert_eq!(
                read_le_u32(&submit_control(&mut dev, &mut mem, request, 24), 0),
                Some(VIRTIO_GPU_RESP_OK_NODATA)
            );
        }

        assert!(dev.scanout().is_none());
        assert_eq!(backend.lock().unwrap().unmapped, vec![15]);
        assert_eq!(backend.lock().unwrap().destroyed, vec![1]);
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
        assert!(dev.gpu.response_scratch.is_empty());

        let mut get = ctrl_req(VIRTIO_GPU_CMD_GET_CAPSET);
        get.extend_from_slice(&4u32.to_le_bytes());
        get.extend_from_slice(&1u32.to_le_bytes());
        let resp = submit_control(&mut dev, &mut mem, &get, 24 + 160);
        assert_eq!(
            read_le_u32(&resp, 0),
            Some(virtio_gpu_3d::VIRTIO_GPU_RESP_OK_CAPSET)
        );
        assert_eq!(read_le_u32(&resp, 24), Some(1));
        let response_capacity = dev.gpu.response_scratch.capacity();
        let response_ptr = dev.gpu.response_scratch.as_ptr();
        assert!(response_capacity >= resp.len());
        assert!(dev.gpu.response_scratch.is_empty());

        let resp = submit_control(&mut dev, &mut mem, &get, 24 + 160);
        assert_eq!(
            read_le_u32(&resp, 0),
            Some(virtio_gpu_3d::VIRTIO_GPU_RESP_OK_CAPSET)
        );
        assert_eq!(dev.gpu.response_scratch.capacity(), response_capacity);
        assert_eq!(dev.gpu.response_scratch.as_ptr(), response_ptr);
        assert!(dev.gpu.response_scratch.is_empty());
    }

    #[test]
    fn legacy_virgl_commands_route_through_common_backing_and_control_queue() {
        let (mut dev, backend) = dev_with_mock();
        let mut mem = TestMem::new(0x4000_0000, 0x30000);

        let mut create = ctrl_req_ctx(VIRTIO_GPU_CMD_RESOURCE_CREATE_3D, 0);
        for field in [31u32, 2, 1, 0x402, 320, 200, 1, 1, 0, 0, 0, 0] {
            create.extend_from_slice(&field.to_le_bytes());
        }
        assert_eq!(
            read_le_u32(&submit_control(&mut dev, &mut mem, &create, 24), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );

        let mut backing = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING);
        backing.extend_from_slice(&31u32.to_le_bytes());
        backing.extend_from_slice(&1u32.to_le_bytes());
        backing.extend_from_slice(&0x4002_0000u64.to_le_bytes());
        backing.extend_from_slice(&0x1000u32.to_le_bytes());
        backing.extend_from_slice(&0u32.to_le_bytes());
        assert_eq!(
            read_le_u32(&submit_control(&mut dev, &mut mem, &backing, 24), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );

        assert_eq!(
            read_le_u32(
                &submit_control(&mut dev, &mut mem, &ctx_create_req(7, 0, b""), 24),
                0
            ),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );
        let mut attach = ctrl_req_ctx(VIRTIO_GPU_CMD_CTX_ATTACH_RESOURCE, 7);
        attach.extend_from_slice(&31u32.to_le_bytes());
        attach.extend_from_slice(&0u32.to_le_bytes());
        assert_eq!(
            read_le_u32(&submit_control(&mut dev, &mut mem, &attach, 24), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );

        let mut transfer = ctrl_req_ctx(VIRTIO_GPU_CMD_TRANSFER_TO_HOST_3D, 7);
        for field in [0u32, 0, 0, 32, 16, 1] {
            transfer.extend_from_slice(&field.to_le_bytes());
        }
        transfer.extend_from_slice(&0u64.to_le_bytes());
        for field in [31u32, 0, 128, 2048] {
            transfer.extend_from_slice(&field.to_le_bytes());
        }
        assert_eq!(
            read_le_u32(&submit_control(&mut dev, &mut mem, &transfer, 24), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );

        let inner = backend.lock().unwrap();
        assert_eq!(inner.created_3d.len(), 1);
        assert_eq!(inner.created_3d[0].resource_id, 31);
        assert_eq!(inner.backing_attached, vec![(31, 1, 0x1000)]);
        assert_eq!(inner.attached, vec![(7, 31)]);
        assert_eq!(inner.transfers_3d.len(), 1);
        assert!(inner.transfers_3d[0].1);
        assert_eq!(inner.transfers_3d[0].0.resource_id, 31);
    }

    #[test]
    fn legacy_virgl_3d_resource_can_drive_cpu_scanout_on_flush() {
        let (mut dev, backend) = dev_with_mock();
        let mut mem = TestMem::new(0x4000_0000, 0x30000);

        let mut create = ctrl_req_ctx(VIRTIO_GPU_CMD_RESOURCE_CREATE_3D, 0);
        for field in [
            31u32,
            2,
            FORMAT_B8G8R8A8_UNORM,
            0x8a,
            1920,
            1080,
            1,
            1,
            0,
            1,
            0,
            0,
        ] {
            create.extend_from_slice(&field.to_le_bytes());
        }
        assert_eq!(
            read_le_u32(&submit_control(&mut dev, &mut mem, &create, 24), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );

        let mut set_scanout = ctrl_req(VIRTIO_GPU_CMD_SET_SCANOUT);
        for field in [0u32, 0, 1280, 800, 0, 31] {
            set_scanout.extend_from_slice(&field.to_le_bytes());
        }
        assert_eq!(
            read_le_u32(&submit_control(&mut dev, &mut mem, &set_scanout, 24), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );

        let mut flush = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_FLUSH);
        for field in [0u32, 0, 1280, 800, 31, 0] {
            flush.extend_from_slice(&field.to_le_bytes());
        }
        assert_eq!(
            read_le_u32(&submit_control(&mut dev, &mut mem, &flush, 24), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );
        assert_eq!(backend.lock().unwrap().scanout_reads, vec![(31, 1280, 800)]);
        let stats = dev.stats();
        assert_eq!(stats.scanout_3d_flushes, 1);
        assert_eq!(stats.scanout_readback_attempts, 1);
        assert_eq!(stats.scanout_readbacks, 1);
        assert_eq!(stats.scanout_readback_throttled, 0);
        assert_eq!(stats.scanout_readback_bytes, 1280 * 800 * 4);
        let scanout = dev.gpu.scanout().expect("3D scanout should be active");
        assert_eq!(&scanout.bytes[..8], &[0, 1, 2, 3, 4, 5, 6, 7]);
    }

    #[test]
    fn smaller_legacy_3d_scanout_uses_resource_dimensions_and_display_stride() {
        let (mut dev, backend) = dev_with_mock();
        dev.gpu = VirtioGpu::with_3d_backend(6, 4, Box::new(backend.clone()));
        let mut mem = TestMem::new(0x4000_0000, 0x30000);

        let mut create = ctrl_req_ctx(VIRTIO_GPU_CMD_RESOURCE_CREATE_3D, 0);
        for field in [
            31u32,
            2,
            FORMAT_B8G8R8A8_UNORM,
            0x8a,
            4,
            3,
            1,
            1,
            0,
            1,
            0,
            0,
        ] {
            create.extend_from_slice(&field.to_le_bytes());
        }
        assert_eq!(
            read_le_u32(&submit_control(&mut dev, &mut mem, &create, 24), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );

        let mut set_scanout = ctrl_req(VIRTIO_GPU_CMD_SET_SCANOUT);
        for field in [0u32, 0, 4, 3, 0, 31] {
            set_scanout.extend_from_slice(&field.to_le_bytes());
        }
        assert_eq!(
            read_le_u32(&submit_control(&mut dev, &mut mem, &set_scanout, 24), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );

        let mut flush = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_FLUSH);
        for field in [0u32, 0, 4, 3, 31, 0] {
            flush.extend_from_slice(&field.to_le_bytes());
        }
        assert_eq!(
            read_le_u32(&submit_control(&mut dev, &mut mem, &flush, 24), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );

        assert_eq!(backend.lock().unwrap().scanout_reads, vec![(31, 4, 3)]);
        let scanout = dev.gpu.scanout().expect("3D scanout should be active");
        assert_eq!(&scanout.bytes[..16], &(0u8..16).collect::<Vec<_>>());
        assert_eq!(&scanout.bytes[16..24], &[0; 8]);
        assert_eq!(&scanout.bytes[24..40], &(16u8..32).collect::<Vec<_>>());
        assert_eq!(&scanout.bytes[40..48], &[0; 8]);
        assert_eq!(&scanout.bytes[72..], &[0; 24]);
        let stats = dev.stats();
        assert_eq!(stats.scanout_readback_attempts, 1);
        assert_eq!(stats.scanout_readbacks, 1);
        assert_eq!(stats.scanout_readback_bytes, 4 * 3 * 4);
    }

    #[test]
    fn venus_wddm_primary_uses_guest_backing_with_dual_renderer_backend() {
        let (mut dev, backend) = dev_with_mock();
        let mut mem = TestMem::new(0x4000_0000, 0x50_0000);

        let mut create = ctrl_req_ctx(VIRTIO_GPU_CMD_RESOURCE_CREATE_3D, 0);
        for field in [
            31u32,
            2,
            FORMAT_B8G8R8A8_UNORM,
            0x4008a,
            1024,
            768,
            1,
            1,
            0,
            0,
            0,
            0,
        ] {
            create.extend_from_slice(&field.to_le_bytes());
        }
        assert_eq!(
            read_le_u32(&submit_control(&mut dev, &mut mem, &create, 24), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );

        let mut ctx_attach = ctrl_req_ctx(VIRTIO_GPU_CMD_CTX_ATTACH_RESOURCE, 3);
        ctx_attach.extend_from_slice(&31u32.to_le_bytes());
        ctx_attach.extend_from_slice(&0u32.to_le_bytes());
        assert_eq!(
            read_le_u32(&submit_control(&mut dev, &mut mem, &ctx_attach, 24), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );

        let backing_addr = 0x4010_0000u64;
        let backing_len = 1024u32 * 768 * 4;
        let mut attach = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING);
        attach.extend_from_slice(&31u32.to_le_bytes());
        attach.extend_from_slice(&1u32.to_le_bytes());
        attach.extend_from_slice(&backing_addr.to_le_bytes());
        attach.extend_from_slice(&backing_len.to_le_bytes());
        attach.extend_from_slice(&0u32.to_le_bytes());
        assert_eq!(
            read_le_u32(&submit_control(&mut dev, &mut mem, &attach, 24), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );
        mem.write(backing_addr, &[1, 2, 3, 4, 5, 6, 7, 8]);

        let mut set_scanout = ctrl_req(VIRTIO_GPU_CMD_SET_SCANOUT);
        for field in [0u32, 0, 1024, 768, 0, 31] {
            set_scanout.extend_from_slice(&field.to_le_bytes());
        }
        assert_eq!(
            read_le_u32(&submit_control(&mut dev, &mut mem, &set_scanout, 24), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );

        let flush = flush_req(
            31,
            Rect {
                x: 0,
                y: 0,
                width: 2,
                height: 1,
            },
        );
        assert_eq!(
            read_le_u32(&submit_control(&mut dev, &mut mem, &flush, 24), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );

        let inner = backend.lock().unwrap();
        assert!(inner.created_3d.is_empty());
        assert!(inner.attached.is_empty());
        assert!(inner.backing_attached.is_empty());
        assert!(inner.scanout_reads.is_empty());
        drop(inner);
        let scanout = dev
            .gpu
            .scanout()
            .expect("local 3D scanout should be active");
        assert_eq!(&scanout.bytes[..8], &[1, 2, 3, 0, 5, 6, 7, 0]);
        let stats = dev.stats();
        assert_eq!(stats.scanout_3d_flushes, 1);
        assert_eq!(stats.scanout_readbacks, 1);
        assert_eq!(stats.scanout_readback_bytes, 8);
    }

    #[test]
    fn legacy_virgl_scanout_readback_can_be_throttled_to_display_pacing() {
        let (mut dev, backend) = dev_with_mock();
        let mut mem = TestMem::new(0x4000_0000, 0x30000);

        let mut create = ctrl_req_ctx(VIRTIO_GPU_CMD_RESOURCE_CREATE_3D, 0);
        for field in [
            31u32,
            2,
            FORMAT_B8G8R8A8_UNORM,
            0x8a,
            1280,
            800,
            1,
            1,
            0,
            1,
            0,
            0,
        ] {
            create.extend_from_slice(&field.to_le_bytes());
        }
        submit_control(&mut dev, &mut mem, &create, 24);
        let mut set_scanout = ctrl_req(VIRTIO_GPU_CMD_SET_SCANOUT);
        for field in [0u32, 0, 1280, 800, 0, 31] {
            set_scanout.extend_from_slice(&field.to_le_bytes());
        }
        submit_control(&mut dev, &mut mem, &set_scanout, 24);
        dev.gpu
            .set_3d_scanout_readback_interval(Duration::from_secs(60));

        let mut flush = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_FLUSH);
        for field in [0u32, 0, 1280, 800, 31, 0] {
            flush.extend_from_slice(&field.to_le_bytes());
        }
        submit_control(&mut dev, &mut mem, &flush, 24);
        submit_control(&mut dev, &mut mem, &flush, 24);

        assert_eq!(backend.lock().unwrap().scanout_reads, vec![(31, 1280, 800)]);
        let stats = dev.stats();
        assert_eq!(stats.scanout_3d_flushes, 2);
        assert_eq!(stats.scanout_readback_attempts, 1);
        assert_eq!(stats.scanout_readbacks, 1);
        assert_eq!(stats.scanout_readback_throttled, 1);
        assert_eq!(stats.scanout_readback_bytes, 1280 * 800 * 4);
    }

    fn deferred_scanout_dev() -> (VirtioPciGpu, Arc<Mutex<MockBackend>>, TestMem) {
        let (mut dev, backend) = dev_with_mock();
        let mut mem = TestMem::new(0x4000_0000, 0x30000);
        let mut create = ctrl_req_ctx(VIRTIO_GPU_CMD_RESOURCE_CREATE_3D, 0);
        for field in [
            31u32,
            2,
            FORMAT_B8G8R8A8_UNORM,
            0x8a,
            1280,
            800,
            1,
            1,
            0,
            1,
            0,
            0,
        ] {
            create.extend_from_slice(&field.to_le_bytes());
        }
        submit_control(&mut dev, &mut mem, &create, 24);
        let mut set_scanout = ctrl_req(VIRTIO_GPU_CMD_SET_SCANOUT);
        for field in [0u32, 0, 1280, 800, 0, 31] {
            set_scanout.extend_from_slice(&field.to_le_bytes());
        }
        submit_control(&mut dev, &mut mem, &set_scanout, 24);
        dev.gpu.set_3d_scanout_deferred(true);
        (dev, backend, mem)
    }

    fn flush_res_31() -> Vec<u8> {
        let mut flush = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_FLUSH);
        for field in [0u32, 0, 1280, 800, 31, 0] {
            flush.extend_from_slice(&field.to_le_bytes());
        }
        flush
    }

    #[test]
    fn deferred_scanout_moves_readback_off_the_flush_path() {
        let (mut dev, backend, mut mem) = deferred_scanout_dev();

        let resp = submit_control(&mut dev, &mut mem, &flush_res_31(), 24);
        assert_eq!(read_le_u32(&resp, 0), Some(VIRTIO_GPU_RESP_OK_NODATA));
        // Flush responded OK without any backend readback.
        assert!(backend.lock().unwrap().scanout_reads.is_empty());

        // The drain pass of the arming exit skips (fresh guard)...
        dev.gpu.service_deferred_3d_scanout();
        assert!(backend.lock().unwrap().scanout_reads.is_empty());
        // ...and the next drain pass services it.
        dev.gpu.service_deferred_3d_scanout();
        assert_eq!(backend.lock().unwrap().scanout_reads, vec![(31, 1280, 800)]);

        let stats = dev.stats();
        assert_eq!(stats.deferred_scanout_flushes, 1);
        assert_eq!(stats.deferred_scanout_serviced, 1);
        assert_eq!(stats.scanout_readbacks, 1);
        assert_eq!(stats.scanout_readback_throttled, 0);
    }

    #[test]
    fn deferred_scanout_coalesces_flushes_into_one_readback() {
        let (mut dev, backend, mut mem) = deferred_scanout_dev();

        submit_control(&mut dev, &mut mem, &flush_res_31(), 24);
        submit_control(&mut dev, &mut mem, &flush_res_31(), 24);
        submit_control(&mut dev, &mut mem, &flush_res_31(), 24);

        dev.gpu.service_deferred_3d_scanout();
        dev.gpu.service_deferred_3d_scanout();
        assert_eq!(backend.lock().unwrap().scanout_reads.len(), 1);

        let stats = dev.stats();
        assert_eq!(stats.deferred_scanout_flushes, 3);
        assert_eq!(stats.deferred_scanout_serviced, 1);
    }

    #[test]
    fn deferred_scanout_holds_pending_when_pacing_not_due_instead_of_dropping() {
        let (mut dev, backend, mut mem) = deferred_scanout_dev();

        // First flush services immediately (no prior readback).
        submit_control(&mut dev, &mut mem, &flush_res_31(), 24);
        dev.gpu.service_deferred_3d_scanout();
        dev.gpu.service_deferred_3d_scanout();
        assert_eq!(backend.lock().unwrap().scanout_reads.len(), 1);

        // With a long pacing interval, the next flush stays pending —
        // not dropped, and not counted as throttled.
        dev.gpu
            .set_3d_scanout_readback_interval(Duration::from_secs(60));
        // Re-arm pacing state: interval setter clears last-readback, so
        // perform one readback to start the pacing window.
        submit_control(&mut dev, &mut mem, &flush_res_31(), 24);
        dev.gpu.service_deferred_3d_scanout();
        dev.gpu.service_deferred_3d_scanout();
        assert_eq!(backend.lock().unwrap().scanout_reads.len(), 2);

        submit_control(&mut dev, &mut mem, &flush_res_31(), 24);
        dev.gpu.service_deferred_3d_scanout();
        dev.gpu.service_deferred_3d_scanout();
        dev.gpu.service_deferred_3d_scanout();
        assert_eq!(backend.lock().unwrap().scanout_reads.len(), 2);
        assert_eq!(dev.stats().scanout_readback_throttled, 0);

        // Dropping the pacing interval lets the held frame service.
        dev.gpu
            .set_3d_scanout_readback_interval(Duration::ZERO);
        dev.gpu.service_deferred_3d_scanout();
        assert_eq!(backend.lock().unwrap().scanout_reads.len(), 3);
        assert_eq!(dev.stats().deferred_scanout_serviced, 3);
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
        let pending_capacity = dev.gpu.pending_fenced.capacity();
        let pending_ptr = dev.gpu.pending_fenced.as_ptr();
        let parked_desc_capacity = dev.gpu.pending_fenced[0].descs.capacity();
        let parked_response_capacity = dev.gpu.pending_fenced[0].response.capacity();
        assert!(pending_capacity >= 1);
        assert!(parked_desc_capacity >= 2);
        assert!(parked_response_capacity >= 24);
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
        assert_eq!(dev.gpu.pending_fenced.capacity(), pending_capacity);
        assert_eq!(dev.gpu.pending_fenced.as_ptr(), pending_ptr);
        let completed_capacity = dev.gpu.completed_fences_scratch.capacity();
        let completed_ptr = dev.gpu.completed_fences_scratch.as_ptr();
        assert!(completed_capacity >= 1);

        backend.lock().unwrap().completed.push(CompletedFence {
            ctx_id: 1,
            ring_idx: 3,
            fence_id: 42,
        });
        dev.drain_completed_fences(&mut mem);
        assert_eq!(dev.stats().three_d.fences_pending, 0);
        assert_eq!(dev.gpu.pending_fenced.capacity(), pending_capacity);
        assert_eq!(dev.gpu.pending_fenced.as_ptr(), pending_ptr);
        assert_eq!(
            dev.gpu.completed_fences_scratch.capacity(),
            completed_capacity
        );
        assert_eq!(dev.gpu.completed_fences_scratch.as_ptr(), completed_ptr);
        assert!(dev.gpu.descriptor_scratch.capacity() >= parked_desc_capacity);
        assert!(dev.gpu.response_scratch.capacity() >= parked_response_capacity);
        assert!(dev.gpu.response_scratch.is_empty());
        assert_eq!(
            u16::from_le_bytes(mem.read(0x4000_3000 + 2, 2).try_into().unwrap()),
            2
        );
    }

    #[test]
    fn completed_fence_buffers_pool_reuses_multiple_parked_responses() {
        let (mut dev, backend) = dev_with_mock();
        let mut mem = TestMem::new(0x4000_0000, 0x40000);
        let _ = submit_control(&mut dev, &mut mem, &ctx_create_req(1, 4, b"ctx"), 24);

        let mut req1 = ctrl_req_fenced(VIRTIO_GPU_CMD_SUBMIT_3D, 1, 3, 42);
        req1.extend_from_slice(&0u32.to_le_bytes());
        req1.extend_from_slice(&0u32.to_le_bytes());
        let mut req2 = ctrl_req_fenced(VIRTIO_GPU_CMD_SUBMIT_3D, 1, 3, 43);
        req2.extend_from_slice(&0u32.to_le_bytes());
        req2.extend_from_slice(&0u32.to_le_bytes());

        let (_resp, used_idx) = submit_control_readable_descs_at(
            &mut dev,
            &mut mem,
            &[&req1],
            24,
            0x4000_1000,
            0x4000_4000,
            0x4000_9000,
        );
        assert_eq!(used_idx, 1);
        let (_resp, used_idx) = submit_control_readable_descs_at(
            &mut dev,
            &mut mem,
            &[&req2],
            24,
            0x4000_1400,
            0x4000_6000,
            0x4000_a000,
        );
        assert_eq!(used_idx, 1);
        assert_eq!(dev.stats().three_d.fences_pending, 2);

        let parked_desc_ptrs = [
            dev.gpu.pending_fenced[0].descs.as_ptr(),
            dev.gpu.pending_fenced[1].descs.as_ptr(),
        ];
        let parked_response_ptrs = [
            dev.gpu.pending_fenced[0].response.as_ptr(),
            dev.gpu.pending_fenced[1].response.as_ptr(),
        ];

        backend.lock().unwrap().completed.extend([
            CompletedFence {
                ctx_id: 1,
                ring_idx: 3,
                fence_id: 42,
            },
            CompletedFence {
                ctx_id: 1,
                ring_idx: 3,
                fence_id: 43,
            },
        ]);
        dev.drain_completed_fences(&mut mem);
        assert_eq!(dev.stats().three_d.fences_pending, 0);
        assert_eq!(dev.gpu.parked_descriptor_scratch.len(), 1);
        assert_eq!(dev.gpu.parked_response_scratch.len(), 1);

        let mut req3 = ctrl_req_fenced(VIRTIO_GPU_CMD_SUBMIT_3D, 1, 3, 44);
        req3.extend_from_slice(&0u32.to_le_bytes());
        req3.extend_from_slice(&0u32.to_le_bytes());
        let mut req4 = ctrl_req_fenced(VIRTIO_GPU_CMD_SUBMIT_3D, 1, 3, 45);
        req4.extend_from_slice(&0u32.to_le_bytes());
        req4.extend_from_slice(&0u32.to_le_bytes());

        let (_resp, used_idx) = submit_control_readable_descs_at(
            &mut dev,
            &mut mem,
            &[&req3],
            24,
            0x4000_1800,
            0x4000_8000,
            0x4000_b000,
        );
        assert_eq!(used_idx, 3);
        let (_resp, used_idx) = submit_control_readable_descs_at(
            &mut dev,
            &mut mem,
            &[&req4],
            24,
            0x4000_1c00,
            0x4000_c000,
            0x4000_d000,
        );
        assert_eq!(used_idx, 3);
        assert_eq!(dev.stats().three_d.fences_pending, 2);

        let reused_desc_ptrs = [
            dev.gpu.pending_fenced[0].descs.as_ptr(),
            dev.gpu.pending_fenced[1].descs.as_ptr(),
        ];
        let reused_response_ptrs = [
            dev.gpu.pending_fenced[0].response.as_ptr(),
            dev.gpu.pending_fenced[1].response.as_ptr(),
        ];
        for ptr in parked_desc_ptrs {
            assert!(reused_desc_ptrs.contains(&ptr));
        }
        for ptr in parked_response_ptrs {
            assert!(reused_response_ptrs.contains(&ptr));
        }
        assert!(dev.gpu.parked_descriptor_scratch.is_empty());
        assert!(dev.gpu.parked_response_scratch.is_empty());
    }

    #[test]
    fn rejected_fence_completes_immediately_without_pending_response() {
        let (mut dev, backend) = dev_with_mock();
        backend.lock().unwrap().reject_fence_ring = Some(3);
        let mut mem = TestMem::new(0x4000_0000, 0x30000);
        let _ = submit_control(&mut dev, &mut mem, &ctx_create_req(1, 4, b"ctx"), 24);
        let mut req = ctrl_req_fenced(VIRTIO_GPU_CMD_SUBMIT_3D, 1, 3, 43);
        req.extend_from_slice(&0u32.to_le_bytes());
        req.extend_from_slice(&0u32.to_le_bytes());

        let (resp, used_idx) = submit_control_readable_descs(&mut dev, &mut mem, &[&req], 24);

        assert_eq!(used_idx, 2);
        assert_eq!(dev.stats().three_d.fences_pending, 0);
        assert_eq!(read_le_u32(&resp, 0), Some(VIRTIO_GPU_RESP_OK_NODATA));
        assert_eq!(
            backend.lock().unwrap().fences,
            vec![CompletedFence {
                ctx_id: 1,
                ring_idx: 3,
                fence_id: 43,
            }]
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

    #[test]
    fn controlq_drains_when_driver_never_writes_queue_size_with_3d_backend() {
        // Reproduces the EDK2 VirtioGpuDxe boot hang: firmware programs the rings
        // and enables the control queue but never writes COMMON_QUEUE_SIZE, so the
        // device's stored size stays at its reset value of 0 even though it reports
        // QUEUE_MAX on read. The control queue must still drain at the advertised
        // maximum; otherwise GET_DISPLAY_INFO never completes and the guest hangs.
        let (mut dev, _backend) = dev_with_mock();
        let mut mem = TestMem::new(0x4000_0000, 0x20000);

        let desc = 0x4000_1000;
        let avail = 0x4000_2000;
        let used = 0x4000_3000;
        let req = 0x4000_4000;
        let resp = 0x4000_5000;

        // Enable the queue the way firmware does: rings + enable, no size write.
        pci_write(&mut dev, COMMON_QUEUE_SELECT, 2, 0, &mut mem);
        pci_write(&mut dev, COMMON_QUEUE_DESC, 8, desc, &mut mem);
        pci_write(&mut dev, COMMON_QUEUE_DRIVER, 8, avail, &mut mem);
        pci_write(&mut dev, COMMON_QUEUE_DEVICE, 8, used, &mut mem);
        pci_write(&mut dev, COMMON_QUEUE_ENABLE, 2, 1, &mut mem);

        // The device advertises the max size but has recorded nothing internally.
        assert_eq!(
            pci_read(&mut dev, COMMON_QUEUE_SIZE, 2, &mut mem),
            u64::from(QUEUE_MAX)
        );
        assert_eq!(dev.stats().queues[0].size, 0);

        // GET_DISPLAY_INFO: readable request desc chained to a writable response.
        let request = ctrl_req(VIRTIO_GPU_CMD_GET_DISPLAY_INFO);
        let display_info_len = 24 + 16 * 24;
        mem.write(req, &request);
        write_desc(&mut mem, desc, 0, req, request.len() as u32, DESC_F_NEXT, 1);
        write_desc(&mut mem, desc, 1, resp, display_info_len, DESC_F_WRITE, 0);
        mem.write(avail + 2, &1u16.to_le_bytes());
        mem.write(avail + 4, &0u16.to_le_bytes());
        pci_write(&mut dev, PCI_NOTIFY_CFG_OFFSET, 4, 0, &mut mem);

        // Used ring advanced, response written, and the used-buffer interrupt set.
        assert_eq!(
            u16::from_le_bytes(mem.read(used + 2, 2).try_into().unwrap()),
            1
        );
        let response = mem.read(resp, 24);
        assert_eq!(
            read_le_u32(&response, 0),
            Some(VIRTIO_GPU_RESP_OK_DISPLAY_INFO)
        );
        assert!(dev.interrupt_line_level());

        // A second bring-up command on the same queue also completes.
        let mut create = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_CREATE_2D);
        create.extend_from_slice(&1u32.to_le_bytes());
        create.extend_from_slice(&FORMAT_B8G8R8X8_UNORM.to_le_bytes());
        create.extend_from_slice(&64u32.to_le_bytes());
        create.extend_from_slice(&64u32.to_le_bytes());
        mem.write(req, &create);
        write_desc(&mut mem, desc, 2, req, create.len() as u32, DESC_F_NEXT, 3);
        write_desc(&mut mem, desc, 3, resp, 24, DESC_F_WRITE, 0);
        mem.write(avail + 2, &2u16.to_le_bytes());
        mem.write(avail + 4 + 2, &2u16.to_le_bytes());
        pci_write(&mut dev, PCI_NOTIFY_CFG_OFFSET, 4, 0, &mut mem);

        assert_eq!(
            u16::from_le_bytes(mem.read(used + 2, 2).try_into().unwrap()),
            2
        );
        let response = mem.read(resp, 24);
        assert_eq!(read_le_u32(&response, 0), Some(VIRTIO_GPU_RESP_OK_NODATA));
        assert_eq!(dev.stats().resources, 1);
    }

    #[test]
    fn fenced_2d_bringup_command_completes_immediately_with_3d_backend() {
        // Firmware sets VIRTIO_GPU_FLAG_FENCE on its 2D bring-up commands. With the
        // 3D backend attached those must still complete on the used ring in the
        // same notify (they are synchronous), rather than being parked behind a
        // backend fence that no context would retire.
        let (mut dev, backend) = dev_with_mock();
        let mut mem = TestMem::new(0x4000_0000, 0x20000);
        let req = ctrl_req_fenced(VIRTIO_GPU_CMD_GET_DISPLAY_INFO, 0, 0, 7);
        let (resp, used_idx) =
            submit_control_readable_descs(&mut dev, &mut mem, &[&req], 24 + 16 * 24);
        assert_eq!(used_idx, 1);
        assert_eq!(read_le_u32(&resp, 0), Some(VIRTIO_GPU_RESP_OK_DISPLAY_INFO));
        assert_eq!(dev.stats().three_d.fences_pending, 0);
        // A 2D command must not have been handed to the backend as a fence.
        assert!(backend.lock().unwrap().fences.is_empty());
    }

    #[test]
    fn fenced_resource_create_3d_completes_without_context_zero_fence() {
        let (mut dev, backend) = dev_with_mock();
        let mut mem = TestMem::new(0x4000_0000, 0x20000);
        let mut req = ctrl_req_fenced(VIRTIO_GPU_CMD_RESOURCE_CREATE_3D, 0, 0, 8);
        for field in [41u32, 2, 1, 0x402, 640, 480, 1, 1, 0, 0, 0, 0] {
            req.extend_from_slice(&field.to_le_bytes());
        }

        let (resp, used_idx) = submit_control_readable_descs(&mut dev, &mut mem, &[&req], 24);

        assert_eq!(used_idx, 1);
        assert_eq!(read_le_u32(&resp, 0), Some(VIRTIO_GPU_RESP_OK_NODATA));
        assert_eq!(dev.stats().three_d.fences_pending, 0);
        assert!(backend.lock().unwrap().fences.is_empty());
    }

    #[test]
    fn fenced_pre_context_local_copy_completes_without_renderer_fence() {
        let (mut dev, backend) = dev_with_mock();
        let mut mem = TestMem::new(0x4000_0000, 0x30000);

        for resource_id in [51u32, 52] {
            let mut create = ctrl_req_ctx(VIRTIO_GPU_CMD_RESOURCE_CREATE_3D, 0);
            for field in [
                resource_id,
                2,
                FORMAT_B8G8R8A8_UNORM,
                0x40080,
                2,
                2,
                1,
                1,
                0,
                0,
                0,
                0,
            ] {
                create.extend_from_slice(&field.to_le_bytes());
            }
            assert_eq!(
                read_le_u32(&submit_control(&mut dev, &mut mem, &create, 24), 0),
                Some(VIRTIO_GPU_RESP_OK_NODATA)
            );

            let backing_addr = 0x4002_0000 + u64::from(resource_id - 51) * 0x100;
            let mut attach = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING);
            attach.extend_from_slice(&resource_id.to_le_bytes());
            attach.extend_from_slice(&1u32.to_le_bytes());
            attach.extend_from_slice(&backing_addr.to_le_bytes());
            attach.extend_from_slice(&16u32.to_le_bytes());
            attach.extend_from_slice(&0u32.to_le_bytes());
            assert_eq!(
                read_le_u32(&submit_control(&mut dev, &mut mem, &attach, 24), 0),
                Some(VIRTIO_GPU_RESP_OK_NODATA)
            );
        }
        let src_pixels = [1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
        mem.write(0x4002_0100, &src_pixels);

        let mut command = Vec::new();
        for dword in [17u32 | (13 << 16), 51, 0, 0, 0, 0, 52, 0, 0, 0, 0, 2, 2, 1] {
            command.extend_from_slice(&dword.to_le_bytes());
        }
        let mut submit = ctrl_req_fenced(VIRTIO_GPU_CMD_SUBMIT_3D, 4, 0, 91);
        submit.extend_from_slice(&(command.len() as u32).to_le_bytes());
        submit.extend_from_slice(&0u32.to_le_bytes());
        submit.extend_from_slice(&command);

        let (response, used_idx) =
            submit_control_readable_descs(&mut dev, &mut mem, &[&submit], 24);
        assert_eq!(used_idx, 5);
        assert_eq!(read_le_u32(&response, 0), Some(VIRTIO_GPU_RESP_OK_NODATA));
        assert_eq!(mem.read(0x4002_0000, 16), src_pixels);
        assert_eq!(dev.stats().three_d.fences_pending, 0);
        let backend = backend.lock().unwrap();
        assert!(backend.fences.is_empty());
        assert!(backend.submits.is_empty());
    }

    #[test]
    fn host_vblank_pacing_parks_empty_context_zero_submits_and_retires_one_per_interval() {
        let (mut dev, backend) = dev_with_mock();
        let interval = Duration::from_millis(8);
        dev.set_vblank_interval(interval);
        let mut mem = TestMem::new(0x4000_0000, 0x20000);
        let request = submit_3d_req(0, &[]);

        let (_, used_idx) = submit_control_readable_descs_at(
            &mut dev,
            &mut mem,
            &[&request],
            24,
            0x4000_1000,
            0x4000_4000,
            0x4000_9000,
        );
        assert_eq!(used_idx, 0);
        let (_, used_idx) = submit_control_readable_descs_at(
            &mut dev,
            &mut mem,
            &[&request],
            24,
            0x4000_1400,
            0x4000_6000,
            0x4000_a000,
        );
        assert_eq!(used_idx, 0);
        let (_, used_idx) = submit_control_readable_descs_at(
            &mut dev,
            &mut mem,
            &[&request],
            24,
            0x4000_1800,
            0x4000_8000,
            0x4000_b000,
        );
        assert_eq!(used_idx, 0);
        assert_eq!(dev.gpu.pending_vblank.len(), 3);
        assert!(backend.lock().unwrap().submits.is_empty());

        let base = Instant::now();
        dev.gpu.drain_host_vblank_at(&mut mem, base);
        assert_eq!(
            u16::from_le_bytes(mem.read(0x4000_3000 + 2, 2).try_into().unwrap()),
            1
        );
        assert_eq!(dev.stats().vblank_paced_count, 1);
        assert_eq!(
            read_le_u32(&mem.read(0x4000_9000, 24), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );

        // A late poll may retire one missed interval, but never catch up in a
        // burst. A second poll at the same host time remains held off.
        let late = base + interval * 10;
        dev.gpu.drain_host_vblank_at(&mut mem, late);
        dev.gpu.drain_host_vblank_at(&mut mem, late);
        assert_eq!(
            u16::from_le_bytes(mem.read(0x4000_3000 + 2, 2).try_into().unwrap()),
            2
        );
        assert_eq!(dev.stats().vblank_paced_count, 2);
        assert_eq!(dev.gpu.pending_vblank.len(), 1);

        dev.gpu.drain_host_vblank_at(&mut mem, late + interval);
        assert_eq!(
            u16::from_le_bytes(mem.read(0x4000_3000 + 2, 2).try_into().unwrap()),
            3
        );
        assert_eq!(dev.stats().vblank_paced_count, 3);
        assert!(dev.gpu.pending_vblank.is_empty());
    }

    #[test]
    fn host_vblank_wake_state_tracks_parking_and_the_absolute_schedule() {
        let (mut dev, _backend) = dev_with_mock();
        let interval = Duration::from_millis(8);
        dev.set_vblank_interval(interval);
        let wake = std::sync::Arc::new(VblankWakeState::new());
        dev.set_vblank_wake(std::sync::Arc::clone(&wake));
        assert!(!wake.parked());
        assert_eq!(wake.time_to_deadline(Instant::now()), None);

        let mut mem = TestMem::new(0x4000_0000, 0x20000);
        let request = submit_3d_req(0, &[]);
        let (_, used_idx) = submit_control_readable_descs_at(
            &mut dev,
            &mut mem,
            &[&request],
            24,
            0x4000_1000,
            0x4000_4000,
            0x4000_9000,
        );
        assert_eq!(used_idx, 0);
        let (_, used_idx) = submit_control_readable_descs_at(
            &mut dev,
            &mut mem,
            &[&request],
            24,
            0x4000_1400,
            0x4000_6000,
            0x4000_a000,
        );
        assert_eq!(used_idx, 0);

        // Parked with no schedule anchor yet: due immediately so the waker
        // fires and the first retire establishes the anchor.
        assert!(wake.parked());
        assert_eq!(wake.time_to_deadline(Instant::now()), Some(Duration::ZERO));

        let base = Instant::now();
        dev.gpu.drain_host_vblank_at(&mut mem, base);
        assert!(wake.parked());
        assert_eq!(wake.time_to_deadline(base), Some(interval));

        // Retire the second NOP half an interval LATE: the next deadline must
        // come from the absolute schedule (base + 2*interval), not from the
        // late retire time, so wake/drain latency cannot lower the long-run
        // pacing rate.
        let late = base + interval + interval / 2;
        dev.gpu.drain_host_vblank_at(&mut mem, late);
        assert!(!wake.parked());
        assert_eq!(wake.time_to_deadline(late), None);

        // The two earlier retires advanced the used index to 2; the third NOP
        // parks again without adding a used entry.
        let (_, used_idx) = submit_control_readable_descs_at(
            &mut dev,
            &mut mem,
            &[&request],
            24,
            0x4000_1800,
            0x4000_8000,
            0x4000_b000,
        );
        assert_eq!(used_idx, 2);
        assert_eq!(dev.gpu.pending_vblank.len(), 1);
        assert!(wake.parked());
        assert_eq!(
            wake.time_to_deadline(base + interval * 2),
            Some(Duration::ZERO)
        );
        assert_eq!(
            wake.time_to_deadline(base + interval + interval * 3 / 4),
            Some(interval / 4)
        );

        // Device reset drops parked NOPs and must quiesce the waker.
        dev.reset_runtime_state();
        assert!(!wake.parked());
        assert_eq!(wake.time_to_deadline(Instant::now()), None);
    }

    #[test]
    fn fenced_empty_context_zero_submit_completes_without_backend_fence() {
        let (mut dev, backend) = dev_with_mock();
        let mut mem = TestMem::new(0x4000_0000, 0x20000);
        let mut req = ctrl_req_fenced(VIRTIO_GPU_CMD_SUBMIT_3D, 0, 0, 9);
        req.extend_from_slice(&0u32.to_le_bytes());
        req.extend_from_slice(&0u32.to_le_bytes());

        let (resp, used_idx) = submit_control_readable_descs(&mut dev, &mut mem, &[&req], 24);

        assert_eq!(used_idx, 1);
        assert_eq!(read_le_u32(&resp, 0), Some(VIRTIO_GPU_RESP_OK_NODATA));
        assert_eq!(dev.stats().three_d.fences_pending, 0);
        assert!(backend.lock().unwrap().fences.is_empty());
    }
}
