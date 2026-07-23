//! Split out of main.rs to keep files under 800 lines.

use crate::*;
use anyhow::Context;
use anyhow::Result;
use bridgevm_agentd::AgentSession;
use bridgevm_storage::GuestToolsAgentUpdateMetadata;
use bridgevm_storage::GuestToolsClipboardMetadata;
use bridgevm_storage::GuestToolsCommandResultMetadata;
use bridgevm_storage::GuestToolsIpAddressMetadata;
use bridgevm_storage::GuestToolsMetricsMetadata;
use bridgevm_storage::GuestToolsRuntimeMetadata;
use bridgevm_storage::GuestToolsSharedFolderMetadata;
use bridgevm_storage::VmStore;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

pub(crate) fn optional_u16_env(name: &str) -> Result<Option<u16>, String> {
    let Ok(value) = std::env::var(name) else {
        return Ok(None);
    };
    value
        .parse::<u16>()
        .ok()
        .filter(|value| *value > 0)
        .map(Some)
        .ok_or_else(|| format!("{name} must be a positive u16, got '{value}'"))
}

pub(crate) fn proxy_window_crop_target(
    window: &serde_json::Value,
) -> Option<ProxyWindowCropTarget> {
    let object = window.as_object()?;
    let id = object.get("id")?.as_str()?.trim();
    if id.is_empty() {
        return None;
    }
    let bounds = object.get("bounds")?.as_object()?;
    Some(ProxyWindowCropTarget {
        id: id.to_string(),
        title: object
            .get("title")
            .and_then(|value| value.as_str())
            .filter(|value| !value.trim().is_empty())
            .map(ToOwned::to_owned),
        x: value_as_i32(bounds.get("x")?)?,
        y: value_as_i32(bounds.get("y")?)?,
        width: value_as_u32(bounds.get("width")?)?,
        height: value_as_u32(bounds.get("height")?)?,
    })
}

