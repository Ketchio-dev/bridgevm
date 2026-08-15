//! Split test module.

use super::helpers::*;
use crate::*;
use bridgevm_api::{BridgeVmRequest, BridgeVmResponse};
use bridgevm_config::{BootMode, Guest, VmManifest, VmMode};
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
    // Set the timeout while the peer is guaranteed open: once the worker has
    // written its response and closed its end, macOS setsockopt(SO_RCVTIMEO)
    // on the disconnected socket fails with EINVAL (CI run at ffce775).
    fast_client
        .set_read_timeout(Some(Duration::from_secs(20)))
        .unwrap();
    serde_json::to_writer(&mut fast_client, &BridgeVmRequest::Doctor).unwrap();
    fast_client.write_all(b"\n").unwrap();

    // Generous deadline: CI schedules the worker slowly under load.
    let pending = request_receiver
        .recv_timeout(Duration::from_secs(20))
        .expect("fast request should not wait for slow client timeout");
    let mut state = DaemonState::new(temp_store());
    pending
        .response_sender
        .send(state.handle_request(pending.request))
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
