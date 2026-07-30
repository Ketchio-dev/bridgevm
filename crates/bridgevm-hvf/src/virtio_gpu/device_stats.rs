use crate::virtio_gpu_3d::{VirtioGpu3dStats, VIRTIO_GPU_CMD_RESOURCE_CREATE_3D};

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
    pub resource_create_3d_count: u64,
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

impl VirtioGpu {
    pub(crate) fn record_3d_command(&mut self, command: u32) {
        if command == VIRTIO_GPU_CMD_RESOURCE_CREATE_3D {
            self.resource_create_3d_count = self.resource_create_3d_count.saturating_add(1);
        }
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
            resource_create_3d_count: self.resource_create_3d_count,
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
}
