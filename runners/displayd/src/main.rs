use clap::{Parser, ValueEnum};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    process,
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_RUNTIME_POLICY_BYTES: usize = 64 * 1024;
const MAX_FRAME_SAMPLE_BYTES: usize = 1024 * 1024;

#[derive(Debug, Parser)]
#[command(
    name = "displayd",
    about = "BridgeVM Fast Mode display pipeline scaffold"
)]
struct Cli {
    #[arg(long)]
    print_plan: bool,
    #[arg(long, value_enum, default_value_t = Visibility::Foreground)]
    visibility: Visibility,
    #[arg(long, default_value_t = 1)]
    dirty_regions: u16,
    #[arg(long, default_value_t = 1920)]
    framebuffer_width: u32,
    #[arg(long, default_value_t = 1080)]
    framebuffer_height: u32,
    #[arg(long, default_value_t = 2)]
    scale: u16,
    #[arg(long, default_value_t = true)]
    cursor_overlay: bool,
    #[arg(long)]
    resize_width: Option<u32>,
    #[arg(long)]
    resize_height: Option<u32>,
    #[arg(long)]
    cursor_x: Option<u32>,
    #[arg(long)]
    cursor_y: Option<u32>,
    #[arg(long, default_value_t = 0)]
    sample_frames: u32,
    #[arg(long, default_value_t = 0)]
    frame_time_micros: u32,
    #[arg(long)]
    frame_sample_file: Option<PathBuf>,
    #[arg(long, value_name = "PATH")]
    framebuffer_rgba_file: Option<PathBuf>,
    #[arg(long, value_name = "PATH")]
    window_crop_rgba_file: Option<PathBuf>,
    #[arg(long, value_name = "PATH")]
    runtime_policy_file: Option<PathBuf>,
    #[arg(long)]
    window_id: Option<String>,
    #[arg(long)]
    window_title: Option<String>,
    #[arg(long)]
    window_x: Option<i32>,
    #[arg(long)]
    window_y: Option<i32>,
    #[arg(long)]
    window_width: Option<u32>,
    #[arg(long)]
    window_height: Option<u32>,
    #[arg(long)]
    window_host_width: Option<u32>,
    #[arg(long)]
    window_host_height: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
enum Visibility {
    Foreground,
    Background,
    Hidden,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct DisplayPlan {
    pipeline: Vec<&'static str>,
    framebuffer: FramebufferPlan,
    pacing: FramePacingPlan,
    dirty_regions: DirtyRegionPlan,
    cursor: CursorPlan,
    window_region: Option<WindowRegionPlan>,
    window_crop_frame: Option<WindowCropFramePlan>,
    input_events: Vec<DisplayInputEvent>,
    timing: FrameTimingPlan,
    metal: MetalPlan,
    runtime_policy: Option<RuntimePolicyPlan>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct FramebufferPlan {
    width: u32,
    height: u32,
    scale: u16,
    retina_backing_width: u32,
    retina_backing_height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct FramePacingPlan {
    visibility: Visibility,
    max_fps: u16,
    idle_fps: u16,
    repaint_when_idle: bool,
    rationale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct RuntimePolicyPlan {
    path: String,
    visibility: Visibility,
    display_fps_cap: String,
    max_fps_override: Option<u16>,
    source: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct RuntimePolicyFile {
    visibility: RuntimePolicyVisibility,
    display_fps_cap: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum RuntimePolicyVisibility {
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
struct DirtyRegionPlan {
    tracked_regions: u16,
    update_strategy: &'static str,
    full_frame_fallback: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct FrameTimingPlan {
    sample_frames: u32,
    average_frame_time_micros: Option<u32>,
    frame_budget_micros: Option<u32>,
    estimated_fps: Option<u16>,
    within_budget: Option<bool>,
    source: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct CursorPlan {
    host_cursor_overlay: bool,
    render_guest_cursor_in_framebuffer: bool,
    position: Option<CursorPosition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct CursorPosition {
    x: u32,
    y: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct WindowRegionPlan {
    window_id: String,
    title: Option<String>,
    source_rect: SignedRect,
    clipped_rect: UnsignedRect,
    host_size: HostSize,
    backing_rect: UnsignedRect,
    input_mapping: WindowInputMapping,
    presentation: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SignedRect {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct UnsignedRect {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HostSize {
    width: u32,
    height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct WindowInputMapping {
    coordinate_origin: &'static str,
    host_width: u32,
    host_height: u32,
    guest_x: u32,
    guest_y: u32,
    guest_width: u32,
    guest_height: u32,
    scale_x_numerator: u32,
    scale_x_denominator: u32,
    scale_y_numerator: u32,
    scale_y_denominator: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct WindowCropFramePlan {
    source_path: String,
    output_path: String,
    pixel_format: &'static str,
    framebuffer_width: u32,
    framebuffer_height: u32,
    crop_rect: UnsignedRect,
    output_width: u32,
    output_height: u32,
    expected_input_bytes: u64,
    output_bytes: u64,
    source_len_bytes: Option<u64>,
    source_modified_unix_nanos: Option<u64>,
    refreshed_at_unix_nanos: Option<u64>,
    presentation: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
enum DisplayInputEvent {
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
struct MetalPlan {
    texture_updates: &'static str,
    presentation_layer: &'static str,
    vnc_fallback_allowed: bool,
}

fn main() {
    if let Err(message) = run() {
        eprintln!("displayd error: {message}");
        process::exit(1);
    }
}

fn run() -> Result<(), String> {
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

fn build_display_plan(cli: &Cli) -> Result<DisplayPlan, String> {
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

fn runtime_policy_plan(path: Option<&Path>) -> Result<Option<RuntimePolicyPlan>, String> {
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

fn parse_display_fps_cap(value: &str) -> Result<Option<u16>, String> {
    if value.eq_ignore_ascii_case("adaptive") {
        return Ok(None);
    }
    let parsed = value.parse::<u16>().map_err(|_| {
        format!("display_fps_cap must be 'adaptive' or a u16 FPS value, got '{value}'")
    })?;
    Ok(Some(parsed))
}

fn frame_timing(cli: &Cli, pacing: &FramePacingPlan) -> Result<FrameTimingPlan, String> {
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

fn read_frame_sample_file(path: &Path) -> Result<Vec<u32>, String> {
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

fn read_bounded_bytes(path: &Path, limit: usize) -> Result<Vec<u8>, String> {
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

fn read_bounded_utf8(path: &Path, limit: usize) -> Result<String, String> {
    String::from_utf8(read_bounded_bytes(path, limit)?)
        .map_err(|error| format!("file is not valid UTF-8: {error}"))
}

fn mean_u32(values: &[u32]) -> u32 {
    let total: u128 = values.iter().map(|value| u128::from(*value)).sum();
    (total / values.len() as u128).min(u128::from(u32::MAX)) as u32
}

fn effective_dirty_regions(cli: &Cli) -> u16 {
    let mut dirty_regions = cli.dirty_regions;
    if cli.resize_width.is_some() || cli.resize_height.is_some() {
        dirty_regions = dirty_regions.max(1);
    }
    dirty_regions
}

fn cursor_position(cli: &Cli, width: u32, height: u32) -> Option<CursorPosition> {
    let (Some(x), Some(y)) = (cli.cursor_x, cli.cursor_y) else {
        return None;
    };

    Some(CursorPosition {
        x: x.min(width.saturating_sub(1)),
        y: y.min(height.saturating_sub(1)),
    })
}

fn window_region_plan(
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

fn positive_window_dimension(value: Option<u32>, flag: &str) -> Result<u32, String> {
    match value {
        Some(value) if value > 0 => Ok(value),
        Some(_) => Err(format!("{flag} must be positive")),
        None => Err(format!("window region planning requires {flag}")),
    }
}

fn clip_rect_to_framebuffer(
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

fn window_crop_frame_plan(
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
struct FileMetadataSnapshot {
    len_bytes: Option<u64>,
    modified_unix_nanos: Option<u64>,
}

fn file_metadata_snapshot(path: &Path) -> Option<FileMetadataSnapshot> {
    let metadata = fs::metadata(path).ok()?;
    Some(FileMetadataSnapshot {
        len_bytes: Some(metadata.len()),
        modified_unix_nanos: metadata.modified().ok().and_then(system_time_unix_nanos),
    })
}

fn now_unix_nanos() -> Option<u64> {
    system_time_unix_nanos(SystemTime::now())
}

fn system_time_unix_nanos(time: SystemTime) -> Option<u64> {
    let nanos = time.duration_since(UNIX_EPOCH).ok()?.as_nanos();
    u64::try_from(nanos).ok()
}

fn rgba_frame_byte_len_u64(width: u32, height: u32) -> Result<u64, String> {
    u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| format!("RGBA frame dimensions {width}x{height} overflow byte length"))
}

fn rgba_frame_byte_len_usize(width: u32, height: u32) -> Result<usize, String> {
    let len = rgba_frame_byte_len_u64(width, height)?;
    usize::try_from(len)
        .map_err(|_| format!("RGBA frame dimensions {width}x{height} exceed host address space"))
}

fn materialize_window_crop(plan: &WindowCropFramePlan) -> Result<(), String> {
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

fn input_events(
    cli: &Cli,
    width: u32,
    height: u32,
    scale: u16,
    backing_width: u32,
    backing_height: u32,
    cursor_position: Option<&CursorPosition>,
) -> Vec<DisplayInputEvent> {
    let mut events = Vec::new();
    if cli.resize_width.is_some() || cli.resize_height.is_some() {
        events.push(DisplayInputEvent::Resize {
            width,
            height,
            scale,
            backing_width,
            backing_height,
        });
    }
    if let Some(position) = cursor_position {
        events.push(DisplayInputEvent::CursorMoved {
            x: position.x,
            y: position.y,
            overlay: cli.cursor_overlay,
        });
    }
    events
}

fn frame_pacing(
    visibility: Visibility,
    dirty_regions: u16,
    max_fps_override: Option<u16>,
) -> FramePacingPlan {
    let mut plan = match (visibility, dirty_regions) {
        (Visibility::Hidden, _) => FramePacingPlan {
            visibility,
            max_fps: 0,
            idle_fps: 0,
            repaint_when_idle: false,
            rationale: "hidden VMs should not repaint".to_string(),
        },
        (_, 0) => FramePacingPlan {
            visibility,
            max_fps: 1,
            idle_fps: 0,
            repaint_when_idle: false,
            rationale: "idle guests should not repaint at a fixed refresh rate".to_string(),
        },
        (Visibility::Background, _) => FramePacingPlan {
            visibility,
            max_fps: 10,
            idle_fps: 0,
            repaint_when_idle: false,
            rationale: "background VMs are throttled for battery and idle CPU".to_string(),
        },
        (Visibility::Foreground, _) => FramePacingPlan {
            visibility,
            max_fps: 60,
            idle_fps: 0,
            repaint_when_idle: false,
            rationale: "foreground productivity VMs can burst to smooth interactive FPS"
                .to_string(),
        },
    };

    if let Some(cap) = max_fps_override {
        let capped = plan.max_fps.min(cap);
        if capped != plan.max_fps {
            plan.rationale = format!(
                "{}; runtime policy caps display pacing at {cap} FPS",
                plan.rationale
            );
            plan.max_fps = capped;
        }
    }

    plan
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cli(visibility: Visibility, dirty_regions: u16) -> Cli {
        Cli {
            print_plan: true,
            visibility,
            dirty_regions,
            framebuffer_width: 1440,
            framebuffer_height: 900,
            scale: 2,
            cursor_overlay: true,
            resize_width: None,
            resize_height: None,
            cursor_x: None,
            cursor_y: None,
            sample_frames: 0,
            frame_time_micros: 0,
            frame_sample_file: None,
            framebuffer_rgba_file: None,
            window_crop_rgba_file: None,
            runtime_policy_file: None,
            window_id: None,
            window_title: None,
            window_x: None,
            window_y: None,
            window_width: None,
            window_height: None,
            window_host_width: None,
            window_host_height: None,
        }
    }

    #[test]
    fn foreground_dirty_plan_allows_interactive_fps_without_vnc_fallback() {
        let plan = build_display_plan(&cli(Visibility::Foreground, 3)).unwrap();

        assert_eq!(plan.pacing.max_fps, 60);
        assert_eq!(plan.dirty_regions.update_strategy, "partial-texture-update");
        assert_eq!(plan.framebuffer.retina_backing_width, 2880);
        assert!(plan.cursor.host_cursor_overlay);
        assert!(!plan.metal.vnc_fallback_allowed);
    }

    #[test]
    fn idle_plan_skips_repaints() {
        let plan = build_display_plan(&cli(Visibility::Foreground, 0)).unwrap();

        assert_eq!(plan.pacing.max_fps, 1);
        assert_eq!(plan.pacing.idle_fps, 0);
        assert!(!plan.pacing.repaint_when_idle);
        assert_eq!(plan.dirty_regions.update_strategy, "idle-skip");
    }

    #[test]
    fn background_plan_throttles_frame_rate() {
        let plan = build_display_plan(&cli(Visibility::Background, 12)).unwrap();

        assert_eq!(plan.pacing.max_fps, 10);
        assert_eq!(
            plan.pacing.rationale,
            "background VMs are throttled for battery and idle CPU"
        );
    }

    #[test]
    fn runtime_policy_file_overrides_visibility_and_caps_fps() {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "bridgevm-displayd-runtime-policy-{}.json",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(
            &path,
            r#"{
              "vm": "fast-dev",
              "mode": "fast",
              "profile": "automatic",
              "visibility": "background",
              "state": "running",
              "on_battery": true,
              "memory": "2048",
              "cpu": "1",
              "display_fps_cap": "5",
              "rationale": "test policy",
              "live_applied": false,
              "live_apply_blockers": [],
              "updated_at_unix": 1
            }"#,
        )
        .unwrap();
        let mut cli = cli(Visibility::Foreground, 12);
        cli.runtime_policy_file = Some(path.clone());

        let plan = build_display_plan(&cli).unwrap();

        assert_eq!(plan.pacing.visibility, Visibility::Background);
        assert_eq!(plan.pacing.max_fps, 5);
        assert!(plan
            .pacing
            .rationale
            .contains("runtime policy caps display pacing at 5 FPS"));
        assert_eq!(
            plan.runtime_policy,
            Some(RuntimePolicyPlan {
                path: path.display().to_string(),
                visibility: Visibility::Background,
                display_fps_cap: "5".to_string(),
                max_fps_override: Some(5),
                source: "runtime-resources",
            })
        );

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn runtime_policy_file_accepts_adaptive_cap() {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "bridgevm-displayd-runtime-policy-adaptive-{}.json",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(
            &path,
            r#"{"visibility":"foreground","display_fps_cap":"adaptive"}"#,
        )
        .unwrap();
        let mut cli = cli(Visibility::Background, 12);
        cli.runtime_policy_file = Some(path.clone());

        let plan = build_display_plan(&cli).unwrap();

        assert_eq!(plan.pacing.visibility, Visibility::Foreground);
        assert_eq!(plan.pacing.max_fps, 60);
        assert_eq!(
            plan.runtime_policy
                .as_ref()
                .and_then(|policy| policy.max_fps_override),
            None
        );

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn hidden_plan_disables_presentation() {
        let plan = build_display_plan(&cli(Visibility::Hidden, 12)).unwrap();

        assert_eq!(plan.pacing.max_fps, 0);
        assert_eq!(plan.pacing.idle_fps, 0);
        assert_eq!(plan.timing.frame_budget_micros, None);
    }

    #[test]
    fn resize_event_updates_framebuffer_and_marks_dirty() {
        let mut cli = cli(Visibility::Foreground, 0);
        cli.resize_width = Some(1680);
        cli.resize_height = Some(1050);

        let plan = build_display_plan(&cli).unwrap();

        assert_eq!(plan.framebuffer.width, 1680);
        assert_eq!(plan.framebuffer.height, 1050);
        assert_eq!(plan.framebuffer.retina_backing_width, 3360);
        assert_eq!(plan.dirty_regions.tracked_regions, 1);
        assert_eq!(
            plan.input_events,
            vec![DisplayInputEvent::Resize {
                width: 1680,
                height: 1050,
                scale: 2,
                backing_width: 3360,
                backing_height: 2100,
            }]
        );
    }

    #[test]
    fn cursor_event_is_clamped_to_framebuffer_and_uses_overlay() {
        let mut cli = cli(Visibility::Foreground, 1);
        cli.cursor_x = Some(2000);
        cli.cursor_y = Some(950);

        let plan = build_display_plan(&cli).unwrap();

        assert_eq!(
            plan.cursor.position,
            Some(CursorPosition { x: 1439, y: 899 })
        );
        assert_eq!(
            plan.input_events,
            vec![DisplayInputEvent::CursorMoved {
                x: 1439,
                y: 899,
                overlay: true,
            }]
        );
    }

    #[test]
    fn window_region_plan_builds_proxy_crop_contract() {
        let mut cli = cli(Visibility::Foreground, 4);
        cli.window_id = Some("0x01200007".to_string());
        cli.window_title = Some("Terminal".to_string());
        cli.window_x = Some(30);
        cli.window_y = Some(40);
        cli.window_width = Some(800);
        cli.window_height = Some(600);
        cli.window_host_width = Some(400);
        cli.window_host_height = Some(300);

        let plan = build_display_plan(&cli).unwrap();

        assert_eq!(
            plan.window_region,
            Some(WindowRegionPlan {
                window_id: "0x01200007".to_string(),
                title: Some("Terminal".to_string()),
                source_rect: SignedRect {
                    x: 30,
                    y: 40,
                    width: 800,
                    height: 600,
                },
                clipped_rect: UnsignedRect {
                    x: 30,
                    y: 40,
                    width: 800,
                    height: 600,
                },
                host_size: HostSize {
                    width: 400,
                    height: 300,
                },
                backing_rect: UnsignedRect {
                    x: 60,
                    y: 80,
                    width: 1600,
                    height: 1200,
                },
                input_mapping: WindowInputMapping {
                    coordinate_origin: "guest-framebuffer-top-left",
                    host_width: 400,
                    host_height: 300,
                    guest_x: 30,
                    guest_y: 40,
                    guest_width: 800,
                    guest_height: 600,
                    scale_x_numerator: 800,
                    scale_x_denominator: 400,
                    scale_y_numerator: 600,
                    scale_y_denominator: 300,
                },
                presentation: "proxy-window-crop",
            })
        );
    }

    #[test]
    fn window_region_plan_clips_to_framebuffer() {
        let mut cli = cli(Visibility::Foreground, 4);
        cli.window_id = Some("0x02000010".to_string());
        cli.window_x = Some(-20);
        cli.window_y = Some(850);
        cli.window_width = Some(100);
        cli.window_height = Some(100);

        let plan = build_display_plan(&cli).unwrap();
        let region = plan.window_region.unwrap();

        assert_eq!(
            region.source_rect,
            SignedRect {
                x: -20,
                y: 850,
                width: 100,
                height: 100,
            }
        );
        assert_eq!(
            region.clipped_rect,
            UnsignedRect {
                x: 0,
                y: 850,
                width: 80,
                height: 50,
            }
        );
        assert_eq!(
            region.backing_rect,
            UnsignedRect {
                x: 0,
                y: 1700,
                width: 160,
                height: 100,
            }
        );
        assert_eq!(
            region.input_mapping,
            WindowInputMapping {
                coordinate_origin: "guest-framebuffer-top-left",
                host_width: 80,
                host_height: 50,
                guest_x: 0,
                guest_y: 850,
                guest_width: 80,
                guest_height: 50,
                scale_x_numerator: 80,
                scale_x_denominator: 80,
                scale_y_numerator: 50,
                scale_y_denominator: 50,
            }
        );
    }

    #[test]
    fn window_region_plan_requires_complete_intersecting_geometry() {
        let mut missing = cli(Visibility::Foreground, 4);
        missing.window_id = Some("0x01200007".to_string());
        let error = build_display_plan(&missing).unwrap_err();
        assert!(error.contains("--window-x"));

        let mut empty = cli(Visibility::Foreground, 4);
        empty.window_id = Some("0x01200007".to_string());
        empty.window_x = Some(10);
        empty.window_y = Some(10);
        empty.window_width = Some(0);
        empty.window_height = Some(100);
        let error = build_display_plan(&empty).unwrap_err();
        assert!(error.contains("--window-width must be positive"));

        let mut offscreen = cli(Visibility::Foreground, 4);
        offscreen.window_id = Some("0x01200007".to_string());
        offscreen.window_x = Some(2000);
        offscreen.window_y = Some(10);
        offscreen.window_width = Some(100);
        offscreen.window_height = Some(100);
        let error = build_display_plan(&offscreen).unwrap_err();
        assert!(error.contains("does not intersect"));
    }

    #[test]
    fn window_crop_frame_materializes_clipped_rgba_pixels() {
        let mut input_path = std::env::temp_dir();
        input_path.push(format!(
            "bridgevm-displayd-rgba-input-{}.rgba",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut output_path = std::env::temp_dir();
        output_path.push(format!(
            "bridgevm-displayd-rgba-output-{}.rgba",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut frame = Vec::new();
        for y in 0u8..3 {
            for x in 0u8..4 {
                frame.extend_from_slice(&[x, y, y * 4 + x, 255]);
            }
        }
        fs::write(&input_path, &frame).unwrap();

        let mut cli = cli(Visibility::Foreground, 4);
        cli.framebuffer_width = 4;
        cli.framebuffer_height = 3;
        cli.scale = 1;
        cli.window_id = Some("0x01200007".to_string());
        cli.window_x = Some(1);
        cli.window_y = Some(1);
        cli.window_width = Some(2);
        cli.window_height = Some(2);
        cli.framebuffer_rgba_file = Some(input_path.clone());
        cli.window_crop_rgba_file = Some(output_path.clone());

        let plan = build_display_plan(&cli).unwrap();
        let crop_plan = plan.window_crop_frame.as_ref().unwrap();

        assert_eq!(crop_plan.source_path, input_path.display().to_string());
        assert_eq!(crop_plan.output_path, output_path.display().to_string());
        assert_eq!(crop_plan.pixel_format, "rgba8");
        assert_eq!(crop_plan.framebuffer_width, 4);
        assert_eq!(crop_plan.framebuffer_height, 3);
        assert_eq!(
            crop_plan.crop_rect,
            UnsignedRect {
                x: 1,
                y: 1,
                width: 2,
                height: 2,
            }
        );
        assert_eq!(crop_plan.output_width, 2);
        assert_eq!(crop_plan.output_height, 2);
        assert_eq!(crop_plan.expected_input_bytes, 48);
        assert_eq!(crop_plan.output_bytes, 16);
        assert_eq!(crop_plan.source_len_bytes, Some(48));
        assert!(crop_plan.source_modified_unix_nanos.is_some());
        assert!(crop_plan.refreshed_at_unix_nanos.is_some());
        assert_eq!(crop_plan.presentation, "proxy-window-crop-frame");

        materialize_window_crop(crop_plan).unwrap();
        assert_eq!(
            fs::read(&output_path).unwrap(),
            vec![1, 1, 5, 255, 2, 1, 6, 255, 1, 2, 9, 255, 2, 2, 10, 255,]
        );

        fs::remove_file(input_path).unwrap();
        fs::remove_file(output_path).unwrap();
    }

    #[test]
    fn window_crop_frame_requires_source_output_and_window_region() {
        let mut missing_region = cli(Visibility::Foreground, 4);
        missing_region.framebuffer_rgba_file = Some(PathBuf::from("frame.rgba"));
        missing_region.window_crop_rgba_file = Some(PathBuf::from("crop.rgba"));
        let error = build_display_plan(&missing_region).unwrap_err();
        assert!(error.contains("complete --window-* geometry"));

        let mut missing_output = cli(Visibility::Foreground, 4);
        missing_output.window_id = Some("0x01200007".to_string());
        missing_output.window_x = Some(1);
        missing_output.window_y = Some(1);
        missing_output.window_width = Some(2);
        missing_output.window_height = Some(2);
        missing_output.framebuffer_rgba_file = Some(PathBuf::from("frame.rgba"));
        let error = build_display_plan(&missing_output).unwrap_err();
        assert!(error.contains("--window-crop-rgba-file"));

        let mut missing_input = missing_output;
        missing_input.framebuffer_rgba_file = None;
        missing_input.window_crop_rgba_file = Some(PathBuf::from("crop.rgba"));
        let error = build_display_plan(&missing_input).unwrap_err();
        assert!(error.contains("--framebuffer-rgba-file"));
    }

    #[test]
    fn window_crop_frame_rejects_wrong_input_byte_count() {
        let mut input_path = std::env::temp_dir();
        input_path.push(format!(
            "bridgevm-displayd-short-rgba-input-{}.rgba",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut output_path = std::env::temp_dir();
        output_path.push(format!(
            "bridgevm-displayd-short-rgba-output-{}.rgba",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(&input_path, [0u8; 4]).unwrap();

        let plan = WindowCropFramePlan {
            source_path: input_path.display().to_string(),
            output_path: output_path.display().to_string(),
            pixel_format: "rgba8",
            framebuffer_width: 2,
            framebuffer_height: 2,
            crop_rect: UnsignedRect {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            },
            output_width: 1,
            output_height: 1,
            expected_input_bytes: 16,
            output_bytes: 4,
            source_len_bytes: Some(4),
            source_modified_unix_nanos: None,
            refreshed_at_unix_nanos: None,
            presentation: "proxy-window-crop-frame",
        };

        let error = materialize_window_crop(&plan).unwrap_err();
        assert!(error.contains("has 4 bytes, expected 16"));
        assert!(!output_path.exists());

        fs::remove_file(input_path).unwrap();
    }

    #[test]
    fn foreground_frame_sample_reports_budget_status() {
        let mut cli = cli(Visibility::Foreground, 8);
        cli.sample_frames = 120;
        cli.frame_time_micros = 16_000;

        let plan = build_display_plan(&cli).unwrap();

        assert_eq!(plan.timing.sample_frames, 120);
        assert_eq!(plan.timing.average_frame_time_micros, Some(16_000));
        assert_eq!(plan.timing.frame_budget_micros, Some(16_666));
        assert_eq!(plan.timing.estimated_fps, Some(62));
        assert_eq!(plan.timing.within_budget, Some(true));
        assert_eq!(plan.timing.source, "cli-sample");
    }

    #[test]
    fn background_frame_sample_reports_over_budget() {
        let mut cli = cli(Visibility::Background, 4);
        cli.sample_frames = 30;
        cli.frame_time_micros = 150_000;

        let plan = build_display_plan(&cli).unwrap();

        assert_eq!(plan.timing.frame_budget_micros, Some(100_000));
        assert_eq!(plan.timing.estimated_fps, Some(6));
        assert_eq!(plan.timing.within_budget, Some(false));
    }

    #[test]
    fn missing_frame_sample_stays_metadata_only() {
        let plan = build_display_plan(&cli(Visibility::Foreground, 4)).unwrap();

        assert_eq!(plan.timing.average_frame_time_micros, None);
        assert_eq!(plan.timing.within_budget, None);
        assert_eq!(plan.timing.source, "metadata-only");
    }

    #[test]
    fn frame_sample_file_overrides_cli_average_and_reports_derived_timing() {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "bridgevm-displayd-frame-samples-{}.json",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(&path, "[16000,17000,18000]").unwrap();
        let mut cli = cli(Visibility::Foreground, 4);
        cli.sample_frames = 999;
        cli.frame_time_micros = 99_999;
        cli.frame_sample_file = Some(path.clone());

        let plan = build_display_plan(&cli).unwrap();

        assert_eq!(plan.timing.sample_frames, 3);
        assert_eq!(plan.timing.average_frame_time_micros, Some(17_000));
        assert_eq!(plan.timing.frame_budget_micros, Some(16_666));
        assert_eq!(plan.timing.estimated_fps, Some(58));
        assert_eq!(plan.timing.within_budget, Some(false));
        assert_eq!(plan.timing.source, "frame-sample-file");

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn frame_sample_file_rejects_empty_or_zero_duration_samples() {
        let mut empty_path = std::env::temp_dir();
        empty_path.push(format!(
            "bridgevm-displayd-empty-frame-samples-{}.json",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(&empty_path, "[]").unwrap();
        let mut cli = cli(Visibility::Foreground, 4);
        cli.frame_sample_file = Some(empty_path.clone());
        let error = build_display_plan(&cli).unwrap_err();
        assert!(error.contains("at least one duration"));

        let mut zero_path = std::env::temp_dir();
        zero_path.push(format!(
            "bridgevm-displayd-zero-frame-samples-{}.json",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(&zero_path, "[16000,0]").unwrap();
        cli.frame_sample_file = Some(zero_path.clone());
        let error = build_display_plan(&cli).unwrap_err();
        assert!(error.contains("zero duration"));

        fs::remove_file(empty_path).unwrap();
        fs::remove_file(zero_path).unwrap();
    }

    #[test]
    fn bounded_reader_rejects_sparse_oversized_input_before_allocation() {
        let path = std::env::temp_dir().join(format!(
            "bridgevm-displayd-oversized-input-{}-{}.bin",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let file = fs::File::create(&path).unwrap();
        file.set_len(512 * 1024 * 1024).unwrap();

        let error = read_bounded_bytes(&path, 4096).unwrap_err();
        let _ = fs::remove_file(&path);

        assert!(error.contains("536870912 bytes"), "{error}");
        assert!(error.contains("4096 byte limit"), "{error}");
    }
}
