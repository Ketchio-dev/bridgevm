use clap::{Parser, ValueEnum};
use serde::Serialize;

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
    input_events: Vec<DisplayInputEvent>,
    timing: FrameTimingPlan,
    metal: MetalPlan,
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
    rationale: &'static str,
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
    let cli = Cli::parse();
    let plan = build_display_plan(&cli);

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
}

fn build_display_plan(cli: &Cli) -> DisplayPlan {
    let dirty_regions = effective_dirty_regions(cli);
    let pacing = frame_pacing(cli.visibility, dirty_regions);
    let scale = cli.scale.max(1);
    let width = cli.resize_width.unwrap_or(cli.framebuffer_width);
    let height = cli.resize_height.unwrap_or(cli.framebuffer_height);
    let retina_backing_width = width.saturating_mul(scale.into());
    let retina_backing_height = height.saturating_mul(scale.into());
    let cursor_position = cursor_position(cli, width, height);
    let input_events = input_events(
        cli,
        width,
        height,
        scale,
        retina_backing_width,
        retina_backing_height,
        cursor_position.as_ref(),
    );
    let timing = frame_timing(cli, &pacing);

    DisplayPlan {
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
        input_events,
        timing,
        metal: MetalPlan {
            texture_updates: "deferred-until-dirty",
            presentation_layer: "coreanimation",
            vnc_fallback_allowed: false,
        },
    }
}

fn frame_timing(cli: &Cli, pacing: &FramePacingPlan) -> FrameTimingPlan {
    let frame_budget_micros = if pacing.max_fps == 0 {
        None
    } else {
        Some(1_000_000 / u32::from(pacing.max_fps))
    };
    if cli.sample_frames == 0 || cli.frame_time_micros == 0 {
        return FrameTimingPlan {
            sample_frames: cli.sample_frames,
            average_frame_time_micros: None,
            frame_budget_micros,
            estimated_fps: None,
            within_budget: None,
            source: "metadata-only",
        };
    }

    let estimated_fps = (1_000_000 / cli.frame_time_micros).min(u32::from(u16::MAX)) as u16;
    FrameTimingPlan {
        sample_frames: cli.sample_frames,
        average_frame_time_micros: Some(cli.frame_time_micros),
        frame_budget_micros,
        estimated_fps: Some(estimated_fps),
        within_budget: frame_budget_micros.map(|budget| cli.frame_time_micros <= budget),
        source: "cli-sample",
    }
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

fn frame_pacing(visibility: Visibility, dirty_regions: u16) -> FramePacingPlan {
    match (visibility, dirty_regions) {
        (Visibility::Hidden, _) => FramePacingPlan {
            visibility,
            max_fps: 0,
            idle_fps: 0,
            repaint_when_idle: false,
            rationale: "hidden VMs should not repaint",
        },
        (_, 0) => FramePacingPlan {
            visibility,
            max_fps: 1,
            idle_fps: 0,
            repaint_when_idle: false,
            rationale: "idle guests should not repaint at a fixed refresh rate",
        },
        (Visibility::Background, _) => FramePacingPlan {
            visibility,
            max_fps: 10,
            idle_fps: 0,
            repaint_when_idle: false,
            rationale: "background VMs are throttled for battery and idle CPU",
        },
        (Visibility::Foreground, _) => FramePacingPlan {
            visibility,
            max_fps: 60,
            idle_fps: 0,
            repaint_when_idle: false,
            rationale: "foreground productivity VMs can burst to smooth interactive FPS",
        },
    }
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
        }
    }

    #[test]
    fn foreground_dirty_plan_allows_interactive_fps_without_vnc_fallback() {
        let plan = build_display_plan(&cli(Visibility::Foreground, 3));

        assert_eq!(plan.pacing.max_fps, 60);
        assert_eq!(plan.dirty_regions.update_strategy, "partial-texture-update");
        assert_eq!(plan.framebuffer.retina_backing_width, 2880);
        assert!(plan.cursor.host_cursor_overlay);
        assert!(!plan.metal.vnc_fallback_allowed);
    }

    #[test]
    fn idle_plan_skips_repaints() {
        let plan = build_display_plan(&cli(Visibility::Foreground, 0));

        assert_eq!(plan.pacing.max_fps, 1);
        assert_eq!(plan.pacing.idle_fps, 0);
        assert!(!plan.pacing.repaint_when_idle);
        assert_eq!(plan.dirty_regions.update_strategy, "idle-skip");
    }

    #[test]
    fn background_plan_throttles_frame_rate() {
        let plan = build_display_plan(&cli(Visibility::Background, 12));

        assert_eq!(plan.pacing.max_fps, 10);
        assert_eq!(
            plan.pacing.rationale,
            "background VMs are throttled for battery and idle CPU"
        );
    }

    #[test]
    fn hidden_plan_disables_presentation() {
        let plan = build_display_plan(&cli(Visibility::Hidden, 12));

        assert_eq!(plan.pacing.max_fps, 0);
        assert_eq!(plan.pacing.idle_fps, 0);
        assert_eq!(plan.timing.frame_budget_micros, None);
    }

    #[test]
    fn resize_event_updates_framebuffer_and_marks_dirty() {
        let mut cli = cli(Visibility::Foreground, 0);
        cli.resize_width = Some(1680);
        cli.resize_height = Some(1050);

        let plan = build_display_plan(&cli);

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

        let plan = build_display_plan(&cli);

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
    fn foreground_frame_sample_reports_budget_status() {
        let mut cli = cli(Visibility::Foreground, 8);
        cli.sample_frames = 120;
        cli.frame_time_micros = 16_000;

        let plan = build_display_plan(&cli);

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

        let plan = build_display_plan(&cli);

        assert_eq!(plan.timing.frame_budget_micros, Some(100_000));
        assert_eq!(plan.timing.estimated_fps, Some(6));
        assert_eq!(plan.timing.within_budget, Some(false));
    }

    #[test]
    fn missing_frame_sample_stays_metadata_only() {
        let plan = build_display_plan(&cli(Visibility::Foreground, 4));

        assert_eq!(plan.timing.average_frame_time_micros, None);
        assert_eq!(plan.timing.within_budget, None);
        assert_eq!(plan.timing.source, "metadata-only");
    }
}
