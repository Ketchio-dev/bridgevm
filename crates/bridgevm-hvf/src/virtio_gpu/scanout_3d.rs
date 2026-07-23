//! 3D scanout presentation: GL readback pacing, IOSurface blit and verify, deferred flush.

use super::*;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::time::Duration;
use std::time::Instant;

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

impl VirtioGpu {
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
        let readback_due = self.last_3d_scanout_readback.is_none_or(|last| {
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
                let any_match = matched || matched_flip || matched_swap || matched_flip_swap;
                if !(any_match || self.scanout_iosurface_dumped) {
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
