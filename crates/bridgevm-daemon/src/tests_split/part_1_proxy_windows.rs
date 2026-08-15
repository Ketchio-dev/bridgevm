//! Proxy-window crop artifact tests, split out of part_1.

use super::helpers::*;
use crate::*;
use bridgevm_storage::RunnerMetadata;
use std::env;
use std::fs;
use std::process::Command;
use std::thread;
use std::time::Duration;

#[test]
fn proxy_window_crop_bounds_oversized_framebuffer_reads() {
    let store = temp_store();
    fs::create_dir_all(store.root()).unwrap();
    let framebuffer = store.root().join("oversized-framebuffer.rgba");
    let file = fs::File::create(&framebuffer).unwrap();
    file.set_len(512 * 1024 * 1024).unwrap();
    let output = store.root().join("crop.rgba");
    let mut config = ProxyWindowCropConfig {
        artifact_dir: store.root().join("artifacts"),
        framebuffer_rgba_file: framebuffer,
        framebuffer_width: 1,
        framebuffer_height: 1,
        backing_scale: 1,
    };
    let clipped = ProxyWindowClippedRect {
        x: 0,
        y: 0,
        width: 1,
        height: 1,
    };

    // The size guards live on the read, which now happens once per frame rather
    // than once per window.
    let error = read_proxy_framebuffer(&config).unwrap_err();
    assert!(error.contains("has 5 bytes, expected 4"));
    assert!(!output.exists());

    // A framebuffer that does not match the configured dimensions must still be
    // refused at the crop, so a caller cannot pass a stale or foreign buffer.
    let error = materialize_proxy_window_crop(&config, &[0u8; 8], &clipped, &output).unwrap_err();
    assert!(error.contains("has 8 bytes, expected 4"));
    assert!(!output.exists());

    config.framebuffer_width = 8193;
    config.framebuffer_height = 8193;
    let error = read_proxy_framebuffer(&config).unwrap_err();
    assert!(error.contains("exceeding the 268435456-byte limit"));
}

#[test]
fn proxy_window_crop_refreshes_cached_targets_when_framebuffer_changes() {
    let _env_lock = PROXY_WINDOW_ENV_LOCK.lock().unwrap();
    let _env_guard = EnvVarGuard::capture(&[
        "BRIDGEVM_PROXY_WINDOW_FRAMEBUFFER_RGBA_FILE",
        "BRIDGEVM_PROXY_WINDOW_FRAMEBUFFER_WIDTH",
        "BRIDGEVM_PROXY_WINDOW_FRAMEBUFFER_HEIGHT",
        "BRIDGEVM_PROXY_WINDOW_BACKING_SCALE",
        "BRIDGEVM_PROXY_WINDOW_ARTIFACT_DIR",
    ]);
    let store = temp_store();
    store.create_vm(&compatibility_manifest("legacy")).unwrap();
    let framebuffer = store.root().join("framebuffer.rgba");
    fs::write(&framebuffer, solid_rgba(4, 4, [0x10, 0x20, 0x30, 0xFF])).unwrap();
    env::set_var("BRIDGEVM_PROXY_WINDOW_FRAMEBUFFER_RGBA_FILE", &framebuffer);
    env::set_var("BRIDGEVM_PROXY_WINDOW_FRAMEBUFFER_WIDTH", "4");
    env::set_var("BRIDGEVM_PROXY_WINDOW_FRAMEBUFFER_HEIGHT", "4");
    env::set_var("BRIDGEVM_PROXY_WINDOW_BACKING_SCALE", "2");

    let child = Command::new("sh").arg("-c").arg("sleep 5").spawn().unwrap();
    let mut backend = SupervisedBackend::new(child);
    backend.proxy_window_crop_targets.insert(
        "window-1".to_string(),
        ProxyWindowCropTarget {
            id: "window-1".to_string(),
            title: Some("Terminal".to_string()),
            x: 1,
            y: 1,
            width: 2,
            height: 2,
        },
    );

    refresh_proxy_window_crop_artifacts(&store, "legacy", &mut backend).unwrap();
    let artifact_dir = store
        .bundle_path("legacy")
        .join("metadata")
        .join("proxy-windows");
    let rgba_path = artifact_dir.join("window-1.rgba");
    let summary_path = artifact_dir.join("window-1.json");
    let crop = fs::read(&rgba_path).unwrap();
    assert_eq!(crop.len(), 2 * 2 * 4);
    assert_eq!(&crop[..4], &[0x10, 0x20, 0x30, 0xFF]);

    thread::sleep(Duration::from_millis(20));
    fs::write(&framebuffer, solid_rgba(4, 4, [0xAA, 0xBB, 0xCC, 0xFF])).unwrap();
    refresh_proxy_window_crop_artifacts(&store, "legacy", &mut backend).unwrap();
    let crop = fs::read(&rgba_path).unwrap();
    assert_eq!(&crop[..4], &[0xAA, 0xBB, 0xCC, 0xFF]);

    let summary: serde_json::Value =
        serde_json::from_slice(&fs::read(summary_path).unwrap()).unwrap();
    assert_eq!(
        summary.pointer("/window_region/window_id"),
        Some(&serde_json::Value::String("window-1".to_string()))
    );
    assert_eq!(
        summary.pointer("/window_crop_frame/output_width"),
        Some(&serde_json::Value::Number(2.into()))
    );
    assert_eq!(
        summary.pointer("/window_crop_frame/source_len_bytes"),
        Some(&serde_json::Value::Number(64.into()))
    );
    assert!(summary
        .pointer("/window_crop_frame/source_modified_unix_nanos")
        .and_then(serde_json::Value::as_u64)
        .is_some_and(|value| value > 0));
    assert!(summary
        .pointer("/window_crop_frame/refreshed_at_unix_nanos")
        .and_then(serde_json::Value::as_u64)
        .is_some_and(|value| value > 0));

    backend.child.kill().unwrap();
    backend.child.wait().unwrap();
}