pub(crate) fn proxy_window_closed_id(window: &serde_json::Value) -> Option<String> {
    let object = window.as_object()?;
    if object.get("closed").and_then(|value| value.as_bool()) != Some(true) {
        return None;
    }
    object
        .get("id")?
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub(crate) fn proxy_window_framebuffer_signature(
    config: &ProxyWindowCropConfig,
) -> Result<ProxyWindowFramebufferSignature, String> {
    let metadata = fs::metadata(&config.framebuffer_rgba_file).map_err(|error| {
        format!(
            "failed to inspect proxy framebuffer RGBA {}: {error}",
            config.framebuffer_rgba_file.display()
        )
    })?;
    Ok(ProxyWindowFramebufferSignature {
        path: config.framebuffer_rgba_file.clone(),
        len: metadata.len(),
        modified: metadata.modified().ok(),
    })
}

pub(crate) fn value_as_i32(value: &serde_json::Value) -> Option<i32> {
    value
        .as_i64()
        .and_then(|value| i32::try_from(value).ok())
        .or_else(|| value.as_str().and_then(|value| value.parse::<i32>().ok()))
}

pub(crate) fn value_as_u32(value: &serde_json::Value) -> Option<u32> {
    value
        .as_u64()
        .and_then(|value| u32::try_from(value).ok())
        .filter(|value| *value > 0)
        .or_else(|| {
            value
                .as_str()
                .and_then(|value| value.parse::<u32>().ok())
                .filter(|value| *value > 0)
        })
}

pub(crate) fn clip_proxy_window_crop_target(
    target: &ProxyWindowCropTarget,
    framebuffer_width: u32,
    framebuffer_height: u32,
) -> Option<ProxyWindowClippedRect> {
    let left = i64::from(target.x);
    let top = i64::from(target.y);
    let right = left.checked_add(i64::from(target.width))?;
    let bottom = top.checked_add(i64::from(target.height))?;
    let clipped_left = left.max(0);
    let clipped_top = top.max(0);
    let clipped_right = right.min(i64::from(framebuffer_width));
    let clipped_bottom = bottom.min(i64::from(framebuffer_height));
    if clipped_left >= clipped_right || clipped_top >= clipped_bottom {
        return None;
    }

    Some(ProxyWindowClippedRect {
        x: clipped_left as u32,
        y: clipped_top as u32,
        width: (clipped_right - clipped_left) as u32,
        height: (clipped_bottom - clipped_top) as u32,
    })
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

pub(crate) enum GuestToolsRuntimeUpdate {
    Connected,
    Heartbeat,
    GuestIp(Vec<GuestToolsIpAddressMetadata>),
    MountShare {
        name: String,
        host_path_token: String,
    },
    UnmountShare {
        name: String,
    },
    Metrics {
        cpu_percent: u8,
        memory_used_mib: u64,
    },
    CommandResult {
        request_id: String,
        capability: Option<String>,
        ok: bool,
        error_code: Option<String>,
        message: Option<String>,
        result: Option<serde_json::Value>,
        metadata: Option<serde_json::Value>,
        completed_at_unix: u64,
    },
    AgentUpdateAvailable {
        current_version: String,
        available_version: String,
        download_url: Option<String>,
        signature: Option<String>,
    },
    Clipboard {
        text: String,
    },
}

pub(crate) fn write_guest_tools_runtime(
    store: &VmStore,
    name: &str,
    session: &AgentSession,
    update: GuestToolsRuntimeUpdate,
) -> Result<()> {
    let now = now_unix();
    let mut metadata = store
        .guest_tools_runtime_metadata(name)
        .context("failed to read guest tools runtime metadata")?
        .unwrap_or_else(|| GuestToolsRuntimeMetadata {
            connected: true,
            guest_os: Some(session.guest_os.clone()),
            agent_version: session.agent_version.clone(),
            capabilities: session
                .capabilities
                .iter()
                .map(|capability| capability.name.clone())
                .collect(),
            last_heartbeat_at_unix: None,
            guest_ip_addresses: Vec::new(),
            shared_folders: Vec::new(),
            metrics: None,
            last_command_result: None,
            agent_update: None,
            clipboard: None,
            updated_at_unix: now,
        });

    metadata.connected = true;
    metadata.guest_os = Some(session.guest_os.clone());
    metadata.agent_version = session.agent_version.clone();
    metadata.capabilities = session
        .capabilities
        .iter()
        .map(|capability| capability.name.clone())
        .collect();
    metadata.updated_at_unix = now;

    match update {
        GuestToolsRuntimeUpdate::Connected => {}
        GuestToolsRuntimeUpdate::Heartbeat => metadata.last_heartbeat_at_unix = Some(now),
        GuestToolsRuntimeUpdate::GuestIp(addresses) => metadata.guest_ip_addresses = addresses,
        GuestToolsRuntimeUpdate::MountShare {
            name,
            host_path_token,
        } => {
            if let Some(folder) = metadata
                .shared_folders
                .iter_mut()
                .find(|folder| folder.name == name)
            {
                folder.host_path_token = host_path_token;
                folder.mounted_at_unix = now;
            } else {
                metadata
                    .shared_folders
                    .push(GuestToolsSharedFolderMetadata {
                        name,
                        host_path_token,
                        mounted_at_unix: now,
                    });
            }
        }
        GuestToolsRuntimeUpdate::UnmountShare { name } => {
            metadata.shared_folders.retain(|folder| folder.name != name);
        }
        GuestToolsRuntimeUpdate::Metrics {
            cpu_percent,
            memory_used_mib,
        } => {
            metadata.metrics = Some(GuestToolsMetricsMetadata {
                cpu_percent,
                memory_used_mib,
                updated_at_unix: now,
            });
        }
        GuestToolsRuntimeUpdate::CommandResult {
            request_id,
            capability,
            ok,
            error_code,
            message,
            result,
            metadata: command_metadata,
            completed_at_unix,
        } => {
            metadata.last_command_result = Some(GuestToolsCommandResultMetadata {
                request_id,
                capability,
                ok,
                error_code,
                message,
                result,
                metadata: command_metadata,
                completed_at_unix,
            });
        }
        GuestToolsRuntimeUpdate::AgentUpdateAvailable {
            current_version,
            available_version,
            download_url,
            signature,
        } => {
            metadata.agent_update = Some(GuestToolsAgentUpdateMetadata {
                current_version,
                available_version,
                download_url,
                signature,
                observed_at_unix: now,
            });
        }
        GuestToolsRuntimeUpdate::Clipboard { text } => {
            metadata.clipboard = Some(GuestToolsClipboardMetadata {
                text,
                updated_at_unix: now,
            });
        }
    }

    store
        .write_guest_tools_runtime_metadata(name, &metadata)
        .context("failed to write guest tools runtime metadata")
}

pub(crate) fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub(crate) fn now_unix_nanos() -> Option<u64> {
    system_time_unix_nanos(SystemTime::now())
}

pub(crate) fn system_time_unix_nanos(time: SystemTime) -> Option<u64> {
    let nanos = time.duration_since(UNIX_EPOCH).ok()?.as_nanos();
    u64::try_from(nanos).ok()
}
