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

    // Read on first use and share it across windows. A payload can carry no
    // croppable window at all, so this must not read the framebuffer eagerly.
    let mut framebuffer: Option<Vec<u8>> = None;
    if let Some(serde_json::Value::Array(windows)) = payload.get_mut("windows") {
        let mut targets = HashMap::new();
        for window in windows {
            if let Some(target) =
                attach_proxy_window_crop_artifact(&config, &mut framebuffer, window)?
            {
                targets.insert(target.id.clone(), target);
            }
        }
        backend.proxy_window_crop_targets = targets;
    }
    if let Some(window) = payload.get_mut("window") {
        if let Some(closed_id) = proxy_window_closed_id(window) {
            backend.proxy_window_crop_targets.remove(&closed_id);
        } else if let Some(target) =
            attach_proxy_window_crop_artifact(&config, &mut framebuffer, window)?
        {
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
    framebuffer: &mut Option<Vec<u8>>,
    window: &mut serde_json::Value,
) -> Result<Option<ProxyWindowCropTarget>, String> {
    let Some(target) = proxy_window_crop_target(window) else {
        return Ok(None);
    };
    let pixels = match framebuffer {
        Some(pixels) => pixels,
        none => none.insert(read_proxy_framebuffer(config)?),
    };
    let Some(summary_path) = materialize_proxy_window_crop_target(config, pixels, &target)? else {
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

    let framebuffer = read_proxy_framebuffer(&config)?;
    for target in backend.proxy_window_crop_targets.values() {
        materialize_proxy_window_crop_target(&config, &framebuffer, target)?;
    }
    backend.proxy_window_framebuffer_signature = Some(signature);
    Ok(())
}

pub(crate) fn materialize_proxy_window_crop_target(
    config: &ProxyWindowCropConfig,
    framebuffer: &[u8],
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
    materialize_proxy_window_crop(config, framebuffer, &clipped, &rgba_path)?;
    write_proxy_window_crop_summary(config, target, &clipped, &summary_path, &rgba_path)?;

    Ok(Some(summary_path))
}

/// Read the whole guest framebuffer once, validated against the configured
/// dimensions.
///
/// Every proxy window crops from the same frame, so this is hoisted out of the
/// per-window path: reading it once per window meant six windows re-read the
/// same 8 MiB six times. Measured at 5.04 ms against 0.97 ms, later at 2.38
/// against 0.53: the absolute cost tracks page-cache warmth, the ratio does not.
pub(crate) fn read_proxy_framebuffer(config: &ProxyWindowCropConfig) -> Result<Vec<u8>, String> {
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
    Ok(framebuffer)
}

pub(crate) fn materialize_proxy_window_crop(
    config: &ProxyWindowCropConfig,
    framebuffer: &[u8],
    clipped: &ProxyWindowClippedRect,
    output_path: &Path,
) -> Result<(), String> {
    let expected = rgba_byte_len(config.framebuffer_width, config.framebuffer_height)?;
    if framebuffer.len() != expected {
        return Err(format!(
            "proxy framebuffer RGBA has {} bytes, expected {}",
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
