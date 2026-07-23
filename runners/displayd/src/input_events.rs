//! Split out of main.rs to keep files under 800 lines.

use crate::*;

pub(crate) fn input_events(
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

pub(crate) fn frame_pacing(
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
