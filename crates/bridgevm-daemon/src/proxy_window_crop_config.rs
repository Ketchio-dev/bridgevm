//! Discovering proxy-window crop config and parsing/clipping window rects from agent JSON.

use anyhow::Result;
use bridgevm_storage::VmStore;
use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

pub(crate) struct ProxyWindowCropConfig {
    pub(crate) artifact_dir: PathBuf,
    pub(crate) framebuffer_rgba_file: PathBuf,
    pub(crate) framebuffer_width: u32,
    pub(crate) framebuffer_height: u32,
    pub(crate) backing_scale: u16,
}

#[derive(Debug, Clone)]
pub(crate) struct ProxyWindowCropTarget {
    pub(crate) id: String,
    pub(crate) title: Option<String>,
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

#[derive(Debug, Clone)]
pub(crate) struct ProxyWindowClippedRect {
    pub(crate) x: u32,
    pub(crate) y: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProxyWindowFramebufferSignature {
    pub(crate) path: PathBuf,
    pub(crate) len: u64,
    pub(crate) modified: Option<SystemTime>,
}

pub(crate) fn runner_arg_path(command: &[String], flag: &str) -> Option<PathBuf> {
    runner_arg_value(command, flag).map(PathBuf::from)
}

pub(crate) fn runner_arg_u32(command: &[String], flag: &str) -> Result<Option<u32>, String> {
    let Some(value) = runner_arg_value(command, flag) else {
        return Ok(None);
    };
    value
        .parse::<u32>()
        .ok()
        .filter(|value| *value > 0)
        .map(Some)
        .ok_or_else(|| format!("{flag} in runner metadata must be a positive u32, got '{value}'"))
}

pub(crate) fn runner_arg_value(command: &[String], flag: &str) -> Option<String> {
    command
        .windows(2)
        .find(|pair| pair[0] == flag)
        .map(|pair| pair[1].clone())
}

pub(crate) fn required_u32_env(name: &str) -> Result<u32, String> {
    let value = std::env::var(name).map_err(|_| {
        format!("{name} must be set when BRIDGEVM_PROXY_WINDOW_FRAMEBUFFER_RGBA_FILE is set")
    })?;
    value
        .parse::<u32>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| format!("{name} must be a positive u32, got '{value}'"))
}

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

impl ProxyWindowCropConfig {
    pub(crate) fn from_env(store: &VmStore, name: &str) -> Result<Option<Self>, String> {
        if let Some(framebuffer_rgba_file) =
            std::env::var_os("BRIDGEVM_PROXY_WINDOW_FRAMEBUFFER_RGBA_FILE")
        {
            let framebuffer_rgba_file = PathBuf::from(framebuffer_rgba_file);
            let framebuffer_width = required_u32_env("BRIDGEVM_PROXY_WINDOW_FRAMEBUFFER_WIDTH")?;
            let framebuffer_height = required_u32_env("BRIDGEVM_PROXY_WINDOW_FRAMEBUFFER_HEIGHT")?;
            return Self::from_parts(
                store,
                name,
                framebuffer_rgba_file,
                framebuffer_width,
                framebuffer_height,
            );
        }
        Self::from_runner_metadata(store, name)
    }

    pub(crate) fn from_runner_metadata(
        store: &VmStore,
        name: &str,
    ) -> Result<Option<Self>, String> {
        let Some(metadata) = store
            .runner_metadata(name)
            .map_err(|error| format!("failed to read runner metadata for {name}: {error}"))?
        else {
            return Ok(None);
        };
        if !metadata
            .command
            .iter()
            .any(|arg| arg == "--apple-vz-display")
        {
            return Ok(None);
        }
        let Some(framebuffer_rgba_file) =
            runner_arg_path(&metadata.command, "--apple-vz-proxy-framebuffer-rgba-file")
        else {
            return Ok(None);
        };
        if !framebuffer_rgba_file.is_file() {
            return Ok(None);
        }
        let framebuffer_width =
            runner_arg_u32(&metadata.command, "--apple-vz-display-width")?.unwrap_or(1280);
        let framebuffer_height =
            runner_arg_u32(&metadata.command, "--apple-vz-display-height")?.unwrap_or(800);
        Self::from_parts(
            store,
            name,
            framebuffer_rgba_file,
            framebuffer_width,
            framebuffer_height,
        )
    }

    pub(crate) fn from_parts(
        store: &VmStore,
        name: &str,
        framebuffer_rgba_file: PathBuf,
        framebuffer_width: u32,
        framebuffer_height: u32,
    ) -> Result<Option<Self>, String> {
        let backing_scale = optional_u16_env("BRIDGEVM_PROXY_WINDOW_BACKING_SCALE")?.unwrap_or(1);
        let artifact_dir = match std::env::var_os("BRIDGEVM_PROXY_WINDOW_ARTIFACT_DIR") {
            Some(path) => PathBuf::from(path),
            None => {
                let (bundle, _) = store
                    .get_vm(name)
                    .map_err(|error| format!("failed to resolve VM bundle for {name}: {error}"))?;
                bundle.join("metadata").join("proxy-windows")
            }
        };

        Ok(Some(Self {
            artifact_dir,
            framebuffer_rgba_file,
            framebuffer_width,
            framebuffer_height,
            backing_scale: backing_scale.max(1),
        }))
    }
}
