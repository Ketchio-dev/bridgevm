//! Content signature for an already-completed 3D scanout readback.

use super::*;
use crate::virtio_gpu_3d::ScanoutPresentResult;
use std::fmt::Write as _;

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ScanoutContent {
    pub(crate) nonzero_pixels: u32,
    pub(crate) white_pixels: u32,
    pub(crate) checksum64: u64,
}

/// Summarise only the central 128x128 region. B4's target occupies that region,
/// and scanning 64 KiB after the existing 5.76 MB readback avoids turning a
/// print-only separator into another full-frame cost.
pub(crate) fn scanout_content(pixels: &[u8], width: u32, height: u32) -> ScanoutContent {
    let side = 128usize.min(width as usize).min(height as usize);
    let x0 = (width as usize).saturating_sub(side) / 2;
    let y0 = (height as usize).saturating_sub(side) / 2;
    let stride = width as usize * 4;
    let mut out = ScanoutContent {
        nonzero_pixels: 0,
        white_pixels: 0,
        checksum64: 0xcbf29ce484222325,
    };
    for y in y0..y0 + side {
        let start = y * stride + x0 * 4;
        let Some(row) = pixels.get(start..start + side * 4) else {
            break;
        };
        for pixel in row.chunks_exact(4) {
            out.nonzero_pixels += u32::from(pixel[..3] != [0, 0, 0]);
            out.white_pixels += u32::from(pixel[..3] == [255, 255, 255]);
            for byte in pixel {
                out.checksum64 = (out.checksum64 ^ u64::from(*byte)).wrapping_mul(0x100000001b3);
            }
        }
    }
    out
}

impl VirtioGpu {
    pub(super) fn record_async_readback(
        &mut self,
        request: PresentRequest,
        result: &ScanoutPresentResult,
        content: Option<ScanoutContent>,
    ) {
        self.last_3d_scanout_readback = Some(std::time::Instant::now());
        self.scanout_readback_count = self.scanout_readback_count.saturating_add(1);
        let bytes = u64::from(request.width) * u64::from(request.height) * 4;
        self.scanout_readback_bytes = self.scanout_readback_bytes.saturating_add(bytes);
        let (resource_id, width, height) = (request.resource_id, request.width, request.height);
        let (transfer_ns, count) = (result.readback_duration_ns, self.scanout_readback_count);
        self.record_trace_fields("scanout_readback", |fields| {
            let _ = write!(fields, ",\"resource_id\":{resource_id},\"width\":{width},\"height\":{height},\"bytes\":{bytes},\"duration_ns\":{transfer_ns},\"transfer_ns\":{transfer_ns},\"composite_ns\":0,\"deferred\":1");
            if let Some(content) = content {
                let _ = write!(fields, ",\"center_nonzero_pixels\":{},\"center_white_pixels\":{},\"center_checksum64\":\"{:016x}\"", content.nonzero_pixels, content.white_pixels, content.checksum64);
            }
            let _ = write!(fields, ",\"count\":{count}");
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_separates_black_blue_and_white_centres() {
        let black = vec![0; 128 * 128 * 4];
        assert_eq!(scanout_content(&black, 128, 128).nonzero_pixels, 0);
        let mut blue = black.clone();
        for pixel in blue.chunks_exact_mut(4) {
            pixel[0] = 160;
        }
        let blue = scanout_content(&blue, 128, 128);
        assert_eq!((blue.nonzero_pixels, blue.white_pixels), (16_384, 0));
        let white = scanout_content(&vec![255; 128 * 128 * 4], 128, 128);
        assert_eq!((white.nonzero_pixels, white.white_pixels), (16_384, 16_384));
        assert_ne!(blue.checksum64, white.checksum64);
    }

    #[test]
    fn larger_frame_uses_only_its_central_region() {
        let (width, height) = (160u32, 144u32);
        let mut pixels = vec![0; scanout_len(width, height)];
        for y in 8..136 {
            for x in 16..144 {
                pixels[(y * 160 + x) * 4..(y * 160 + x) * 4 + 3].fill(255);
            }
        }
        let content = scanout_content(&pixels, width, height);
        assert_eq!(
            (content.nonzero_pixels, content.white_pixels),
            (16_384, 16_384)
        );
    }

    #[test]
    fn short_buffer_is_safely_reported_as_empty() {
        assert_eq!(scanout_content(&[], 1600, 900).nonzero_pixels, 0);
    }
}
