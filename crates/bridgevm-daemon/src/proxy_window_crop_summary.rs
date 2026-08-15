//! The JSON summary that accompanies each proxy-window RGBA crop.
//!
//! Split out of proxy_window_crop_artifacts.rs so the pixel path and the
//! metadata path can each be read on their own.

use crate::*;
use std::fs;
use std::path::Path;

pub(crate) fn write_proxy_window_crop_summary(
    config: &ProxyWindowCropConfig,
    target: &ProxyWindowCropTarget,
    clipped: &ProxyWindowClippedRect,
    summary_path: &Path,
    rgba_path: &Path,
) -> Result<(), String> {
    let scale = u32::from(config.backing_scale.max(1));
    let output_bytes = u64::from(clipped.width) * u64::from(clipped.height) * 4;
    let expected_input_bytes =
        u64::from(config.framebuffer_width) * u64::from(config.framebuffer_height) * 4;
    let framebuffer_metadata = fs::metadata(&config.framebuffer_rgba_file).ok();
    let source_len_bytes = framebuffer_metadata.as_ref().map(|metadata| metadata.len());
    let source_modified_unix_nanos = framebuffer_metadata
        .and_then(|metadata| metadata.modified().ok())
        .and_then(system_time_unix_nanos);
    let summary = serde_json::json!({
        "window_region": {
            "window_id": &target.id,
            "title": &target.title,
            "source_rect": {
                "x": target.x,
                "y": target.y,
                "width": target.width,
                "height": target.height,
            },
            "clipped_rect": {
                "x": clipped.x,
                "y": clipped.y,
                "width": clipped.width,
                "height": clipped.height,
            },
            "host_size": {
                "width": clipped.width,
                "height": clipped.height,
            },
            "backing_rect": {
                "x": clipped.x.saturating_mul(scale),
                "y": clipped.y.saturating_mul(scale),
                "width": clipped.width.saturating_mul(scale),
                "height": clipped.height.saturating_mul(scale),
            },
            "input_mapping": {
                "coordinate_origin": "guest-framebuffer-top-left",
                "host_width": clipped.width,
                "host_height": clipped.height,
                "guest_x": clipped.x,
                "guest_y": clipped.y,
                "guest_width": clipped.width,
                "guest_height": clipped.height,
                "scale_x_numerator": clipped.width,
                "scale_x_denominator": clipped.width,
                "scale_y_numerator": clipped.height,
                "scale_y_denominator": clipped.height,
            },
            "presentation": "proxy-window-crop",
        },
        "window_crop_frame": {
            "source_path": config.framebuffer_rgba_file.display().to_string(),
            "output_path": rgba_path.display().to_string(),
            "pixel_format": "rgba8",
            "framebuffer_width": config.framebuffer_width,
            "framebuffer_height": config.framebuffer_height,
            "crop_rect": {
                "x": clipped.x,
                "y": clipped.y,
                "width": clipped.width,
                "height": clipped.height,
            },
            "output_width": clipped.width,
            "output_height": clipped.height,
            "expected_input_bytes": expected_input_bytes,
            "output_bytes": output_bytes,
            "source_len_bytes": source_len_bytes,
            "source_modified_unix_nanos": source_modified_unix_nanos,
            "refreshed_at_unix_nanos": now_unix_nanos(),
            "presentation": "proxy-window-crop-frame",
        },
    });

    fs::write(
        summary_path,
        serde_json::to_vec_pretty(&summary)
            .map_err(|error| format!("failed to encode proxy window crop summary: {error}"))?,
    )
    .map_err(|error| {
        format!(
            "failed to write proxy window crop summary {}: {error}",
            summary_path.display()
        )
    })
}
