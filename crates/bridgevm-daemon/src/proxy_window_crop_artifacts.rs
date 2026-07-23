//! Attaching and refreshing proxy-window artifacts: RGBA crop extraction and summary JSON.

use crate::*;
use anyhow::Result;
use bridgevm_storage::VmStore;
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;

pub(crate) fn attach_proxy_window_crop_artifacts(
    store: &VmStore,
    name: &str,
    backend: &mut SupervisedBackend,
    result: Option<&mut serde_json::Value>,
) -> Result<(), String> {
    let Some(config) = ProxyWindowCropConfig::from_env(store, name)? else {
        return Ok(());
    };
    let Some(result) = result else {
        return Ok(());
    };
    let Some(payload) = result.as_object_mut() else {
        return Ok(());
    };

    if let Some(serde_json::Value::Array(windows)) = payload.get_mut("windows") {
        let mut targets = HashMap::new();
        for window in windows {
            if let Some(target) = attach_proxy_window_crop_artifact(&config, window)? {
                targets.insert(target.id.clone(), target);
            }
        }
        backend.proxy_window_crop_targets = targets;
    }
    if let Some(window) = payload.get_mut("window") {
        if let Some(closed_id) = proxy_window_closed_id(window) {
            backend.proxy_window_crop_targets.remove(&closed_id);
        } else if let Some(target) = attach_proxy_window_crop_artifact(&config, window)? {
            backend
                .proxy_window_crop_targets
                .insert(target.id.clone(), target);
        }
    }
    backend.proxy_window_framebuffer_signature = Some(proxy_window_framebuffer_signature(&config)?);

    Ok(())
}

pub(crate) fn attach_proxy_window_crop_artifact(
    config: &ProxyWindowCropConfig,
    window: &mut serde_json::Value,
) -> Result<Option<ProxyWindowCropTarget>, String> {
    let Some(target) = proxy_window_crop_target(window) else {
        return Ok(None);
    };
    let Some(summary_path) = materialize_proxy_window_crop_target(config, &target)? else {
        return Ok(None);
    };

    if let Some(map) = window.as_object_mut() {
        map.insert(
            "window_crop_frame_summary_path".to_string(),
            serde_json::Value::String(summary_path.display().to_string()),
        );
    }

    Ok(Some(target))
}

pub(crate) fn refresh_proxy_window_crop_artifacts(
    store: &VmStore,
    name: &str,
    backend: &mut SupervisedBackend,
) -> Result<(), String> {
    if backend.proxy_window_crop_targets.is_empty() {
        return Ok(());
    }
    let Some(config) = ProxyWindowCropConfig::from_env(store, name)? else {
        return Ok(());
    };
    let signature = proxy_window_framebuffer_signature(&config)?;
    if backend.proxy_window_framebuffer_signature.as_ref() == Some(&signature) {
        return Ok(());
    }

    for target in backend.proxy_window_crop_targets.values() {
        materialize_proxy_window_crop_target(&config, target)?;
    }
    backend.proxy_window_framebuffer_signature = Some(signature);
    Ok(())
}

pub(crate) fn materialize_proxy_window_crop_target(
    config: &ProxyWindowCropConfig,
    target: &ProxyWindowCropTarget,
) -> Result<Option<PathBuf>, String> {
    let Some(clipped) =
        clip_proxy_window_crop_target(target, config.framebuffer_width, config.framebuffer_height)
    else {
        return Ok(None);
    };

    let slug = safe_proxy_window_artifact_slug(&target.id);
    let summary_path = config.artifact_dir.join(format!("{slug}.json"));
    let rgba_path = config.artifact_dir.join(format!("{slug}.rgba"));
    fs::create_dir_all(&config.artifact_dir).map_err(|error| {
        format!(
            "failed to create proxy window artifact directory {}: {error}",
            config.artifact_dir.display()
        )
    })?;
    materialize_proxy_window_crop(config, &clipped, &rgba_path)?;
    write_proxy_window_crop_summary(config, target, &clipped, &summary_path, &rgba_path)?;

    Ok(Some(summary_path))
}

pub(crate) fn materialize_proxy_window_crop(
    config: &ProxyWindowCropConfig,
    clipped: &ProxyWindowClippedRect,
    output_path: &Path,
) -> Result<(), String> {
    let expected = rgba_byte_len(config.framebuffer_width, config.framebuffer_height)?;
    if expected > MAX_PROXY_FRAMEBUFFER_BYTES {
        return Err(format!(
            "proxy framebuffer RGBA dimensions {}x{} require {} bytes, exceeding the {}-byte limit",
            config.framebuffer_width,
            config.framebuffer_height,
            expected,
            MAX_PROXY_FRAMEBUFFER_BYTES
        ));
    }
    let read_limit = u64::try_from(expected)
        .ok()
        .and_then(|expected| expected.checked_add(1))
        .ok_or_else(|| "proxy framebuffer RGBA read limit overflowed".to_string())?;
    let mut framebuffer = Vec::new();
    fs::File::open(&config.framebuffer_rgba_file)
        .and_then(|file| file.take(read_limit).read_to_end(&mut framebuffer))
        .map_err(|error| {
            format!(
                "failed to read proxy framebuffer RGBA {}: {error}",
                config.framebuffer_rgba_file.display()
            )
        })?;
    if framebuffer.len() != expected {
        return Err(format!(
            "proxy framebuffer RGBA {} has {} bytes, expected {}",
            config.framebuffer_rgba_file.display(),
            framebuffer.len(),
            expected
        ));
    }

    let framebuffer_row_bytes = rgba_byte_len(config.framebuffer_width, 1)?;
    let crop_row_bytes = rgba_byte_len(clipped.width, 1)?;
    let crop_x_bytes = usize::try_from(u64::from(clipped.x) * 4)
        .map_err(|_| "proxy crop x byte offset exceeds host address space".to_string())?;
    let crop_y = usize::try_from(clipped.y)
        .map_err(|_| "proxy crop y offset exceeds host address space".to_string())?;
    let crop_height = usize::try_from(clipped.height)
        .map_err(|_| "proxy crop height exceeds host address space".to_string())?;
    let mut output = Vec::with_capacity(rgba_byte_len(clipped.width, clipped.height)?);

    for row in 0..crop_height {
        let start = (crop_y + row)
            .checked_mul(framebuffer_row_bytes)
            .and_then(|offset| offset.checked_add(crop_x_bytes))
            .ok_or_else(|| "proxy crop byte offset overflowed".to_string())?;
        let end = start
            .checked_add(crop_row_bytes)
            .ok_or_else(|| "proxy crop row byte range overflowed".to_string())?;
        if end > framebuffer.len() {
            return Err("proxy crop row exceeds framebuffer byte range".to_string());
        }
        output.extend_from_slice(&framebuffer[start..end]);
    }

    fs::write(output_path, output).map_err(|error| {
        format!(
            "failed to write proxy window RGBA crop {}: {error}",
            output_path.display()
        )
    })
}

pub(crate) fn rgba_byte_len(width: u32, height: u32) -> Result<usize, String> {
    let bytes = u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| format!("RGBA byte length overflow for {width}x{height}"))?;
    usize::try_from(bytes)
        .map_err(|_| format!("RGBA byte length for {width}x{height} exceeds host address space"))
}

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

pub(crate) fn safe_proxy_window_artifact_slug(id: &str) -> String {
    let slug = id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    let slug = slug.trim_matches('.');
    if slug.is_empty() {
        "window".to_string()
    } else {
        slug.to_string()
    }
}

pub(crate) const MAX_PROXY_FRAMEBUFFER_BYTES: usize = 256 * 1024 * 1024;