#[test]
fn proxy_window_crop_reads_the_framebuffer_once_for_all_windows() {
    let _env_lock = PROXY_WINDOW_ENV_LOCK.lock().unwrap();
    let _env_guard = EnvVarGuard::capture(&[
        "BRIDGEVM_PROXY_WINDOW_FRAMEBUFFER_RGBA_FILE",
        "BRIDGEVM_PROXY_WINDOW_FRAMEBUFFER_WIDTH",
        "BRIDGEVM_PROXY_WINDOW_FRAMEBUFFER_HEIGHT",
        "BRIDGEVM_PROXY_WINDOW_BACKING_SCALE",
        "BRIDGEVM_PROXY_WINDOW_ARTIFACT_DIR",
    ]);
    let store = temp_store();
    store.create_vm(&compatibility_manifest("legacy")).unwrap();
    let framebuffer = store.root().join("framebuffer.rgba");
    fs::write(&framebuffer, solid_rgba(4, 4, [0x10, 0x20, 0x30, 0xFF])).unwrap();
    env::set_var("BRIDGEVM_PROXY_WINDOW_FRAMEBUFFER_RGBA_FILE", &framebuffer);
    env::set_var("BRIDGEVM_PROXY_WINDOW_FRAMEBUFFER_WIDTH", "4");
    env::set_var("BRIDGEVM_PROXY_WINDOW_FRAMEBUFFER_HEIGHT", "4");
    env::set_var("BRIDGEVM_PROXY_WINDOW_BACKING_SCALE", "2");

    let child = Command::new("sh").arg("-c").arg("sleep 5").spawn().unwrap();
    let mut backend = SupervisedBackend::new(child);
    for id in ["window-1", "window-2", "window-3"] {
        backend.proxy_window_crop_targets.insert(
            id.to_string(),
            ProxyWindowCropTarget {
                id: id.to_string(),
                title: Some(id.to_string()),
                x: 1,
                y: 1,
                width: 2,
                height: 2,
            },
        );
    }

    // Every window is cropped from one frame.
    refresh_proxy_window_crop_artifacts(&store, "legacy", &mut backend).unwrap();
    let artifact_dir = store
        .bundle_path("legacy")
        .join("metadata")
        .join("proxy-windows");
    for id in ["window-1", "window-2", "window-3"] {
        let crop = fs::read(artifact_dir.join(format!("{id}.rgba"))).unwrap();
        assert_eq!(crop.len(), 2 * 2 * 4);
        assert_eq!(&crop[..4], &[0x10, 0x20, 0x30, 0xFF]);
    }

    // A missing framebuffer must fail the whole refresh and leave the previous
    // crops untouched, rather than partially rewriting some windows.
    thread::sleep(Duration::from_millis(20));
    fs::remove_file(&framebuffer).unwrap();
    backend.proxy_window_framebuffer_signature = None;
    let error = refresh_proxy_window_crop_artifacts(&store, "legacy", &mut backend).unwrap_err();
    assert!(
        error.contains("proxy framebuffer"),
        "unexpected error: {error}"
    );
    for id in ["window-1", "window-2", "window-3"] {
        let crop = fs::read(artifact_dir.join(format!("{id}.rgba"))).unwrap();
        assert_eq!(&crop[..4], &[0x10, 0x20, 0x30, 0xFF]);
    }

    // The behavioural assertions above pass whether the framebuffer is read
    // once or once per window, so they cannot defend the cost on their own:
    // reverting the hoist keeps them green. Pin the structure that actually
    // carries it, the way the SMP trace record path is pinned.
    let source = include_str!("../proxy_window_crop_artifacts.rs");
    let refresh = source
        .split_once("pub(crate) fn refresh_proxy_window_crop_artifacts")
        .and_then(|(_, rest)| rest.split_once("\n}\n"))
        .map(|(body, _)| body)
        .expect("refresh_proxy_window_crop_artifacts must exist");
    let (before_loop, in_loop) = refresh
        .split_once(".values()")
        .expect("the refresh must iterate its crop targets");
    assert!(
        before_loop.contains("read_proxy_framebuffer"),
        "the framebuffer read must be hoisted above the per-window loop"
    );
    assert!(
        !in_loop.contains("read_proxy_framebuffer"),
        "the framebuffer must not be re-read inside the per-window loop: \
         six windows re-reading 8 MiB measured 5.04 ms against 0.97 ms"
    );

    backend.child.kill().unwrap();
    backend.child.wait().unwrap();
}

