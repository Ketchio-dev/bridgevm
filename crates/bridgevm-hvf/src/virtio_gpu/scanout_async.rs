//! Asynchronous scanout present service.
//!
//! The synchronous path in `scanout_3d.rs` keeps the device thread blocked on
//! the renderer worker for the whole present. This path hands the present off
//! and returns, so the vCPU stops paying renderer latency, then applies the
//! result on a later drain.
//!
//! Enabled by opt-in. With `scanout_async_present` false, nothing here runs
//! and the inline path is used exactly as before, so the synchronous behaviour
//! stays available as a fallback.

use super::*;
use crate::virtio_gpu_3d::ScanoutPresentResult;
use std::fmt::Write as _;
use std::time::Instant;

impl VirtioGpu {
    pub fn set_3d_scanout_async_present(&mut self, enabled: bool) {
        if self.scanout_async_present == enabled {
            return;
        }
        // Never flip modes with work outstanding: the worker owns the readback
        // buffer, so it has to be collected before the path changes.
        self.finish_async_present();
        self.scanout_async_present = enabled;
    }

    /// Invalidate presents computed against a superseded scanout.
    pub(crate) fn bump_present_epoch(&mut self) {
        self.async_present.bump_epoch();
    }

    /// Block until any in-flight present completes, dropping its result.
    ///
    /// Required before the resource or device goes away: the worker is writing
    /// into a buffer it owns, and abandoning it would leave the renderer
    /// touching memory during teardown.
    pub(crate) fn finish_async_present(&mut self) {
        let Some((mut pending, _, _)) = self.async_present_pending.take() else {
            return;
        };
        let collected = self.three_d.scanout_present_poll(&mut pending, true);
        if let Some((_, Some(buffer))) = collected {
            self.scanout_readback_scratch = buffer;
        }
        self.async_present.complete(self.scanout_resource);
    }

    /// Offer the pending deferred frame to the mailbox and dispatch it when
    /// the worker is idle. Returns true when the async path handled the frame.
    pub(crate) fn service_async_present(&mut self, resource_id: u32, rect: Rect) -> bool {
        if !self.scanout_async_present {
            return false;
        }
        self.collect_async_present();
        let Some(info) = self.three_d.scanout_3d_info(resource_id) else {
            return false;
        };
        let width = info.width.min(self.width);
        let height = info.height.min(self.height);
        match self.async_present.offer(resource_id, width, height) {
            PresentAdmission::Dispatch(request) => {
                // A backend with no async path leaves the frame unpresented,
                // so hand it back to the synchronous path rather than dropping
                // it -- silently losing a frame here would stall the display.
                self.dispatch_async_present(request, rect)
            }
            // Already busy: the frame is retained and will be promoted when
            // the in-flight present finishes. Superseded frames are dropped
            // rather than queued, so the depth stays at one.
            PresentAdmission::Retained | PresentAdmission::ReplacedRetained => true,
        }
    }

    /// Returns false when the backend has no asynchronous path, leaving the
    /// caller responsible for presenting the frame synchronously.
    fn dispatch_async_present(&mut self, request: PresentRequest, rect: Rect) -> bool {
        let readback_due = self.scanout_readback_due(Instant::now());
        let readback = readback_due.then(|| {
            let len = scanout_len(request.width, request.height);
            let mut buffer = std::mem::take(&mut self.scanout_readback_scratch);
            buffer.resize(len, 0);
            buffer
        });
        let blit = self.scanout_iosurface;
        match self.three_d.scanout_present_start(
            request.resource_id,
            request.width,
            request.height,
            blit,
            readback,
        ) {
            Some(pending) => {
                self.async_present_pending = Some((pending, request, rect));
                true
            }
            None => {
                // Undo the admission so the synchronous path resumes exactly as
                // if async had never been enabled. The scratch buffer was moved
                // into the refused call and is gone; the synchronous path
                // resizes it before use, so it costs one reallocation on this
                // single transition and nothing afterwards.
                self.async_present.complete(self.scanout_resource);
                self.scanout_async_present = false;
                false
            }
        }
    }

    /// Apply a finished present, if one is ready. Non-blocking.
    pub(crate) fn collect_async_present(&mut self) {
        let Some((mut pending, request, rect)) = self.async_present_pending.take() else {
            return;
        };
        let Some((result, buffer)) = self.three_d.scanout_present_poll(&mut pending, false) else {
            // Still executing: keep ownership so the buffer stays alive.
            self.async_present_pending = Some((pending, request, rect));
            return;
        };
        if let Some(buffer) = buffer {
            self.scanout_readback_scratch = buffer;
        }
        let (cancel, next) = self.async_present.complete(self.scanout_resource);
        if let Some(reason) = cancel {
            self.record_async_present_cancel(request, reason);
        } else {
            self.apply_async_present(request, rect, &result);
        }
        if let Some(next) = next {
            self.dispatch_async_present(next, rect);
        }
    }

    fn apply_async_present(
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
        #[rustfmt::skip]
        let content = self.trace.enabled().then(|| super::scanout_content_trace::scanout_content(&self.scanout_readback_scratch, request.width, request.height));
        if !composite_host_3d_to_scanout(
            &self.scanout_readback_scratch,
            request.width,
            request.height,
            &mut self.scanout,
            self.width,
            self.height,
            rect,
        ) {
            return;
        }
        self.record_async_readback(request, result, content);
        self.publish_scanout_fb();
    }

    fn record_async_present_cancel(
        &mut self,
        request: PresentRequest,
        reason: PresentCancelReason,
    ) {
        let cancelled = self.async_present.cancelled();
        if cancelled > 8 {
            return;
        }
        let resource_id = request.resource_id;
        let reason = match reason {
            PresentCancelReason::StaleEpoch => "stale_epoch",
            PresentCancelReason::ResourceChanged => "resource_changed",
            PresentCancelReason::ResourceGone => "resource_gone",
        };
        self.record_trace_fields("scanout_present_cancelled", |fields| {
            let _ = write!(
                fields,
                ",\"resource_id\":{resource_id},\"reason\":\"{reason}\",\"count\":{cancelled}"
            );
        });
    }
}
