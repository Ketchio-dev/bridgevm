//! virtio-gpu runtime: scanout and resolution API, fence and vblank polling, BAR handlers.

use super::*;
use crate::fwcfg::GuestMemoryMut;
use crate::virtio_gpu::VblankWakeState;
use crate::virtio_gpu::VirtioGpuResult;
use crate::virtio_gpu::VirtioGpuScanout;
use crate::virtio_gpu::VirtioGpuStats;
use crate::virtio_gpu::VirtioPciGpu;
use crate::virtio_gpu::VirtioPciGpuOp;
use crate::virtio_gpu_3d::GpuShmMapPort;
use std::sync::Arc;

impl VirtPlatform {
    pub fn virtio_gpu_stats(&self) -> Option<VirtioGpuStats> {
        self.virtio_gpu.as_ref().map(VirtioPciGpu::stats)
    }

    pub fn virtio_gpu_scanout(&self) -> Option<VirtioGpuScanout<'_>> {
        self.virtio_gpu.as_ref().and_then(VirtioPciGpu::scanout)
    }

    /// Wake signal for the host vblank waker thread; `Some` only when
    /// `BRIDGEVM_VBLANK_HZ` pacing is active on the virtio-gpu device.
    pub fn virtio_gpu_vblank_wake(&self) -> Option<Arc<VblankWakeState>> {
        self.virtio_gpu.as_ref().and_then(VirtioPciGpu::vblank_wake)
    }

    /// Request a guest scanout resize. Returns true when the geometry changed
    /// and the DISPLAY event + config-change interrupt were armed; the pending
    /// MSI-X is delivered by the next `drain_pending_msix_into` on the drain
    /// path. Returns false when no GPU is present or the size is a no-op.
    pub fn request_virtio_gpu_resolution(&mut self, width: u32, height: u32) -> bool {
        self.virtio_gpu
            .as_mut()
            .is_some_and(|gpu| gpu.request_display_resolution(width, height))
    }

    /// Current virtio-gpu scanout geometry, if the device is present.
    pub fn virtio_gpu_resolution(&self) -> Option<(u32, u32)> {
        self.virtio_gpu
            .as_ref()
            .map(VirtioPciGpu::display_resolution)
    }

    pub fn set_virtio_gpu_shm_map_port(&mut self, port: Box<dyn GpuShmMapPort>) -> bool {
        let Some(window_size) = self.pcie.virtio_gpu_host_visible_bar_size() else {
            return false;
        };
        let Some(gpu) = self.virtio_gpu.as_mut() else {
            return false;
        };
        gpu.set_shm_map_port(port, window_size);
        true
    }

    pub fn virtio_gpu_host_visible_bar_base(&self) -> Option<u64> {
        self.pcie.virtio_gpu_host_visible_bar_base()
    }

    pub(crate) fn virtio_gpu_msix_access(&mut self, offset: u64, op: MmioOp) -> MmioOutcome {
        let Some(dev) = self.virtio_gpu.as_mut() else {
            return MmioOutcome::KnownUnimplemented("virtio-gpu-pci");
        };
        let is_write = matches!(op, MmioOp::Write { .. });
        let result = match op {
            MmioOp::Read { size } => dev.msix_bar_access(offset, VirtioPciGpuOp::Read { size }),
            MmioOp::Write { size, value } => {
                dev.msix_bar_access(offset, VirtioPciGpuOp::Write { size, value })
            }
        };
        if is_write {
            self.flush_virtio_gpu_pending_msix();
        }
        match result {
            VirtioGpuResult::ReadValue(v) => MmioOutcome::ReadValue(v),
            VirtioGpuResult::WriteAck => MmioOutcome::WriteAck,
        }
    }

    pub(crate) fn virtio_gpu_access(
        &mut self,
        offset: u64,
        op: MmioOp,
        mem: &mut dyn GuestMemoryMut,
    ) -> MmioOutcome {
        let Some(dev) = self.virtio_gpu.as_mut() else {
            return MmioOutcome::KnownUnimplemented("virtio-gpu-pci");
        };
        let result = match op {
            MmioOp::Read { size } => dev.access(offset, VirtioPciGpuOp::Read { size }, mem),
            MmioOp::Write { size, value } => {
                dev.access(offset, VirtioPciGpuOp::Write { size, value }, mem)
            }
        };
        dev.drain_completed_fences(mem);
        self.flush_virtio_gpu_pending_msix();
        match result {
            VirtioGpuResult::ReadValue(v) => MmioOutcome::ReadValue(v),
            VirtioGpuResult::WriteAck => MmioOutcome::WriteAck,
        }
    }

    /// Retire host-paced vblank NOPs and venus fences, then flush their
    /// interrupts without guest MMIO. Completion must not depend on the guest
    /// touching the device: a guest blocked in vkWaitForFences sits in WFI
    /// generating no virtio-gpu accesses, so the per-exit drain path calls this
    /// while timer exits keep it running.
    pub fn poll_virtio_gpu_fences(&mut self, mem: &mut dyn GuestMemoryMut) {
        if let Some(dev) = self.virtio_gpu.as_mut() {
            dev.service_deferred_3d_scanout();
            dev.drain_host_vblank(mem);
            dev.drain_completed_fences(mem);
            self.flush_virtio_gpu_pending_msix();
        }
    }
}
