//! Split test module.

use super::helpers::*;
use crate::*;
use bridgevm_api::BridgeVmRequest;
use bridgevm_api::BridgeVmResponse;
use bridgevm_config::BootMode;
use bridgevm_config::Guest;
use bridgevm_config::VmManifest;
use bridgevm_config::VmMode;
use bridgevm_storage::RunnerMetadata;
use std::env;
use std::fs;
use std::io::BufRead;
use std::io::BufReader;
use std::io::ErrorKind;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;
use std::os::unix::net::UnixStream;
use std::process::Command;
use std::sync::atomic::AtomicUsize;
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

#[test]
fn bounded_command_output_rejects_oversized_stream() {
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "printf 12345"]);

    let error = run_bounded_command_output(command, Duration::from_secs(1), 4).unwrap_err();

    assert_eq!(error.kind(), ErrorKind::InvalidData);
}

#[test]
fn daemon_connection_workers_isolate_slow_clients() {
    let (mut slow_client, slow_server) = UnixStream::pair().unwrap();
    let (mut fast_client, fast_server) = UnixStream::pair().unwrap();
    let (request_sender, request_receiver) = mpsc::channel();
    let active_clients = Arc::new(AtomicUsize::new(0));

    spawn_connection_worker(
        slow_server,
        request_sender.clone(),
        Arc::clone(&active_clients),
    );
    slow_client.write_all(b"{").unwrap();
    spawn_connection_worker(fast_server, request_sender, Arc::clone(&active_clients));
    serde_json::to_writer(&mut fast_client, &BridgeVmRequest::Doctor).unwrap();
    fast_client.write_all(b"\n").unwrap();

    let pending = request_receiver
        .recv_timeout(Duration::from_millis(500))
        .expect("fast request should not wait for slow client timeout");
    let mut state = DaemonState::new(temp_store());
    pending
        .response_sender
        .send(state.handle_request(pending.request))
        .unwrap();

    fast_client
        .set_read_timeout(Some(Duration::from_millis(500)))
        .unwrap();
    let mut response = String::new();
    BufReader::new(fast_client)
        .read_line(&mut response)
        .unwrap();
    assert!(!response.is_empty());
    drop(slow_client);
}

#[test]
fn daemon_request_reader_rejects_oversized_frame() {
    let (mut client, server) = UnixStream::pair().unwrap();
    let writer = thread::spawn(move || {
        let oversized = vec![b'x'; MAX_DAEMON_FRAME_BYTES as usize + 1];
        let _ = client.write_all(&oversized);
    });

    let error = read_daemon_request(&server).unwrap_err();
    assert!(error.to_string().contains("exceeded 16777216 bytes"));
    writer.join().unwrap();
}

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

    let error = materialize_proxy_window_crop(&config, &clipped, &output).unwrap_err();
    assert!(error.contains("has 5 bytes, expected 4"));
    assert!(!output.exists());

    config.framebuffer_width = 8193;
    config.framebuffer_height = 8193;
    let error = materialize_proxy_window_crop(&config, &clipped, &output).unwrap_err();
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

#[test]
fn bind_daemon_listener_refuses_live_socket() {
    let store = temp_store();
    let socket_path = store.root().join("run").join("bridgevmd.sock");
    fs::create_dir_all(socket_path.parent().unwrap()).unwrap();
    let _live_listener = UnixListener::bind(&socket_path).unwrap();

    let error = bind_daemon_listener(&socket_path).unwrap_err();

    assert!(error.to_string().contains("already in use"));
    assert!(UnixStream::connect(&socket_path).is_ok());
}

#[test]
fn bind_daemon_listener_uses_owner_only_permissions() {
    let store = temp_store();
    let run_dir = store.root().join("run");
    let socket_path = run_dir.join("bridgevmd.sock");

    let _listener = bind_daemon_listener(&socket_path).unwrap();

    assert_eq!(
        fs::metadata(&run_dir).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(&socket_path).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[test]
fn bind_daemon_listener_refuses_non_socket_path() {
    let store = temp_store();
    let socket_path = store.root().join("run").join("bridgevmd.sock");
    fs::create_dir_all(socket_path.parent().unwrap()).unwrap();
    fs::write(&socket_path, "not a socket").unwrap();

    let error = bind_daemon_listener(&socket_path).unwrap_err();

    assert!(error.to_string().contains("not a socket"));
    assert_eq!(fs::read_to_string(&socket_path).unwrap(), "not a socket");
}

#[test]
fn daemon_connection_lists_vms_with_swift_dashboard_wire_shape() {
    let store = temp_store();
    store.create_vm(&compatibility_manifest("legacy")).unwrap();

    let response = daemon_request(store.clone(), BridgeVmRequest::ListVms);
    let BridgeVmResponse::VmList { vms } = response else {
        panic!("expected VM list response");
    };
    assert_eq!(vms.len(), 1);
    assert_eq!(vms[0].name, "legacy");
    assert_eq!(vms[0].mode, "compatibility");
    assert_eq!(vms[0].guest_os, "ubuntu");
    assert_eq!(vms[0].guest_arch, "x86_64");
    assert_eq!(vms[0].state, "stopped");
    assert!(vms[0].path.ends_with("vms/legacy.vmbridge"));

    let json = serde_json::to_string(&BridgeVmResponse::VmList { vms }).unwrap();
    assert!(json.contains(r#""type":"vm_list""#));
    assert!(json.contains(r#""guest_os":"ubuntu""#));
    assert!(json.contains(r#""guest_arch":"x86_64""#));
}

#[test]
fn daemon_connection_creates_vm_from_dashboard_manifest_shape() {
    let store = temp_store();
    let mut manifest = VmManifest::new(
        "Ubuntu Daily",
        VmMode::Fast,
        Guest {
            os: "ubuntu".to_string(),
            version: None,
            arch: "arm64".to_string(),
        },
        "80GiB",
    );
    manifest.boot = Some(bridgevm_config::Boot {
        mode: BootMode::LinuxInstaller,
        installer_image: Some("installers/ubuntu-arm64.iso".to_string()),
        kernel_path: None,
        initrd_path: None,
        kernel_command_line: None,
        macos_restore_image: None,
    });

    let response = daemon_request(store.clone(), BridgeVmRequest::create_vm(manifest.clone()));
    let BridgeVmResponse::Vm { vm } = response else {
        panic!("expected VM create response");
    };
    assert_eq!(vm.name, "Ubuntu Daily");
    assert_eq!(vm.mode, "fast");
    assert_eq!(vm.guest_os, "ubuntu");
    assert_eq!(vm.guest_arch, "arm64");
    assert_eq!(vm.state, "stopped");

    let (_, stored) = store.get_vm("Ubuntu Daily").unwrap();
    assert_eq!(stored.network.hostname, "ubuntu-daily.bridgevm.local");
    assert_eq!(
        stored
            .boot
            .as_ref()
            .and_then(|boot| boot.installer_image.as_deref()),
        Some("installers/ubuntu-arm64.iso")
    );

    let response = daemon_request(store, BridgeVmRequest::ListVms);
    let BridgeVmResponse::VmList { vms } = response else {
        panic!("expected VM list response");
    };
    assert_eq!(vms.len(), 1);
    assert_eq!(vms[0].name, "Ubuntu Daily");
}
