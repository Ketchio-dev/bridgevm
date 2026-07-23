//! The VirtioGpu struct, construction and env config, stats snapshot, runtime reset.

use super::*;
use crate::virtio_gpu_3d::CompletedFence;
use crate::virtio_gpu_3d::GpuShmMapPort;
use crate::virtio_gpu_3d::VirtioGpu3d;
use crate::virtio_gpu_3d::VirtioGpu3dBackend;
use crate::virtio_gpu_3d::VirtioGpu3dStats;
use crate::virtio_gpu_trace::VirtioGpuTraceRecorder;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

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

pub(crate) fn parse_resolution_env() -> (u32, u32) {
    let value = std::env::var("BRIDGEVM_VIRTIO_GPU_RES").unwrap_or_else(|_| "1280x800".into());
    parse_resolution(&value).unwrap_or_else(|| {
        panic!("BRIDGEVM_VIRTIO_GPU_RES must be WIDTHxHEIGHT, for example 1600x900")
    })
}

pub(crate) fn parse_resolution(value: &str) -> Option<(u32, u32)> {
    let (width, height) = value.trim().split_once('x')?;
    let width = width.parse::<u32>().ok()?;
    let height = height.parse::<u32>().ok()?;
    (width > 0 && height > 0).then_some((width, height))
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
}
