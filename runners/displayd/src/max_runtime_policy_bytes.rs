//! Split out of main.rs to keep files under 800 lines.

use crate::*;
use clap::Parser;
use clap::ValueEnum;
use serde::Deserialize;
use serde::Serialize;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;
use std::process;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

pub(crate) const MAX_RUNTIME_POLICY_BYTES: usize = 64 * 1024;
pub(crate) const MAX_FRAME_SAMPLE_BYTES: usize = 1024 * 1024;

#[derive(Debug, Parser)]
#[command(
    name = "displayd",
    about = "BridgeVM Fast Mode display pipeline scaffold"
)]
pub(crate) struct Cli {
    #[arg(long)]
    pub(crate) print_plan: bool,
    #[arg(long, value_enum, default_value_t = Visibility::Foreground)]
    pub(crate) visibility: Visibility,
    #[arg(long, default_value_t = 1)]
    pub(crate) dirty_regions: u16,
    #[arg(long, default_value_t = 1920)]
    pub(crate) framebuffer_width: u32,
    #[arg(long, default_value_t = 1080)]
    pub(crate) framebuffer_height: u32,
    #[arg(long, default_value_t = 2)]
    pub(crate) scale: u16,
    #[arg(long, default_value_t = true)]
    pub(crate) cursor_overlay: bool,
    #[arg(long)]
    pub(crate) resize_width: Option<u32>,
    #[arg(long)]
    pub(crate) resize_height: Option<u32>,
    #[arg(long)]
    pub(crate) cursor_x: Option<u32>,
    #[arg(long)]
    pub(crate) cursor_y: Option<u32>,
    #[arg(long, default_value_t = 0)]
    pub(crate) sample_frames: u32,
    #[arg(long, default_value_t = 0)]
    pub(crate) frame_time_micros: u32,
    #[arg(long)]
    pub(crate) frame_sample_file: Option<PathBuf>,
    #[arg(long, value_name = "PATH")]
    pub(crate) framebuffer_rgba_file: Option<PathBuf>,
    #[arg(long, value_name = "PATH")]
    pub(crate) window_crop_rgba_file: Option<PathBuf>,
    #[arg(long, value_name = "PATH")]
    pub(crate) runtime_policy_file: Option<PathBuf>,
    #[arg(long)]
    pub(crate) window_id: Option<String>,
    #[arg(long)]
    pub(crate) window_title: Option<String>,
    #[arg(long)]
    pub(crate) window_x: Option<i32>,
    #[arg(long)]
    pub(crate) window_y: Option<i32>,
    #[arg(long)]
    pub(crate) window_width: Option<u32>,
    #[arg(long)]
    pub(crate) window_height: Option<u32>,
    #[arg(long)]
    pub(crate) window_host_width: Option<u32>,
    #[arg(long)]
    pub(crate) window_host_height: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Visibility {
    Foreground,
    Background,
    Hidden,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct DisplayPlan {
    pub(crate) pipeline: Vec<&'static str>,
    pub(crate) framebuffer: FramebufferPlan,
    pub(crate) pacing: FramePacingPlan,
    pub(crate) dirty_regions: DirtyRegionPlan,
    pub(crate) cursor: CursorPlan,
    pub(crate) window_region: Option<WindowRegionPlan>,
    pub(crate) window_crop_frame: Option<WindowCropFramePlan>,
    pub(crate) input_events: Vec<DisplayInputEvent>,
    pub(crate) timing: FrameTimingPlan,
    pub(crate) metal: MetalPlan,
    pub(crate) runtime_policy: Option<RuntimePolicyPlan>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct FramebufferPlan {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) scale: u16,
    pub(crate) retina_backing_width: u32,
    pub(crate) retina_backing_height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct FramePacingPlan {
    pub(crate) visibility: Visibility,
    pub(crate) max_fps: u16,
    pub(crate) idle_fps: u16,
    pub(crate) repaint_when_idle: bool,
    pub(crate) rationale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct RuntimePolicyPlan {
    pub(crate) path: String,
    pub(crate) visibility: Visibility,
    pub(crate) display_fps_cap: String,
    pub(crate) max_fps_override: Option<u16>,
    pub(crate) source: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct RuntimePolicyFile {
    pub(crate) visibility: RuntimePolicyVisibility,
    pub(crate) display_fps_cap: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum RuntimePolicyVisibility {
    Foreground,
    Background,
}

impl From<RuntimePolicyVisibility> for Visibility {
    fn from(value: RuntimePolicyVisibility) -> Self {
        match value {
            RuntimePolicyVisibility::Foreground => Visibility::Foreground,
            RuntimePolicyVisibility::Background => Visibility::Background,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct DirtyRegionPlan {
    pub(crate) tracked_regions: u16,
    pub(crate) update_strategy: &'static str,
    pub(crate) full_frame_fallback: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct FrameTimingPlan {
    pub(crate) sample_frames: u32,
    pub(crate) average_frame_time_micros: Option<u32>,
    pub(crate) frame_budget_micros: Option<u32>,
    pub(crate) estimated_fps: Option<u16>,
    pub(crate) within_budget: Option<bool>,
    pub(crate) source: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct CursorPlan {
    pub(crate) host_cursor_overlay: bool,
    pub(crate) render_guest_cursor_in_framebuffer: bool,
    pub(crate) position: Option<CursorPosition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct CursorPosition {
    pub(crate) x: u32,
    pub(crate) y: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct WindowRegionPlan {
    pub(crate) window_id: String,
    pub(crate) title: Option<String>,
    pub(crate) source_rect: SignedRect,
    pub(crate) clipped_rect: UnsignedRect,
    pub(crate) host_size: HostSize,
    pub(crate) backing_rect: UnsignedRect,
    pub(crate) input_mapping: WindowInputMapping,
    pub(crate) presentation: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct SignedRect {
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct UnsignedRect {
    pub(crate) x: u32,
    pub(crate) y: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct HostSize {
    pub(crate) width: u32,
    pub(crate) height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct WindowInputMapping {
    pub(crate) coordinate_origin: &'static str,
    pub(crate) host_width: u32,
    pub(crate) host_height: u32,
    pub(crate) guest_x: u32,
    pub(crate) guest_y: u32,
    pub(crate) guest_width: u32,
    pub(crate) guest_height: u32,
    pub(crate) scale_x_numerator: u32,
    pub(crate) scale_x_denominator: u32,
    pub(crate) scale_y_numerator: u32,
    pub(crate) scale_y_denominator: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct WindowCropFramePlan {
    pub(crate) source_path: String,
    pub(crate) output_path: String,
    pub(crate) pixel_format: &'static str,
    pub(crate) framebuffer_width: u32,
    pub(crate) framebuffer_height: u32,
    pub(crate) crop_rect: UnsignedRect,
    pub(crate) output_width: u32,
    pub(crate) output_height: u32,
    pub(crate) expected_input_bytes: u64,
    pub(crate) output_bytes: u64,
    pub(crate) source_len_bytes: Option<u64>,
    pub(crate) source_modified_unix_nanos: Option<u64>,
    pub(crate) refreshed_at_unix_nanos: Option<u64>,
    pub(crate) presentation: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub(crate) enum DisplayInputEvent {
    Resize {
        width: u32,
        height: u32,
        scale: u16,
        backing_width: u32,
        backing_height: u32,
    },
    CursorMoved {
        x: u32,
        y: u32,
        overlay: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct MetalPlan {
    pub(crate) texture_updates: &'static str,
    pub(crate) presentation_layer: &'static str,
    pub(crate) vnc_fallback_allowed: bool,
}

pub(crate) fn main_entry() {
    if let Err(message) = run() {
        eprintln!("displayd error: {message}");
        process::exit(1);
    }
}

pub(crate) fn run() -> Result<(), String> {
    let cli = Cli::parse();
    let plan = build_display_plan(&cli)?;
    if let Some(crop_plan) = plan.window_crop_frame.as_ref() {
        materialize_window_crop(crop_plan)?;
    }

    if cli.print_plan {
        println!("{}", serde_json::to_string_pretty(&plan).unwrap());
    } else {
        println!(
            "displayd ready: {}x{}@{}x, {} max fps, {} dirty region(s)",
            plan.framebuffer.width,
            plan.framebuffer.height,
            plan.framebuffer.scale,
            plan.pacing.max_fps,
            plan.dirty_regions.tracked_regions
        );
    }

    Ok(())
}

pub(crate) fn build_display_plan(cli: &Cli) -> Result<DisplayPlan, String> {
    let runtime_policy = runtime_policy_plan(cli.runtime_policy_file.as_deref())?;
    let dirty_regions = effective_dirty_regions(cli);
    let visibility = runtime_policy
        .as_ref()
        .map(|policy| policy.visibility)
        .unwrap_or(cli.visibility);
    let pacing = frame_pacing(
        visibility,
        dirty_regions,
        runtime_policy
            .as_ref()
            .and_then(|policy| policy.max_fps_override),
    );
    let scale = cli.scale.max(1);
    let width = cli.resize_width.unwrap_or(cli.framebuffer_width);
    let height = cli.resize_height.unwrap_or(cli.framebuffer_height);
    let retina_backing_width = width.saturating_mul(scale.into());
    let retina_backing_height = height.saturating_mul(scale.into());
    let cursor_position = cursor_position(cli, width, height);
    let window_region = window_region_plan(cli, width, height, scale)?;
    let window_crop_frame = window_crop_frame_plan(cli, width, height, window_region.as_ref())?;
    let input_events = input_events(
        cli,
        width,
        height,
        scale,
        retina_backing_width,
        retina_backing_height,
        cursor_position.as_ref(),
    );
    let timing = frame_timing(cli, &pacing)?;

    Ok(DisplayPlan {
        pipeline: vec![
            "guest-framebuffer",
            "dirty-region-detection",
            "shared-memory-transport",
            "metal-texture-update",
            "coreanimation-layer",
            "host-cursor-overlay",
            "adaptive-frame-pacing",
        ],
        framebuffer: FramebufferPlan {
            width,
            height,
            scale,
            retina_backing_width,
            retina_backing_height,
        },
        pacing,
        dirty_regions: DirtyRegionPlan {
            tracked_regions: dirty_regions,
            update_strategy: if dirty_regions == 0 {
                "idle-skip"
            } else if dirty_regions <= 32 {
                "partial-texture-update"
            } else {
                "coalesced-texture-update"
            },
            full_frame_fallback: dirty_regions > 128,
        },
        cursor: CursorPlan {
            host_cursor_overlay: cli.cursor_overlay,
            render_guest_cursor_in_framebuffer: !cli.cursor_overlay,
            position: cursor_position,
        },
        window_region,
        window_crop_frame,
        input_events,
        timing,
        metal: MetalPlan {
            texture_updates: "deferred-until-dirty",
            presentation_layer: "coreanimation",
            vnc_fallback_allowed: false,
        },
        runtime_policy,
    })
}

pub(crate) fn runtime_policy_plan(
    path: Option<&Path>,
) -> Result<Option<RuntimePolicyPlan>, String> {
    let Some(path) = path else {
        return Ok(None);
    };
    let content = read_bounded_utf8(path, MAX_RUNTIME_POLICY_BYTES).map_err(|error| {
        format!(
            "failed to read runtime resource policy file '{}': {error}",
            path.display()
        )
    })?;
    let policy: RuntimePolicyFile = serde_json::from_str(&content).map_err(|error| {
        format!(
            "failed to parse runtime resource policy file '{}': {error}",
            path.display()
        )
    })?;
    let max_fps_override = parse_display_fps_cap(&policy.display_fps_cap)?;
    Ok(Some(RuntimePolicyPlan {
        path: path.display().to_string(),
        visibility: policy.visibility.into(),
        display_fps_cap: policy.display_fps_cap,
        max_fps_override,
        source: "runtime-resources",
    }))
}

pub(crate) fn parse_display_fps_cap(value: &str) -> Result<Option<u16>, String> {
    if value.eq_ignore_ascii_case("adaptive") {
        return Ok(None);
    }
    let parsed = value.parse::<u16>().map_err(|_| {
        format!("display_fps_cap must be 'adaptive' or a u16 FPS value, got '{value}'")
    })?;
    Ok(Some(parsed))
}

pub(crate) fn frame_timing(cli: &Cli, pacing: &FramePacingPlan) -> Result<FrameTimingPlan, String> {
    let frame_budget_micros = if pacing.max_fps == 0 {
        None
    } else {
        Some(1_000_000 / u32::from(pacing.max_fps))
    };
    if let Some(path) = &cli.frame_sample_file {
        let samples = read_frame_sample_file(path)?;
        let average_frame_time_micros = mean_u32(&samples);
        let estimated_fps = (1_000_000 / average_frame_time_micros).min(u32::from(u16::MAX)) as u16;
        return Ok(FrameTimingPlan {
            sample_frames: samples.len().min(u32::MAX as usize) as u32,
            average_frame_time_micros: Some(average_frame_time_micros),
            frame_budget_micros,
            estimated_fps: Some(estimated_fps),
            within_budget: frame_budget_micros.map(|budget| average_frame_time_micros <= budget),
            source: "frame-sample-file",
        });
    }
    if cli.sample_frames == 0 || cli.frame_time_micros == 0 {
        return Ok(FrameTimingPlan {
            sample_frames: cli.sample_frames,
            average_frame_time_micros: None,
            frame_budget_micros,
            estimated_fps: None,
            within_budget: None,
            source: "metadata-only",
        });
    }

    let estimated_fps = (1_000_000 / cli.frame_time_micros).min(u32::from(u16::MAX)) as u16;
    Ok(FrameTimingPlan {
        sample_frames: cli.sample_frames,
        average_frame_time_micros: Some(cli.frame_time_micros),
        frame_budget_micros,
        estimated_fps: Some(estimated_fps),
        within_budget: frame_budget_micros.map(|budget| cli.frame_time_micros <= budget),
        source: "cli-sample",
    })
}

pub(crate) fn read_frame_sample_file(path: &Path) -> Result<Vec<u32>, String> {
    let content = read_bounded_utf8(path, MAX_FRAME_SAMPLE_BYTES).map_err(|error| {
        format!(
            "failed to read frame sample file '{}': {error}",
            path.display()
        )
    })?;
    let samples: Vec<u32> = serde_json::from_str(&content).map_err(|error| {
        format!(
            "failed to parse frame sample file '{}' as a JSON array of positive integer microsecond durations: {error}",
            path.display()
        )
    })?;
    if samples.is_empty() {
        return Err(format!(
            "frame sample file '{}' must contain at least one duration",
            path.display()
        ));
    }
    if samples.contains(&0) {
        return Err(format!(
            "frame sample file '{}' contains a zero duration; durations must be positive microseconds",
            path.display()
        ));
    }

    Ok(samples)
}

pub(crate) fn read_bounded_bytes(path: &Path, limit: usize) -> Result<Vec<u8>, String> {
    let file = fs::File::open(path).map_err(|error| error.to_string())?;
    let limit_u64 = u64::try_from(limit).map_err(|_| "read limit exceeds u64".to_string())?;
    let metadata_len = file.metadata().map_err(|error| error.to_string())?.len();
    if metadata_len > limit_u64 {
        return Err(format!(
            "file is {metadata_len} bytes, larger than the {limit} byte limit"
        ));
    }
    let read_limit = limit_u64
        .checked_add(1)
        .ok_or_else(|| "read limit is too large".to_string())?;
    let mut bytes = Vec::with_capacity(metadata_len as usize);
    file.take(read_limit)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > limit {
        return Err(format!(
            "file grew beyond the {limit} byte limit while being read"
        ));
    }
    Ok(bytes)
}

pub(crate) fn read_bounded_utf8(path: &Path, limit: usize) -> Result<String, String> {
    String::from_utf8(read_bounded_bytes(path, limit)?)
        .map_err(|error| format!("file is not valid UTF-8: {error}"))
}

pub(crate) fn mean_u32(values: &[u32]) -> u32 {
    let total: u128 = values.iter().map(|value| u128::from(*value)).sum();
    (total / values.len() as u128).min(u128::from(u32::MAX)) as u32
}

pub(crate) fn effective_dirty_regions(cli: &Cli) -> u16 {
    let mut dirty_regions = cli.dirty_regions;
    if cli.resize_width.is_some() || cli.resize_height.is_some() {
        dirty_regions = dirty_regions.max(1);
    }
    dirty_regions
}

pub(crate) fn cursor_position(cli: &Cli, width: u32, height: u32) -> Option<CursorPosition> {
    let (Some(x), Some(y)) = (cli.cursor_x, cli.cursor_y) else {
        return None;
    };

    Some(CursorPosition {
        x: x.min(width.saturating_sub(1)),
        y: y.min(height.saturating_sub(1)),
    })
}

pub(crate) fn window_region_plan(
    cli: &Cli,
    framebuffer_width: u32,
    framebuffer_height: u32,
    scale: u16,
) -> Result<Option<WindowRegionPlan>, String> {
    let has_window_region_input = cli.window_id.is_some()
        || cli.window_title.is_some()
        || cli.window_x.is_some()
        || cli.window_y.is_some()
        || cli.window_width.is_some()
        || cli.window_height.is_some()
        || cli.window_host_width.is_some()
        || cli.window_host_height.is_some();
    if !has_window_region_input {
        return Ok(None);
    }

    let window_id = cli
        .window_id
        .as_ref()
        .filter(|id| !id.trim().is_empty())
        .cloned()
        .ok_or_else(|| "window region planning requires --window-id".to_string())?;
    let x = cli
        .window_x
        .ok_or_else(|| "window region planning requires --window-x".to_string())?;
    let y = cli
        .window_y
        .ok_or_else(|| "window region planning requires --window-y".to_string())?;
    let source_width = positive_window_dimension(cli.window_width, "--window-width")?;
    let source_height = positive_window_dimension(cli.window_height, "--window-height")?;
    let source_rect = SignedRect {
        x,
        y,
        width: source_width,
        height: source_height,
    };
    let clipped_rect =
        clip_rect_to_framebuffer(&source_rect, framebuffer_width, framebuffer_height).ok_or_else(
            || {
                format!(
                    "window region '{}' does not intersect the {}x{} framebuffer",
                    window_id, framebuffer_width, framebuffer_height
                )
            },
        )?;
    let host_width = cli.window_host_width.unwrap_or(clipped_rect.width);
    let host_height = cli.window_host_height.unwrap_or(clipped_rect.height);
    if host_width == 0 {
        return Err("--window-host-width must be positive".to_string());
    }
    if host_height == 0 {
        return Err("--window-host-height must be positive".to_string());
    }
    let scale_u32 = u32::from(scale.max(1));

    Ok(Some(WindowRegionPlan {
        window_id,
        title: cli.window_title.clone(),
        source_rect,
        clipped_rect: clipped_rect.clone(),
        host_size: HostSize {
            width: host_width,
            height: host_height,
        },
        backing_rect: UnsignedRect {
            x: clipped_rect.x.saturating_mul(scale_u32),
            y: clipped_rect.y.saturating_mul(scale_u32),
            width: clipped_rect.width.saturating_mul(scale_u32),
            height: clipped_rect.height.saturating_mul(scale_u32),
        },
        input_mapping: WindowInputMapping {
            coordinate_origin: "guest-framebuffer-top-left",
            host_width,
            host_height,
            guest_x: clipped_rect.x,
            guest_y: clipped_rect.y,
            guest_width: clipped_rect.width,
            guest_height: clipped_rect.height,
            scale_x_numerator: clipped_rect.width,
            scale_x_denominator: host_width,
            scale_y_numerator: clipped_rect.height,
            scale_y_denominator: host_height,
        },
        presentation: "proxy-window-crop",
    }))
}

pub(crate) fn positive_window_dimension(value: Option<u32>, flag: &str) -> Result<u32, String> {
    match value {
        Some(value) if value > 0 => Ok(value),
        Some(_) => Err(format!("{flag} must be positive")),
        None => Err(format!("window region planning requires {flag}")),
    }
}

pub(crate) fn clip_rect_to_framebuffer(
    source_rect: &SignedRect,
    framebuffer_width: u32,
    framebuffer_height: u32,
) -> Option<UnsignedRect> {
    let left = i64::from(source_rect.x);
    let top = i64::from(source_rect.y);
    let right = left + i64::from(source_rect.width);
    let bottom = top + i64::from(source_rect.height);
    let clipped_left = left.max(0);
    let clipped_top = top.max(0);
    let clipped_right = right.min(i64::from(framebuffer_width));
    let clipped_bottom = bottom.min(i64::from(framebuffer_height));
    if clipped_left >= clipped_right || clipped_top >= clipped_bottom {
        return None;
    }

    Some(UnsignedRect {
        x: clipped_left as u32,
        y: clipped_top as u32,
        width: (clipped_right - clipped_left) as u32,
        height: (clipped_bottom - clipped_top) as u32,
    })
}

pub(crate) fn window_crop_frame_plan(
    cli: &Cli,
    framebuffer_width: u32,
    framebuffer_height: u32,
    window_region: Option<&WindowRegionPlan>,
) -> Result<Option<WindowCropFramePlan>, String> {
    let has_crop_input = cli.framebuffer_rgba_file.is_some() || cli.window_crop_rgba_file.is_some();
    if !has_crop_input {
        return Ok(None);
    }

    let source_path = cli
        .framebuffer_rgba_file
        .as_ref()
        .ok_or_else(|| "window crop frame planning requires --framebuffer-rgba-file".to_string())?;
    let output_path = cli
        .window_crop_rgba_file
        .as_ref()
        .ok_or_else(|| "window crop frame planning requires --window-crop-rgba-file".to_string())?;
    let window_region = window_region.ok_or_else(|| {
        "window crop frame planning requires complete --window-* geometry".to_string()
    })?;
    let crop_rect = window_region.clipped_rect.clone();
    let expected_input_bytes = rgba_frame_byte_len_u64(framebuffer_width, framebuffer_height)?;
    let output_bytes = rgba_frame_byte_len_u64(crop_rect.width, crop_rect.height)?;
    let source_metadata = file_metadata_snapshot(source_path);

    Ok(Some(WindowCropFramePlan {
        source_path: source_path.display().to_string(),
        output_path: output_path.display().to_string(),
        pixel_format: "rgba8",
        framebuffer_width,
        framebuffer_height,
        crop_rect: crop_rect.clone(),
        output_width: crop_rect.width,
        output_height: crop_rect.height,
        expected_input_bytes,
        output_bytes,
        source_len_bytes: source_metadata
            .as_ref()
            .and_then(|metadata| metadata.len_bytes),
        source_modified_unix_nanos: source_metadata
            .as_ref()
            .and_then(|metadata| metadata.modified_unix_nanos),
        refreshed_at_unix_nanos: now_unix_nanos(),
        presentation: "proxy-window-crop-frame",
    }))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FileMetadataSnapshot {
    pub(crate) len_bytes: Option<u64>,
    pub(crate) modified_unix_nanos: Option<u64>,
}

pub(crate) fn file_metadata_snapshot(path: &Path) -> Option<FileMetadataSnapshot> {
    let metadata = fs::metadata(path).ok()?;
    Some(FileMetadataSnapshot {
        len_bytes: Some(metadata.len()),
        modified_unix_nanos: metadata.modified().ok().and_then(system_time_unix_nanos),
    })
}

pub(crate) fn now_unix_nanos() -> Option<u64> {
    system_time_unix_nanos(SystemTime::now())
}

pub(crate) fn system_time_unix_nanos(time: SystemTime) -> Option<u64> {
    let nanos = time.duration_since(UNIX_EPOCH).ok()?.as_nanos();
    u64::try_from(nanos).ok()
}

pub(crate) fn rgba_frame_byte_len_u64(width: u32, height: u32) -> Result<u64, String> {
    u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| format!("RGBA frame dimensions {width}x{height} overflow byte length"))
}

pub(crate) fn rgba_frame_byte_len_usize(width: u32, height: u32) -> Result<usize, String> {
    let len = rgba_frame_byte_len_u64(width, height)?;
    usize::try_from(len)
        .map_err(|_| format!("RGBA frame dimensions {width}x{height} exceed host address space"))
}

pub(crate) fn materialize_window_crop(plan: &WindowCropFramePlan) -> Result<(), String> {
    let expected_input_bytes = usize::try_from(plan.expected_input_bytes).map_err(|_| {
        format!(
            "RGBA framebuffer '{}' is too large for this host",
            plan.source_path
        )
    })?;
    let input = read_bounded_bytes(Path::new(&plan.source_path), expected_input_bytes).map_err(
        |error| {
            format!(
                "failed to read RGBA framebuffer '{}': {error}",
                plan.source_path
            )
        },
    )?;
    if input.len() != expected_input_bytes {
        return Err(format!(
            "RGBA framebuffer '{}' has {} bytes, expected {} for {}x{} rgba8",
            plan.source_path,
            input.len(),
            plan.expected_input_bytes,
            plan.framebuffer_width,
            plan.framebuffer_height
        ));
    }

    let output_len = usize::try_from(plan.output_bytes).map_err(|_| {
        format!(
            "RGBA crop '{}' is too large for this host",
            plan.output_path
        )
    })?;
    let mut output = Vec::with_capacity(output_len);
    let row_stride = rgba_frame_byte_len_usize(plan.framebuffer_width, 1)?;
    let crop_row_bytes = rgba_frame_byte_len_usize(plan.crop_rect.width, 1)?;
    let crop_x_bytes = usize::try_from(u64::from(plan.crop_rect.x) * 4)
        .map_err(|_| "window crop x offset exceeds host address space".to_string())?;
    let crop_y = usize::try_from(plan.crop_rect.y)
        .map_err(|_| "window crop y offset exceeds host address space".to_string())?;
    let crop_height = usize::try_from(plan.crop_rect.height)
        .map_err(|_| "window crop height exceeds host address space".to_string())?;

    for row in 0..crop_height {
        let start = (crop_y + row)
            .checked_mul(row_stride)
            .and_then(|offset| offset.checked_add(crop_x_bytes))
            .ok_or_else(|| "window crop byte offset overflowed".to_string())?;
        let end = start
            .checked_add(crop_row_bytes)
            .ok_or_else(|| "window crop row byte range overflowed".to_string())?;
        output.extend_from_slice(input.get(start..end).ok_or_else(|| {
            format!(
                "window crop rect {}x{} at {},{} exceeds RGBA framebuffer {}x{}",
                plan.crop_rect.width,
                plan.crop_rect.height,
                plan.crop_rect.x,
                plan.crop_rect.y,
                plan.framebuffer_width,
                plan.framebuffer_height
            )
        })?);
    }

    fs::write(&plan.output_path, output).map_err(|error| {
        format!(
            "failed to write RGBA window crop '{}': {error}",
            plan.output_path
        )
    })
}
