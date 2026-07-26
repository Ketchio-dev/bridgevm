//! IOSurface scanout publication and trace accounting.

use super::*;
use std::fmt::Write as _;
use std::time::Instant;

impl VirtioGpu {
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
        self.record_3d_scanout_blit(resource_id, surface_id, width, height, duration_ns);
    }

    pub(super) fn record_3d_scanout_blit(
        &mut self,
        resource_id: u32,
        surface_id: u32,
        width: u32,
        height: u32,
        duration_ns: u64,
    ) {
        self.scanout_blit_count = self.scanout_blit_count.saturating_add(1);
        self.scanout_blit_nanoseconds = self.scanout_blit_nanoseconds.saturating_add(duration_ns);
        if self.scanout_iosurface_id != Some(surface_id) {
            self.scanout_iosurface_id = Some(surface_id);
            eprintln!("virtio-gpu: scanout IOSurface global id={surface_id} ({width}x{height})");
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
}
