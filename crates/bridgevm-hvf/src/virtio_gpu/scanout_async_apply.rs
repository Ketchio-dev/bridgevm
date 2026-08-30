//! Apply a completed asynchronous present to the display and evidence feed.

use super::scanout_readback::full_frame_readback;
use super::*;
use crate::virtio_gpu_3d::ScanoutPresentResult;
use std::fmt::Write as _;
use std::time::Instant;

impl VirtioGpu {
    pub(super) fn apply_async_present(
        &mut self,
        request: PresentRequest,
        rect: Rect,
        result: &ScanoutPresentResult,
    ) {
        if let Some(surface_id) = result.surface_id {
            self.record_3d_scanout_blit(
                request.resource_id,
                surface_id,
                request.width,
                request.height,
                result.blit_duration_ns,
            );
            self.pending_3d_scanout_blitted = true;
        }
        if result.readback_ok != Some(true) {
            return;
        }
        let len = scanout_len(request.width, request.height);
        if self.scanout_readback_scratch.len() < len {
            return;
        }
        let swap_readback =
            full_frame_readback(request.width, request.height, self.width, self.height, rect);
        let composited = swap_readback
            || composite_host_3d_to_scanout(
                &self.scanout_readback_scratch,
                request.width,
                request.height,
                &mut self.scanout,
                self.width,
                self.height,
                rect,
            );
        if !composited {
            return;
        }
        if swap_readback {
            std::mem::swap(&mut self.scanout, &mut self.scanout_readback_scratch);
        }
        self.last_3d_scanout_readback = Some(Instant::now());
        self.scanout_readback_count = self.scanout_readback_count.saturating_add(1);
        let bytes = u64::from(request.width)
            .saturating_mul(u64::from(request.height))
            .saturating_mul(4);
        self.scanout_readback_bytes = self.scanout_readback_bytes.saturating_add(bytes);
        let count = self.scanout_readback_count;
        let (width, height) = (request.width, request.height);
        let resource_id = request.resource_id;
        let transfer_ns = result.readback_duration_ns;
        self.record_trace_fields("scanout_readback", |fields| {
            let _ = write!(
                fields,
                ",\"resource_id\":{resource_id},\"width\":{width},\"height\":{height},\"bytes\":{bytes},\"duration_ns\":{transfer_ns},\"transfer_ns\":{transfer_ns},\"composite_ns\":0,\"deferred\":1,\"count\":{count}"
            );
        });
        self.publish_scanout_fb();
    }
}