#[test]
fn proxy_window_crop_uses_apple_vz_display_runner_metadata_framebuffer_when_env_unset() {
    let _env_lock = PROXY_WINDOW_ENV_LOCK.lock().unwrap();
    let _env_guard = EnvVarGuard::capture(&[
        "BRIDGEVM_PROXY_WINDOW_FRAMEBUFFER_RGBA_FILE",
        "BRIDGEVM_PROXY_WINDOW_FRAMEBUFFER_WIDTH",
        "BRIDGEVM_PROXY_WINDOW_FRAMEBUFFER_HEIGHT",
        "BRIDGEVM_PROXY_WINDOW_BACKING_SCALE",
        "BRIDGEVM_PROXY_WINDOW_ARTIFACT_DIR",
    ]);
    for key in [
        "BRIDGEVM_PROXY_WINDOW_FRAMEBUFFER_RGBA_FILE",
        "BRIDGEVM_PROXY_WINDOW_FRAMEBUFFER_WIDTH",
        "BRIDGEVM_PROXY_WINDOW_FRAMEBUFFER_HEIGHT",
        "BRIDGEVM_PROXY_WINDOW_BACKING_SCALE",
        "BRIDGEVM_PROXY_WINDOW_ARTIFACT_DIR",
    ] {
        env::remove_var(key);
    }

    let store = temp_store();
    store.create_vm(&fast_manifest("fast-display")).unwrap();
    let bundle = store.bundle_path("fast-display");
    let framebuffer = bundle
        .join("metadata")
        .join("apple-vz-display-framebuffer.rgba");
    fs::create_dir_all(framebuffer.parent().unwrap()).unwrap();
    fs::write(&framebuffer, solid_rgba(4, 4, [0x33, 0x44, 0x55, 0xFF])).unwrap();
    store
        .write_runner_metadata(
            "fast-display",
            &RunnerMetadata {
                engine: "lightvm".to_string(),
                pid: Some(42),
                command: vec![
                    "lightvm-runner".to_string(),
                    "--apple-vz-display".to_string(),
                    "--apple-vz-display-width".to_string(),
                    "4".to_string(),
                    "--apple-vz-display-height".to_string(),
                    "4".to_string(),
                    "--apple-vz-proxy-framebuffer-rgba-file".to_string(),
                    framebuffer.display().to_string(),
                ],
                log_path: bundle.join("logs/lightvm.log"),
                started_at_unix: now_unix(),
                dry_run: false,
                launch_spec_path: None,
                guest_tools: None,
                disk: None,
                active_disk: None,
                launch_readiness: None,
                runtime_control: None,
            },
        )
        .unwrap();

    let child = Command::new("sh").arg("-c").arg("sleep 5").spawn().unwrap();
    let mut backend = SupervisedBackend::new(child);
    let mut result = serde_json::json!({
        "windows": [{
            "id": "window-1",
            "title": "Terminal",
            "bounds": {"x": 1, "y": 1, "width": 2, "height": 2}
        }]
    });

    attach_proxy_window_crop_artifacts(&store, "fast-display", &mut backend, Some(&mut result))
        .unwrap();

    let summary_path = result
        .pointer("/windows/0/window_crop_frame_summary_path")
        .and_then(serde_json::Value::as_str)
        .expect("window crop summary path");
    let summary: serde_json::Value =
        serde_json::from_slice(&fs::read(summary_path).unwrap()).unwrap();
    let crop_path = summary
        .pointer("/window_crop_frame/output_path")
        .and_then(serde_json::Value::as_str)
        .expect("crop output path");
    let crop = fs::read(crop_path).unwrap();
    assert_eq!(crop.len(), 2 * 2 * 4);
    assert_eq!(&crop[..4], &[0x33, 0x44, 0x55, 0xFF]);

    backend.child.kill().unwrap();
    backend.child.wait().unwrap();
}
