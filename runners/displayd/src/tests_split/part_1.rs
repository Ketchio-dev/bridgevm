//! Split test module.

use super::helpers::*;
use crate::*;
use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

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
